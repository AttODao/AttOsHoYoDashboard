//! The third-party code list (`GET <base>/codes?game=…`): bounded requests and strict
//! validation. Everything it returns is untrusted; errors carry only a class, never a body.
use reqwest::{Client, StatusCode, Url, redirect::Policy};
use serde_json::Value;
use std::{collections::BTreeSet, time::Duration};

/// Whole request, including the body.
const TIMEOUT: Duration = Duration::from_secs(15);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
/// A real list is a few kilobytes; anything far larger is not a code list.
pub(super) const MAX_BODY: usize = 256 * 1024;
/// Rows considered per game and fetch; the rest are rejected.
pub(super) const MAX_ROWS: usize = 200;
/// Reward text is cut to this many characters.
pub(super) const MAX_REWARDS: usize = 300;
const CODE_LENGTH: std::ops::RangeInclusive<usize> = 4..=32;
/// Same-host redirects followed before giving up.
const MAX_REDIRECTS: usize = 3;
/// Punctuation kept in reward text besides letters, digits and single spaces. Markup
/// characters (`[]<>`, backticks, `_~|#@\`) and controls are dropped.
/// `:` is dropped too, so a reward text never forms a link a chat client would open.
const REWARD_PUNCTUATION: &str = "*;,.'’()&+-/%!×";

/// Why a fetch failed; `as_str` is what status, logs and metrics carry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::dashboard) enum FetchError {
    Timeout,
    Connect,
    HttpStatus,
    /// A redirect to another host, scheme or port, or too many redirects.
    Redirect,
    TooLarge,
    InvalidPayload,
}

impl FetchError {
    pub(in crate::dashboard) const ALL: [Self; 6] = [
        Self::Timeout,
        Self::Connect,
        Self::HttpStatus,
        Self::Redirect,
        Self::TooLarge,
        Self::InvalidPayload,
    ];

    pub(in crate::dashboard) fn as_str(self) -> &'static str {
        match self {
            Self::Timeout => "timeout",
            Self::Connect => "connect",
            Self::HttpStatus => "http_status",
            Self::Redirect => "redirect",
            Self::TooLarge => "too_large",
            Self::InvalidPayload => "invalid_payload",
        }
    }

    fn from_reqwest(error: &reqwest::Error) -> Self {
        if error.is_timeout() {
            Self::Timeout
        } else if error.is_redirect() {
            Self::Redirect
        } else if error.is_body() || error.is_decode() {
            Self::InvalidPayload
        } else {
            Self::Connect
        }
    }
}

/// One valid code row: normalized code and sanitized reward text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct SourceCode {
    pub(super) code: String,
    pub(super) rewards: String,
}

/// The codes the source lists for one game, plus how many rows were rejected.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct Listing {
    pub(super) codes: Vec<SourceCode>,
    pub(super) rejected: u64,
}

/// The source's game name for an AHD game kind; other games are never requested.
pub(super) fn source_game(game_kind: &str) -> Option<&'static str> {
    match game_kind {
        "genshin" => Some("genshin"),
        "starrail" => Some("hkrpg"),
        _ => None,
    }
}

pub(in crate::dashboard) struct Source {
    base: Url,
    client: Option<Client>,
}

impl Source {
    /// `base` ends in `/` (see `config::codes_source`).
    pub(in crate::dashboard) fn new(base: Url) -> Self {
        Self::build(base, TIMEOUT)
    }

    pub(super) fn build(base: Url, timeout: Duration) -> Self {
        let host = base.host_str().map(str::to_owned);
        let scheme = base.scheme().to_owned();
        let port = base.port_or_known_default();
        let policy = Policy::custom(move |attempt| {
            let url = attempt.url();
            let same = url.scheme() == scheme
                && url.host_str() == host.as_deref()
                && url.port_or_known_default() == port;
            if same && attempt.previous().len() <= MAX_REDIRECTS {
                attempt.follow()
            } else {
                // Returns the redirect response itself, classified as `Redirect`.
                attempt.stop()
            }
        });
        let client = Client::builder()
            .timeout(timeout)
            .connect_timeout(CONNECT_TIMEOUT.min(timeout))
            .redirect(policy)
            .user_agent("AHD redemption codes")
            .build()
            .ok();
        Self { base, client }
    }

    /// The source's host, shown to administrators.
    pub(in crate::dashboard) fn host(&self) -> &str {
        self.base.host_str().unwrap_or_default()
    }

    fn url(&self, game: &str) -> Result<Url, FetchError> {
        let mut url = self.base.join("codes").map_err(|_| FetchError::Connect)?;
        url.query_pairs_mut().append_pair("game", game);
        Ok(url)
    }

    /// One bounded GET for `game_kind`.
    pub(super) async fn fetch(&self, game_kind: &str) -> Result<Listing, FetchError> {
        let game = source_game(game_kind).ok_or(FetchError::InvalidPayload)?;
        let client = self.client.as_ref().ok_or(FetchError::Connect)?;
        let mut response = client
            .get(self.url(game)?)
            .header(reqwest::header::ACCEPT, "application/json")
            .send()
            .await
            .map_err(|error| FetchError::from_reqwest(&error))?;
        let status = response.status();
        if status.is_redirection() {
            return Err(FetchError::Redirect);
        }
        if status != StatusCode::OK {
            return Err(FetchError::HttpStatus);
        }
        if response
            .content_length()
            .is_some_and(|length| length > MAX_BODY as u64)
        {
            return Err(FetchError::TooLarge);
        }
        let mut body = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|error| FetchError::from_reqwest(&error))?
        {
            if body.len() + chunk.len() > MAX_BODY {
                return Err(FetchError::TooLarge);
            }
            body.extend_from_slice(&chunk);
        }
        parse(&body, game)
    }
}

/// Validates a `{"codes": [...], "game": …}` body for `game` (the source's name).
///
/// A row is kept when its code is 4 to 32 ASCII letters or digits (normalized to upper
/// case), its `game` (if present) is `game`, and its `rewards` is a string, null or
/// absent; rows whose `status` is present and not `OK` are not listed and not counted.
/// Other rows, and rows beyond `MAX_ROWS`, are rejected and counted. A body that is not
/// such an object, or whose rows were all rejected, is `InvalidPayload`.
pub(super) fn parse(body: &[u8], game: &str) -> Result<Listing, FetchError> {
    let root: Value = serde_json::from_slice(body).map_err(|_| FetchError::InvalidPayload)?;
    let rows = root
        .get("codes")
        .and_then(Value::as_array)
        .ok_or(FetchError::InvalidPayload)?;
    match root.get("game") {
        None => {}
        Some(Value::String(named)) if named == game => {}
        Some(_) => return Err(FetchError::InvalidPayload),
    }
    let mut listing = Listing::default();
    let mut seen = BTreeSet::new();
    let mut inactive = 0_u64;
    for row in rows.iter().take(MAX_ROWS) {
        match row_code(row, game) {
            Row::Valid(code) => {
                if seen.insert(code.code.clone()) {
                    listing.codes.push(code);
                }
            }
            Row::Inactive => inactive += 1,
            Row::Rejected => listing.rejected += 1,
        }
    }
    listing.rejected += rows.len().saturating_sub(MAX_ROWS) as u64;
    if listing.codes.is_empty() && inactive == 0 && listing.rejected > 0 {
        return Err(FetchError::InvalidPayload);
    }
    Ok(listing)
}

enum Row {
    Valid(SourceCode),
    Inactive,
    Rejected,
}

fn row_code(row: &Value, game: &str) -> Row {
    let Some(row) = row.as_object() else {
        return Row::Rejected;
    };
    match row.get("game") {
        None => {}
        Some(Value::String(named)) if named == game => {}
        Some(_) => return Row::Rejected,
    }
    let Some(code) = row.get("code").and_then(Value::as_str).and_then(code) else {
        return Row::Rejected;
    };
    let rewards = match row.get("rewards") {
        None | Some(Value::Null) => String::new(),
        Some(Value::String(text)) => rewards(text),
        Some(_) => return Row::Rejected,
    };
    match row.get("status") {
        None => {}
        Some(Value::String(status)) if status == "OK" => {}
        Some(Value::String(_)) => return Row::Inactive,
        Some(_) => return Row::Rejected,
    }
    Row::Valid(SourceCode { code, rewards })
}

/// A code as stored: 4 to 32 ASCII letters or digits, upper case (redemption ignores case).
pub(super) fn code(raw: &str) -> Option<String> {
    let code = raw.trim();
    (CODE_LENGTH.contains(&code.len()) && code.bytes().all(|b| b.is_ascii_alphanumeric()))
        .then(|| code.to_ascii_uppercase())
}

/// Reward text as stored: letters, digits, single spaces and plain punctuation only, at
/// most `MAX_REWARDS` characters.
pub(super) fn rewards(raw: &str) -> String {
    let mut out = String::new();
    let mut count = 0;
    let mut space = false;
    for c in raw.chars() {
        if c.is_whitespace() {
            space = true;
            continue;
        }
        if !(c.is_alphanumeric() || REWARD_PUNCTUATION.contains(c)) {
            continue;
        }
        let needed = if space && count > 0 { 2 } else { 1 };
        if count + needed > MAX_REWARDS {
            break;
        }
        if needed == 2 {
            out.push(' ');
        }
        out.push(c);
        count += needed;
        space = false;
    }
    out
}

#[cfg(test)]
#[path = "source_tests.rs"]
mod tests;
