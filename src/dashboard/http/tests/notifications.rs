//! App installations register, read their inbox and unregister over the session cookie.
use super::*;
use crate::dashboard::test_support::TestDatabase;

#[tokio::test]
async fn app_devices_register_read_the_inbox_and_unregister_with_a_session() {
    let db = TestDatabase::new().await;
    let (accounts, admin) = db.ready_admin().await;
    let app = app(accounts);
    let session = cookie(&app, admin.user.id).await;
    let base = "/api/v1/me/notifications/app-devices";

    // Unsafe methods need the Origin, like every other cookie route.
    let mut missing_origin = request("POST", base, Some(&session), Some(json!({})));
    missing_origin.headers_mut().remove(header::ORIGIN);
    error(
        call(&app, missing_origin).await,
        StatusCode::FORBIDDEN,
        "forbidden",
    )
    .await;
    error(
        call(&app, request("POST", base, None, Some(json!({})))).await,
        StatusCode::UNAUTHORIZED,
        "unauthorized",
    )
    .await;

    let response = call(&app, request("POST", base, Some(&session), Some(json!({})))).await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let device = body(response).await;
    assert_eq!(device["cursor"], 0);
    let id = device["id"].as_str().unwrap().to_owned();

    // The test send queues the notice for the app.
    let response = call(
        &app,
        request(
            "POST",
            "/api/v1/me/notifications/test",
            Some(&session),
            Some(json!({})),
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        body(response).await,
        json!({"webhook": "not_configured", "push": 0, "app": true})
    );

    // A background poll is a plain GET with the cookie (no Origin needed).
    let mut poll = request(
        "GET",
        &format!("{base}/{id}/inbox?after=0"),
        Some(&session),
        None,
    );
    poll.headers_mut().remove(header::ORIGIN);
    let response = call(&app, poll).await;
    assert_eq!(response.status(), StatusCode::OK);
    let inbox = body(response).await;
    assert_eq!(inbox["more"], false);
    let items = inbox["items"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["kind"], "test");
    assert_eq!(items[0]["title"], "テスト通知");
    assert_eq!(items[0]["url"], "/settings/notifications");
    assert_eq!(inbox["cursor"], items[0]["id"]);
    let response = call(
        &app,
        request(
            "GET",
            &format!("{base}/{id}/inbox?after={}", inbox["cursor"]),
            Some(&session),
            None,
        ),
    )
    .await;
    assert_eq!(body(response).await["items"], json!([]));
    let response = call(
        &app,
        request(
            "GET",
            &format!("{base}/{id}/inbox?after=0&x=1"),
            Some(&session),
            None,
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let response = call(
        &app,
        request("DELETE", &format!("{base}/{id}"), Some(&session), None),
    )
    .await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    error(
        call(
            &app,
            request("GET", &format!("{base}/{id}/inbox"), Some(&session), None),
        )
        .await,
        StatusCode::NOT_FOUND,
        "not_found",
    )
    .await;
}
