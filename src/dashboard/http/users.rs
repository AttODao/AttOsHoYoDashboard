//! User view DTO and account-management routes; lifecycle rules remain in Accounts.
use super::App;
use crate::dashboard::{
    auth::sessions::Identity,
    db::entities::user,
    error::{Error, Result},
};
use axum::{
    Json, Router,
    extract::{Extension, Path, State},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, patch, put},
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

pub(super) fn protected_routes() -> Router<App> {
    Router::new()
        .route("/api/v1/me", get(me).delete(delete_me))
        .route("/api/v1/admin/users", get(list_users).post(create_user))
        .route(
            "/api/v1/admin/users/{id}",
            patch(set_active).delete(delete_user),
        )
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

#[derive(Serialize)]
struct MeView {
    #[serde(flatten)]
    user: UserView,
    onboarding: serde_json::Value,
    capabilities: serde_json::Value,
}
async fn me(
    State(app): State<App>,
    Extension(actor): Extension<Arc<Identity>>,
) -> Result<Json<MeView>> {
    let row = match &app.cookies {
        Some(store) => store.status(&actor).await?,
        None => None,
    };
    let available = app
        .cookies
        .as_ref()
        .is_some_and(|store| store.verification_available());
    let complete = available
        && row
            .as_ref()
            .is_some_and(|row| row.auth_status == "verified" && row.hoyolab_uid.is_some());
    Ok(Json(MeView {
        user: UserView::from(actor.user.clone()),
        onboarding: serde_json::json!({"password_change_required":actor.user.must_change_password,"cookie_setup_complete":complete,"cookie_status":row.as_ref().map(|row|row.auth_status.as_str()).unwrap_or(if app.cookies.is_some(){"not_configured"}else{"storage_unavailable"}),"verification_available":available}),
        capabilities: serde_json::json!({"reward_posting_available":app.cookies.as_ref().is_some_and(|store|store.reward_available())}),
    }))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DeletionInput {
    current_password: String,
    confirmation: String,
}
async fn delete_me(
    State(app): State<App>,
    Extension(actor): Extension<Arc<Identity>>,
    Json(input): Json<DeletionInput>,
) -> Result<Response> {
    app.accounts
        .request_deletion(
            &actor,
            actor.user.id,
            input.current_password,
            input.confirmation,
        )
        .await?;
    let mut response = (
        StatusCode::ACCEPTED,
        Json(serde_json::json!({"state":"deleting"})),
    )
        .into_response();
    response.headers_mut().insert(
        header::SET_COOKIE,
        super::session::set_cookie(&app.config, "", 0)?,
    );
    Ok(response)
}
async fn delete_user(
    State(app): State<App>,
    Extension(actor): Extension<Arc<Identity>>,
    Path(id): Path<Uuid>,
    Json(input): Json<DeletionInput>,
) -> Result<StatusCode> {
    if !actor.user.is_admin {
        return Err(Error::Forbidden);
    }
    app.accounts
        .request_deletion(&actor, id, input.current_password, input.confirmation)
        .await?;
    Ok(StatusCode::ACCEPTED)
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
