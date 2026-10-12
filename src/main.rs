use axum::{Json, Router, response::Html, routing::get};
use serde_json::json;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // No game API handlers existed in the Go server; keep only its documentation routes.
    let app = Router::new()
        .route("/openapi.json", get(|| async {
            Json(json!({"openapi": "3.1.0", "info": {"title": "User API", "version": "1.0.0"}, "paths": {}}))
        }))
        .route("/docs", get(|| async {
            Html(r#"<!doctype html><html lang="ja"><head><meta charset="utf-8"><title>User API</title></head><body><script id="api-reference" data-url="/openapi.json"></script><script src="https://cdn.jsdelivr.net/npm/@scalar/api-reference"></script></body></html>"#)
        }));
    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await?;
    println!("Listening on http://localhost:8080 (docs: /docs)");
    axum::serve(listener, app).await?;
    Ok(())
}
