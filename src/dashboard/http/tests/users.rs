use super::*;
use crate::dashboard::test_support::{PASSWORD, TEMP_PASSWORD, TestDatabase};

#[tokio::test]
async fn user_routes_enforce_auth_admin_and_temporary_password_exceptions() {
    let db = TestDatabase::new().await;
    let (accounts, actor) = db.ready_admin().await;
    let app = app(accounts);
    let admin_cookie = cookie(&app, actor.user.id).await;
    let missing_id = Uuid::new_v4();
    let admin_routes = [
        ("GET", "/api/v1/admin/users".to_owned(), None),
        (
            "POST",
            "/api/v1/admin/users".to_owned(),
            Some(json!({"username": "ordinary", "temporary_password": TEMP_PASSWORD})),
        ),
        (
            "PATCH",
            format!("/api/v1/admin/users/{missing_id}"),
            Some(json!({"active": true})),
        ),
        (
            "PUT",
            format!("/api/v1/admin/users/{missing_id}/temporary-password"),
            Some(json!({"temporary_password": TEMP_PASSWORD})),
        ),
        (
            "DELETE",
            format!("/api/v1/admin/users/{missing_id}"),
            Some(json!({"current_password": PASSWORD, "confirmation": "missing"})),
        ),
    ];
    for (method, path, input) in &admin_routes {
        error(
            call(&app, request(method, path, None, input.clone())).await,
            StatusCode::UNAUTHORIZED,
            "unauthorized",
        )
        .await;
    }
    let created = call(
        &app,
        request(
            "POST",
            "/api/v1/admin/users",
            Some(&admin_cookie),
            Some(json!({"username": "ordinary", "temporary_password": TEMP_PASSWORD})),
        ),
    )
    .await;
    assert_eq!(created.status(), StatusCode::CREATED);
    let created = body(created).await;
    let id: Uuid = created["id"].as_str().unwrap().parse().unwrap();
    assert_eq!(
        created,
        json!({"id": id, "username": "ordinary", "is_admin": false, "active": true, "must_change_password": true})
    );
    let login = call(
        &app,
        request(
            "POST",
            "/api/v1/auth/login",
            None,
            Some(json!({"username": "ordinary", "password": TEMP_PASSWORD})),
        ),
    )
    .await;
    assert_eq!(login.status(), StatusCode::OK);
    let temporary = response_cookie(&login);
    let mut expected = created.clone();
    expected["onboarding"] = json!({"password_change_required":true,"cookie_setup_complete":false,"cookie_status":"not_configured","verification_available":true});
    expected["capabilities"] = json!({"reward_posting_available":true});
    assert_eq!(
        body(call(&app, request("GET", "/api/v1/me", Some(&temporary), None)).await).await,
        expected
    );
    for (method, path, input) in &admin_routes {
        error(
            call(&app, request(method, path, Some(&temporary), input.clone())).await,
            StatusCode::FORBIDDEN,
            "password_change_required",
        )
        .await;
    }
    for (method, path, input) in [
        ("GET", "/api/v1/server-settings".to_owned(), None),
        ("GET", "/api/v1/me/passkeys".to_owned(), None),
        (
            "POST",
            "/api/v1/me/passkeys/options".to_owned(),
            Some(json!({"label": "synthetic"})),
        ),
        ("DELETE", format!("/api/v1/me/passkeys/{missing_id}"), None),
    ] {
        error(
            call(&app, request(method, &path, Some(&temporary), input)).await,
            StatusCode::FORBIDDEN,
            "password_change_required",
        )
        .await;
    }
    // Logout and password change are explicitly allowed before ordinary-use readiness.
    assert_eq!(
        call(
            &app,
            request("POST", "/api/v1/auth/logout", Some(&temporary), None)
        )
        .await
        .status(),
        StatusCode::NO_CONTENT
    );
    error(
        call(&app, request("GET", "/api/v1/me", Some(&temporary), None)).await,
        StatusCode::UNAUTHORIZED,
        "unauthorized",
    )
    .await;
    let login = call(
        &app,
        request(
            "POST",
            "/api/v1/auth/login",
            None,
            Some(json!({"username": "ordinary", "password": TEMP_PASSWORD})),
        ),
    )
    .await;
    let temporary = response_cookie(&login);
    let changed = call(
        &app,
        request(
            "PUT",
            "/api/v1/me/password",
            Some(&temporary),
            Some(json!({"new_password": PASSWORD})),
        ),
    )
    .await;
    assert_eq!(changed.status(), StatusCode::OK);
    let ordinary = response_cookie(&changed);
    assert_eq!(body(changed).await["must_change_password"], false);
    for (method, path, input) in &admin_routes {
        error(
            call(&app, request(method, path, Some(&ordinary), input.clone())).await,
            StatusCode::FORBIDDEN,
            "forbidden",
        )
        .await;
    }
    let settings = call(
        &app,
        request("GET", "/api/v1/server-settings", Some(&ordinary), None),
    )
    .await;
    assert_eq!(settings.status(), StatusCode::OK);
    assert_eq!(
        body(settings).await,
        json!({"timezone_offset": "+09:00", "timezone": null, "session_ttl_days": 7})
    );
    let listing = call(
        &app,
        request("GET", "/api/v1/admin/users", Some(&admin_cookie), None),
    )
    .await;
    assert_eq!(listing.status(), StatusCode::OK);
    let listing = body(listing).await;
    assert_eq!(listing.as_array().unwrap().len(), 2);
    for view in listing.as_array().unwrap() {
        assert_eq!(view.as_object().unwrap().len(), 5);
        assert!(view.get("password_hash").is_none());
        assert!(view.get("created_at").is_none());
    }
    error(
        call(
            &app,
            request(
                "POST",
                "/api/v1/admin/users",
                Some(&admin_cookie),
                Some(json!({"username": "ordinary", "temporary_password": TEMP_PASSWORD})),
            ),
        )
        .await,
        StatusCode::CONFLICT,
        "conflict",
    )
    .await;
    error(
        call(
            &app,
            request(
                "PATCH",
                &format!("/api/v1/admin/users/{missing_id}"),
                Some(&admin_cookie),
                Some(json!({"active": true})),
            ),
        )
        .await,
        StatusCode::NOT_FOUND,
        "not_found",
    )
    .await;

    let reset_path = format!("/api/v1/admin/users/{id}/temporary-password");
    assert_eq!(
        call(
            &app,
            request(
                "PUT",
                &reset_path,
                Some(&admin_cookie),
                Some(json!({"temporary_password": "reset-temporary-password"}))
            )
        )
        .await
        .status(),
        StatusCode::NO_CONTENT
    );
    error(
        call(&app, request("GET", "/api/v1/me", Some(&ordinary), None)).await,
        StatusCode::UNAUTHORIZED,
        "unauthorized",
    )
    .await;
    let active_path = format!("/api/v1/admin/users/{id}");
    assert_eq!(
        call(
            &app,
            request(
                "PATCH",
                &active_path,
                Some(&admin_cookie),
                Some(json!({"active": false}))
            )
        )
        .await
        .status(),
        StatusCode::NO_CONTENT
    );
    error(
        call(
            &app,
            request(
                "POST",
                "/api/v1/auth/login",
                None,
                Some(json!({"username": "ordinary", "password": "reset-temporary-password"})),
            ),
        )
        .await,
        StatusCode::UNAUTHORIZED,
        "unauthorized",
    )
    .await;
    assert_eq!(
        call(
            &app,
            request(
                "PATCH",
                &active_path,
                Some(&admin_cookie),
                Some(json!({"active": true}))
            )
        )
        .await
        .status(),
        StatusCode::NO_CONTENT
    );
    let login = call(
        &app,
        request(
            "POST",
            "/api/v1/auth/login",
            None,
            Some(json!({"username": "ordinary", "password": "reset-temporary-password"})),
        ),
    )
    .await;
    assert_eq!(login.status(), StatusCode::OK);
    assert_eq!(body(login).await["must_change_password"], true);
}
