//! Cookie setup transport; plaintext credentials and unverified UID claims are never returned.
use super::App;
use crate::dashboard::{
    auth::sessions::Identity,
    error::{Error, Result},
    hoyolab::store::CookieStore,
};
use axum::{
    Json, Router,
    extract::{Extension, State},
    http::StatusCode,
    routing::{get, post, put},
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::Arc;

pub(super) fn protected_routes() -> Router<App> {
    Router::new()
        .route("/api/v1/me/hoyolab", get(status))
        .route("/api/v1/me/hoyolab/cookie", put(submit).delete(remove))
        .route("/api/v1/me/hoyolab/verify", post(verify))
        .route("/api/v1/me/hoyolab/sync", post(sync))
}

fn store(app: &App) -> Result<&CookieStore> {
    app.cookies
        .as_deref()
        .ok_or(Error::Unavailable("Cookie storage is not configured"))
}

async fn status(
    State(app): State<App>,
    Extension(actor): Extension<Arc<Identity>>,
) -> Result<Json<Value>> {
    let row = store(&app)?.status(&actor).await?;
    Ok(Json(json!({
        "status": row.as_ref().map(|row| row.auth_status.as_str()).unwrap_or("not_configured"),
        "setup_complete": row.as_ref().is_some_and(|row| row.hoyolab_uid.is_some() && row.auth_status == "verified"),
        "updated_at": row.as_ref().map(|row| row.updated_at),
        "id":row.as_ref().map(|row|row.id),
        "hoyolab_uid":row.as_ref().and_then(|row|row.hoyolab_uid.as_deref()),
        "verified_at":row.as_ref().and_then(|row|row.verified_at),
        "verification_available":true,
    })))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EmptyInput {}
async fn verify(
    State(app): State<App>,
    Extension(actor): Extension<Arc<Identity>>,
    Json(_): Json<EmptyInput>,
) -> Result<StatusCode> {
    store(&app)?.verify(&actor).await?;
    Ok(StatusCode::NO_CONTENT)
}
async fn sync(
    State(app): State<App>,
    Extension(actor): Extension<Arc<Identity>>,
    Json(_): Json<EmptyInput>,
) -> Result<StatusCode> {
    store(&app)?.sync(&actor).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn remove(
    State(app): State<App>,
    Extension(actor): Extension<Arc<Identity>>,
) -> Result<StatusCode> {
    store(&app)?.remove_cookie(&actor).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CookieInput {
    cookie: String,
}

async fn submit(
    State(app): State<App>,
    Extension(actor): Extension<Arc<Identity>>,
    Json(input): Json<CookieInput>,
) -> Result<(StatusCode, Json<Value>)> {
    store(&app)?.submit(&actor, &input.cookie).await?;
    let row = store(&app)?.status(&actor).await?;
    let complete = row
        .as_ref()
        .is_some_and(|row| row.auth_status == "verified" && row.hoyolab_uid.is_some());
    Ok((
        if complete {
            StatusCode::OK
        } else {
            StatusCode::ACCEPTED
        },
        Json(
            json!({"status":row.as_ref().map(|row|row.auth_status.as_str()).unwrap_or("not_configured"),"setup_complete":complete}),
        ),
    ))
}
