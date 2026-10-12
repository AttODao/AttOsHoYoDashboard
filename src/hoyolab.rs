use rand::Rng;
use reqwest::{
    Client,
    header::{COOKIE, HeaderValue, USER_AGENT},
};
use serde::Deserialize;
use serde_json::value::RawValue;
use std::{
    collections::HashMap,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

type Result<T> = std::result::Result<T, String>;
const LANG: &str = "ja-jp";
const UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/58.0.3029.110 Safari/537.36";
const DS_SALT: &str = "6s25p5ox5y14umn1p61aqyyvbvvl3lrt";
const MAX_RESPONSE: usize = 4 << 20;
const ROLES_URL: &str = "https://api-os-takumi.mihoyo.com/binding/api/getUserGameRolesByCookie";

#[derive(Deserialize)]
struct Envelope {
    retcode: i64,
    #[serde(default)]
    message: String,
    data: Option<Box<RawValue>>,
}

#[derive(Deserialize)]
struct Accounts {
    list: Vec<Account>,
}

#[derive(Deserialize)]
struct Account {
    game_biz: String,
    region: String,
    game_uid: String,
    #[serde(default)]
    nickname: String,
    #[serde(default)]
    level: i64,
}

#[derive(Deserialize)]
struct DailyInfo {
    total_sign_day: i64,
    is_sign: bool,
}

struct Game {
    name: &'static str,
    biz: &'static str,
    sign_game: &'static str,
    daily_url: &'static str,
    record_base: &'static str,
    notes: &'static str,
}

const GAMES: [Game; 2] = [
    Game {
        name: "原神",
        biz: "hk4e_global",
        sign_game: "hk4e",
        daily_url: "https://sg-hk4e-api.hoyolab.com/event/sol/info?act_id=e202102251931481",
        record_base: "https://sg-public-api.hoyolab.com/event/game_record/genshin/api",
        notes: "dailyNote",
    },
    Game {
        name: "崩壊：スターレイル",
        biz: "hkrpg_global",
        sign_game: "hkrpg",
        daily_url: "https://sg-public-api.hoyolab.com/event/luna/hkrpg/os/info?act_id=e202303301540311",
        record_base: "https://bbs-api-os.hoyolab.com/game_record/hkrpg/api",
        notes: "note",
    },
];

pub async fn run(env_path: &str, raw: bool) -> Result<()> {
    let config = load_config(env_path)?;
    let get = |key: &str| config.get(key).map(|v| v.trim()).unwrap_or("");
    let mut cookie = get("HOYOLAB_COOKIE").to_owned();
    if cookie.is_empty()
        && !get("HOYOLAB_LTUID_V2").is_empty()
        && !get("HOYOLAB_LTOKEN_V2").is_empty()
    {
        cookie = format!(
            "ltuid_v2={}; ltoken_v2={}",
            get("HOYOLAB_LTUID_V2"),
            get("HOYOLAB_LTOKEN_V2")
        );
    }
    let legacy = validate_cookie(&cookie)?;
    if legacy {
        eprintln!("WARN: 旧形式のltuid/ltokenを検出しました。現行のv2 Cookieを推奨します");
    }
    let mut cookie = HeaderValue::from_str(&cookie).map_err(|_| "Cookieヘッダーが不正です")?;
    cookie.set_sensitive(true);
    let ua = if get("HOYOLAB_USER_AGENT").is_empty() {
        UA
    } else {
        get("HOYOLAB_USER_AGENT")
    };
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert(COOKIE, cookie);
    headers.insert(
        USER_AGENT,
        HeaderValue::from_str(ua).map_err(|_| "User-Agentが不正です")?,
    );
    let client = Client::builder()
        .default_headers(headers)
        .timeout(Duration::from_secs(20))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| "HTTPクライアントの作成に失敗しました")?;

    println!("HoYoLAB Cookie認証とゲームアカウント一覧を確認します（Cookie値は出力しません）");
    let accounts: Accounts = parse(
        &request(&client, ROLES_URL, &[], None)
            .await
            .map_err(|e| format!("ゲームアカウント一覧: {e}"))?,
    )?;
    if accounts.list.is_empty() {
        return Err("Cookieは受理されましたが、連携済みゲームアカウントがありません".into());
    }
    println!("\n[連携アカウント]");
    for account in &accounts.list {
        println!(
            "- {}: UID={}, server={}, nickname={}, level={}",
            account.game_biz, account.game_uid, account.region, account.nickname, account.level
        );
    }
    let mut failures = Vec::new();
    for game in &GAMES {
        println!("\n[{}]", game.name);
        let Some(account) = select_account(&accounts.list, game.biz) else {
            println!(
                "SKIP: このHoYoLABアカウントに{}の海外版アカウントが連携されていません",
                game.name
            );
            failures.push(format!(
                "{}: 連携済み海外版アカウントがありません",
                game.name
            ));
            continue;
        };
        let daily = request(
            &client,
            game.daily_url,
            &[("lang", LANG)],
            Some(game.sign_game),
        )
        .await
        .and_then(|data| parse::<DailyInfo>(&data));
        match daily {
            Ok(info) => println!(
                "OK   ログインボーナス: 本日受取済み={}, 今月の受取日数={}",
                info.is_sign, info.total_sign_day
            ),
            Err(err) => {
                println!("FAIL ログインボーナス状態: {err}");
                failures.push(format!("{} ログインボーナス状態: {err}", game.name));
            }
        }
        for (label, endpoint) in [
            ("基本ゲームレコード", "index"),
            ("リアルタイム状態", game.notes),
        ] {
            let url = format!("{}/{endpoint}", game.record_base);
            match request(
                &client,
                &url,
                &[("role_id", &account.game_uid), ("server", &account.region)],
                None,
            )
            .await
            {
                Ok(data) => {
                    println!("OK   {label}: {} bytes取得", data.get().len());
                    if raw {
                        let value: serde_json::Value = parse(&data)?;
                        println!(
                            "  {label}:\n{}",
                            serde_json::to_string_pretty(&value)
                                .map_err(|_| "JSONの整形に失敗しました")?
                        );
                    }
                }
                Err(err) => {
                    println!("FAIL {label}: {err}");
                    failures.push(format!("{} {label}: {err}", game.name));
                }
            }
        }
    }
    if !failures.is_empty() {
        return Err(failures.join("\n"));
    }
    println!(
        "\nOK: 読み取り専用の全API確認が完了しました（ログインボーナスの受け取り操作はしていません）"
    );
    Ok(())
}

fn select_account<'a>(accounts: &'a [Account], biz: &str) -> Option<&'a Account> {
    // Keep the first account on ties, matching the original client.
    accounts
        .iter()
        .filter(|a| a.game_biz == biz)
        .reduce(|a, b| if b.level > a.level { b } else { a })
}

async fn request(
    client: &Client,
    url: &str,
    query: &[(&str, &str)],
    sign_game: Option<&str>,
) -> Result<Box<RawValue>> {
    let mut request = client.get(url).query(query);
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
    let envelope: Envelope =
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
    serde_json::from_str(data.get()).map_err(|_| "レスポンスdataの形式が不正です".into())
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
        return Err("test/.env に HOYOLAB_COOKIE を設定してください".into());
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
        return Err("test/.env のプレースホルダーを実際のCookie値に置き換えてください".into());
    }
    Ok(legacy)
}

fn load_config(path: &str) -> Result<HashMap<String, String>> {
    let entries = dotenvy::from_path_iter(path).map_err(|_| {
        format!("{path} を開けません。test/.env.example をコピーしてCookieを設定してください")
    })?;
    let mut config: HashMap<String, String> = std::env::vars().collect();
    for entry in entries {
        // Never print dotenv parser errors: they may contain the secret line.
        let (key, value) = entry.map_err(|_| format!("{path}: KEY=VALUE 形式が不正です"))?;
        config.entry(key).or_insert(value);
    }
    Ok(config)
}

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
        let accounts: Accounts = serde_json::from_str(r#"{"list":[{"game_biz":"hk4e_global","region":"os_asia","game_uid":"1","level":20},{"game_biz":"hk4e_global","region":"os_usa","game_uid":"2","level":30},{"game_biz":"hk4e_global","region":"os_euro","game_uid":"3","level":30}]}"#).unwrap();
        assert_eq!(
            select_account(&accounts.list, "hk4e_global")
                .unwrap()
                .game_uid,
            "2"
        );
        assert!(select_account(&accounts.list, "hkrpg_global").is_none());
    }

    #[test]
    fn dotenv_precedence_and_redaction() {
        let path = std::env::temp_dir().join(format!(
            "attos-dotenv-{}-{}",
            std::process::id(),
            rand::random::<u64>()
        ));
        std::fs::write(
            &path,
            "PATH=do-not-override\nexport ATTOS_CONFIG_TEST='quoted value'\n",
        )
        .unwrap();
        let config = load_config(path.to_str().unwrap()).unwrap();
        assert_eq!(config["PATH"], std::env::var("PATH").unwrap());
        assert_eq!(config["ATTOS_CONFIG_TEST"], "quoted value");
        std::fs::write(&path, "HOYOLAB_COOKIE=\"secret-unclosed\n").unwrap();
        let error = load_config(path.to_str().unwrap()).unwrap_err();
        assert!(!error.contains("secret-unclosed"));
        std::fs::remove_file(path).unwrap();
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
