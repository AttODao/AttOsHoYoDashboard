//! Settings file transport: export as a download, preview and apply an import.
//! Session-only: these routes are not on the API token allow-list.
use super::App;
use crate::dashboard::{
    auth::sessions::Identity,
    error::Result,
    settings_transfer::service::{Outcome, SettingsTransfer},
};
use axum::{
    Json, Router,
    extract::{Extension, State},
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde_json::{Value, json};
use std::sync::Arc;

pub(super) fn protected_routes() -> Router<App> {
    Router::new()
        .route("/api/v1/me/settings/export", get(export))
        .route("/api/v1/me/settings/import/preview", post(preview))
        .route("/api/v1/me/settings/import", post(import))
}

fn service(app: &App) -> SettingsTransfer {
    let cookies = app.cookies.clone();
    SettingsTransfer::new(
        app.accounts.db.clone(),
        app.config.timezone,
        app.operations.clone(),
        move || cookies.as_ref().is_some_and(|store| store.is_closing()),
    )
}

async fn export(
    State(app): State<App>,
    Extension(actor): Extension<Arc<Identity>>,
) -> Result<Response> {
    let file = service(&app).export(&actor).await?;
    let now = chrono::Utc::now();
    let date = now
        .with_timezone(&app.config.timezone.offset_at(now))
        .format("%Y-%m-%d");
    let mut response = Json(file).into_response();
    if let Ok(value) = HeaderValue::from_str(&format!(
        "attachment; filename=\"ahd-settings-{date}.json\""
    )) {
        response
            .headers_mut()
            .insert(header::CONTENT_DISPOSITION, value);
    }
    Ok(response)
}

fn report(outcome: Outcome, invalid_status: StatusCode) -> Response {
    match outcome {
        Outcome::Planned(report) => {
            let mut body = serde_json::to_value(report).unwrap_or(Value::Null);
            body["valid"] = Value::Bool(true);
            Json(body).into_response()
        }
        Outcome::Invalid(errors) if invalid_status.is_success() => {
            Json(json!({"valid": false, "errors": errors})).into_response()
        }
        Outcome::Invalid(errors) => (
            invalid_status,
            Json(json!({
                "error": "invalid_input",
                "message": "settings file is invalid",
                "errors": errors,
            })),
        )
            .into_response(),
    }
}

/// A file with problems is a normal preview result (200, `valid: false`).
async fn preview(
    State(app): State<App>,
    Extension(actor): Extension<Arc<Identity>>,
    Json(file): Json<Value>,
) -> Result<Response> {
    Ok(report(
        service(&app).preview(&actor, &file).await?,
        StatusCode::OK,
    ))
}

/// Applying a file with problems is a 400 that lists them; nothing is written.
async fn import(
    State(app): State<App>,
    Extension(actor): Extension<Arc<Identity>>,
    Json(file): Json<Value>,
) -> Result<Response> {
    Ok(report(
        service(&app).import(&actor, &file).await?,
        StatusCode::BAD_REQUEST,
    ))
}
