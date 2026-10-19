//! Combined-router static boundaries use only a synthetic local bundle.
use super::*;

pub(super) struct Bundle(std::path::PathBuf);
impl Drop for Bundle {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

pub(super) async fn fixture() -> (Bundle, App) {
    let bundle = Bundle(std::env::temp_dir().join(format!("attos-ui-{}", Uuid::new_v4())));
    std::fs::create_dir_all(bundle.0.join("assets")).unwrap();
    for (path, content) in [
        (
            "index.html",
            "<!doctype html><script type=\"module\" src=\"/assets/app.js\"></script>".as_bytes(),
        ),
        ("assets/app.js", b"export function boot() {}"),
        ("assets/style.css", b"body{color:black}"),
        ("assets/app.wasm", b"\0asm\x01\0\0\0"),
        (
            "assets/app-dxh2bdb9778a3bf3b39.js",
            b"export function boot() {}",
        ),
        ("assets/app_bg-dxh1ccc56b941a84aa.wasm", b"\0asm\x01\0\0\0"),
        ("assets/font-293812b054e0.woff2", b"wOF2synthetic"),
        ("private.txt", b"synthetic-private-marker"),
    ] {
        std::fs::write(bundle.0.join(path), content).unwrap();
    }
    let mut app = disconnected_app().await;
    Arc::get_mut(&mut app).unwrap().config.ui_dir = bundle.0.clone();
    (bundle, app)
}

#[tokio::test]
async fn compiled_assets_are_public_local_and_hardened() {
    let (_bundle, app) = fixture().await;
    for (path, content_type) in [
        ("/", "text/html"),
        ("/docs", "text/html"),
        ("/login", "text/html"),
        ("/dashboard", "text/html"),
        ("/settings/games", "text/html"),
        ("/settings/bonus", "text/html"),
        ("/settings/link", "text/html"),
        ("/settings/security", "text/html"),
        ("/settings/delete", "text/html"),
        ("/admin", "text/html"),
        ("/assets/app.js", "text/javascript"),
        ("/assets/style.css", "text/css"),
        ("/assets/app.wasm", "application/wasm"),
        ("/assets/app-dxh2bdb9778a3bf3b39.js", "text/javascript"),
        ("/assets/app_bg-dxh1ccc56b941a84aa.wasm", "application/wasm"),
        ("/assets/font-293812b054e0.woff2", "font/woff2"),
        ("/openapi.json", "application/json"),
    ] {
        // Public GETs need neither an Origin nor a session cookie.
        let request = Request::builder().uri(path).body(Body::empty()).unwrap();
        let response = call(&app, request).await;
        assert_eq!(response.status(), StatusCode::OK);
        assert!(
            response.headers()[header::CONTENT_TYPE]
                .to_str()
                .unwrap()
                .starts_with(content_type)
        );
        // Only content-fingerprinted asset names are cached for good; others revalidate.
        let cache = if !path.starts_with("/assets/") {
            "no-store"
        } else if path.contains("-dxh") || path.contains("-293812b054e0") {
            "public, max-age=31536000, immutable"
        } else {
            "no-cache"
        };
        assert_eq!(response.headers()[header::CACHE_CONTROL], cache, "{path}");
        if content_type == "text/html" {
            assert_eq!(
                response.headers()[header::CONTENT_SECURITY_POLICY],
                "default-src 'self'; script-src 'self' 'wasm-unsafe-eval'; style-src 'self'; img-src 'self' https://upload-os-bbs.hoyolab.com https://act-upload.hoyolab.com https://act-webstatic.hoyoverse.com; connect-src 'self'; object-src 'none'; frame-ancestors 'none'; base-uri 'none'"
            );
        }
        assert!(
            !to_bytes(response.into_body(), 128 * 1024)
                .await
                .unwrap()
                .is_empty()
        );
    }
    for path in [
        "/assets/../private.txt",
        "/assets/%2e%2e/private.txt",
        "/assets/%2e%2e%2fprivate.txt",
        "/private.txt",
        "/index.html",
        "/assets/missing.js",
        "/src/main.rs",
        "/unknown",
        "/settings/unknown",
        "/dashboard/unknown",
        "/api/v1/unknown",
        "/api/unknown",
    ] {
        let response = call(&app, request("GET", path, None, None)).await;
        assert!(!response.status().is_success());
        let bytes = to_bytes(response.into_body(), 128 * 1024).await.unwrap();
        assert!(!String::from_utf8_lossy(&bytes).contains("synthetic-private-marker"));
    }
}

#[tokio::test]
async fn settings_redirects_and_api_routes_never_fall_back_to_spa() {
    let (_bundle, app) = fixture().await;
    let response = call(&app, request("GET", "/settings", None, None)).await;
    assert_eq!(response.status(), StatusCode::PERMANENT_REDIRECT);
    assert_eq!(response.headers()[header::LOCATION], "/settings/games");
    error(
        call(&app, request("GET", "/api/v1/me", None, None)).await,
        StatusCode::UNAUTHORIZED,
        "unauthorized",
    )
    .await;
    for path in ["/unknown", "/settings/unknown", "/api/v1/unknown"] {
        let response = call(&app, request("GET", path, None, None)).await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        assert_ne!(
            response
                .headers()
                .get(header::CONTENT_TYPE)
                .map(|v| v.as_bytes()),
            Some(b"text/html; charset=utf-8".as_slice())
        );
    }
}

#[tokio::test]
async fn server_rejects_missing_index_before_binding_or_starting_worker() {
    let app = Arc::try_unwrap(disconnected_app().await).ok().unwrap();
    assert!(matches!(
        super::super::serve(app.accounts, app.config).await,
        Err(crate::dashboard::error::Error::Unavailable(
            "build the Dioxus UI before starting the server"
        ))
    ));
}

#[tokio::test]
async fn missing_bundle_returns_hardened_unavailable_without_breaking_api_routes() {
    let app = disconnected_app().await;
    for path in [
        "/",
        "/docs",
        "/login",
        "/dashboard",
        "/settings/security",
        "/admin",
    ] {
        error(
            call(&app, request("GET", path, None, None)).await,
            StatusCode::SERVICE_UNAVAILABLE,
            "unavailable",
        )
        .await;
    }
    error(
        call(&app, request("GET", "/api/v1/me", None, None)).await,
        StatusCode::UNAUTHORIZED,
        "unauthorized",
    )
    .await;
}

#[test]
fn dx_and_font_fingerprints_are_recognized_from_real_bundle_names() {
    use super::super::ui::fingerprinted;
    for name in [
        "attos-dashboard-ui-dxh2bdb9778a3bf3b39.js",
        "attos-dashboard-ui_bg-dxhddaf8fab04e538.wasm",
        "dashboard-dxh1ccc56b941a84aa.css",
        "favicon-dxh35b6a291bc41547.png",
        "LICENSE-MIT-dxh882daef2a2aa2d7a",
        "noto-sans-jp-OFL-dxh9f25b8d1c04b62b2.txt",
        "noto-sans-jp-293812b054e0.woff2",
        "orbitron-4354932df8f5.woff2",
    ] {
        assert!(fingerprinted(&format!("/assets/{name}")), "{name}");
    }
    for name in [
        "app.js",
        "noto-sans-jp-OFL.txt",
        "focus-trap.js",
        "app-dxh.js",
        "site-cafe.css",
    ] {
        assert!(!fingerprinted(&format!("/assets/{name}")), "{name}");
    }
}
