use super::test_support::{ExpectedRequest, TlsMockServer};
use super::*;
use reqwest::header::{COOKIE, USER_AGENT};
use serde_json::{Value, json};

const COOKIE_VALUE: &str = "ltuid_v2=111; ltoken_v2=fake";
const ACCOUNT_TARGET: &str = "/auth/api/getUserAccountInfoByLToken";
const COMMUNITY_TARGET: &str = "/community/painter/wapi/user/full";

#[tokio::test]
async fn identity_candidates_use_exact_tls_get_protocol_and_canonicalize_ids() {
    assert_current_identity_urls();

    let server = TlsMockServer::start(vec![
        account_get(ok(json!({"account_id": "000123"}))),
        community_get(ok(json!({"user_info": {"uid": "456"}}))),
        community_get(ok(json!({"user_info": {"uid": 789}}))),
    ]);
    let client = HoyolabClient {
        http: server.client(COOKIE_VALUE, UA),
    };

    let account = client.account_identity().await.unwrap();
    assert_eq!(account, "123");
    let community = client.community_identity().await.unwrap();
    assert_eq!(community, "456");
    // Different authenticated candidate responses are not token-to-UID binding proof.
    assert_ne!(account, community);
    assert_eq!(client.community_identity().await.unwrap(), "789");

    server.assert_finished();
}

#[tokio::test]
async fn identity_candidates_reject_nullable_malformed_invalid_and_upstream_envelopes() {
    assert_current_identity_urls();

    let server = TlsMockServer::start(vec![
        account_get(ok(json!({"account_id": null}))),
        account_get(ok(json!([]))),
        account_get(ok(json!({"account_id": "0"}))),
        account_get(ok(json!({
            "account_id": "18446744073709551616"
        }))),
        account_get(
            json!({"retcode": 10001, "message": "secret", "data": {"account_id": "999"}})
                .to_string(),
        ),
        account_get(json!({"message": "OK", "data": {"account_id": "1"}}).to_string()),
        community_get(ok(json!({"user_info": {"uid": 0}}))),
        community_get(ok(json!({"user_info": {"uid": false}}))),
        community_get(ok(json!({}))),
    ]);
    let client = HoyolabClient {
        http: server.client(COOKIE_VALUE, UA),
    };

    assert!(matches!(
        client.account_identity().await,
        Err(Error::MissingData)
    ));
    assert!(matches!(
        client.account_identity().await,
        Err(Error::InvalidData(_))
    ));
    assert!(matches!(
        client.account_identity().await,
        Err(Error::InvalidData(_))
    ));
    assert!(matches!(
        client.account_identity().await,
        Err(Error::InvalidData(_))
    ));

    let error = client.account_identity().await.unwrap_err();
    assert!(matches!(error, Error::Upstream { code: 10001 }));
    for diagnostic in [error.to_string(), format!("{error:?}")] {
        assert!(!diagnostic.contains("secret"));
        assert!(!diagnostic.contains("999"));
    }

    assert!(matches!(
        client.account_identity().await,
        Err(Error::InvalidEnvelope)
    ));
    assert!(matches!(
        client.community_identity().await,
        Err(Error::InvalidData(_))
    ));
    assert!(matches!(
        client.community_identity().await,
        Err(Error::InvalidData(_))
    ));
    assert!(matches!(
        client.community_identity().await,
        Err(Error::MissingData)
    ));

    server.assert_finished();
}

fn account_get(body: String) -> ExpectedRequest {
    common_headers(ExpectedRequest::get(
        "api-account-os.hoyolab.com",
        ACCOUNT_TARGET,
        body,
    ))
    .absent_header("ds")
    .absent_header("x-rpc-app_version")
    .absent_header("x-rpc-client_type")
    .absent_header("x-rpc-language")
    .absent_header("x-rpc-lang")
    .absent_header("x-rpc-signgame")
}

fn community_get(body: String) -> ExpectedRequest {
    common_headers(ExpectedRequest::get(
        "bbs-api-os.hoyolab.com",
        COMMUNITY_TARGET,
        body,
    ))
    .header("x-rpc-app_version", "1.5.0")
    .header("x-rpc-client_type", "5")
    .header("x-rpc-language", LANG)
    .header("x-rpc-lang", LANG)
    .present_header("ds")
    .absent_header("x-rpc-signgame")
}

fn common_headers(request: ExpectedRequest) -> ExpectedRequest {
    request
        .header(COOKIE.as_str(), COOKIE_VALUE)
        .header(USER_AGENT.as_str(), UA)
        .header("connection", "close")
}

fn ok(data: Value) -> String {
    json!({"retcode": 0, "message": "OK", "data": data}).to_string()
}

fn assert_current_identity_urls() {
    assert_eq!(
        super::identity::ACCOUNT_IDENTITY_URL,
        "https://api-account-os.hoyolab.com/auth/api/getUserAccountInfoByLToken"
    );
    assert_eq!(
        super::identity::COMMUNITY_IDENTITY_URL,
        "https://bbs-api-os.hoyolab.com/community/painter/wapi/user/full"
    );
}
