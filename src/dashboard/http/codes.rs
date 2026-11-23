//! Redemption-code transport: catalogue, per-UID modes and history, administrator controls.
use super::App;
use crate::dashboard::{
    auth::sessions::Identity,
    codes::{
        admin::{self, AdminInput},
        settings::CodeSettings,
    },
    error::Result,
};
use axum::{
    Json, Router,
    extract::{Extension, Path, State},
    http::StatusCode,
    routing::{get, patch},
};
use serde::Deserialize;
use serde_json::Value;
use std::sync::Arc;
use uuid::Uuid;

pub(super) fn protected_routes() -> Router<App> {
    Router::new()
        .route("/api/v1/codes", get(catalogue))
        .route("/api/v1/code-settings", get(list))
        .route("/api/v1/code-settings/{id}", patch(update))
        .route("/api/v1/code-history", get(history))
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
