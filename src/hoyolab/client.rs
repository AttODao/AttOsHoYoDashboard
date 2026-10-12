use rand::Rng;
use reqwest::{
    Client,
    header::{COOKIE, HeaderValue, USER_AGENT},
};
use serde::Deserialize;

use super::{
    game::Game as GameSpec,
    models::{ApiResponse, account::Account},
};
use serde_json::value::RawValue;
use std::{
    collections::HashMap,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

pub type Result<T> = std::result::Result<T, String>;
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
        .map_err(|_| "HTTPクライアントの作成に失敗しました".into())
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
    let mut response = request
        .send()
        .await
        .map_err(|_| "HTTPリクエストに失敗しました")?;
    if !response.status().is_success() {
        return Err(format!("HTTP {}", response.status()));
    }
    let mut body = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| "レスポンス読み取りに失敗しました")?
    {
        if chunk.len() > MAX_RESPONSE - body.len() {
            return Err(format!("レスポンスが上限{MAX_RESPONSE} bytesを超えました"));
        }
        body.extend_from_slice(&chunk);
    }
    decode_response(&body)
}

fn decode_response(body: &[u8]) -> Result<Box<RawValue>> {
    let envelope: ApiResponse<Box<RawValue>> =
        serde_json::from_slice(body).map_err(|_| "JSONレスポンスの形式が不正です")?;
    if envelope.retcode != 0 {
        let hint = match envelope.retcode {
            -100 | 10001 | -1071 => "（Cookieが無効または期限切れです）",
            10101 => "（ゲームレコードの1日あたり参照上限の可能性があります）",
            10102 => "（HoYoLABで戦績またはリアルタイム状態の公開を有効にしてください）",
            _ => "",
        };
        return Err(format!(
            "HoYoLAB retcode={} message={:?}{hint}",
            envelope.retcode, envelope.message
        ));
    }
    envelope
        .data
        .ok_or_else(|| "HoYoLAB APIがdataを返しませんでした".into())
}

fn parse<T: serde::de::DeserializeOwned>(data: &RawValue) -> Result<T> {
    serde_json::from_str(data.get()).map_err(|error| {
        // Report missing schema fields, never unexpected values from private payloads.
        let detail = error.to_string();
        if detail.starts_with("missing field `") {
            format!("レスポンスdataの形式が不正です: {detail}")
        } else {
            format!(
                "レスポンスdataの形式が不正です（line {}, column {}）",
                error.line(),
                error.column()
            )
        }
    })
}

fn dynamic_secret() -> Result<String> {
    const LETTERS: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ";
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| "システム時刻が不正です")?
        .as_secs();
    let mut rng = rand::rng();
    let nonce: String = (0..6)
        .map(|_| LETTERS[rng.random_range(0..LETTERS.len())] as char)
        .collect();
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
mod endpoint_tests;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protocol_and_validation() {
        assert_eq!(
            validate_cookie("ltuid_v2=123; ltoken_v2=v2_secret"),
            Ok(false)
        );
        assert_eq!(validate_cookie("account_id=123; ltoken=secret"), Ok(true));
        for cookie in [
            "",
            "ltuid_v2=123",
            "ltuid_v2=YOUR_UID; ltoken_v2=secret",
            "ltuid_v2=123; ltoken_v2=secret\r\nInjected: true",
        ] {
            assert!(validate_cookie(cookie).is_err());
        }
        assert_eq!(
            sign_secret(1700000000, "AbCdEf"),
            "1700000000,AbCdEf,52762c606b53f6830e4692f53664105e"
        );
        let ds = dynamic_secret().unwrap();
        let parts: Vec<_> = ds.split(',').collect();
        assert_eq!(parts.len(), 3);
        assert_eq!(parts[1].len(), 6);
        assert!(parts[1].bytes().all(|b| b.is_ascii_alphabetic()));
        assert_eq!(ds, sign_secret(parts[0].parse().unwrap(), parts[1]));
        assert_eq!(
            decode_response(br#"{"retcode":0,"data":{"ok":true}}"#)
                .unwrap()
                .get(),
            r#"{"ok":true}"#
        );
        for body in [
            br#"{"retcode":0,"data":null}"#.as_slice(),
            br#"{"retcode":0}"#,
            br#"{"data":{}}"#,
            b"not json",
        ] {
            assert!(decode_response(body).is_err());
        }
        assert!(
            decode_response(br#"{"retcode":10102,"message":"private","data":null}"#)
                .unwrap_err()
                .contains("公開")
        );
    }

    #[tokio::test]
    async fn read_only_http() {
        use axum::{Router, extract::Query, http::HeaderMap, routing::get};
        let app = Router::new()
            .route(
                "/record",
                get(
                    |headers: HeaderMap, Query(query): Query<HashMap<String, String>>| async move {
                        assert_eq!(headers["cookie"], "ltuid_v2=123; ltoken_v2=fake");
                        assert_eq!(headers["x-rpc-language"], LANG);
                        assert_eq!(headers["x-rpc-app_version"], "1.5.0");
                        assert!(headers.contains_key("ds"));
                        assert_eq!(query["role_id"], "123");
                        r#"{"retcode":0,"data":{"ok":true}}"#
                    },
                ),
            )
            .route(
                "/daily",
                get(
                    |headers: HeaderMap, Query(query): Query<HashMap<String, String>>| async move {
                        assert_eq!(headers["x-rpc-signgame"], "hk4e");
                        assert_eq!(headers["referer"], "https://act.hoyolab.com/");
                        assert!(!headers.contains_key("ds"));
                        assert_eq!(query["act_id"], "test");
                        assert_eq!(query["lang"], LANG);
                        r#"{"retcode":0,"data":{"is_sign":true,"total_sign_day":1}}"#
                    },
                ),
            )
            .route(
                "/redirect",
                get(|| async { axum::response::Redirect::temporary("/record") }),
            )
            .route("/large", get(|| async { "x".repeat(MAX_RESPONSE + 1) }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            COOKIE,
            HeaderValue::from_static("ltuid_v2=123; ltoken_v2=fake"),
        );
        let client = Client::builder()
            .default_headers(headers)
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap();
        assert!(
            request(
                &client,
                &format!("{base}/record"),
                &[("role_id", "123")],
                None
            )
            .await
            .is_ok()
        );
        assert!(
            request(
                &client,
                &format!("{base}/daily?act_id=test"),
                &[("lang", LANG)],
                Some("hk4e")
            )
            .await
            .is_ok()
        );
        assert!(
            request(&client, &format!("{base}/redirect"), &[], None)
                .await
                .unwrap_err()
                .contains("307")
        );
        assert!(
            request(&client, &format!("{base}/large"), &[], None)
                .await
                .unwrap_err()
                .contains("上限")
        );
        server.abort();
    }
}
