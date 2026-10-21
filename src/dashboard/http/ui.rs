//! Serve the compiled Dioxus browser bundle; never expose the build/source directory.
use super::{App, Error, Result};
use axum::{
    Router,
    extract::{Request, State},
    http::{HeaderValue, StatusCode, header},
    middleware::{Next, from_fn},
    response::{IntoResponse, Redirect, Response},
    routing::get,
};
use tower_http::services::ServeDir;

const OPENAPI: &str = include_str!("../../../docs/dashboard-api.json");
// DX emits an external loader. Only expedition and check-in reward images may use the official image CDNs;
// scripts, styles and API connections remain same-origin.
const CSP: &str = "default-src 'self'; script-src 'self' 'wasm-unsafe-eval'; style-src 'self'; img-src 'self' https://upload-os-bbs.hoyolab.com https://act-upload.hoyolab.com https://act-webstatic.hoyoverse.com https://fastcdn.hoyoverse.com; connect-src 'self'; object-src 'none'; frame-ancestors 'none'; base-uri 'none'";

const IMMUTABLE: &str = "public, max-age=31536000, immutable";
/// Install metadata for adding the dashboard to a home screen; the icon is an unhashed asset.
const MANIFEST: &str = r##"{"name":"AHD","short_name":"AHD","description":"HoYoLAB dashboard","lang":"ja","start_url":"/dashboard","scope":"/","display":"standalone","background_color":"#0C0B0E","theme_color":"#0C0B0E","icons":[{"src":"/assets/dashboard-icon.png","sizes":"256x256","type":"image/png","purpose":"any"}]}"##;

pub(super) fn routes(app: &App) -> Router<App> {
    let assets = Router::new()
        .nest_service(
            "/assets",
            ServeDir::new(app.config.ui_dir.join("assets"))
                .precompressed_br()
                .precompressed_gzip(),
        )
        .layer(from_fn(asset_cache));
    Router::new()
        .route("/", get(index))
        .route("/docs", get(index))
        .route("/login", get(index))
        .route("/dashboard", get(index))
        .route(
            "/settings",
            get(|| async { Redirect::permanent("/settings/games") }),
        )
        .route("/settings/games", get(index))
        .route("/settings/bonus", get(index))
        .route("/settings/link", get(index))
        .route("/settings/security", get(index))
        .route("/settings/delete", get(index))
        .route("/admin", get(index))
        .route("/manifest.webmanifest", get(manifest))
        .merge(assets)
}

/// Content-fingerprinted assets are cached forever; anything else revalidates.
/// HTML, API and error responses keep the boundary's `no-store`.
async fn asset_cache(request: Request, next: Next) -> Response {
    let path = request.uri().path().to_owned();
    let fingerprinted = fingerprinted(&path);
    let mut response = next.run(request).await;
    if response.status().is_success() || response.status() == StatusCode::NOT_MODIFIED {
        let policy = if fingerprinted { IMMUTABLE } else { "no-cache" };
        response
            .headers_mut()
            .insert(header::CACHE_CONTROL, HeaderValue::from_static(policy));
        // The Web Push service worker lives under /assets but must control the whole
        // dashboard; browsers only accept that broader scope when this header allows it.
        if path == "/assets/push-sw.js" {
            response
                .headers_mut()
                .insert("service-worker-allowed", HeaderValue::from_static("/"));
        }
    }
    response
}

/// DX appends `-dxh<hex>`; the self-hosted fonts keep a `-<sha256 prefix>` name instead.
pub(super) fn fingerprinted(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or_default();
    let stem = name.split('.').next().unwrap_or_default();
    let Some((_, suffix)) = stem.rsplit_once('-') else {
        return false;
    };
    let hex = |value: &str, min: usize| {
        value.len() >= min && value.bytes().all(|byte| byte.is_ascii_hexdigit())
    };
    suffix.strip_prefix("dxh").is_some_and(|hash| hex(hash, 8)) || hex(suffix, 12)
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

async fn manifest() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "application/manifest+json")],
        MANIFEST,
    )
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
