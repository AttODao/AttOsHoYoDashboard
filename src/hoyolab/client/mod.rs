mod identity;

use rand::distr::{Alphabetic, SampleString};
use reqwest::{
    Client,
    header::{COOKIE, HeaderValue, USER_AGENT},
};
use serde::Deserialize;

use super::{
    error::{Error, Result},
    game::Game as GameSpec,
    models::{ApiResponse, account::Account},
};
use serde_json::value::RawValue;
use std::{
    collections::HashMap,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

const LANG: &str = "ja-jp";
const UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/58.0.3029.110 Safari/537.36";
const DS_SALT: &str = "6s25p5ox5y14umn1p61aqyyvbvvl3lrt";
const MAX_RESPONSE: usize = 4 << 20;
const ROLES_URL: &str = "https://api-os-takumi.mihoyo.com/binding/api/getUserGameRolesByCookie";

/// Cookie-authenticated client for overseas HoYoLAB APIs.
/// The cookie is sensitive and is never included in error messages.
pub struct HoyolabClient {
    http: Client,
}

#[derive(Debug, Clone)]
pub struct RecordAccount {
    pub uid: String,
    pub server: String,
}

impl HoyolabClient {
    pub fn new(cookie: &str) -> Result<Self> {
        Self::with_user_agent(cookie, UA)
    }

    pub fn with_user_agent(cookie: &str, user_agent: &str) -> Result<Self> {
        Ok(Self {
            http: build_client(cookie, user_agent)?,
        })
    }

    /// Lists linked game accounts without exposing the authentication cookie.
    pub async fn accounts(&self) -> Result<Vec<Account>> {
        #[derive(Deserialize)]
        struct Accounts {
            list: Vec<Account>,
        }
        let data = request(&self.http, ROLES_URL, &[], None).await?;
        Ok(parse::<Accounts>(&data)?.list)
    }

    pub async fn index<G: GameSpec>(&self, game: G, account: &RecordAccount) -> Result<G::Index> {
        self.record(game, account, "index").await
    }

    pub async fn daily_note<G: GameSpec>(
        &self,
        game: G,
        account: &RecordAccount,
    ) -> Result<G::DailyNote> {
        self.record(game, account, G::NOTE_PATH).await
    }

    pub async fn daily_info<G: GameSpec>(&self, game: G) -> Result<G::DailyInfo> {
        self.daily(game, "info", reqwest::Method::GET).await
    }

    /// Claims the reward. This POST changes account state and is not retried.
    /// Check the returned risk/verification fields before treating it as a completed claim.
    pub async fn daily_sign<G: GameSpec>(&self, game: G) -> Result<G::DailySign> {
        self.daily(game, "sign", reqwest::Method::POST).await
    }

    pub async fn daily_home<G: GameSpec>(&self, game: G) -> Result<G::DailyHome> {
        self.daily(game, "home", reqwest::Method::GET).await
    }

    async fn record<G: GameSpec, T: serde::de::DeserializeOwned>(
        &self,
        _game: G,
        account: &RecordAccount,
        endpoint: &str,
    ) -> Result<T> {
        if account.uid.is_empty()
            || !account.uid.bytes().all(|b| b.is_ascii_digit())
            || account.server.trim().is_empty()
        {
            return Err("ゲームUIDまたはserverが不正です".into());
        }
        let url = format!("{}/{endpoint}", G::RECORD_BASE);
        let data = request(
            &self.http,
            &url,
            &[("role_id", &account.uid), ("server", &account.server)],
            None,
        )
        .await?;
        parse(&data)
    }

    async fn daily<G: GameSpec, T: serde::de::DeserializeOwned>(
        &self,
        _game: G,
        endpoint: &str,
        method: reqwest::Method,
    ) -> Result<T> {
        let url = format!("{}/{endpoint}", G::DAILY_BASE);
        let request = prepare_request(
            &self.http,
            &url,
            &[("act_id", G::ACT_ID), ("lang", LANG)],
            Some(G::SIGN_GAME),
            method,
        )?;
        parse(&send(request).await?)
    }
}

fn build_client(cookie: &str, ua: &str) -> Result<Client> {
    validate_cookie(cookie)?;
    let mut cookie = HeaderValue::from_str(cookie).map_err(|_| "Cookieヘッダーが不正です")?;
    cookie.set_sensitive(true);
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert(COOKIE, cookie);
    headers.insert(
        USER_AGENT,
        HeaderValue::from_str(ua).map_err(|_| "User-Agentが不正です")?,
    );
    Client::builder()
        .default_headers(headers)
        .timeout(Duration::from_secs(20))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| Error::ClientConfiguration)
}

async fn request(
    client: &Client,
    url: &str,
    query: &[(&str, &str)],
    sign_game: Option<&str>,
) -> Result<Box<RawValue>> {
    send(prepare_request(
        client,
        url,
        query,
        sign_game,
        reqwest::Method::GET,
    )?)
    .await
}

fn prepare_request(
    client: &Client,
    url: &str,
    query: &[(&str, &str)],
    sign_game: Option<&str>,
    method: reqwest::Method,
) -> Result<reqwest::RequestBuilder> {
    let mut request = client.request(method, url).query(query);
    if let Some(game) = sign_game {
        request = request
            .header("Referer", "https://act.hoyolab.com/")
            .header("x-rpc-signgame", game);
    } else {
        request = request
            .header("DS", dynamic_secret()?)
            .header("x-rpc-app_version", "1.5.0")
            .header("x-rpc-client_type", "5")
            .header("x-rpc-language", LANG)
            .header("x-rpc-lang", LANG);
    }
    Ok(request)
}

async fn send(request: reqwest::RequestBuilder) -> Result<Box<RawValue>> {
    let mut response = request.send().await.map_err(|_| Error::Network)?;
    if !response.status().is_success() {
        return Err(Error::Http(response.status().as_u16()));
    }
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| Error::ResponseRead)? {
        if chunk.len() > MAX_RESPONSE - body.len() {
            return Err(Error::ResponseTooLarge(MAX_RESPONSE));
        }
        body.extend_from_slice(&chunk);
    }
    decode_response(&body)
}

fn decode_response(body: &[u8]) -> Result<Box<RawValue>> {
    let envelope: ApiResponse<Box<RawValue>> =
        serde_json::from_slice(body).map_err(|_| Error::InvalidEnvelope)?;
    if envelope.retcode != 0 {
        // Upstream messages may echo private values; retain only the numeric code.
        return Err(Error::Upstream {
            code: envelope.retcode,
        });
    }
    envelope.data.ok_or(Error::MissingData)
}

fn parse<T: serde::de::DeserializeOwned>(data: &RawValue) -> Result<T> {
    serde_json::from_str(data.get()).map_err(|error| {
        // Report missing schema fields, never unexpected values from private payloads.
        let detail = error.to_string();
        let safe_detail = if detail.starts_with("missing field `") {
            format!("レスポンスdataの形式が不正です: {detail}")
        } else {
            format!(
                "レスポンスdataの形式が不正です（line {}, column {}）",
                error.line(),
                error.column()
            )
        };
        Error::InvalidData(safe_detail)
    })
}

fn dynamic_secret() -> Result<String> {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| Error::Clock)?
        .as_secs();
    let nonce = Alphabetic.sample_string(&mut rand::rng(), 6);
    Ok(sign_secret(timestamp, &nonce))
}

fn sign_secret(timestamp: u64, nonce: &str) -> String {
    // MD5 is required by HoYoLAB's DS protocol, not used for security here.
    let hash = md5::compute(format!("salt={DS_SALT}&t={timestamp}&r={nonce}"));
    format!("{timestamp},{nonce},{hash:x}")
}

fn validate_cookie(raw: &str) -> Result<bool> {
    if raw.is_empty() {
        return Err("環境変数または.envに HOYOLAB_COOKIE を設定してください".into());
    }
    if raw.contains(['\r', '\n']) || HeaderValue::from_str(raw).is_err() {
        return Err("HOYOLAB_COOKIE に不正な文字を含めることはできません".into());
    }
    let values: HashMap<_, _> = raw
        .split(';')
        .filter_map(|part| part.trim().split_once('='))
        .map(|(key, value)| (key.trim(), value.trim().trim_matches('"')))
        .collect();
    let pair = |uid, fallback, token| {
        let uid = values
            .get(uid)
            .filter(|v| !v.is_empty())
            .or_else(|| values.get(fallback))
            .copied()
            .unwrap_or("");
        let token = values.get(token).copied().unwrap_or("");
        (!uid.is_empty() && !token.is_empty()).then_some((uid, token))
    };
    let (uid, token, legacy) =
        if let Some((uid, token)) = pair("ltuid_v2", "account_id_v2", "ltoken_v2") {
            (uid, token, false)
        } else if let Some((uid, token)) = pair("ltuid", "account_id", "ltoken") {
            (uid, token, true)
        } else {
            return Err("HOYOLAB_COOKIE には少なくとも ltuid_v2 と ltoken_v2 が必要です".into());
        };
    if [uid, token]
        .iter()
        .any(|value| value.contains("...") || value.starts_with("YOUR_"))
    {
        return Err(".env のプレースホルダーを実際のCookie値に置き換えてください".into());
    }
    Ok(legacy)
}

#[cfg(test)]
mod test_support;

#[cfg(test)]
mod endpoint_tests;

#[cfg(test)]
mod identity_tests;

#[cfg(test)]
mod protocol_tests;

#[cfg(test)]
#[path = "validation_tests.rs"]
mod tests;

#[cfg(test)]
mod live_tests;
