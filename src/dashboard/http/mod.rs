//! HTTP composition and server lifetime; domain rules live outside this directory.
mod auth;
mod middleware;
mod passkeys;
mod response;
mod session;
mod users;

use super::{
    auth::{
        accounts::{self, Accounts},
        passkeys::Passkeys,
        sessions::Identity,
    },
    config::Config,
    error::{Error, Result},
};
use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Extension, State},
    middleware::from_fn_with_state,
    routing::get,
};
use sea_orm::DatabaseConnection;
use serde_json::json;
use std::{
    sync::{Arc, Mutex},
    time::Instant,
};

type App = Arc<HttpState>;
struct HttpState {
    accounts: Accounts,
    passkeys: Passkeys,
    config: Config,
    login_budget: Mutex<auth::LoginBudget>,
}

pub(super) async fn serve(db: DatabaseConnection, config: Config) -> Result<()> {
    let accounts = Accounts::new(db, config.session_ttl).await?;
    let passkeys = Passkeys::new(&config.rp_id, &config.origin)?;
    let app = Arc::new(HttpState {
        accounts,
        passkeys,
        config,
        login_budget: Mutex::new(auth::LoginBudget {
            started: Instant::now(),
            attempts: 0,
        }),
    });
    let protected = Router::new()
        .route("/api/v1/server-settings", get(server_settings))
        .merge(auth::protected_routes())
        .merge(users::protected_routes())
        .merge(passkeys::protected_routes())
        .route_layer(from_fn_with_state(app.clone(), middleware::authenticate));
    let router = Router::new()
        .route("/health", get(health))
        .merge(auth::public_routes())
        .merge(passkeys::public_routes())
        .merge(protected)
        .layer(DefaultBodyLimit::max(16 * 1024))
        .layer(from_fn_with_state(app.clone(), middleware::boundary))
        .with_state(app.clone());
    let listener = tokio::net::TcpListener::bind(app.config.listen)
        .await
        .map_err(|_| Error::Internal)?;
    tracing::info!(address = %app.config.listen, timezone = %app.config.timezone, "dashboard API listening");
    axum::serve(listener, router)
        .with_graceful_shutdown(shutdown())
        .await
        .map_err(|_| Error::Internal)?;
    Ok(())
}

async fn health(State(app): State<App>) -> Result<Json<serde_json::Value>> {
    app.accounts.db.ping().await?;
    Ok(Json(json!({"status": "ok"})))
}
async fn server_settings(
    State(app): State<App>,
    Extension(actor): Extension<Arc<Identity>>,
) -> Result<Json<serde_json::Value>> {
    accounts::require_ready(&actor.user)?;
    Ok(Json(
        json!({"timezone_offset": app.config.timezone.to_string(), "session_ttl_days": app.config.session_ttl.num_days()}),
    ))
}

async fn shutdown() {
    #[cfg(unix)]
    {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut terminate) => {
                tokio::select! { _ = tokio::signal::ctrl_c() => {}, _ = terminate.recv() => {} }
            }
            Err(_) => {
                let _ = tokio::signal::ctrl_c().await;
            }
        }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}
