//! HTTP composition and server lifetime; domain rules live outside this directory.
mod auth;
mod games;
mod hoyolab;
mod metrics;
mod middleware;
mod passkeys;
mod response;
mod rewards;
mod runtime;
mod session;
mod ui;
mod users;

#[cfg(test)]
mod tests;

use super::{
    auth::{
        accounts::{self, Accounts},
        passkeys::Passkeys,
        sessions::Identity,
    },
    config::Config,
    error::{Error, Result},
    hoyolab::store::CookieStore,
};
use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Extension, State},
    middleware::from_fn_with_state,
    routing::get,
};
use serde_json::json;
use std::sync::Arc;

type App = Arc<HttpState>;
struct HttpState {
    accounts: Accounts,
    passkeys: Passkeys,
    cookies: Option<Arc<CookieStore>>,
    operations: super::hoyolab::operations::Operations,
    heartbeat: runtime::Heartbeat,
    config: Config,
}

pub(super) async fn serve(accounts: Accounts, config: Config) -> Result<()> {
    let db = accounts.db.clone();
    let operations = super::hoyolab::operations::Operations::default();
    let cookies = config.cookie_key.as_ref().map(|key| {
        let mut store = CookieStore::new(db.clone(), Arc::clone(key));
        store.operations = operations.clone();
        store.timezone = config.timezone;
        store.tuning = config.tuning;
        Arc::new(store)
    });
    let passkeys = Passkeys::new(&config.rp_id, &config.origin, config.cookie_key.as_deref())?;
    let app = Arc::new(HttpState {
        accounts,
        passkeys,
        cookies,
        operations,
        heartbeat: runtime::Heartbeat::new(),
        config,
    });
    if !tokio::fs::metadata(app.config.ui_dir.join("index.html"))
        .await
        .is_ok_and(|metadata| metadata.is_file())
    {
        return Err(Error::Unavailable(
            "build the Dioxus UI before starting the server",
        ));
    }
    let router = router(app.clone());
    let listener = tokio::net::TcpListener::bind(app.config.listen)
        .await
        .map_err(|_| Error::Internal)?;
    tracing::info!(address = %app.config.listen, timezone = %app.config.timezone, "dashboard listening");
    let (stop, stopped) = tokio::sync::watch::channel(false);
    let worker = tokio::spawn(runtime::run(app.clone(), stopped));
    let stopping = app.cookies.clone();
    axum::serve(listener, router)
        .with_graceful_shutdown(async move {
            shutdown().await;
            if let Some(store) = stopping {
                store.stop_new_operations();
            }
            let _ = stop.send(true);
        })
        .await
        .map_err(|_| Error::Internal)?;
    worker.await.map_err(|_| Error::Internal)?;
    Ok(())
}

fn router(app: App) -> Router {
    let protected = Router::new()
        .route("/api/v1/server-settings", get(server_settings))
        .merge(auth::protected_routes())
        .merge(users::protected_routes())
        .merge(hoyolab::protected_routes())
        .merge(games::protected_routes())
        .merge(rewards::protected_routes())
        .merge(passkeys::protected_routes())
        .route_layer(from_fn_with_state(app.clone(), middleware::authenticate));
    Router::new()
        .route("/health", get(health))
        .route("/metrics", get(metrics::metrics))
        .route("/openapi.json", get(ui::openapi))
        .merge(ui::routes(&app))
        .merge(auth::public_routes())
        .merge(passkeys::public_routes())
        .merge(protected)
        .layer(DefaultBodyLimit::max(16 * 1024))
        .layer(from_fn_with_state(app.clone(), middleware::boundary))
        .with_state(app)
}

/// Database reachability plus the reward worker's liveness; a stalled worker is 503.
async fn health(
    State(app): State<App>,
) -> Result<(axum::http::StatusCode, Json<serde_json::Value>)> {
    app.accounts.db.ping().await?;
    let worker = if app.cookies.is_none() {
        "disabled"
    } else if app.heartbeat.age_secs() > runtime::STALLED_AFTER_SECS {
        "stalled"
    } else {
        "ok"
    };
    let (code, status) = if worker == "stalled" {
        (axum::http::StatusCode::SERVICE_UNAVAILABLE, "degraded")
    } else {
        (axum::http::StatusCode::OK, "ok")
    };
    Ok((code, Json(json!({"status": status, "worker": worker}))))
}
async fn server_settings(
    State(app): State<App>,
    Extension(actor): Extension<Arc<Identity>>,
) -> Result<Json<serde_json::Value>> {
    accounts::require_ready(&actor.user)?;
    Ok(Json(
        json!({"timezone_offset": app.config.timezone.offset_at(chrono::Utc::now()).to_string(), "timezone": app.config.timezone.name(), "session_ttl_days": app.config.session_ttl.num_days()}),
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
