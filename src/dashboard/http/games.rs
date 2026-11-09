//! Owner-scoped game status transport, never a credential or raw upstream payload DTO.
use super::App;
use crate::dashboard::{
    auth::sessions::Identity,
    error::{Error, Result},
    games::{GameCalendar, GameChallenges, GameStatus, Games, ResourceHistory},
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
        .route("/api/v1/game-accounts/{id}/challenges", get(challenges))
        .route("/api/v1/game-accounts/{id}/calendar", get(calendar))
        .route(
            "/api/v1/game-accounts/{id}/resource-history",
            get(resource_history),
        )
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
async fn challenges(
    State(app): State<App>,
    Extension(actor): Extension<Arc<Identity>>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>> {
    let row: GameChallenges = service(&app)?.challenges(&actor, id).await?;
    Ok(Json(json!({
        "game_kind": row.game_kind,
        "game_uid": row.game_uid,
        "region": row.region,
        "visible": row.visible,
        "detected": row.detected,
        "modes": row.modes,
        "fetched_at": row.fetched_at,
        "error_code": row.error_code,
        "error_at": row.error_at,
        "stale": row.stale,
    })))
}
async fn calendar(
    State(app): State<App>,
    Extension(actor): Extension<Arc<Identity>>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>> {
    let row: GameCalendar = service(&app)?.calendar(&actor, id).await?;
    Ok(Json(json!({
        "game_kind": row.game_kind,
        "game_uid": row.game_uid,
        "region": row.region,
        "visible": row.visible,
        "detected": row.detected,
        "items": row.items,
        "fetched_at": row.fetched_at,
        "error_code": row.error_code,
        "error_at": row.error_at,
        "stale": row.stale,
    })))
}
async fn resource_history(
    State(app): State<App>,
    Extension(actor): Extension<Arc<Identity>>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>> {
    let row: ResourceHistory = service(&app)?.resource_history(&actor, id).await?;
    let segments: Vec<Value> = row
        .cap_time
        .segments
        .iter()
        .map(|segment| {
            json!({
                "state": segment.state.as_str(),
                "start": segment.start,
                "end": segment.end,
                "seconds": (segment.end - segment.start).num_seconds(),
            })
        })
        .collect();
    Ok(Json(json!({
        "game_kind": row.game_kind,
        "game_uid": row.game_uid,
        "region": row.region,
        "visible": row.visible,
        "detected": row.detected,
        "resource": row.resource,
        "unit_seconds": row.unit.num_seconds(),
        "max_gap_seconds": crate::dashboard::games::MAX_GAP.num_seconds(),
        "window_start": row.window_start,
        "window_end": row.window_end,
        "sample_count": row.sample_count,
        "at_cap_seconds": row.cap_time.at_cap.num_seconds(),
        "unknown_seconds": row.cap_time.unknown.num_seconds(),
        "segments": segments,
    })))
}
