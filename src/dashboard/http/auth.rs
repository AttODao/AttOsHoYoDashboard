//! Password sign-in/change/sign-out payloads; the sign-in budget lives in `auth`.
use super::{
    App,
    session::{set_cookie, signed_in},
};
use crate::dashboard::{
    auth::sessions::{Identity, Sessions},
    error::Result,
};
use axum::{
    Json, Router,
    extract::{Extension, Path, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{delete, get, post, put},
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

pub(super) fn public_routes() -> Router<App> {
    Router::new().route("/api/v1/auth/login", post(login))
}
pub(super) fn protected_routes() -> Router<App> {
    Router::new()
        .route("/api/v1/me/password", put(change_password))
        .route("/api/v1/auth/logout", post(logout))
        .route("/api/v1/me/sessions", get(list_sessions))
        .route("/api/v1/me/sessions/{id}", delete(revoke_session))
        .route(
            "/api/v1/me/sessions/revoke-others",
            post(revoke_other_sessions),
        )
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Login {
    username: String,
    password: String,
}
async fn login(
    State(app): State<App>,
    headers: HeaderMap,
    Json(input): Json<Login>,
) -> Result<Response> {
    let (user, session) = app.accounts.login(input.username, input.password).await?;
    signed_in(&app, &headers, user, session).await
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
    headers: HeaderMap,
    Json(input): Json<PasswordChange>,
) -> Result<Response> {
    let (user, session) = app
        .accounts
        .change_password(&actor, input.current_password, input.new_password)
        .await?;
    signed_in(&app, &headers, user, session).await
}

#[derive(Serialize)]
struct SessionView {
    id: Uuid,
    created_at: DateTime<Utc>,
    last_seen_at: Option<DateTime<Utc>>,
    expires_at: DateTime<Utc>,
    user_agent: Option<String>,
    current: bool,
}

async fn list_sessions(
    State(app): State<App>,
    Extension(actor): Extension<Arc<Identity>>,
) -> Result<Json<Vec<SessionView>>> {
    let rows = Sessions::list(&app.accounts.db, actor.user.id).await?;
    Ok(Json(
        rows.into_iter()
            .map(|row| SessionView {
                current: row.token_hash == actor.token_hash,
                id: row.id,
                created_at: row.created_at,
                last_seen_at: row.last_seen_at,
                expires_at: row.expires_at,
                user_agent: row.user_agent,
            })
            .collect(),
    ))
}

async fn revoke_session(
    State(app): State<App>,
    Extension(actor): Extension<Arc<Identity>>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode> {
    Sessions::revoke(&app.accounts.db, &actor, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Empty {}

async fn revoke_other_sessions(
    State(app): State<App>,
    Extension(actor): Extension<Arc<Identity>>,
    Json(_): Json<Empty>,
) -> Result<Json<serde_json::Value>> {
    let revoked = Sessions::revoke_others(&app.accounts.db, &actor).await?;
    Ok(Json(serde_json::json!({ "revoked": revoked })))
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
