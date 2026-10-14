// Fixtures are synthetic old-schema payloads, not captured live API responses.
use super::test_support::{ExpectedRequest, TlsMockServer};
use super::*;
use crate::game::{Game as GameSpec, GameKind, Genshin, Starrail};
use serde_json::{Value, json};

const COOKIE_VALUE: &str = "ltuid_v2=123; ltoken_v2=fake";
const RECORD_UID: &str = "1";
const RECORD_SERVER: &str = "test_server";

#[tokio::test]
async fn implemented_endpoints_use_production_urls_over_local_tls() {
    assert_current_production_urls();

    let server = TlsMockServer::start(expected_requests());
    let client = HoyolabClient {
        http: server.client(COOKIE_VALUE, UA),
    };
    let account = RecordAccount {
        uid: RECORD_UID.into(),
        server: RECORD_SERVER.into(),
    };

    let accounts = client.accounts().await.unwrap();
    assert_eq!(accounts.len(), 2);
    assert_eq!(accounts[0].game_biz, "hk4e_global");
    assert_eq!(accounts[0].region, "os_asia");
    assert_eq!(accounts[0].game_uid, "1");
    assert_eq!(accounts[1].game_biz, "hkrpg_global");

    check_game(&client, Genshin, &account).await;
    check_game(&client, Starrail, &account).await;

    let error = client
        .index(Genshin, &account_for_uid("2"))
        .await
        .unwrap_err();
    assert!(matches!(error, Error::Upstream { code: 10102 }));
    for diagnostic in [error.to_string(), format!("{error:?}")] {
        assert!(!diagnostic.contains("private"));
        assert!(!diagnostic.contains("stats"));
    }

    assert!(matches!(
        client
            .index(Genshin, &account_for_uid("3"))
            .await
            .unwrap_err(),
        Error::MissingData
    ));

    let error = client
        .index(Genshin, &account_for_uid("4"))
        .await
        .unwrap_err();
    assert!(matches!(error, Error::InvalidData(_)));
    for diagnostic in [error.to_string(), format!("{error:?}")] {
        assert!(!diagnostic.contains("false"));
    }

    assert!(matches!(
        client
            .index(Genshin, &account_for_uid("5"))
            .await
            .unwrap_err(),
        Error::Http(307)
    ));
    assert!(matches!(
        client
            .index(Genshin, &account_for_uid("6"))
            .await
            .unwrap_err(),
        Error::ResponseTooLarge(MAX_RESPONSE)
    ));
    assert!(
        client
            .index(Genshin, &account_for_uid("not-a-uid"))
            .await
            .is_err()
    );

    server.assert_finished();
    assert!(HoyolabClient::new("ltuid_v2=123; ltoken_v2=bad\r\nInjected: true").is_err());
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

fn expected_requests() -> Vec<ExpectedRequest> {
    vec![
        record_get(
            "api-os-takumi.mihoyo.com",
            "/binding/api/getUserGameRolesByCookie",
            ok(json!({
                "list": [
                    {"game_biz": "hk4e_global", "region": "os_asia", "game_uid": "1", "nickname": "Traveler", "level": 60},
                    {"game_biz": "hkrpg_global", "region": "prod_official_asia", "game_uid": "1", "nickname": "Trailblazer", "level": 70}
                ]
            })),
        ),
        record_get(
            "sg-public-api.hoyolab.com",
            "/event/game_record/genshin/api/index?role_id=1&server=test_server",
            fixture("genshin-index.json"),
        ),
        record_get(
            "sg-public-api.hoyolab.com",
            "/event/game_record/genshin/api/dailyNote?role_id=1&server=test_server",
            fixture("genshin-note.json"),
        ),
        daily_get(
            "sg-hk4e-api.hoyolab.com",
            "/event/sol/info?act_id=e202102251931481&lang=ja-jp",
            "hk4e",
            ok(json!({"is_sign": false, "total_sign_day": 3, "sign_cnt_missed": 1})),
        ),
        daily_get(
            "sg-hk4e-api.hoyolab.com",
            "/event/sol/home?act_id=e202102251931481&lang=ja-jp",
            "hk4e",
            ok(
                json!({"awards": [{"name": "Reward", "cnt": 20, "icon": "https://example.com/icon"}], "month": 10}),
            ),
        ),
        daily_post(
            "sg-hk4e-api.hoyolab.com",
            "/event/sol/sign?act_id=e202102251931481&lang=ja-jp",
            "hk4e",
            ok(
                json!({"gt_result": {"risk_code": 1, "gt": "gt", "challenge": "challenge", "success": 1}, "future_field": "preserved"}),
            ),
        ),
        record_get(
            "bbs-api-os.hoyolab.com",
            "/game_record/hkrpg/api/index?role_id=1&server=test_server",
            fixture("starrail-index.json"),
        ),
        record_get(
            "bbs-api-os.hoyolab.com",
            "/game_record/hkrpg/api/note?role_id=1&server=test_server",
            fixture("starrail-note.json"),
        ),
        daily_get(
            "sg-public-api.hoyolab.com",
            "/event/luna/hkrpg/os/info?act_id=e202303301540311&lang=ja-jp",
            "hkrpg",
            ok(json!({"is_sign": false, "total_sign_day": 3, "sign_cnt_missed": 1})),
        ),
        daily_get(
            "sg-public-api.hoyolab.com",
            "/event/luna/hkrpg/os/home?act_id=e202303301540311&lang=ja-jp",
            "hkrpg",
            ok(
                json!({"awards": [{"name": "Reward", "cnt": 20, "icon": "https://example.com/icon"}], "month": 10}),
            ),
        ),
        daily_post(
            "sg-public-api.hoyolab.com",
            "/event/luna/hkrpg/os/sign?act_id=e202303301540311&lang=ja-jp",
            "hkrpg",
            ok(
                json!({"risk_code": 1, "gt": "gt", "challenge": "challenge", "success": 1, "future_field": "preserved"}),
            ),
        ),
        record_get(
            "sg-public-api.hoyolab.com",
            "/event/game_record/genshin/api/index?role_id=2&server=test_server",
            json!({"retcode": 10102, "message": "private", "data": {"stats": false}}).to_string(),
        ),
        record_get(
            "sg-public-api.hoyolab.com",
            "/event/game_record/genshin/api/index?role_id=3&server=test_server",
            json!({"retcode": 0, "message": "OK", "data": null}).to_string(),
        ),
        record_get(
            "sg-public-api.hoyolab.com",
            "/event/game_record/genshin/api/index?role_id=4&server=test_server",
            ok(json!({"stats": false})),
        ),
        record_status(
            "sg-public-api.hoyolab.com",
            "/event/game_record/genshin/api/index?role_id=5&server=test_server",
            307,
        ),
        record_large(
            "sg-public-api.hoyolab.com",
            "/event/game_record/genshin/api/index?role_id=6&server=test_server",
        ),
    ]
}

async fn check_game<G: GameSpec + Copy>(client: &HoyolabClient, game: G, account: &RecordAccount) {
    assert!(
        serde_json::to_value(client.index(game, account).await.unwrap()).unwrap()["stats"]
            .is_object()
    );
    let note = serde_json::to_value(client.daily_note(game, account).await.unwrap()).unwrap();
    match G::KIND {
        GameKind::Genshin(_) => assert!(note["resin_recovery_time"].is_string()),
        GameKind::Starrail(_) => assert!(note["stamina_recover_time"].is_number()),
    }
    assert_eq!(note["expeditions"][0]["status"], "Ongoing");
    let info = serde_json::to_value(client.daily_info(game).await.unwrap()).unwrap();
    assert_eq!(info["total_sign_day"], 3);
    let home = serde_json::to_value(client.daily_home(game).await.unwrap()).unwrap();
    assert_eq!(home["awards"][0]["cnt"], 20);
    let sign = serde_json::to_value(client.daily_sign(game).await.unwrap()).unwrap();
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

fn account_for_uid(uid: &str) -> RecordAccount {
    RecordAccount {
        uid: uid.into(),
        server: RECORD_SERVER.into(),
    }
}

fn record_get(host: &str, target: &str, body: String) -> ExpectedRequest {
    record_headers(ExpectedRequest::get(host, target, body))
}

fn record_status(host: &str, target: &str, status: u16) -> ExpectedRequest {
    record_headers(ExpectedRequest::status(host, target, status))
}

fn record_large(host: &str, target: &str) -> ExpectedRequest {
    record_headers(ExpectedRequest::large(host, target, MAX_RESPONSE + 1))
}

fn record_headers(request: ExpectedRequest) -> ExpectedRequest {
    common_headers(request)
        .header("x-rpc-app_version", "1.5.0")
        .header("x-rpc-client_type", "5")
        .header("x-rpc-language", LANG)
        .header("x-rpc-lang", LANG)
        .present_header("ds")
        .absent_header("x-rpc-signgame")
}

fn daily_get(host: &str, target: &str, sign_game: &str, body: String) -> ExpectedRequest {
    daily_headers(ExpectedRequest::get(host, target, body), sign_game)
}

fn daily_post(host: &str, target: &str, sign_game: &str, body: String) -> ExpectedRequest {
    daily_headers(ExpectedRequest::post(host, target, body), sign_game)
}

fn daily_headers(request: ExpectedRequest, sign_game: &str) -> ExpectedRequest {
    common_headers(request)
        .header("referer", "https://act.hoyolab.com/")
        .header("x-rpc-signgame", sign_game)
        .absent_header("ds")
}

fn common_headers(request: ExpectedRequest) -> ExpectedRequest {
    request
        .header(COOKIE.as_str(), COOKIE_VALUE)
        .header(USER_AGENT.as_str(), UA)
        .header("connection", "close")
}

fn fixture(name: &str) -> String {
    ok(match name {
        "genshin-index.json" => serde_json::from_str(include_str!(
            "../../../tests/fixtures/hoyolab/genshin-index.json"
        )),
        "genshin-note.json" => serde_json::from_str(include_str!(
            "../../../tests/fixtures/hoyolab/genshin-note.json"
        )),
        "starrail-index.json" => serde_json::from_str(include_str!(
            "../../../tests/fixtures/hoyolab/starrail-index.json"
        )),
        "starrail-note.json" => serde_json::from_str(include_str!(
            "../../../tests/fixtures/hoyolab/starrail-note.json"
        )),
        _ => unreachable!("unknown fixture"),
    }
    .unwrap())
}

fn ok(data: Value) -> String {
    json!({"retcode": 0, "message": "OK", "data": data}).to_string()
}

fn assert_current_production_urls() {
    assert_eq!(
        ROLES_URL,
        "https://api-os-takumi.mihoyo.com/binding/api/getUserGameRolesByCookie"
    );
    assert_eq!(
        Genshin::RECORD_BASE,
        "https://sg-public-api.hoyolab.com/event/game_record/genshin/api"
    );
    assert_eq!(Genshin::NOTE_PATH, "dailyNote");
    assert_eq!(
        Genshin::DAILY_BASE,
        "https://sg-hk4e-api.hoyolab.com/event/sol"
    );
    assert_eq!(Genshin::ACT_ID, "e202102251931481");
    assert_eq!(Genshin::SIGN_GAME, "hk4e");
    assert_eq!(
        Starrail::RECORD_BASE,
        "https://bbs-api-os.hoyolab.com/game_record/hkrpg/api"
    );
    assert_eq!(Starrail::NOTE_PATH, "note");
    assert_eq!(
        Starrail::DAILY_BASE,
        "https://sg-public-api.hoyolab.com/event/luna/hkrpg/os"
    );
    assert_eq!(Starrail::ACT_ID, "e202303301540311");
    assert_eq!(Starrail::SIGN_GAME, "hkrpg");
}
