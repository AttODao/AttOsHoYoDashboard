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
        ("/assets/app.js", "text/javascript"),
        ("/assets/style.css", "text/css"),
        ("/assets/app.wasm", "application/wasm"),
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
        if content_type == "text/html" {
            assert_eq!(
                response.headers()[header::CONTENT_SECURITY_POLICY],
                "default-src 'self'; script-src 'self' 'wasm-unsafe-eval'; style-src 'self'; connect-src 'self'; object-src 'none'; frame-ancestors 'none'; base-uri 'none'"
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
    ] {
        let response = call(&app, request("GET", path, None, None)).await;
        assert!(!response.status().is_success());
        let bytes = to_bytes(response.into_body(), 128 * 1024).await.unwrap();
        assert!(!String::from_utf8_lossy(&bytes).contains("synthetic-private-marker"));
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
    for path in ["/", "/docs"] {
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
