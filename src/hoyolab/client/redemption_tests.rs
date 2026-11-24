// Synthetic answers shaped like genshin.py expects them; none was captured from HoYoLAB.
use super::super::test_support::{ExpectedRequest, TlsMockServer};
use super::*;
use crate::game::{Genshin, Starrail};

const COOKIE: &str =
    "cookie_token_v2=synthetic-cookie-token; account_id_v2=42; account_mid_v2=synthetic_mid";
const UA: &str = "ahd-test";

fn genshin(code: &str) -> RedemptionRequest<'_> {
    RedemptionRequest {
        game: GameKind::Genshin(Genshin),
        uid: "800000001",
        region: "os_asia",
        code,
    }
}

fn starrail(code: &str) -> RedemptionRequest<'_> {
    RedemptionRequest {
        game: GameKind::Starrail(Starrail),
        uid: "800000002",
        region: "prod_official_asia",
        code,
    }
}

fn envelope(retcode: i64) -> String {
    format!(r#"{{"retcode":{retcode},"message":"synthetic-message-with-secret","data":null}}"#)
}

#[test]
fn every_documented_retcode_has_its_class() {
    use RedemptionReply::*;
    for (retcode, class) in [
        (0, Redeemed),
        (-2017, AlreadyUsed),
        (-2018, AlreadyUsed),
        (-2001, Expired),
        (-2006, Expired),
        (-1065, Invalid),
        (-2003, Invalid),
        (-2004, Invalid),
        (-2008, NotForAccount),
        (-2014, NotForAccount),
        (-2011, RankTooLow),
        (-2021, RankTooLow),
        (-1071, CookieInvalid),
        (-1073, CookieInvalid),
        (-100, CookieInvalid),
        (10001, CookieInvalid),
        (10035, CaptchaRequired),
        (5003, CaptchaRequired),
        (10041, CaptchaRequired),
        (1034, CaptchaRequired),
        (-2016, Cooldown),
        (-110, Cooldown),
        (1028, Cooldown),
        (-1, Other(-1)),
        (-5003, Other(-5003)),
    ] {
        assert_eq!(RedemptionReply::classify(retcode), class, "{retcode}");
        assert_eq!(decode(envelope(retcode).as_bytes()), Ok(class), "{retcode}");
    }
}

#[test]
fn answers_are_reduced_to_their_class_and_challenges_win_over_success() {
    // Success needs no data.
    assert_eq!(decode(br#"{"retcode":0}"#), Ok(RedemptionReply::Redeemed));
    assert_eq!(
        decode(br#"{"retcode":0,"message":"OK","data":{"msg":"ok","special_shipping":0}}"#),
        Ok(RedemptionReply::Redeemed)
    );
    // A risk challenge in data (genshin.py `check_for_geetest`), directly or in gt_result.
    for body in [
        r#"{"retcode":0,"data":{"risk_code":5001,"gt":"g","challenge":"c","success":1}}"#,
        r#"{"retcode":0,"data":{"gt_result":{"risk_code":375,"gt":"g","challenge":"c"}}}"#,
        r#"{"retcode":-2017,"data":{"gt_result":{"gt":"g","challenge":"c","success":1}}}"#,
    ] {
        assert_eq!(
            decode(body.as_bytes()),
            Ok(RedemptionReply::CaptchaRequired),
            "{body}"
        );
    }
    // Not a challenge: risk_code 0, success 0, or an empty gt/challenge.
    for body in [
        r#"{"retcode":0,"data":{"gt_result":{"risk_code":0,"gt":"g","challenge":"c","success":1}}}"#,
        r#"{"retcode":0,"data":{"gt_result":{"risk_code":1,"gt":"g","challenge":"c","success":0}}}"#,
        r#"{"retcode":0,"data":{"gt_result":{"risk_code":1,"gt":"","challenge":"c"}}}"#,
        r#"{"retcode":0,"data":{"gt_result":null}}"#,
        r#"{"retcode":0,"data":"text"}"#,
    ] {
        assert_eq!(
            decode(body.as_bytes()),
            Ok(RedemptionReply::Redeemed),
            "{body}"
        );
    }
    // Anything that is not the envelope leaves the outcome unknown.
    for body in [
        b"<html>blocked</html>".as_slice(),
        br#"{"data":{}}"#,
        br#"{"retcode":"0"}"#,
        b"",
    ] {
        assert_eq!(decode(body), Err(Error::InvalidEnvelope));
    }
}

#[tokio::test]
async fn requests_match_genshin_py_over_local_tls() {
    let server = TlsMockServer::start(vec![
        ExpectedRequest::get(
            "public-operation-hk4e.hoyoverse.com",
            "/common/apicdkey/api/webExchangeCdkey?uid=800000001&region=os_asia&lang=ja&cdkey=GENSHINGIFT&game_biz=hk4e_global",
            envelope(0),
        )
        .header("cookie", COOKIE)
        .header("origin", "https://genshin.hoyoverse.com")
        .header("referer", "https://genshin.hoyoverse.com/")
        .header("user-agent", UA)
        .absent_header("ds")
        .absent_header("x-rpc-signgame"),
        ExpectedRequest::post(
            "public-operation-hkrpg.hoyoverse.com",
            "/common/apicdkey/api/webExchangeCdkeyRisk",
            envelope(-2017),
        )
        .header("cookie", COOKIE)
        .header("origin", "https://hsr.hoyoverse.com")
        .header("referer", "https://hsr.hoyoverse.com/")
        .header("content-type", "application/json")
        .absent_header("ds")
        .request_body(
            r#"{"cdkey":"STARRAILGIFT","device_uuid":"00000000-0000-4000-8000-000000000001","game_biz":"hkrpg_global","lang":"ja","platform":"4","region":"prod_official_asia","t":1700000000,"uid":800000002}"#,
        ),
        ExpectedRequest::status(
            "public-operation-hk4e.hoyoverse.com",
            "/common/apicdkey/api/webExchangeCdkey?uid=800000001&region=os_asia&lang=ja&cdkey=SERVERERROR&game_biz=hk4e_global",
            502,
        ),
    ]);
    let client = RedemptionClient::with_http(server.client(UA), COOKIE).unwrap();
    assert_eq!(
        client
            .redeem_with(&genshin("GENSHINGIFT"), "unused-for-get", 1)
            .await,
        Ok(RedemptionReply::Redeemed)
    );
    assert_eq!(
        client
            .redeem_with(
                &starrail("STARRAILGIFT"),
                "00000000-0000-4000-8000-000000000001",
                1_700_000_000
            )
            .await,
        Ok(RedemptionReply::AlreadyUsed)
    );
    // An HTTP error is an error (unknown outcome), never a class.
    assert_eq!(
        client.redeem_with(&genshin("SERVERERROR"), "x", 1).await,
        Err(Error::Http(502))
    );
    server.assert_finished();
}

#[tokio::test]
async fn invalid_input_is_refused_before_any_request() {
    // No mock server: any request would fail with a network error instead.
    let client = RedemptionClient::with_http(Client::new(), COOKIE).unwrap();
    for request in [
        genshin("lowercase"),
        genshin("AB"),
        genshin("WITH SPACE"),
        RedemptionRequest {
            uid: "0",
            ..genshin("GOODCODE")
        },
        RedemptionRequest {
            uid: "12a",
            ..genshin("GOODCODE")
        },
        RedemptionRequest {
            region: "os asia",
            ..genshin("GOODCODE")
        },
        RedemptionRequest {
            region: "",
            ..starrail("GOODCODE")
        },
    ] {
        assert!(
            matches!(client.redeem(&request).await, Err(Error::InvalidInput(_))),
            "{request:?}"
        );
    }
    for cookie in [
        "",
        "ltoken_v2=x; ltuid_v2=42",
        "cookie_token_v2=x",
        "account_id_v2=42",
        "cookie_token_v2=; account_id_v2=42",
        "cookie_token_v2=x; account_id_v2=42\r\nInjected: 1",
    ] {
        assert!(
            RedemptionClient::with_http(Client::new(), cookie).is_err(),
            "{cookie:?}"
        );
    }
}

#[test]
fn the_cookie_is_sensitive_and_never_in_diagnostics() {
    let client = RedemptionClient::with_http(Client::new(), COOKIE).unwrap();
    let request = client
        .prepare(&genshin("GENSHINGIFT"), "x", 1)
        .unwrap()
        .build()
        .unwrap();
    assert!(request.headers()["cookie"].is_sensitive());
    assert!(!format!("{:?}", request.headers()).contains("synthetic-cookie-token"));
    let error = decode(b"synthetic-cookie-token").unwrap_err();
    assert!(!format!("{error:?} {error}").contains("synthetic-cookie-token"));
}
