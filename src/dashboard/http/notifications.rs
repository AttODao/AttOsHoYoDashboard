//! Personal and administrator notification channels, browser push subscriptions and the
//! app installations' inbox.
use super::App;
use crate::dashboard::{
    auth::sessions::Identity,
    error::Result,
    notifications::{
        channel::{ChannelInput, PersonalInput},
        push::{SubscriptionInput, Unsubscription},
    },
};
use axum::{
    Json, Router,
    extract::{Extension, Path, Query, State},
    http::StatusCode,
    routing::{delete, get, post},
};
use serde_json::Value;
use std::sync::Arc;
use uuid::Uuid;

pub(super) fn protected_routes() -> Router<App> {
    Router::new()
        .route("/api/v1/me/notifications", get(me_view).put(me_update))
        .route("/api/v1/me/notifications/test", post(me_test))
        .route(
            "/api/v1/me/notifications/push",
            post(subscribe).delete(unsubscribe),
        )
        .route(
            "/api/v1/me/notifications/app-devices",
            post(register_app_device),
        )
        .route(
            "/api/v1/me/notifications/app-devices/{id}",
            delete(unregister_app_device),
        )
        .route(
            "/api/v1/me/notifications/app-devices/{id}/inbox",
            get(app_inbox),
        )
        .route(
            "/api/v1/admin/notifications",
            get(admin_view).put(admin_update),
        )
        .route("/api/v1/admin/notifications/test", post(admin_test))
}

fn service(app: &App) -> &crate::dashboard::notifications::channel::Notifications {
    app.notifications.as_ref()
}

async fn me_view(
    State(app): State<App>,
    Extension(actor): Extension<Arc<Identity>>,
) -> Result<Json<Value>> {
    Ok(Json(
        serde_json::to_value(service(&app).personal_view(&actor).await?).unwrap_or(Value::Null),
    ))
}

async fn me_update(
    State(app): State<App>,
    Extension(actor): Extension<Arc<Identity>>,
    Json(input): Json<PersonalInput>,
) -> Result<StatusCode> {
    service(&app).update_personal(&actor, input).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn me_test(
    State(app): State<App>,
    Extension(actor): Extension<Arc<Identity>>,
    Json(_): Json<EmptyInput>,
) -> Result<Json<Value>> {
    Ok(Json(
        serde_json::to_value(service(&app).test_personal(&actor).await?).unwrap_or(Value::Null),
    ))
}

async fn subscribe(
    State(app): State<App>,
    Extension(actor): Extension<Arc<Identity>>,
    Json(input): Json<SubscriptionInput>,
) -> Result<StatusCode> {
    service(&app).subscribe(&actor, input).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn unsubscribe(
    State(app): State<App>,
    Extension(actor): Extension<Arc<Identity>>,
    Json(input): Json<Unsubscription>,
) -> Result<StatusCode> {
    service(&app).unsubscribe(&actor, input).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn register_app_device(
    State(app): State<App>,
    Extension(actor): Extension<Arc<Identity>>,
    Json(_): Json<EmptyInput>,
) -> Result<(StatusCode, Json<Value>)> {
    let view = service(&app).register_app_device(&actor).await?;
    Ok((
        StatusCode::CREATED,
        Json(serde_json::to_value(view).unwrap_or(Value::Null)),
    ))
}

async fn unregister_app_device(
    State(app): State<App>,
    Extension(actor): Extension<Arc<Identity>>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode> {
    service(&app).unregister_app_device(&actor, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct InboxQuery {
    #[serde(default)]
    after: i64,
}

async fn app_inbox(
    State(app): State<App>,
    Extension(actor): Extension<Arc<Identity>>,
    Path(id): Path<Uuid>,
    Query(query): Query<InboxQuery>,
) -> Result<Json<Value>> {
    Ok(Json(
        serde_json::to_value(service(&app).app_inbox(&actor, id, query.after).await?)
            .unwrap_or(Value::Null),
    ))
}

async fn admin_view(
    State(app): State<App>,
    Extension(actor): Extension<Arc<Identity>>,
) -> Result<Json<Value>> {
    Ok(Json(
        serde_json::to_value(service(&app).admin_view(&actor).await?).unwrap_or(Value::Null),
    ))
}

async fn admin_update(
    State(app): State<App>,
    Extension(actor): Extension<Arc<Identity>>,
    Json(input): Json<ChannelInput>,
) -> Result<StatusCode> {
    service(&app).update_admin(&actor, input).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn admin_test(
    State(app): State<App>,
    Extension(actor): Extension<Arc<Identity>>,
    Json(_): Json<EmptyInput>,
) -> Result<Json<Value>> {
    Ok(Json(
        serde_json::to_value(service(&app).test_admin(&actor).await?).unwrap_or(Value::Null),
    ))
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct EmptyInput {}
