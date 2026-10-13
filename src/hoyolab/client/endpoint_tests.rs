// Fixtures are synthetic old-schema payloads, not captured live API responses.
use super::*;
use crate::game::{Game as GameSpec, GameKind, Genshin, Starrail};
use axum::{
    Json, Router,
    extract::{Path, Query},
    http::{HeaderMap, Method},
    routing::get,
};
use serde_json::{Value, json};

// A local transport with the real games' associated response types and parameters.
struct Local<G>(G);
impl<G: GameSpec> GameSpec for Local<G> {
    const KIND: GameKind = G::KIND;
    const RECORD_BASE: &'static str = match G::KIND {
        GameKind::Genshin(_) => "http://hoyolab.test/genshin/record",
        GameKind::Starrail(_) => "http://hoyolab.test/starrail/record",
    };
    const DAILY_BASE: &'static str = match G::KIND {
        GameKind::Genshin(_) => "http://hoyolab.test/genshin/daily",
        GameKind::Starrail(_) => "http://hoyolab.test/starrail/daily",
    };
    const NOTE_PATH: &'static str = G::NOTE_PATH;
    const ACT_ID: &'static str = G::ACT_ID;
    const SIGN_GAME: &'static str = G::SIGN_GAME;
    type Index = G::Index;
    type DailyNote = G::DailyNote;
    type DailyInfo = G::DailyInfo;
    type DailySign = G::DailySign;
    type DailyHome = G::DailyHome;
}

async fn mock_endpoint(
    Path((game, group, endpoint)): Path<(String, String, String)>,
    Query(query): Query<HashMap<String, String>>,
    headers: HeaderMap,
    method: Method,
    body: String,
) -> Json<Value> {
    assert_eq!(headers[COOKIE], "ltuid_v2=123; ltoken_v2=fake");
    assert!(headers.contains_key(USER_AGENT));
    assert_eq!(
        method,
        if endpoint == "sign" {
            Method::POST
        } else {
            Method::GET
        }
    );
    assert!(body.is_empty());
    let genshin = game == "genshin";
    if group == "record" {
        assert_eq!(query["server"], "test_server");
        assert_eq!(headers["x-rpc-app_version"], "1.5.0");
        assert_eq!(headers["x-rpc-client_type"], "5");
        assert_eq!(headers["x-rpc-language"], LANG);
        assert!(headers.contains_key("ds"));
        assert!(!headers.contains_key("x-rpc-signgame"));
        if query["role_id"] == "2" {
            // A malformed success payload must not mask an upstream API error.
            return Json(json!({"retcode": 10102, "message": "private", "data": {"stats": false}}));
        }
        if query["role_id"] == "3" {
            return Json(json!({"retcode": 0, "message": "OK", "data": null}));
        }
        if query["role_id"] == "4" {
            return Json(json!({"retcode": 0, "message": "OK", "data": {"stats": false}}));
        }
        assert_eq!(query["role_id"], "1");
    } else {
        assert_eq!(group, "daily");
        assert_eq!(
            query["act_id"],
            if genshin {
                "e202102251931481"
            } else {
                "e202303301540311"
            }
        );
        assert_eq!(query["lang"], LANG);
        assert_eq!(
            headers["x-rpc-signgame"],
            if genshin { "hk4e" } else { "hkrpg" }
        );
        assert_eq!(headers["referer"], "https://act.hoyolab.com/");
        assert!(!headers.contains_key("ds"));
    }
    let data = match (game.as_str(), endpoint.as_str()) {
        ("genshin", "index") => serde_json::from_str(include_str!(
            "../../../tests/fixtures/hoyolab/genshin-index.json"
        ))
        .unwrap(),
        ("starrail", "index") => serde_json::from_str(include_str!(
            "../../../tests/fixtures/hoyolab/starrail-index.json"
        ))
        .unwrap(),
        ("genshin", "dailyNote") => serde_json::from_str(include_str!(
            "../../../tests/fixtures/hoyolab/genshin-note.json"
        ))
        .unwrap(),
        ("starrail", "note") => serde_json::from_str(include_str!(
            "../../../tests/fixtures/hoyolab/starrail-note.json"
        ))
        .unwrap(),
        (_, "info") => json!({"is_sign": false, "total_sign_day": 3, "sign_cnt_missed": 1}),
        (_, "home") => {
            json!({"awards": [{"name": "Reward", "cnt": 20, "icon": "https://example.com/icon"}], "month": 10})
        }
        ("genshin", "sign") => {
            json!({"gt_result": {"risk_code": 1, "gt": "gt", "challenge": "challenge", "success": 1}, "future_field": "preserved"})
        }
        ("starrail", "sign") => {
            json!({"risk_code": 1, "gt": "gt", "challenge": "challenge", "success": 1, "future_field": "preserved"})
        }
        _ => panic!("unexpected endpoint: {game}/{endpoint}"),
    };
    Json(json!({"retcode": 0, "message": "OK", "data": data}))
}

#[tokio::test]
async fn typed_endpoints() {
    let app = Router::new().route(
        "/{game}/{group}/{endpoint}",
        get(mock_endpoint).post(mock_endpoint),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let mut headers = HeaderMap::new();
    headers.insert(
        COOKIE,
        HeaderValue::from_static("ltuid_v2=123; ltoken_v2=fake"),
    );
    headers.insert(USER_AGENT, HeaderValue::from_static(UA));
    let client = HoyolabClient {
        http: Client::builder()
            .default_headers(headers)
            .resolve("hoyolab.test", address)
            .no_proxy()
            .timeout(Duration::from_secs(5))
            .build()
            .unwrap(),
    };
    let account = RecordAccount {
        uid: "1".into(),
        server: "test_server".into(),
    };
    check_game(&client, Local(Genshin), &account).await;
    check_game(&client, Local(Starrail), &account).await;
    for uid in ["2", "3", "4", "not-a-uid"] {
        let account = RecordAccount {
            uid: uid.into(),
            server: "test_server".into(),
        };
        let error = client.index(Local(Genshin), &account).await.unwrap_err();
        if uid == "2" {
            assert!(error.to_string().contains("10102") && error.to_string().contains("公開"));
        }
    }
    server.abort();
    assert!(HoyolabClient::new("ltuid_v2=123; ltoken_v2=bad\r\nInjected: true").is_err());
    assert_eq!(
        Genshin::RECORD_BASE,
        "https://sg-public-api.hoyolab.com/event/game_record/genshin/api"
    );
    assert_eq!(
        Genshin::DAILY_BASE,
        "https://sg-hk4e-api.hoyolab.com/event/sol"
    );
    assert_eq!(
        Starrail::RECORD_BASE,
        "https://bbs-api-os.hoyolab.com/game_record/hkrpg/api"
    );
    assert_eq!(
        Starrail::DAILY_BASE,
        "https://sg-public-api.hoyolab.com/event/luna/hkrpg/os"
    );
    assert_eq!(
        serde_json::from_str::<GameKind>("\"genshin\"").unwrap(),
        Genshin::KIND
    );
    assert_eq!(
        serde_json::to_string(&Starrail::KIND).unwrap(),
        "\"starrail\""
    );
    assert!(serde_json::from_str::<GameKind>("\"unknown\"").is_err());
}

async fn check_game<G: GameSpec + Copy>(
    client: &HoyolabClient,
    game: Local<G>,
    account: &RecordAccount,
) {
    let game_value = game.0;
    assert!(serde_json::to_value(client.index(Local(game_value), account).await.unwrap()).unwrap()["stats"].is_object());
    let note =
        serde_json::to_value(client.daily_note(Local(game_value), account).await.unwrap()).unwrap();
    match G::KIND {
        GameKind::Genshin(_) => assert!(note["resin_recovery_time"].is_string()),
        GameKind::Starrail(_) => assert!(note["stamina_recover_time"].is_number()),
    }
    assert_eq!(note["expeditions"][0]["status"], "Ongoing");
    let info = serde_json::to_value(client.daily_info(Local(game_value)).await.unwrap()).unwrap();
    assert_eq!(info["total_sign_day"], 3);
    let home = serde_json::to_value(client.daily_home(Local(game_value)).await.unwrap()).unwrap();
    assert_eq!(home["awards"][0]["cnt"], 20);
    let sign = serde_json::to_value(client.daily_sign(Local(game_value)).await.unwrap()).unwrap();
    match G::KIND {
        GameKind::Genshin(_) => assert_eq!(sign["gt_result"]["risk_code"], 1),
        GameKind::Starrail(_) => assert_eq!(sign["risk_code"], 1),
    }
    assert_eq!(sign["future_field"], "preserved");
    let sign: crate::models::daily::DailySign = serde_json::from_value(sign).unwrap();
    assert_eq!(sign.requires_verification(), Some(true));
    for (data, expected) in [
        (json!({}), None),
        (json!({"risk_code": 0}), Some(false)),
        (
            json!({"risk_code": 1, "gt": "gt", "challenge": "challenge", "success": 1}),
            Some(true),
        ),
        (json!({"gt_result": {"risk_code": 0}}), Some(false)),
        (
            json!({"gt_result": {"risk_code": 0, "challenge": "challenge"}}),
            Some(true),
        ),
    ] {
        let sign: crate::models::daily::DailySign = serde_json::from_value(data).unwrap();
        assert_eq!(sign.requires_verification(), expected);
    }
}
