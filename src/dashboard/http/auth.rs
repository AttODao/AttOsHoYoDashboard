//! Password sign-in/change/sign-out payloads; the sign-in budget lives in `auth`.
use super::{
    App,
    session::{set_cookie, signed_in},
};
use crate::dashboard::{auth::sessions::Identity, error::Result};
use axum::{
    Json, Router,
    extract::{Extension, State},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
    routing::{post, put},
};
use serde::Deserialize;
use std::sync::Arc;

pub(super) fn public_routes() -> Router<App> {
    Router::new().route("/api/v1/auth/login", post(login))
}
pub(super) fn protected_routes() -> Router<App> {
    Router::new()
        .route("/api/v1/me/password", put(change_password))
        .route("/api/v1/auth/logout", post(logout))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Login {
    username: String,
    password: String,
}
async fn login(State(app): State<App>, Json(input): Json<Login>) -> Result<Response> {
    let (user, session) = app.accounts.login(input.username, input.password).await?;
    signed_in(&app, user, session)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PasswordChange {
    current_password: Option<String>,
    new_password: String,
}
async fn change_password(
    State(app): State<App>,
    Extension(actor): Extension<Arc<Identity>>,
    Json(input): Json<PasswordChange>,
) -> Result<Response> {
    let (user, session) = app
        .accounts
        .change_password(&actor, input.current_password, input.new_password)
        .await?;
    signed_in(&app, user, session)
}

async fn logout(
    State(app): State<App>,
    Extension(actor): Extension<Arc<Identity>>,
) -> Result<Response> {
    app.accounts.logout(&actor).await?;
    let mut response = StatusCode::NO_CONTENT.into_response();
    response
        .headers_mut()
        .insert(header::SET_COOKIE, set_cookie(&app.config, "", 0)?);
    Ok(response)
}
