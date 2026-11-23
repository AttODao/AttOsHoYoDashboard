//! Redemption-code transport: catalogue, per-UID modes and history, redemption state and
//! requests, administrator controls.
use super::App;
use crate::dashboard::{
    auth::sessions::Identity,
    codes::{
        admin::{self, AdminInput},
        manual::{self, Operation},
        redeem::RedemptionRuns,
        settings::CodeSettings,
    },
    error::{Error, Result},
};
use axum::{
    Json, Router,
    extract::{Extension, Path, State},
    http::StatusCode,
    routing::{get, patch, post},
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{sync::Arc, time::Instant};
use uuid::Uuid;

pub(super) fn protected_routes() -> Router<App> {
    Router::new()
        .route("/api/v1/codes", get(catalogue))
        .route("/api/v1/code-settings", get(list))
        .route("/api/v1/code-settings/{id}", patch(update))
        .route("/api/v1/code-history", get(history))
        .route("/api/v1/code-redemption", get(redemption))
        .route("/api/v1/code-redemption/run", post(redeem_now))
        .route("/api/v1/code-redemption/codes", post(enter_code))
        .route("/api/v1/admin/codes", get(admin_view).put(admin_update))
}

fn service(app: &App) -> CodeSettings {
    CodeSettings::new(app.accounts.db.clone())
}

async fn catalogue(
    State(app): State<App>,
    Extension(actor): Extension<Arc<Identity>>,
) -> Result<Json<Value>> {
    Ok(Json(
        serde_json::to_value(service(&app).catalogue(&actor).await?).unwrap_or(Value::Null),
    ))
}

async fn list(
    State(app): State<App>,
    Extension(actor): Extension<Arc<Identity>>,
) -> Result<Json<Value>> {
    Ok(Json(
        serde_json::to_value(service(&app).list(&actor).await?).unwrap_or(Value::Null),
    ))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ModeInput {
    mode: String,
}

async fn update(
    State(app): State<App>,
    Extension(actor): Extension<Arc<Identity>>,
    Path(id): Path<Uuid>,
    Json(input): Json<ModeInput>,
) -> Result<StatusCode> {
    service(&app).update(&actor, id, &input.mode).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn history(
    State(app): State<App>,
    Extension(actor): Extension<Arc<Identity>>,
) -> Result<Json<Value>> {
    Ok(Json(
        serde_json::to_value(service(&app).history(&actor).await?).unwrap_or(Value::Null),
    ))
}

fn runs(app: &App) -> Result<Arc<dyn RedemptionRuns>> {
    app.redemption
        .clone()
        .ok_or(Error::Unavailable("Cookie storage is not configured"))
}

async fn redemption(
    State(app): State<App>,
    Extension(actor): Extension<Arc<Identity>>,
) -> Result<Json<Value>> {
    let running = app
        .redemption
        .as_ref()
        .is_some_and(|runs| runs.is_running(actor.user.id));
    let view = manual::view(&app.accounts.db, &actor, app.redemption.is_some(), running).await?;
    Ok(Json(serde_json::to_value(view).unwrap_or(Value::Null)))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EmptyInput {}

/// Redeem now: also the explicit retry after a CAPTCHA or Cookie answer. Session only.
async fn redeem_now(
    State(app): State<App>,
    Extension(actor): Extension<Arc<Identity>>,
    Json(_): Json<EmptyInput>,
) -> Result<(StatusCode, Json<Value>)> {
    let runs = runs(&app)?;
    app.code_throttle
        .admit(actor.user.id, Operation::RedeemNow, Instant::now())?;
    manual::redeem_now(&app.accounts.db, &actor).await?;
    runs.trigger(actor.user.id);
    Ok((StatusCode::ACCEPTED, Json(json!({"started": true}))))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CodeInput {
    game_kind: String,
    code: String,
    /// One detected UID of that game; absent means every one of them.
    #[serde(default)]
    game_account_id: Option<Uuid>,
}

/// Enters a private code for every detected UID of a game (or one of them) and starts a
/// run. Session only.
async fn enter_code(
    State(app): State<App>,
    Extension(actor): Extension<Arc<Identity>>,
    Json(input): Json<CodeInput>,
) -> Result<(StatusCode, Json<Value>)> {
    let runs = runs(&app)?;
    app.code_throttle
        .admit(actor.user.id, Operation::EnterCode, Instant::now())?;
    let entered = manual::enter_code(
        &app.accounts.db,
        &actor,
        &input.game_kind,
        &input.code,
        input.game_account_id,
    )
    .await?;
    runs.trigger(actor.user.id);
    Ok((
        StatusCode::ACCEPTED,
        Json(json!({"entries": serde_json::to_value(entered).unwrap_or(Value::Null)})),
    ))
}

async fn admin_view(
    State(app): State<App>,
    Extension(actor): Extension<Arc<Identity>>,
) -> Result<Json<Value>> {
    let view = admin::view(&app.accounts.db, &actor, app.codes.source_host()).await?;
    Ok(Json(serde_json::to_value(view).unwrap_or(Value::Null)))
}

async fn admin_update(
    State(app): State<App>,
    Extension(actor): Extension<Arc<Identity>>,
    Json(input): Json<AdminInput>,
) -> Result<StatusCode> {
    admin::update(&app.accounts.db, &actor, input).await?;
    Ok(StatusCode::NO_CONTENT)
}
