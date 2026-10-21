//! The published contract refers to mounted routes, never to imagined API endpoints.
use super::*;
use crate::dashboard::test_support::TestDatabase;
#[tokio::test]
async fn openapi_31_paths_methods_and_refs_match_the_live_router() {
    let fixture = TestDatabase::new().await;
    let (accounts, _) = fixture.ready_admin().await;
    let app = app(accounts);
    let response = call(&app, request("GET", "/openapi.json", None, None)).await;
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = to_bytes(response.into_body(), 1_048_576).await.unwrap();
    let spec: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(spec["openapi"], "3.1.0");
    for (template, operations) in spec["paths"].as_object().unwrap() {
        let path = template.replace("{id}", &Uuid::new_v4().to_string());
        for (method, _) in operations
            .as_object()
            .unwrap()
            .iter()
            .filter(|(method, _)| {
                matches!(method.as_str(), "get" | "post" | "put" | "patch" | "delete")
            })
        {
            let response = call(
                &app,
                request(
                    &method.to_uppercase(),
                    &path,
                    None,
                    if method == "get" {
                        None
                    } else {
                        Some(json!({}))
                    },
                ),
            )
            .await;
            assert_ne!(
                response.status(),
                StatusCode::NOT_FOUND,
                "unmounted contract route {method} {path}"
            );
            assert_ne!(
                response.status(),
                StatusCode::METHOD_NOT_ALLOWED,
                "unmounted contract method {method} {path}"
            );
        }
    }
    fn refs(value: &Value, spec: &Value) {
        match value {
            Value::Object(map) => {
                if let Some(reference) = map.get("$ref").and_then(Value::as_str)
                    && let Some(pointer) = reference.strip_prefix('#')
                {
                    assert!(
                        spec.pointer(pointer).is_some(),
                        "unresolved reference {reference}"
                    );
                }
                for child in map.values() {
                    refs(child, spec);
                }
            }
            Value::Array(items) => {
                for child in items {
                    refs(child, spec);
                }
            }
            _ => {}
        }
    }
    refs(&spec, &spec);
}
