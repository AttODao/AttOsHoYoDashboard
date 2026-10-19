//! HTTP ceremony payloads only; WebAuthn and user policy live in the business modules.
use super::{App, session::signed_in};
use crate::dashboard::{
    auth::{
        passkeys::{AuthenticationChallenge, PasskeyView, RegistrationChallenge},
        sessions::Identity,
    },
    error::Result,
};
use axum::{
    Json, Router,
    extract::{Extension, Path, State},
    http::StatusCode,
    response::Response,
    routing::{delete, get, post},
};
use serde::Deserialize;
use std::sync::Arc;
use uuid::Uuid;
use webauthn_rs::prelude::{PublicKeyCredential, RegisterPublicKeyCredential};

pub(super) fn public_routes() -> Router<App> {
    Router::new()
        .route("/api/v1/auth/passkey/options", post(authentication_start))
        .route("/api/v1/auth/passkey/verify", post(authentication_finish))
}

pub(super) fn protected_routes() -> Router<App> {
    Router::new()
        .route("/api/v1/me/passkeys", get(list))
        .route("/api/v1/me/passkeys/options", post(registration_start))
        .route("/api/v1/me/passkeys/verify", post(registration_finish))
        .route("/api/v1/me/passkeys/{id}", delete(remove))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Label {
    label: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Registration {
    challenge_id: Uuid,
    response: RegisterPublicKeyCredential,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Username {
    username: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Authentication {
    challenge_id: Uuid,
    response: PublicKeyCredential,
}

async fn registration_start(
    State(app): State<App>,
    Extension(actor): Extension<Arc<Identity>>,
    Json(input): Json<Label>,
) -> Result<Json<RegistrationChallenge>> {
    Ok(Json(
        app.passkeys
            .registration_start(&app.accounts, &actor, input.label)
            .await?,
    ))
}

async fn registration_finish(
    State(app): State<App>,
    Extension(actor): Extension<Arc<Identity>>,
    Json(input): Json<Registration>,
) -> Result<(StatusCode, Json<PasskeyView>)> {
    let key = app
        .passkeys
        .registration_finish(&app.accounts, &actor, input.challenge_id, input.response)
        .await?;
    Ok((StatusCode::CREATED, Json(key)))
}

async fn list(
    State(app): State<App>,
    Extension(actor): Extension<Arc<Identity>>,
) -> Result<Json<Vec<PasskeyView>>> {
    Ok(Json(app.passkeys.list(&app.accounts, &actor).await?))
}

async fn remove(
    State(app): State<App>,
    Extension(actor): Extension<Arc<Identity>>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode> {
    app.passkeys.delete(&app.accounts, &actor, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn authentication_start(
    State(app): State<App>,
    Json(input): Json<Username>,
) -> Result<Json<AuthenticationChallenge>> {
    Ok(Json(
        app.passkeys
            .authentication_start(&app.accounts, input.username)
            .await?,
    ))
}

async fn authentication_finish(
    State(app): State<App>,
    Json(input): Json<Authentication>,
) -> Result<Response> {
    let (user, session) = app
        .passkeys
        .authentication_finish(&app.accounts, input.challenge_id, input.response)
        .await?;
    signed_in(&app, user, session)
}
