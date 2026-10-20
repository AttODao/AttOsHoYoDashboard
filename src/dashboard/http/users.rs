//! User view DTO and account-management routes; lifecycle rules remain in Accounts.
use super::App;
use crate::dashboard::{
    auth::sessions::Identity,
    db::entities::user,
    error::{Error, Result},
};
use axum::{
    Json, Router,
    extract::{Extension, Path, Query, State},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, patch, put},
};
use chrono::{DateTime, Utc};
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
        .route("/api/v1/admin/audit", get(audit_log))
}

#[derive(Serialize)]
pub(super) struct UserView {
    id: Uuid,
    username: String,
    is_admin: bool,
    active: bool,
    must_change_password: bool,
    last_login_at: Option<chrono::DateTime<chrono::Utc>>,
}
impl From<user::Model> for UserView {
    fn from(user: user::Model) -> Self {
        Self {
            id: user.id,
            username: user.username,
            is_admin: user.is_admin,
            active: user.active,
            must_change_password: user.must_change_password,
            last_login_at: user.last_login_at,
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
    let available = app.cookies.is_some();
    let complete = available
        && row
            .as_ref()
            .is_some_and(|row| row.auth_status == "verified" && row.hoyolab_uid.is_some());
    Ok(Json(MeView {
        user: UserView::from(actor.user.clone()),
        onboarding: serde_json::json!({"password_change_required":actor.user.must_change_password,"cookie_setup_complete":complete,"cookie_status":row.as_ref().map(|row|row.auth_status.as_str()).unwrap_or(if app.cookies.is_some(){"not_configured"}else{"storage_unavailable"}),"verification_available":available}),
        capabilities: serde_json::json!({"reward_posting_available":available}),
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
    app.accounts
        .request_deletion(&actor, id, input.current_password, input.confirmation)
        .await?;
    Ok(StatusCode::ACCEPTED)
}
/// Administrator listing: the public user fields plus account and linkage health.
#[derive(Serialize)]
struct AdminUserView {
    #[serde(flatten)]
    user: UserView,
    created_at: DateTime<Utc>,
    link_status: String,
    automatic_claims: u64,
}

async fn list_users(
    State(app): State<App>,
    Extension(actor): Extension<Arc<Identity>>,
) -> Result<Json<Vec<AdminUserView>>> {
    Ok(Json(
        app.accounts
            .user_overview(&actor)
            .await?
            .into_iter()
            .map(|row| AdminUserView {
                created_at: row.user.created_at,
                user: UserView::from(row.user),
                link_status: row.link_status,
                automatic_claims: row.automatic_claims,
            })
            .collect(),
    ))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AuditQuery {
    limit: Option<u64>,
    before: Option<DateTime<Utc>>,
}

#[derive(Serialize)]
struct AuditView {
    id: Uuid,
    action: String,
    created_at: DateTime<Utc>,
    actor: Option<String>,
    target: Option<String>,
}

async fn audit_log(
    State(app): State<App>,
    Extension(actor): Extension<Arc<Identity>>,
    Query(query): Query<AuditQuery>,
) -> Result<Json<Vec<AuditView>>> {
    let rows = app
        .accounts
        .audit_log(&actor, query.limit.unwrap_or(50), query.before)
        .await?;
    Ok(Json(
        rows.into_iter()
            .map(|row| AuditView {
                id: row.id,
                action: row.action,
                created_at: row.created_at,
                actor: row.actor,
                target: row.target,
            })
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
/// Exactly one change per request: activation or administrator role.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct UserChange {
    active: Option<bool>,
    is_admin: Option<bool>,
}
async fn set_active(
    State(app): State<App>,
    Extension(actor): Extension<Arc<Identity>>,
    Path(id): Path<Uuid>,
    Json(input): Json<UserChange>,
) -> Result<StatusCode> {
    match (input.active, input.is_admin) {
        (Some(active), None) => app.accounts.set_active(&actor, id, active).await?,
        (None, Some(is_admin)) => app.accounts.set_admin(&actor, id, is_admin).await?,
        _ => {
            return Err(Error::Invalid("send exactly one of active or is_admin"));
        }
    }
    Ok(StatusCode::NO_CONTENT)
}
