//! HTTP app sign-in payloads only; PKCE and session policy live in the business module.
use super::{App, session};
use crate::dashboard::{auth::sessions::Identity, error::Result};
use axum::{
    Json, Router,
    extract::{Extension, State},
    http::HeaderMap,
    response::Response,
    routing::post,
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::Arc;

pub(super) fn public_routes() -> Router<App> {
    Router::new().route("/api/v1/auth/app-login/exchange", post(exchange))
}
pub(super) fn protected_routes() -> Router<App> {
    Router::new().route("/api/v1/me/app-login", post(approve))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Exchange {
    code: String,
    code_verifier: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Approval {
    code_challenge: String,
}

async fn exchange(
    State(app): State<App>,
    headers: HeaderMap,
    Json(input): Json<Exchange>,
) -> Result<Response> {
    let (user, issued) = app
        .app_logins
        .exchange(&app.accounts, &input.code, &input.code_verifier)
        .await?;
    session::signed_in(&app, &headers, user, issued).await
}
async fn approve(
    State(app): State<App>,
    Extension(actor): Extension<Arc<Identity>>,
    Json(input): Json<Approval>,
) -> Result<Json<Value>> {
    let code = app
        .app_logins
        .approve(&app.accounts, &actor, input.code_challenge)
        .await?;
    Ok(Json(json!({"code": code})))
}
