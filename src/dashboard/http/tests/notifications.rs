//! App installations register, read their inbox, store a push subscription and unregister
//! over the session cookie.
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
        json!({"webhook": "not_configured", "app": true})
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

#[tokio::test]
async fn retired_browser_push_routes_are_not_found() {
    let db = TestDatabase::new().await;
    let (accounts, admin) = db.ready_admin().await;
    let app = app(accounts);
    let session = cookie(&app, admin.user.id).await;
    for method in ["POST", "DELETE"] {
        let response = call(
            &app,
            request(
                method,
                "/api/v1/me/notifications/push",
                Some(&session),
                Some(json!({})),
            ),
        )
        .await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
}

#[tokio::test]
async fn app_devices_store_a_sealed_push_subscription_only_for_their_owner() {
    use crate::dashboard::db::entities::app_device;
    use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
    let db = TestDatabase::new().await;
    let (accounts, admin) = db.ready_admin().await;
    let member = accounts
        .create_user(
            &admin,
            "member".to_owned(),
            crate::dashboard::test_support::TEMP_PASSWORD.to_owned(),
        )
        .await
        .unwrap();
    // Skip the member's mandatory password change: only device ownership is under test.
    crate::dashboard::db::entities::user::Entity::update_many()
        .col_expr(
            crate::dashboard::db::entities::user::Column::MustChangePassword,
            sea_orm::sea_query::Expr::value(false),
        )
        .filter(crate::dashboard::db::entities::user::Column::Id.eq(member.id))
        .exec(&accounts.db)
        .await
        .unwrap();
    let app = app(accounts);
    let session = cookie(&app, admin.user.id).await;
    let base = "/api/v1/me/notifications/app-devices";
    let response = call(&app, request("POST", base, Some(&session), Some(json!({})))).await;
    let id = body(response).await["id"].as_str().unwrap().to_owned();
    let push = format!("{base}/{id}/push");

    let response = call(&app, request("GET", &push, Some(&session), None)).await;
    assert_eq!(response.status(), StatusCode::OK);
    let view = body(response).await;
    assert_eq!(view["registered"], false);
    let vapid = view["vapid_public_key"].as_str().unwrap().to_owned();
    assert_eq!(vapid.len(), 87);

    // A real P-256 point (the RFC 8291 user agent key) and a 16-byte auth secret.
    let subscription = json!({
        "endpoint": "https://ntfy.example.test/upSyntheticTopic?up=1",
        "p256dh": "BCVxsr7N_eNgVRqvHtD0zTZsEc6-VV-JvLexhqUzORcxaOzi6-AYWXvTBHm4bjyPjs7Vd8pZGH6SRpkNtoIAiw4",
        "auth": "BTBZMqHH6r4Tts7J_aSIgg",
    });
    let mut missing_origin = request("PUT", &push, Some(&session), Some(subscription.clone()));
    missing_origin.headers_mut().remove(header::ORIGIN);
    error(
        call(&app, missing_origin).await,
        StatusCode::FORBIDDEN,
        "forbidden",
    )
    .await;
    error(
        call(
            &app,
            request("PUT", &push, None, Some(subscription.clone())),
        )
        .await,
        StatusCode::UNAUTHORIZED,
        "unauthorized",
    )
    .await;
    for bad in [
        json!({"endpoint": "http://ntfy.example.test/up", "p256dh": subscription["p256dh"], "auth": subscription["auth"]}),
        json!({"endpoint": "https://10.0.0.1/up", "p256dh": subscription["p256dh"], "auth": subscription["auth"]}),
        json!({"endpoint": subscription["endpoint"], "p256dh": "BAAA", "auth": subscription["auth"]}),
        json!({"endpoint": subscription["endpoint"], "p256dh": subscription["p256dh"], "auth": "AAAA"}),
    ] {
        error(
            call(&app, request("PUT", &push, Some(&session), Some(bad))).await,
            StatusCode::BAD_REQUEST,
            "invalid_input",
        )
        .await;
    }
    let response = call(
        &app,
        request("PUT", &push, Some(&session), Some(subscription.clone())),
    )
    .await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    let response = call(&app, request("GET", &push, Some(&session), None)).await;
    assert_eq!(
        body(response).await,
        json!({"vapid_public_key": vapid, "registered": true})
    );
    // Stored sealed: neither the endpoint nor the auth secret appears in the row.
    let row = app_device::Entity::find_by_id(id.parse::<Uuid>().unwrap())
        .one(&app.accounts.db)
        .await
        .unwrap()
        .unwrap();
    let sealed = row.push_subscription.unwrap();
    for plain in [&b"ntfy.example.test"[..], b"SyntheticTopic", b"BTBZMqHH"] {
        assert!(!sealed.windows(plain.len()).any(|window| window == plain));
    }

    // Another user's session cannot read, replace or remove it.
    let other = cookie(&app, member.id).await;
    for (method, payload) in [
        ("GET", None),
        ("PUT", Some(subscription.clone())),
        ("DELETE", None),
    ] {
        error(
            call(&app, request(method, &push, Some(&other), payload)).await,
            StatusCode::NOT_FOUND,
            "not_found",
        )
        .await;
    }
    let response = call(&app, request("GET", &push, Some(&session), None)).await;
    assert_eq!(body(response).await["registered"], true);

    let response = call(&app, request("DELETE", &push, Some(&session), None)).await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    let response = call(&app, request("GET", &push, Some(&session), None)).await;
    assert_eq!(body(response).await["registered"], false);

    // Removing the installation removes its subscription with it.
    call(
        &app,
        request("PUT", &push, Some(&session), Some(subscription)),
    )
    .await;
    let response = call(
        &app,
        request("DELETE", &format!("{base}/{id}"), Some(&session), None),
    )
    .await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    error(
        call(&app, request("GET", &push, Some(&session), None)).await,
        StatusCode::NOT_FOUND,
        "not_found",
    )
    .await;
}
