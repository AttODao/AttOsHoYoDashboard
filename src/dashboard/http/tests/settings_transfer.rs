//! Settings export/import over the session cookie: download headers, preview, apply.
use super::*;
use crate::dashboard::test_support::TestDatabase;

#[tokio::test]
async fn settings_export_previews_and_imports_over_the_session() {
    let db = TestDatabase::new().await;
    let (accounts, admin) = db.ready_admin().await;
    let app = app(accounts);
    let session = cookie(&app, admin.user.id).await;
    let export = "/api/v1/me/settings/export";
    let preview = "/api/v1/me/settings/import/preview";
    let import = "/api/v1/me/settings/import";

    error(
        call(&app, request("GET", export, None, None)).await,
        StatusCode::UNAUTHORIZED,
        "unauthorized",
    )
    .await;
    let response = call(&app, request("GET", export, Some(&session), None)).await;
    assert_eq!(response.status(), StatusCode::OK);
    let disposition = response.headers()[header::CONTENT_DISPOSITION]
        .to_str()
        .unwrap()
        .to_owned();
    assert!(
        disposition.starts_with("attachment; filename=\"ahd-settings-")
            && disposition.ends_with(".json\""),
        "{disposition}"
    );
    let mut file = body(response).await;
    assert_eq!(file["format"], "ahd-settings");
    assert_eq!(file["rewards"], json!([]));
    assert_eq!(file["notifications"]["webhook_configured"], false);

    // Unsafe methods need the Origin, like every other cookie route.
    let mut missing_origin = request("POST", import, Some(&session), Some(file.clone()));
    missing_origin.headers_mut().remove(header::ORIGIN);
    error(
        call(&app, missing_origin).await,
        StatusCode::FORBIDDEN,
        "forbidden",
    )
    .await;

    file["notifications"]["language"] = json!("en");
    let response = call(
        &app,
        request("POST", preview, Some(&session), Some(file.clone())),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        body(response).await,
        json!({
            "valid": true,
            "changes": [{"section": "notifications", "setting": "language", "from": "ja", "to": "en"}],
            "skipped": [],
            "webhook": {"in_file": false, "configured": false},
        })
    );

    let mut bad = file.clone();
    bad["version"] = json!(2);
    let response = call(
        &app,
        request("POST", preview, Some(&session), Some(bad.clone())),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        body(response).await,
        json!({"valid": false, "errors": [{"field": "version", "code": "unsupported"}]})
    );
    let response = call(&app, request("POST", import, Some(&session), Some(bad))).await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let rejected = body(response).await;
    assert_eq!(rejected["error"], "invalid_input");
    assert_eq!(
        rejected["errors"],
        json!([{"field": "version", "code": "unsupported"}])
    );

    let response = call(&app, request("POST", import, Some(&session), Some(file))).await;
    assert_eq!(response.status(), StatusCode::OK);
    let applied = body(response).await;
    assert_eq!(applied["valid"], true);
    assert_eq!(applied["changes"][0]["setting"], "language");
    let response = call(
        &app,
        request("GET", "/api/v1/me/notifications", Some(&session), None),
    )
    .await;
    assert_eq!(body(response).await["language"], "en");
}
