//! Gift-code redemption on HoYoverse's web endpoints, as genshin.py's `redeem_code` sends
//! it (pinned commit in `docs/hoyolab/code-redemption.md`). One call is one request that
//! may redeem a code: it is never retried here, and any transport or parse error means the
//! outcome is unknown, never "not sent". Only the numeric retcode of an answer is kept.
use super::{Error, Result, read_body, shared_http};
use crate::game::GameKind;
use reqwest::{
    Client, Method,
    header::{CONTENT_TYPE, COOKIE, HeaderValue},
};
use serde::Deserialize;
use serde_json::Value;
use std::time::{SystemTime, UNIX_EPOCH};

/// Genshin Impact: a GET whose query carries the code (genshin.py `CODE_URL`).
pub const GENSHIN_URL: &str =
    "https://public-operation-hk4e.hoyoverse.com/common/apicdkey/api/webExchangeCdkey";
/// Honkai: Star Rail: a POST with a JSON body (genshin.py `CODE_URL`).
pub const STARRAIL_URL: &str =
    "https://public-operation-hkrpg.hoyoverse.com/common/apicdkey/api/webExchangeCdkeyRisk";
/// genshin.py sends `create_short_lang_code(lang)`: `ja-jp` becomes `ja`.
const LANG: &str = "ja";
/// genshin.py `GEETEST_RETCODES`: answers that ask for a CAPTCHA.
const GEETEST_RETCODES: [i64; 4] = [10035, 5003, 10041, 1034];

/// One code for one game account. `region` is the server the role list reported
/// (`os_asia`, `prod_official_asia`, …).
#[derive(Debug, Clone, Copy)]
pub struct RedemptionRequest<'a> {
    pub game: GameKind,
    pub uid: &'a str,
    pub region: &'a str,
    pub code: &'a str,
}

/// What HoYoLAB answered, classified from genshin.py's `errors.py` retcode table. None of
/// these were confirmed by a live response.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RedemptionReply {
    /// retcode 0 without a CAPTCHA challenge.
    Redeemed,
    /// -2017, -2018 (`RedemptionClaimed`): this account already used it.
    AlreadyUsed,
    /// -2001 (expired), -2006 (usage limit reached): no account can use it any more.
    Expired,
    /// -1065, -2003 (badly formatted), -2004: the code does not exist.
    Invalid,
    /// -2008 (region lock), -2014 (not activated): not usable by this account now.
    NotForAccount,
    /// -2011, -2021 (`RedeemGameLevelTooLow`).
    RankTooLow,
    /// -1071, -100, 10001 (`InvalidCookies`), -1073 (no game account bound to the Cookie).
    CookieInvalid,
    /// A Geetest retcode, or a `gt_result`/risk challenge in `data`.
    CaptchaRequired,
    /// -2016 (`RedemptionCooldown`), -110 and 1028 (`VisitsTooFrequently`): not processed.
    Cooldown,
    /// Any other retcode: whether the code was used is unknown.
    Other(i64),
}

impl RedemptionReply {
    /// The class of a retcode (without the CAPTCHA checks on `data`).
    pub fn classify(retcode: i64) -> Self {
        match retcode {
            0 => Self::Redeemed,
            -2017 | -2018 => Self::AlreadyUsed,
            -2001 | -2006 => Self::Expired,
            -1065 | -2003 | -2004 => Self::Invalid,
            -2008 | -2014 => Self::NotForAccount,
            -2011 | -2021 => Self::RankTooLow,
            -1071 | -1073 | -100 | 10001 => Self::CookieInvalid,
            code if GEETEST_RETCODES.contains(&code) => Self::CaptchaRequired,
            -2016 | -110 | 1028 => Self::Cooldown,
            code => Self::Other(code),
        }
    }
}

/// A client holding only the redemption Cookie (`cookie_token_v2`, `account_id_v2` and
/// optionally `account_mid_v2`); the cookie is never included in errors.
pub struct RedemptionClient {
    http: Client,
    cookie: HeaderValue,
}

impl RedemptionClient {
    pub fn new(cookie: &str) -> Result<Self> {
        Self::with_http(shared_http()?, cookie)
    }

    pub(super) fn with_http(http: Client, cookie: &str) -> Result<Self> {
        validate_cookie(cookie)?;
        let mut cookie = HeaderValue::from_str(cookie).map_err(|_| "Cookieヘッダーが不正です")?;
        cookie.set_sensitive(true);
        Ok(Self { http, cookie })
    }

    /// Sends the one request that may redeem `request.code`.
    pub async fn redeem(&self, request: &RedemptionRequest<'_>) -> Result<RedemptionReply> {
        let t = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| Error::Clock)?
            .as_secs();
        self.redeem_with(request, &uuid::Uuid::new_v4().to_string(), t)
            .await
    }

    async fn redeem_with(
        &self,
        request: &RedemptionRequest<'_>,
        device_uuid: &str,
        t: u64,
    ) -> Result<RedemptionReply> {
        decode(&read_body(self.prepare(request, device_uuid, t)?).await?)
    }

    /// genshin.py: Genshin is a GET with `uid, region, lang, cdkey, game_biz` as query;
    /// Star Rail is a POST whose JSON body also carries a random `device_uuid`,
    /// `platform: "4"` and the Unix time `t`. Both send the game site's Origin/Referer and
    /// the Cookie, no DS header.
    fn prepare(
        &self,
        request: &RedemptionRequest<'_>,
        device_uuid: &str,
        t: u64,
    ) -> Result<reqwest::RequestBuilder> {
        validate_request(request)?;
        let builder = match request.game {
            GameKind::Genshin(_) => self
                .http
                .request(Method::GET, GENSHIN_URL)
                .query(&[
                    ("uid", request.uid),
                    ("region", request.region),
                    ("lang", LANG),
                    ("cdkey", request.code),
                    ("game_biz", "hk4e_global"),
                ])
                .header("Origin", "https://genshin.hoyoverse.com")
                .header("Referer", "https://genshin.hoyoverse.com/"),
            GameKind::Starrail(_) => {
                let uid: u64 = request.uid.parse().map_err(|_| "ゲームUIDが不正です")?;
                // Keys in genshin.py's order (also alphabetical, as serde_json writes them).
                let body = serde_json::json!({
                    "cdkey": request.code,
                    "device_uuid": device_uuid,
                    "game_biz": "hkrpg_global",
                    "lang": LANG,
                    "platform": "4",
                    "region": request.region,
                    "t": t,
                    "uid": uid,
                });
                self.http
                    .request(Method::POST, STARRAIL_URL)
                    .header("Origin", "https://hsr.hoyoverse.com")
                    .header("Referer", "https://hsr.hoyoverse.com/")
                    .header(CONTENT_TYPE, "application/json")
                    .body(body.to_string())
            }
        };
        Ok(builder.header(COOKIE, self.cookie.clone()))
    }
}

fn validate_request(request: &RedemptionRequest<'_>) -> Result<()> {
    let uid_ok = (1..=20).contains(&request.uid.len())
        && request.uid.bytes().all(|b| b.is_ascii_digit())
        && request.uid.parse::<u64>().is_ok_and(|uid| uid > 0);
    let region_ok = (1..=64).contains(&request.region.len())
        && request
            .region
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_');
    let code_ok = (4..=32).contains(&request.code.len())
        && request
            .code
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit());
    if uid_ok && region_ok && code_ok {
        Ok(())
    } else {
        Err("ゲームUID、serverまたはコードが不正です".into())
    }
}

/// The redemption Cookie must name `cookie_token_v2` and `account_id_v2`.
fn validate_cookie(raw: &str) -> Result<()> {
    if raw.is_empty() || raw.contains(['\r', '\n']) || HeaderValue::from_str(raw).is_err() {
        return Err("引き換え用のCookieが不正です".into());
    }
    let has = |name: &str| {
        raw.split(';')
            .filter_map(|part| part.trim().split_once('='))
            .any(|(key, value)| key == name && !value.is_empty())
    };
    if !has("cookie_token_v2") || !has("account_id_v2") {
        return Err("引き換えには cookie_token_v2 と account_id_v2 が必要です".into());
    }
    Ok(())
}

/// genshin.py `_request`: `check_for_geetest` first, then the retcode. A body that is not
/// the JSON envelope is an error (outcome unknown); messages and `data` are discarded.
fn decode(body: &[u8]) -> Result<RedemptionReply> {
    #[derive(Deserialize)]
    struct Envelope {
        retcode: i64,
        #[serde(default)]
        data: Option<Value>,
    }
    let envelope: Envelope = serde_json::from_slice(body).map_err(|_| Error::InvalidEnvelope)?;
    if envelope.data.as_ref().is_some_and(risk_challenge) {
        return Ok(RedemptionReply::CaptchaRequired);
    }
    Ok(RedemptionReply::classify(envelope.retcode))
}

/// genshin.py `check_for_geetest` on `data`: its `gt_result` (or `data` itself) with
/// `risk_code != 0`, a non-empty `gt` and `challenge`, and `success != 0`.
fn risk_challenge(data: &Value) -> bool {
    if !truthy(data) {
        return false;
    }
    let Some(object) = data.as_object() else {
        return false;
    };
    let result = object.get("gt_result").unwrap_or(data);
    if !truthy(result) {
        return false;
    }
    let not_zero = |name: &str| result.get(name).is_none_or(|value| !is_zero(value));
    not_zero("risk_code")
        && result.get("gt").is_some_and(truthy)
        && result.get("challenge").is_some_and(truthy)
        && not_zero("success")
}

/// Python truthiness of a JSON value.
fn truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(flag) => *flag,
        Value::Number(number) => number.as_f64().is_some_and(|n| n != 0.0),
        Value::String(text) => !text.is_empty(),
        Value::Array(items) => !items.is_empty(),
        Value::Object(fields) => !fields.is_empty(),
    }
}

/// Python `value == 0` (`False == 0` too).
fn is_zero(value: &Value) -> bool {
    match value {
        Value::Number(number) => number.as_f64() == Some(0.0),
        Value::Bool(flag) => !flag,
        _ => false,
    }
}

#[cfg(test)]
#[path = "redemption_tests.rs"]
mod tests;
