//! Transport tests use the production router and isolated synthetic PostgreSQL accounts.
mod api_tokens;
mod auth;
mod dashboard;
mod hoyolab;
mod middleware;
mod notifications;
mod openapi;
mod passkey_roundtrip;
mod passkeys;
mod response;
mod runtime;
mod session;
mod ui;
mod users;

use super::{App, CookieStore, HttpState, router, session::cookie_name};
use crate::dashboard::{
    auth::{accounts::Accounts, passkeys::Passkeys, sessions::Sessions},
    config::Config,
};
use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode, header},
    response::Response,
};
use chrono::{Duration, FixedOffset, Utc};
use sea_orm::DatabaseConnection;
use serde_json::{Value, json};
use std::sync::Arc;
use tower::ServiceExt;
use uuid::Uuid;

const ORIGIN: &str = "https://dashboard.example.test";

fn config(secure_cookie: bool) -> Config {
    Config {
        database_url: "postgres://unused@localhost/unused".to_owned(),
        listen: "127.0.0.1:0".parse().unwrap(),
        ui_dir: std::env::temp_dir().join(format!("attos-missing-ui-{}", Uuid::new_v4())),
        origin: ORIGIN.to_owned(),
        rp_id: "dashboard.example.test".to_owned(),
        secure_cookie,
        session_ttl: Duration::days(7),
        timezone: FixedOffset::east_opt(9 * 3600).unwrap().into(),
        tuning: crate::dashboard::config::Tuning::default(),
        metrics_token: Some("synthetic-metrics-token".to_owned()),
        cookie_key: Some(Arc::new(
            crate::dashboard::hoyolab::credentials::CookieKey::from_hex(&"11".repeat(32)).unwrap(),
        )),
    }
}

fn app(accounts: Accounts) -> App {
    let config = config(true);
    let operations = crate::dashboard::hoyolab::operations::Operations::default();
    let cookies = config.cookie_key.as_ref().map(|key| {
        let mut store = CookieStore::new(accounts.db.clone(), Arc::clone(key));
        store.operations = operations.clone();
        Arc::new(store)
    });
    Arc::new(HttpState {
        cookies,
        operations,
        passkeys: Passkeys::new(&config.rp_id, &config.origin, config.cookie_key.as_deref())
            .unwrap(),
        notifications: Arc::new(
            crate::dashboard::notifications::channel::Notifications::new(
                accounts.db.clone(),
                config.cookie_key.clone(),
                config.origin.clone(),
            ),
        ),
        notifier_rewards_games: None,
        accounts,
        heartbeat: super::runtime::Heartbeat::new(),
        config,
    })
}

async fn disconnected_app() -> App {
    app(
        Accounts::new(DatabaseConnection::default(), Duration::days(7))
            .await
            .unwrap(),
    )
}

fn request(method: &str, path: &str, cookie: Option<&str>, body: Option<Value>) -> Request<Body> {
    let mut builder = Request::builder()
        .method(method)
        .uri(path)
        .header(header::ORIGIN, ORIGIN);
    if let Some(cookie) = cookie {
        builder = builder.header(header::COOKIE, cookie);
    }
    let body = match body {
        Some(body) => {
            builder = builder.header(header::CONTENT_TYPE, "application/json");
            Body::from(body.to_string())
        }
        None => Body::empty(),
    };
    builder.body(body).unwrap()
}

async fn call(app: &App, request: Request<Body>) -> Response {
    let asset = request.uri().path().starts_with("/assets/");
    let response = router(app.clone()).oneshot(request).await.unwrap();
    // Successful compiled assets carry their own policy, checked in `tests/ui.rs`.
    if !(asset && response.status().is_success()) {
        assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
    }
    assert_eq!(response.headers()["x-content-type-options"], "nosniff");
    response
}

async fn body(response: Response) -> Value {
    let bytes = to_bytes(response.into_body(), 64 * 1024).await.unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

async fn error(response: Response, status: StatusCode, code: &str) {
    assert_eq!(response.status(), status);
    assert_eq!(body(response).await["error"], code);
}

async fn cookie(app: &App, user_id: Uuid) -> String {
    let issued = Sessions::issue_until(&app.accounts.db, user_id, Utc::now() + Duration::hours(1))
        .await
        .unwrap();
    format!("{}={}", cookie_name(&app.config), issued.token)
}

fn response_cookie(response: &Response) -> String {
    response.headers()[header::SET_COOKIE]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_owned()
}

fn assertion_response() -> Value {
    // Structurally valid transport input, deliberately not a cryptographic proof.
    json!({
        "id": "AQ", "rawId": "AQ", "type": "public-key", "extensions": {},
        "response": {"authenticatorData": "AQ", "clientDataJSON": "AQ", "signature": "AQ", "userHandle": null}
    })
}
