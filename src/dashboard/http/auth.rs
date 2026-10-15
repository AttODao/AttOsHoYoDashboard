//! Password sign-in/change/sign-out payloads and a shared HTTP login budget.
use super::{
    App, HttpState,
    session::{set_cookie, signed_in},
};
use crate::dashboard::{
    auth::sessions::Identity,
    error::{Error, Result},
};
use axum::{
    Json, Router,
    extract::{Extension, State},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
    routing::{post, put},
};
use serde::Deserialize;
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

pub(super) struct LoginBudget {
    pub started: Instant,
    pub attempts: u32,
}

pub(super) fn public_routes() -> Router<App> {
    Router::new().route("/api/v1/auth/login", post(login))
}
pub(super) fn protected_routes() -> Router<App> {
    Router::new()
        .route("/api/v1/me/password", put(change_password))
        .route("/api/v1/auth/logout", post(logout))
}

pub(super) fn check_login_budget(app: &HttpState) -> Result<()> {
    // ponytail: global budget for the private app; use per-user/IP limits if usage grows.
    let mut budget = app.login_budget.lock().map_err(|_| Error::Internal)?;
    if budget.started.elapsed() >= Duration::from_secs(60) {
        budget.started = Instant::now();
        budget.attempts = 0;
    }
    if budget.attempts >= 20 {
        return Err(Error::RateLimited);
    }
    budget.attempts += 1;
    Ok(())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Login {
    username: String,
    password: String,
}
async fn login(State(app): State<App>, Json(input): Json<Login>) -> Result<Response> {
    check_login_budget(&app)?;
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
