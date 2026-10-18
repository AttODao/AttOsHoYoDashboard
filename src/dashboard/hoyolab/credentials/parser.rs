//! Cookie parsing and normalization, without ownership verification.
use super::{Credentials, MAX_COOKIE_LEN};
use crate::dashboard::error::{Error, Result};
use std::collections::{HashMap, HashSet};

#[cfg(test)]
mod tests;

impl Credentials {
    /// New user submissions need the full v2 bundle: whether upstream identity checks work
    /// without MID is unproven, so a Cookie lacking it is never accepted.
    pub(in crate::dashboard) fn parse_submitted(raw: &str) -> Result<Self> {
        let pairs = Pairs::parse(checked_header(raw)?);
        let token = parse_cookie_token(
            pairs
                .get("ltoken_v2")?
                .ok_or(Error::Invalid("Cookie header is missing ltoken_v2"))?,
        )?;
        let claimed_uid = parse_uid(
            pairs
                .get("ltuid_v2")?
                .ok_or(Error::Invalid("Cookie header is missing ltuid_v2"))?,
        )?;
        let mid = parse_cookie_token(
            pairs
                .get("ltmid_v2")?
                .ok_or(Error::Invalid("Cookie header is missing ltmid_v2"))?,
        )?;
        Ok(v2_credentials(token, claimed_uid, Some(mid)))
    }

    /// Normalizes any supported family; stored Cookies saved before MID was required still open.
    pub(in crate::dashboard) fn parse(raw: &str) -> Result<Self> {
        let pairs = Pairs::parse(checked_header(raw)?);
        // A complete v2 pair is sufficient; every other cookie is optional context.
        if pairs.contains("ltoken_v2") && pairs.contains("ltuid_v2") {
            return build_v2(&pairs);
        }
        let has_legacy_token = pairs.contains("ltoken");
        let has_v2_token = pairs.contains("ltoken_v2");
        if has_legacy_token && has_v2_token {
            return Err(Error::Invalid(
                "Cookie header mixes HoYoLAB credential families",
            ));
        }
        if has_legacy_token {
            return build_legacy(&pairs);
        }
        if has_v2_token {
            return build_v2(&pairs);
        }
        Err(Error::Invalid(
            "Cookie header is missing HoYoLAB credentials",
        ))
    }
}

fn checked_header(raw: &str) -> Result<&str> {
    let raw = strip_header_name(raw.trim());
    if raw.is_empty() || raw.len() > MAX_COOKIE_LEN {
        return Err(Error::Invalid("Cookie header must contain 1 to 8192 bytes"));
    }
    if !raw.is_ascii()
        || raw
            .bytes()
            .any(|byte| byte.is_ascii_control() || byte == 0x7f)
    {
        return Err(Error::Invalid("Cookie header must be printable ASCII"));
    }
    Ok(raw)
}

const COOKIE_HEADER_NAME: &str = "cookie:";

/// Accepts a pasted `Cookie: ...` request-header line as well as its bare value.
fn strip_header_name(raw: &str) -> &str {
    match raw.split_at_checked(COOKIE_HEADER_NAME.len()) {
        Some((name, value)) if name.eq_ignore_ascii_case(COOKIE_HEADER_NAME) => value.trim_start(),
        _ => raw,
    }
}

/// Non-empty cookies by name. Repeated names are kept aside and fail only when read.
struct Pairs<'a> {
    values: HashMap<&'a str, &'a str>,
    duplicates: HashSet<&'a str>,
}

impl<'a> Pairs<'a> {
    /// Browser header copies may contain empty, valueless, or unrelated malformed elements;
    /// none of them can carry credentials, so they are skipped.
    fn parse(raw: &'a str) -> Self {
        let mut pairs = Self {
            values: HashMap::new(),
            duplicates: HashSet::new(),
        };
        for part in raw.split(';') {
            let Some((name, value)) = part.trim_matches(' ').split_once('=') else {
                continue;
            };
            if value.is_empty() || name.is_empty() || !name.bytes().all(is_cookie_name_byte) {
                continue;
            }
            if pairs.values.insert(name, value).is_some() {
                pairs.duplicates.insert(name);
            }
        }
        pairs
    }

    fn contains(&self, name: &str) -> bool {
        self.values.contains_key(name)
    }

    fn get(&self, name: &str) -> Result<Option<&'a str>> {
        if self.duplicates.contains(name) {
            return Err(Error::Invalid("Cookie header contains duplicate names"));
        }
        Ok(self.values.get(name).copied())
    }
}

fn build_legacy(pairs: &Pairs) -> Result<Credentials> {
    let token = parse_cookie_token(pairs.get("ltoken")?.ok_or(Error::Invalid(
        "Cookie header is missing HoYoLAB credentials",
    ))?)?;
    let claimed_uid = resolve_uid_aliases(pairs, &["ltuid", "account_id"])?;
    reject_conflicting_uid_aliases(pairs, &claimed_uid, &["ltuid", "account_id"])?;
    let header = format!("ltoken={token}; ltuid={claimed_uid}");
    Ok(Credentials {
        header,
        claimed_uid,
    })
}

fn build_v2(pairs: &Pairs) -> Result<Credentials> {
    let token = parse_cookie_token(pairs.get("ltoken_v2")?.ok_or(Error::Invalid(
        "Cookie header is missing HoYoLAB credentials",
    ))?)?;
    let claimed_uid = match pairs.get("ltuid_v2")? {
        Some(uid) => parse_uid(uid)?,
        None => {
            let uid = resolve_uid_aliases(pairs, &["account_id_v2"])?;
            reject_conflicting_uid_aliases(pairs, &uid, &["account_id_v2"])?;
            uid
        }
    };
    Ok(v2_credentials(token, claimed_uid, optional_mid(pairs)))
}

fn v2_credentials(token: &str, claimed_uid: String, mid: Option<&str>) -> Credentials {
    let mut header = format!("ltoken_v2={token}; ltuid_v2={claimed_uid}");
    if let Some(mid) = mid {
        header.push_str("; ltmid_v2=");
        header.push_str(mid);
    }
    Credentials {
        header,
        claimed_uid,
    }
}

fn resolve_uid_aliases(pairs: &Pairs, aliases: &[&str]) -> Result<String> {
    let mut uid = None;
    for alias in aliases {
        if let Some(value) = pairs.get(alias)? {
            let parsed = parse_uid(value)?;
            if uid.as_deref().is_some_and(|uid| uid != parsed) {
                return Err(Error::Invalid(
                    "Cookie header contains conflicting UID aliases",
                ));
            }
            uid = Some(parsed);
        }
    }
    uid.ok_or(Error::Invalid("Cookie header is missing HoYoLAB UID"))
}

fn reject_conflicting_uid_aliases(
    pairs: &Pairs,
    selected_uid: &str,
    selected_aliases: &[&str],
) -> Result<()> {
    for alias in [
        "ltuid",
        "account_id",
        "ltuid_v2",
        "account_id_v2",
        "stuid",
        "stuid_v2",
    ] {
        if selected_aliases.contains(&alias) {
            continue;
        }
        if let Some(value) = pairs.get(alias)? {
            let parsed = parse_uid(value)?;
            if parsed != selected_uid {
                return Err(Error::Invalid(
                    "Cookie header contains conflicting UID aliases",
                ));
            }
        }
    }
    Ok(())
}

/// MID is optional context, so an unusable value is dropped rather than blocking the token.
fn optional_mid<'a>(pairs: &Pairs<'a>) -> Option<&'a str> {
    ["ltmid_v2", "account_mid_v2"]
        .into_iter()
        .find_map(|alias| {
            let value = pairs.get(alias).ok()??;
            parse_cookie_token(value).ok()
        })
}

fn parse_uid(value: &str) -> Result<String> {
    reject_placeholder(value)?;
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(Error::Invalid(
            "HoYoLAB UID must be a positive decimal integer",
        ));
    }
    let uid: u64 = value
        .parse()
        .map_err(|_| Error::Invalid("HoYoLAB UID must be a positive decimal integer"))?;
    if uid == 0 {
        return Err(Error::Invalid(
            "HoYoLAB UID must be a positive decimal integer",
        ));
    }
    Ok(uid.to_string())
}

fn parse_cookie_token(value: &str) -> Result<&str> {
    reject_placeholder(value)?;
    if value.is_empty() || !value.bytes().all(is_cookie_value_token_byte) {
        return Err(Error::Invalid(
            "HoYoLAB Cookie token contains invalid characters",
        ));
    }
    Ok(value)
}

fn reject_placeholder(value: &str) -> Result<()> {
    if value.starts_with("YOUR_") || value.contains("...") {
        return Err(Error::Invalid(
            "HoYoLAB Cookie contains a placeholder value",
        ));
    }
    Ok(())
}

fn is_cookie_name_byte(byte: u8) -> bool {
    matches!(
        byte,
        b'!' | b'#'
            | b'$'
            | b'%'
            | b'&'
            | b'\''
            | b'*'
            | b'+'
            | b'-'
            | b'.'
            | b'^'
            | b'_'
            | b'`'
            | b'|'
            | b'~'
            | b'0'..=b'9'
            | b'A'..=b'Z'
            | b'a'..=b'z'
    )
}

fn is_cookie_value_token_byte(byte: u8) -> bool {
    byte.is_ascii_graphic() && !matches!(byte, b';' | b'"' | b',' | b'\\')
}
