//! DB-independent static-serving and real loopback proxy boundary tests.
use super::*;
use axum::http::{Method, Uri};
use std::sync::{Arc, Mutex};
use tower::ServiceExt;

struct Bundle(PathBuf);
impl Bundle {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("attos-ui-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(root.join("public/assets")).unwrap();
        std::fs::write(root.join("public/index.html"), "<html>synthetic UI</html>").unwrap();
        std::fs::write(root.join("public/assets/app.js"), "// synthetic asset").unwrap();
        std::fs::write(root.join("public/source.rs"), "synthetic source").unwrap();
        std::fs::write(root.join("synthetic-secret"), "synthetic secret").unwrap();
        Self(root)
    }

    fn config(&self, api_origin: reqwest::Url) -> DashboardConfig {
        DashboardConfig {
            listen: "127.0.0.1:0".parse().unwrap(),
            api_origin,
            ui_dir: self.0.join("public"),
        }
    }
}
impl Drop for Bundle {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}

struct Mock {
    origin: reqwest::Url,
    task: tokio::task::JoinHandle<()>,
}
impl Mock {
    async fn start(router: Router) -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap())
            .parse()
            .unwrap();
        let task = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        Self { origin, task }
    }
}
impl Drop for Mock {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn body(response: Response) -> String {
    String::from_utf8(
        to_bytes(response.into_body(), RESPONSE_LIMIT + 1)
            .await
            .unwrap()
            .to_vec(),
    )
    .unwrap()
}

fn request(path: &str) -> Request {
    Request::builder().uri(path).body(Body::empty()).unwrap()
}

#[tokio::test]
async fn static_routes_are_scoped_and_keep_csp() {
    let bundle = Bundle::new();
    let router = router(bundle.config("http://127.0.0.1:1".parse().unwrap()))
        .await
        .unwrap();
    for path in ["/", "/docs"] {
        let response = router.clone().oneshot(request(path)).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()[header::CONTENT_SECURITY_POLICY], CSP);
        assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
        assert_eq!(response.headers()["x-content-type-options"], "nosniff");
        assert_eq!(body(response).await, "<html>synthetic UI</html>");
    }
    let response = router
        .clone()
        .oneshot(request("/assets/app.js"))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
    assert_eq!(response.headers()["x-content-type-options"], "nosniff");
    assert_eq!(body(response).await, "// synthetic asset");
    for path in [
        "/source.rs",
        "/synthetic-secret",
        "/public/index.html",
        "/assets/../source.rs",
        "/assets/%2e%2e/source.rs",
        "/assets/%2e%2e/%2e%2e/synthetic-secret",
        "/api/v2",
        "/api/v10",
        "/health/extra",
        "/openapi.json/extra",
    ] {
        let response = router.clone().oneshot(request(path)).await.unwrap();
        assert!(!response.status().is_success(), "unexpected access: {path}");
        let text = body(response).await;
        assert!(!text.contains("synthetic source") && !text.contains("synthetic secret"));
    }
}

#[tokio::test]
async fn missing_bundle_fails_without_a_database() {
    let bundle = Bundle::new();
    std::fs::remove_file(bundle.0.join("public/index.html")).unwrap();
    assert!(
        router(bundle.config("http://127.0.0.1:1".parse().unwrap()))
            .await
            .is_err()
    );
}

type Captured = (Method, Uri, HeaderMap, Vec<u8>);

#[tokio::test]
async fn proxy_preserves_request_and_response_without_hop_headers() {
    let captured: Arc<Mutex<Option<Captured>>> = Arc::default();
    let record = captured.clone();
    let mock = Mock::start(Router::new().fallback(any(move |request: Request| {
        let record = record.clone();
        async move {
            let (parts, body) = request.into_parts();
            let bytes = to_bytes(body, REQUEST_LIMIT).await.unwrap();
            *record.lock().unwrap() =
                Some((parts.method, parts.uri, parts.headers, bytes.to_vec()));
            let mut response = (StatusCode::CREATED, "synthetic reply").into_response();
            response.headers_mut().append(
                header::SET_COOKIE,
                "first=synthetic; HttpOnly; Path=/".parse().unwrap(),
            );
            response.headers_mut().append(
                header::SET_COOKIE,
                "second=synthetic; SameSite=Strict; Path=/".parse().unwrap(),
            );
            response
                .headers_mut()
                .insert(header::CONNECTION, "x-response-hop".parse().unwrap());
            response
                .headers_mut()
                .insert("x-response-hop", "remove".parse().unwrap());
            response
        }
    })))
    .await;
    let bundle = Bundle::new();
    let router = router(bundle.config(mock.origin.clone())).await.unwrap();
    let response = router
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/auth/login?next=%2Fdocs&n=1&n=2")
                .header(header::HOST, "untrusted.invalid")
                .header(header::ORIGIN, "http://localhost:8080")
                .header(header::COOKIE, "session=synthetic")
                .header(header::CONNECTION, "x-request-hop, keep-alive")
                .header("x-request-hop", "remove")
                .header("proxy-authorization", "synthetic")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from("{\"synthetic\":true}"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    let cookies: Vec<_> = response
        .headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .map(|value| value.to_str().unwrap())
        .collect();
    assert_eq!(
        cookies,
        [
            "first=synthetic; HttpOnly; Path=/",
            "second=synthetic; SameSite=Strict; Path=/"
        ]
    );
    assert!(!response.headers().contains_key(header::CONNECTION));
    assert!(!response.headers().contains_key("x-response-hop"));
    assert_eq!(body(response).await, "synthetic reply");
    let captured = captured.lock().unwrap();
    let (method, uri, headers, bytes) = captured.as_ref().unwrap();
    assert_eq!(method, Method::POST);
    assert_eq!(uri.to_string(), "/api/v1/auth/login?next=%2Fdocs&n=1&n=2");
    assert_eq!(headers[header::ORIGIN], "http://localhost:8080");
    assert_eq!(headers[header::COOKIE], "session=synthetic");
    assert_ne!(headers[header::HOST], "untrusted.invalid");
    assert!(!headers.contains_key("x-request-hop"));
    assert!(!headers.contains_key("proxy-authorization"));
    assert_eq!(bytes, b"{\"synthetic\":true}");
}

#[tokio::test]
async fn proxy_routes_bounds_and_redirects() {
    let mock = Mock::start(Router::new().fallback(any(|request: Request| async move {
        match request.uri().path() {
            "/health" => (
                StatusCode::TEMPORARY_REDIRECT,
                [(header::LOCATION, "http://127.0.0.1:1/private")],
                "redirect",
            )
                .into_response(),
            "/openapi.json" => "synthetic schema".into_response(),
            "/api/v1/server-settings" => "synthetic settings".into_response(),
            _ => vec![b'x'; RESPONSE_LIMIT + 1].into_response(),
        }
    })))
    .await;
    let bundle = Bundle::new();
    let router = router(bundle.config(mock.origin.clone())).await.unwrap();
    for (path, expected) in [
        ("/api/v1/server-settings", "synthetic settings"),
        ("/openapi.json", "synthetic schema"),
    ] {
        assert_eq!(
            body(router.clone().oneshot(request(path)).await.unwrap()).await,
            expected
        );
    }
    let response = router.clone().oneshot(request("/health")).await.unwrap();
    assert_eq!(response.status(), StatusCode::TEMPORARY_REDIRECT);
    assert_eq!(
        response.headers()[header::LOCATION],
        "http://127.0.0.1:1/private"
    );
    let response = router
        .clone()
        .oneshot(request("/api/v1/users"))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
    assert_eq!(body(response).await, "API unavailable");
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/auth/login")
                .body(Body::from(vec![b'x'; REQUEST_LIMIT + 1]))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
    let response = router
        .oneshot(request("/api/v1/%2e%2e/%2e%2e/private"))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn chunked_response_is_bounded_without_content_length() {
    use std::io::{Read, Write};

    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap())
        .parse()
        .unwrap();
    let upstream = tokio::task::spawn_blocking(move || {
        let (mut socket, _) = listener.accept().unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        socket
            .set_write_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut request = Vec::new();
        let mut byte = [0];
        while !request.ends_with(b"\r\n\r\n") {
            socket.read_exact(&mut byte).unwrap();
            request.push(byte[0]);
        }
        socket
            .write_all(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n")
            .unwrap();
        let bytes = vec![b'x'; RESPONSE_LIMIT + 1];
        // A disconnect is expected if the proxy detects the limit before this finishes.
        let _ = write!(socket, "{:x}\r\n", bytes.len())
            .and_then(|()| socket.write_all(&bytes))
            .and_then(|()| socket.write_all(b"\r\n0\r\n\r\n"));
    });
    let bundle = Bundle::new();
    let router = router(bundle.config(origin)).await.unwrap();
    let response = router.oneshot(request("/api/v1/users")).await.unwrap();
    assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
    assert_eq!(body(response).await, "API unavailable");
    upstream.await.unwrap();
}

#[tokio::test]
async fn upstream_failure_is_sanitized_and_api_schema_remains_static() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap())
        .parse()
        .unwrap();
    drop(listener);
    let bundle = Bundle::new();
    let router = router(bundle.config(origin)).await.unwrap();
    let response = router.oneshot(request("/health")).await.unwrap();
    assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
    assert_eq!(body(response).await, "API unavailable");
    let response = openapi().await.into_response();
    assert_eq!(
        response.headers()[header::CONTENT_TYPE],
        "application/json; charset=utf-8"
    );
    let schema: serde_json::Value = serde_json::from_str(&body(response).await).unwrap();
    assert!(schema.get("openapi").is_some());
}
