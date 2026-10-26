//! API token management transport; issuance and revocation are session-only.
use super::App;
use crate::dashboard::{
    auth::{
        api_tokens::{ApiTokens, Scope},
        sessions::Identity,
    },
    db::entities::api_token,
    error::Result,
};
use axum::{
    Json, Router,
    extract::{Extension, Path, State},
    http::StatusCode,
    routing::{delete, get},
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

pub(super) fn protected_routes() -> Router<App> {
    Router::new()
        .route("/api/v1/me/api-tokens", get(list).post(issue))
        .route("/api/v1/me/api-tokens/{id}", delete(revoke))
}

#[derive(Serialize)]
struct TokenView {
    id: Uuid,
    name: String,
    scope: String,
    created_at: DateTime<Utc>,
    last_used_at: Option<DateTime<Utc>>,
}

impl From<api_token::Model> for TokenView {
    fn from(row: api_token::Model) -> Self {
        Self {
            id: row.id,
            name: row.name,
            scope: row.scope,
            created_at: row.created_at,
            last_used_at: row.last_used_at,
        }
    }
}

/// The only response that carries the plaintext token.
#[derive(Serialize)]
struct IssuedView {
    #[serde(flatten)]
    view: TokenView,
    token: String,
}

async fn list(
    State(app): State<App>,
    Extension(actor): Extension<Arc<Identity>>,
) -> Result<Json<Vec<TokenView>>> {
    let rows = ApiTokens::list(&app.accounts.db, actor.user.id).await?;
    Ok(Json(rows.into_iter().map(TokenView::from).collect()))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct IssueInput {
    name: String,
    scope: String,
}

async fn issue(
    State(app): State<App>,
    Extension(actor): Extension<Arc<Identity>>,
    Json(input): Json<IssueInput>,
) -> Result<(StatusCode, Json<IssuedView>)> {
    let scope = Scope::parse(&input.scope)?;
    let issued = ApiTokens::issue(&app.accounts.db, &actor, &input.name, scope).await?;
    Ok((
        StatusCode::CREATED,
        Json(IssuedView {
            view: TokenView::from(issued.row),
            token: issued.token,
        }),
    ))
}

async fn revoke(
    State(app): State<App>,
    Extension(actor): Extension<Arc<Identity>>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode> {
    ApiTokens::revoke(&app.accounts.db, &actor, id).await?;
    Ok(StatusCode::NO_CONTENT)
}
