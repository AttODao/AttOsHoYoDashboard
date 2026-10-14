//! Owner-scoped game status transport, never a credential or raw upstream payload DTO.
use super::App;
use crate::dashboard::{
    auth::sessions::Identity,
    error::{Error, Result},
    games::{GameStatus, Games},
};
use axum::{
    Json, Router,
    extract::{Extension, Path, State},
    http::StatusCode,
    routing::{get, patch},
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::Arc;
use uuid::Uuid;
pub(super) fn protected_routes() -> Router<App> {
    Router::new()
        .route("/api/v1/game-accounts", get(list))
        .route("/api/v1/game-accounts/{id}", patch(set_visible))
        .route("/api/v1/game-accounts/{id}/status", get(status))
}
fn service(app: &App) -> Result<Games> {
    Ok(Games::new(app.cookies.clone().ok_or(
        Error::Unavailable("Cookie storage is not configured"),
    )?))
}
async fn list(
    State(app): State<App>,
    Extension(actor): Extension<Arc<Identity>>,
) -> Result<Json<Value>> {
    let rows = service(&app)?.list(&actor).await?;
    Ok(Json(Value::Array(rows.into_iter().map(|row|json!({"id":row.id,"hoyolab_account_id":row.hoyolab_account_id,"game_kind":row.game_kind,"game_uid":row.game_uid,"region":row.region,"visible":row.visible,"detected":row.detected,"last_detected_at":row.last_detected_at})).collect())))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Visibility {
    visible: bool,
}
async fn set_visible(
    State(app): State<App>,
    Extension(actor): Extension<Arc<Identity>>,
    Path(id): Path<Uuid>,
    Json(input): Json<Visibility>,
) -> Result<StatusCode> {
    service(&app)?
        .set_visible(&actor, id, input.visible)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}
async fn status(
    State(app): State<App>,
    Extension(actor): Extension<Arc<Identity>>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>> {
    let row: GameStatus = service(&app)?.status(&actor, id).await?;
    Ok(Json(
        json!({"game_kind":row.game_kind,"game_uid":row.game_uid,"region":row.region,"visible":row.visible,"detected":row.detected,"data":row.payload,"fetched_at":row.fetched_at,"error_code":row.error_code,"error_at":row.error_at,"stale":row.stale}),
    ))
}
