//! Serve the compiled Dioxus browser bundle; never expose the build/source directory.
use super::{App, Error, Result};
use axum::{
    Router,
    extract::State,
    http::header,
    response::{IntoResponse, Response},
    routing::get,
};
use tower_http::services::ServeDir;
const OPENAPI: &str = include_str!("../../../docs/dashboard-api.json");
// DX 0.7.10 emits an external module loader. Allow Wasm compilation, not JS eval or inline scripts.
const CSP: &str = "default-src 'self'; script-src 'self' 'wasm-unsafe-eval'; style-src 'self'; connect-src 'self'; object-src 'none'; frame-ancestors 'none'; base-uri 'none'";

pub(super) fn routes(app: &App) -> Router<App> {
    Router::new()
        .route("/", get(index))
        .route("/docs", get(index))
        .route("/openapi.json", get(openapi))
        .nest_service(
            "/assets",
            ServeDir::new(app.config.ui_dir.join("assets"))
                .precompressed_br()
                .precompressed_gzip(),
        )
}
async fn index(State(app): State<App>) -> Result<Response> {
    let html = tokio::fs::read_to_string(app.config.ui_dir.join("index.html"))
        .await
        .map_err(|_| Error::Unavailable("Dioxus UI bundle is unavailable"))?;
    Ok((
        [
            (header::CONTENT_TYPE, "text/html; charset=utf-8"),
            (header::CONTENT_SECURITY_POLICY, CSP),
        ],
        html,
    )
        .into_response())
}
async fn openapi() -> impl IntoResponse {
    (
        [
            (header::CONTENT_TYPE, "application/json; charset=utf-8"),
            (
                header::CONTENT_SECURITY_POLICY,
                "default-src 'none'; frame-ancestors 'none'",
            ),
        ],
        OPENAPI,
    )
}
