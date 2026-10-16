//! Serve the compiled Dioxus bundle and a bounded, fixed-origin API proxy.
use super::{Error, Result};
use crate::dashboard::config::DashboardConfig;
use axum::{
    Router,
    body::{Body, to_bytes},
    extract::{Request, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{any, get},
};
use std::{path::PathBuf, time::Duration};
use tower_http::services::ServeDir;

const OPENAPI: &str = include_str!("../../../docs/dashboard-api.json");
// DX 0.7.10 emits an external module loader. Allow Wasm compilation, not JS eval or inline scripts.
const CSP: &str = "default-src 'self'; script-src 'self' 'wasm-unsafe-eval'; style-src 'self'; connect-src 'self'; object-src 'none'; frame-ancestors 'none'; base-uri 'none'";
const REQUEST_LIMIT: usize = 16 * 1024;
const RESPONSE_LIMIT: usize = 1024 * 1024;

#[derive(Clone)]
struct UiState {
    ui_dir: PathBuf,
    api_origin: reqwest::Url,
    client: reqwest::Client,
}

pub(in crate::dashboard) async fn serve(config: DashboardConfig) -> Result<()> {
    let listen = config.listen;
    let router = router(config).await?;
    let listener = tokio::net::TcpListener::bind(listen)
        .await
        .map_err(|_| Error::Internal)?;
    tracing::info!(address = %listen, "dashboard listening");
    axum::serve(listener, router)
        .with_graceful_shutdown(super::shutdown())
        .await
        .map_err(|_| Error::Internal)
}

async fn router(config: DashboardConfig) -> Result<Router> {
    if !tokio::fs::metadata(config.ui_dir.join("index.html"))
        .await
        .is_ok_and(|metadata| metadata.is_file())
    {
        return Err(Error::Unavailable(
            "build the Dioxus UI before starting the dashboard",
        ));
    }
    let client = reqwest::Client::builder()
        .no_proxy()
        .no_gzip()
        .no_brotli()
        .no_deflate()
        .no_zstd()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(120))
        .build()
        .map_err(|_| Error::Internal)?;
    let assets = ServeDir::new(config.ui_dir.join("assets"))
        .precompressed_br()
        .precompressed_gzip();
    Ok(Router::new()
        .route("/", get(index))
        .route("/docs", get(index))
        .nest_service("/assets", assets)
        .route("/api/v1", any(proxy))
        .route("/api/v1/{*path}", any(proxy))
        .route("/openapi.json", any(proxy))
        .route("/health", any(proxy))
        .layer(axum::middleware::map_response(|response| async move {
            super::middleware::harden(response)
        }))
        .with_state(UiState {
            ui_dir: config.ui_dir,
            api_origin: config.api_origin,
            client,
        }))
}

async fn index(State(state): State<UiState>) -> Result<Response> {
    let html = tokio::fs::read_to_string(state.ui_dir.join("index.html"))
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

// Strip both the standard hop-by-hop headers and every Connection-nominated header.
fn strip_hop_headers(headers: &mut HeaderMap) {
    let nominated: Vec<_> = headers
        .get_all(header::CONNECTION)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(','))
        .filter_map(|name| header::HeaderName::from_bytes(name.trim().as_bytes()).ok())
        .collect();
    for name in nominated {
        headers.remove(name);
    }
    for name in [
        "connection",
        "keep-alive",
        "proxy-authenticate",
        "proxy-authorization",
        "proxy-connection",
        "te",
        "trailer",
        "transfer-encoding",
        "upgrade",
    ] {
        headers.remove(name);
    }
}

async fn proxy(State(state): State<UiState>, request: Request) -> Response {
    let (mut parts, body) = request.into_parts();
    // Never join a caller-supplied URL: only replace the fixed origin's path/query.
    let mut url = state.api_origin.clone();
    url.set_path(parts.uri.path());
    url.set_query(parts.uri.query());
    // URL parsers normalize dot segments; do not let them escape the allowed routes.
    if url.path() != parts.uri.path() {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let body = match to_bytes(body, REQUEST_LIMIT).await {
        Ok(body) => body,
        Err(_) => return StatusCode::PAYLOAD_TOO_LARGE.into_response(),
    };
    strip_hop_headers(&mut parts.headers);
    parts.headers.remove(header::HOST);
    parts.headers.remove(header::CONTENT_LENGTH);
    let upstream = state
        .client
        .request(parts.method, url)
        .headers(parts.headers)
        .body(body)
        .send()
        .await;
    let Ok(mut upstream) = upstream else {
        return bad_gateway();
    };
    if upstream
        .content_length()
        .is_some_and(|size| size > RESPONSE_LIMIT as u64)
    {
        return bad_gateway();
    }
    let status = upstream.status();
    let mut headers = upstream.headers().clone();
    strip_hop_headers(&mut headers);
    let mut body = Vec::new();
    loop {
        match upstream.chunk().await {
            Ok(Some(chunk)) if chunk.len() <= RESPONSE_LIMIT - body.len() => {
                body.extend_from_slice(&chunk);
            }
            Ok(None) => break,
            _ => return bad_gateway(),
        }
    }
    let mut response = Response::new(Body::from(body));
    *response.status_mut() = status;
    // Assign the entire map rather than inserting individually: preserve all Set-Cookie values.
    *response.headers_mut() = headers;
    response
}

fn bad_gateway() -> Response {
    (StatusCode::BAD_GATEWAY, "API unavailable").into_response()
}

pub(super) async fn openapi() -> impl IntoResponse {
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

#[cfg(test)]
#[path = "tests/ui.rs"]
mod tests;
