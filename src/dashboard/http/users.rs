//! User view DTO and account-management routes; lifecycle rules remain in Accounts.
use super::App;
use crate::dashboard::{auth::sessions::Identity, db::entities::user, error::Result};
use axum::{
    Json, Router,
    extract::{Extension, Path, State},
    http::StatusCode,
    routing::{get, patch, put},
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

pub(super) fn protected_routes() -> Router<App> {
    Router::new()
        .route("/api/v1/me", get(me))
        .route("/api/v1/admin/users", get(list_users).post(create_user))
        .route("/api/v1/admin/users/{id}", patch(set_active))
        .route(
            "/api/v1/admin/users/{id}/temporary-password",
            put(reset_password),
        )
}

#[derive(Serialize)]
pub(super) struct UserView {
    id: Uuid,
    username: String,
    is_admin: bool,
    active: bool,
    must_change_password: bool,
}
impl From<user::Model> for UserView {
    fn from(user: user::Model) -> Self {
        Self {
            id: user.id,
            username: user.username,
            is_admin: user.is_admin,
            active: user.active,
            must_change_password: user.must_change_password,
        }
    }
}

async fn me(Extension(actor): Extension<Arc<Identity>>) -> Json<UserView> {
    Json(UserView::from(actor.user.clone()))
}
async fn list_users(
    State(app): State<App>,
    Extension(actor): Extension<Arc<Identity>>,
) -> Result<Json<Vec<UserView>>> {
    Ok(Json(
        app.accounts
            .list_users(&actor)
            .await?
            .into_iter()
            .map(UserView::from)
            .collect(),
    ))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct UserCreation {
    username: String,
    temporary_password: String,
}
async fn create_user(
    State(app): State<App>,
    Extension(actor): Extension<Arc<Identity>>,
    Json(input): Json<UserCreation>,
) -> Result<(StatusCode, Json<UserView>)> {
    let user = app
        .accounts
        .create_user(&actor, input.username, input.temporary_password)
        .await?;
    Ok((StatusCode::CREATED, Json(user.into())))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TemporaryPassword {
    temporary_password: String,
}
async fn reset_password(
    State(app): State<App>,
    Extension(actor): Extension<Arc<Identity>>,
    Path(id): Path<Uuid>,
    Json(input): Json<TemporaryPassword>,
) -> Result<StatusCode> {
    app.accounts
        .reset_password(&actor, id, input.temporary_password)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Activation {
    active: bool,
}
async fn set_active(
    State(app): State<App>,
    Extension(actor): Extension<Arc<Identity>>,
    Path(id): Path<Uuid>,
    Json(input): Json<Activation>,
) -> Result<StatusCode> {
    app.accounts.set_active(&actor, id, input.active).await?;
    Ok(StatusCode::NO_CONTENT)
}
