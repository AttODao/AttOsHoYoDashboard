//! Bearer API tokens reach only their permitted routes, without Origin, and never with a cookie.
use super::*;
use crate::dashboard::{db::entities::api_token, test_support::TestDatabase};
use axum::http::HeaderValue;
use sea_orm::EntityTrait;

fn bearer(method: &str, path: &str, token: &str, body: Option<Value>) -> Request<Body> {
    let mut builder = Request::builder()
        .method(method)
        .uri(path)
        .header(header::AUTHORIZATION, format!("Bearer {token}"));
    let body = match body {
        Some(body) => {
            builder = builder.header(header::CONTENT_TYPE, "application/json");
            Body::from(body.to_string())
        }
        None => Body::empty(),
    };
    builder.body(body).unwrap()
}

async fn issue(app: &App, session: &str, scope: &str) -> (Uuid, String) {
    let response = call(
        app,
        request(
            "POST",
            "/api/v1/me/api-tokens",
            Some(session),
            Some(json!({"name": format!("{scope} tool"), "scope": scope})),
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let issued = body(response).await;
    assert_eq!(issued["scope"], scope);
    assert_eq!(issued["last_used_at"], Value::Null);
    (
        issued["id"].as_str().unwrap().parse().unwrap(),
        issued["token"].as_str().unwrap().to_owned(),
    )
}

#[tokio::test]
async fn sessions_issue_list_and_revoke_tokens_and_the_list_never_returns_a_secret() {
    let db = TestDatabase::new().await;
    let (accounts, admin) = db.ready_admin().await;
    let app = app(accounts);
    let session = cookie(&app, admin.user.id).await;
    let (id, token) = issue(&app, &session, "read").await;
    assert!(token.starts_with("ahd_"));
    let response = call(
        &app,
        request("GET", "/api/v1/me/api-tokens", Some(&session), None),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let listed = body(response).await;
    assert_eq!(listed.as_array().unwrap().len(), 1);
    assert_eq!(listed[0]["id"], id.to_string());
    assert_eq!(listed[0]["name"], "read tool");
    assert!(listed[0].get("token").is_none());
    assert!(
        !listed
            .to_string()
            .contains(token.trim_start_matches("ahd_"))
    );

    error(
        call(
            &app,
            request(
                "POST",
                "/api/v1/me/api-tokens",
                Some(&session),
                Some(json!({"name": "bad", "scope": "admin"})),
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
            "DELETE",
            &format!("/api/v1/me/api-tokens/{id}"),
            Some(&session),
            None,
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    error(
        call(&app, bearer("GET", "/api/v1/me", &token, None)).await,
        StatusCode::UNAUTHORIZED,
        "unauthorized",
    )
    .await;
}

#[tokio::test]
async fn bearer_reads_need_no_origin_and_record_last_use() {
    let db = TestDatabase::new().await;
    let (accounts, admin) = db.ready_admin().await;
    let app = app(accounts);
    let session = cookie(&app, admin.user.id).await;
    let (id, token) = issue(&app, &session, "read").await;
    let response = call(&app, bearer("GET", "/api/v1/me", &token, None)).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(body(response).await["username"], "admin");
    let row = api_token::Entity::find_by_id(id)
        .one(&app.accounts.db)
        .await
        .unwrap()
        .unwrap();
    assert!(row.last_used_at.is_some());
    // A permitted read reaches its handler (here: no verified HoYoLAB link yet).
    error(
        call(&app, bearer("GET", "/api/v1/reward-settings", &token, None)).await,
        StatusCode::FORBIDDEN,
        "initial_setup_required",
    )
    .await;
}

#[tokio::test]
async fn the_manual_run_needs_the_run_scope_and_no_origin() {
    let db = TestDatabase::new().await;
    let (accounts, admin) = db.ready_admin().await;
    let app = app(accounts);
    let session = cookie(&app, admin.user.id).await;
    let (_, read) = issue(&app, &session, "read").await;
    let (_, run) = issue(&app, &session, "run").await;
    let path = format!("/api/v1/reward-settings/{}/run", Uuid::new_v4());
    error(
        call(&app, bearer("POST", &path, &read, Some(json!({})))).await,
        StatusCode::FORBIDDEN,
        "forbidden",
    )
    .await;
    // The run token passes the boundary and the scope check and reaches the handler.
    let response = call(&app, bearer("POST", &path, &run, Some(json!({})))).await;
    assert_ne!(response.status(), StatusCode::UNAUTHORIZED);
    assert_ne!(body(response).await["error"], "forbidden");
}

#[tokio::test]
async fn tokens_cannot_reach_session_only_routes() {
    let db = TestDatabase::new().await;
    let (accounts, admin) = db.ready_admin().await;
    let app = app(accounts);
    let session = cookie(&app, admin.user.id).await;
    let (id, run) = issue(&app, &session, "run").await;
    for (method, path, body) in [
        ("GET", "/api/v1/me/api-tokens".to_owned(), None),
        (
            "POST",
            "/api/v1/me/api-tokens".to_owned(),
            Some(json!({"name": "nested", "scope": "run"})),
        ),
        ("DELETE", format!("/api/v1/me/api-tokens/{id}"), None),
        (
            "PATCH",
            format!("/api/v1/reward-settings/{}", Uuid::new_v4()),
            Some(json!({"enabled": true, "local_time": "06:00"})),
        ),
        ("GET", "/api/v1/admin/users".to_owned(), None),
        ("GET", "/api/v1/me/sessions".to_owned(), None),
        ("POST", "/api/v1/auth/logout".to_owned(), None),
    ] {
        error(
            call(&app, bearer(method, &path, &run, body)).await,
            StatusCode::FORBIDDEN,
            "forbidden",
        )
        .await;
    }
    assert_eq!(
        api_token::Entity::find()
            .all(&app.accounts.db)
            .await
            .unwrap()
            .len(),
        1
    );
}

#[tokio::test]
async fn unknown_malformed_and_ambiguous_credentials_are_rejected() {
    let db = TestDatabase::new().await;
    let (accounts, admin) = db.ready_admin().await;
    let app = app(accounts);
    let session = cookie(&app, admin.user.id).await;
    let (_, token) = issue(&app, &session, "run").await;
    let unknown = format!("ahd_{}", "a".repeat(64));
    for value in [
        format!("Bearer {unknown}"),
        "Bearer x".to_owned(),
        "Bearer ".to_owned(),
        format!("bearer {token}"),
        format!("Basic {token}"),
    ] {
        let req = Request::builder()
            .uri("/api/v1/me")
            .header(header::AUTHORIZATION, value)
            .body(Body::empty())
            .unwrap();
        error(
            call(&app, req).await,
            StatusCode::UNAUTHORIZED,
            "unauthorized",
        )
        .await;
    }
    let req = Request::builder()
        .uri("/api/v1/me")
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    error(
        call(&app, req).await,
        StatusCode::UNAUTHORIZED,
        "unauthorized",
    )
    .await;
    // A session cookie beside the token is ambiguous, even on a safe read.
    let mut req = bearer("GET", "/api/v1/me", &token, None);
    req.headers_mut()
        .insert(header::COOKIE, HeaderValue::from_str(&session).unwrap());
    error(
        call(&app, req).await,
        StatusCode::UNAUTHORIZED,
        "unauthorized",
    )
    .await;
    // With any cookie present the Origin boundary applies to unsafe methods again.
    let mut req = bearer(
        "POST",
        &format!("/api/v1/reward-settings/{}/run", Uuid::new_v4()),
        &token,
        Some(json!({})),
    );
    req.headers_mut()
        .insert(header::COOKIE, HeaderValue::from_static("unrelated=1"));
    error(call(&app, req).await, StatusCode::FORBIDDEN, "forbidden").await;
    // A cookie-only unsafe request still needs Origin.
    let req = Request::builder()
        .method("POST")
        .uri("/api/v1/auth/logout")
        .header(header::COOKIE, session)
        .body(Body::empty())
        .unwrap();
    error(call(&app, req).await, StatusCode::FORBIDDEN, "forbidden").await;
}
