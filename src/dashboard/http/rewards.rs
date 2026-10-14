//! Reward policy transport; all operations remain owned by the caller.
use super::App;
use crate::dashboard::{
    auth::sessions::Identity,
    error::{Error, Result},
    rewards::service::Rewards,
};
use axum::{
    Json, Router,
    extract::{Extension, Path, Query, State},
    http::StatusCode,
    routing::{get, patch, post},
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::Arc;
use uuid::Uuid;
pub(super) fn protected_routes() -> Router<App> {
    Router::new()
        .route("/api/v1/reward-settings", get(list))
        .route("/api/v1/reward-settings/{id}", patch(update))
        .route("/api/v1/reward-settings/{id}/run", post(run))
        .route("/api/v1/reward-runs", get(history))
}
fn service(app: &App) -> Result<Rewards> {
    Ok(Rewards::new(app.cookies.clone().ok_or(
        Error::Unavailable("Cookie storage is not configured"),
    )?))
}
async fn list(
    State(app): State<App>,
    Extension(actor): Extension<Arc<Identity>>,
) -> Result<Json<Value>> {
    let rows = service(&app)?.list(&actor).await?;
    Ok(Json(Value::Array(rows.into_iter().map(|row|json!({"id":row.setting.id,"game_kind":row.setting.game_kind,"enabled":row.setting.enabled,"local_time":row.setting.local_time,"auth_status":row.setting.auth_status,"next_run_at":row.setting.next_run_at,"last_run":row.last_run.map(|run|json!({"id":run.id,"state":run.state,"ended_at":run.ended_at})),"is_sign":row.is_sign,"last_checked_at":row.last_checked_at,"error_code":row.error_code,"error_at":row.error_at})).collect())))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SettingInput {
    enabled: bool,
    local_time: String,
}
async fn update(
    State(app): State<App>,
    Extension(actor): Extension<Arc<Identity>>,
    Path(id): Path<Uuid>,
    Json(input): Json<SettingInput>,
) -> Result<StatusCode> {
    service(&app)?
        .update(&actor, id, input.enabled, input.local_time)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EmptyInput {}
async fn run(
    State(app): State<App>,
    Extension(actor): Extension<Arc<Identity>>,
    Path(id): Path<Uuid>,
    Json(_): Json<EmptyInput>,
) -> Result<(StatusCode, Json<Value>)> {
    let row = service(&app)?.enqueue(&actor, id).await?;
    Ok((
        StatusCode::ACCEPTED,
        Json(json!({"id":row.id,"state":row.state})),
    ))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct HistoryQuery {
    limit: Option<u64>,
}
async fn history(
    State(app): State<App>,
    Extension(actor): Extension<Arc<Identity>>,
    Query(query): Query<HistoryQuery>,
) -> Result<Json<Value>> {
    let rows = service(&app)?
        .history(&actor, query.limit.unwrap_or(100))
        .await?;
    Ok(Json(Value::Array(rows.into_iter().map(|row|json!({"id":row.run.id,"setting_id":row.run.setting_id,"game_kind":row.game_kind,"trigger":row.run.trigger,"state":row.run.state,"created_at":row.run.created_at,"ended_at":row.run.ended_at,"attempt_count":row.run.attempt_count,"attempts":row.attempts.into_iter().map(|attempt|json!({"number":attempt.number,"outcome":attempt.outcome,"error_code":attempt.error_code,"started_at":attempt.started_at,"finished_at":attempt.finished_at})).collect::<Vec<_>>()})).collect())))
}
