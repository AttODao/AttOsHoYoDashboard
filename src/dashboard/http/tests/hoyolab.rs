use super::*;
use crate::dashboard::test_support::TestDatabase;

#[tokio::test]
async fn cookie_setup_routes_store_pending_without_returning_credentials_or_claimed_uid() {
    let fixture = TestDatabase::new().await;
    let (accounts, actor) = fixture.ready_admin().await;
    let app = app(accounts);
    let session = cookie(&app, actor.user.id).await;
    let initial = call(
        &app,
        request("GET", "/api/v1/me/hoyolab", Some(&session), None),
    )
    .await;
    assert_eq!(initial.status(), StatusCode::OK);
    assert_eq!(
        body(initial).await,
        json!({"status":"not_configured","setup_complete":false,"updated_at":null,"id":null,"hoyolab_uid":null,"verification_available":true})
    );
    let response = call(
        &app,
        request(
            "PUT",
            "/api/v1/me/hoyolab/cookie",
            Some(&session),
            Some(json!({
                "cookie":"ltuid_v2=123; ltoken_v2=synthetic-private-token"
            })),
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::ACCEPTED);
    assert_eq!(
        body(response).await,
        json!({"status":"pending_verification","setup_complete":false})
    );
    let response = call(
        &app,
        request("GET", "/api/v1/me/hoyolab", Some(&session), None),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let data = body(response).await;
    assert_eq!(data["status"], "pending_verification");
    assert_eq!(data["setup_complete"], false);
    assert!(data["updated_at"].is_string());
    assert_eq!(data.as_object().unwrap().len(), 6);
    assert!(data["id"].is_string());
    assert!(data["hoyolab_uid"].is_null());
    assert_eq!(data["verification_available"], true);
    assert!(!data.to_string().contains("synthetic-private-token"));
}

#[tokio::test]
async fn cookie_routes_use_existing_auth_origin_dto_and_configuration_boundaries() {
    let fixture = TestDatabase::new().await;
    let (accounts, actor) = fixture.ready_admin().await;
    let mut app = app(accounts);
    let session = cookie(&app, actor.user.id).await;
    error(
        call(
            &app,
            request(
                "PUT",
                "/api/v1/me/hoyolab/cookie",
                None,
                Some(json!({"cookie":"secret"})),
            ),
        )
        .await,
        StatusCode::UNAUTHORIZED,
        "unauthorized",
    )
    .await;
    let mut wrong_origin = request(
        "PUT",
        "/api/v1/me/hoyolab/cookie",
        Some(&session),
        Some(json!({"cookie":"secret"})),
    );
    wrong_origin.headers_mut().insert(
        header::ORIGIN,
        "https://wrong.example.test".parse().unwrap(),
    );
    error(
        call(&app, wrong_origin).await,
        StatusCode::FORBIDDEN,
        "forbidden",
    )
    .await;
    let unknown = call(
        &app,
        request(
            "PUT",
            "/api/v1/me/hoyolab/cookie",
            Some(&session),
            Some(json!({"cookie":"secret","hoyolab_uid":"123"})),
        ),
    )
    .await;
    assert_eq!(unknown.status(), StatusCode::UNPROCESSABLE_ENTITY);
    error(
        call(
            &app,
            request(
                "PUT",
                "/api/v1/me/hoyolab/cookie",
                Some(&session),
                Some(json!({"cookie":"secret"})),
            ),
        )
        .await,
        StatusCode::BAD_REQUEST,
        "invalid_input",
    )
    .await;
    let state = Arc::get_mut(&mut app).unwrap();
    state.cookies = None;
    state.config.cookie_key = None;
    let me = body(call(&app, request("GET", "/api/v1/me", Some(&session), None)).await).await;
    assert_eq!(me["onboarding"]["cookie_setup_complete"], false);
    assert_eq!(me["onboarding"]["cookie_status"], "storage_unavailable");
    assert_eq!(me["onboarding"]["verification_available"], false);
    assert_eq!(me["capabilities"]["reward_posting_available"], false);
    error(
        call(
            &app,
            request(
                "PUT",
                "/api/v1/me/hoyolab/cookie",
                Some(&session),
                Some(json!({"cookie":"secret"})),
            ),
        )
        .await,
        StatusCode::SERVICE_UNAVAILABLE,
        "unavailable",
    )
    .await;
}
