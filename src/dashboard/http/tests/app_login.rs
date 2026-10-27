//! App sign-in handoff uses the production router and synthetic browser sessions.
use super::*;
use crate::dashboard::{
    auth::api_tokens::{ApiTokens, Scope},
    test_support::TestDatabase,
};

const CHALLENGE: &str = "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM";
const VERIFIER: &str = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
const APPROVE: &str = "/api/v1/me/app-login";
const EXCHANGE: &str = "/api/v1/auth/app-login/exchange";

#[tokio::test]
async fn browser_approval_and_rfc_exchange_issue_an_independent_session_and_reject_replay() {
    let db = TestDatabase::new().await;
    let (accounts, admin) = db.ready_admin().await;
    let app = app(accounts);
    let browser = cookie(&app, admin.user.id).await;
    error(
        call(
            &app,
            request(
                "POST",
                APPROVE,
                None,
                Some(json!({"code_challenge": CHALLENGE})),
            ),
        )
        .await,
        StatusCode::UNAUTHORIZED,
        "unauthorized",
    )
    .await;
    error(
        call(
            &app,
            request(
                "POST",
                APPROVE,
                Some(&browser),
                Some(json!({"code_challenge": "invalid"})),
            ),
        )
        .await,
        StatusCode::BAD_REQUEST,
        "invalid_input",
    )
    .await;
    let response = call(
        &app,
        request(
            "POST",
            APPROVE,
            Some(&browser),
            Some(json!({"code_challenge": CHALLENGE})),
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let value = body(response).await;
    let code = value["code"].as_str().unwrap();
    assert_eq!(code.len(), 64);
    assert!(
        code.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    );
    let input = json!({"code": code, "code_verifier": VERIFIER});
    let response = call(&app, request("POST", EXCHANGE, None, Some(input.clone()))).await;
    assert_eq!(response.status(), StatusCode::OK);
    let app_cookie = response_cookie(&response);
    assert_ne!(app_cookie, browser);
    let set_cookie = response.headers()[header::SET_COOKIE].to_str().unwrap();
    for attribute in ["Path=/", "HttpOnly", "SameSite=Strict", "Secure"] {
        assert!(set_cookie.contains(attribute));
    }
    assert_eq!(body(response).await["username"], "admin");
    assert_eq!(
        call(&app, request("GET", "/api/v1/me", Some(&app_cookie), None))
            .await
            .status(),
        StatusCode::OK
    );
    error(
        call(&app, request("POST", EXCHANGE, None, Some(input))).await,
        StatusCode::UNAUTHORIZED,
        "unauthorized",
    )
    .await;
}

#[tokio::test]
async fn exchange_unknown_fields_and_api_token_approval_are_rejected() {
    let db = TestDatabase::new().await;
    let (accounts, admin) = db.ready_admin().await;
    let token = ApiTokens::issue(&accounts.db, &admin, "synthetic app", Scope::Run)
        .await
        .unwrap();
    let app = app(accounts);
    let response = call(
        &app,
        request(
            "POST",
            EXCHANGE,
            None,
            Some(json!({"code": "a".repeat(64), "code_verifier": VERIFIER, "unknown": true})),
        ),
    )
    .await;
    assert!(response.status().is_client_error());
    let req = Request::builder()
        .method("POST")
        .uri(APPROVE)
        .header(header::AUTHORIZATION, format!("Bearer {}", token.token))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(json!({"code_challenge": CHALLENGE}).to_string()))
        .unwrap();
    error(call(&app, req).await, StatusCode::FORBIDDEN, "forbidden").await;
}
