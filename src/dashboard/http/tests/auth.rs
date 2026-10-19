use super::*;
use crate::dashboard::{
    auth::{password::token_hash, sessions::Sessions},
    test_support::{PASSWORD, TestDatabase},
};

#[tokio::test]
async fn password_and_passkey_logins_share_a_per_username_budget() {
    let db = TestDatabase::new().await;
    let (accounts, _) = db.ready_admin().await;
    let app = app(accounts);
    let inputs = |username: &str| {
        [
            (
                "/api/v1/auth/login",
                json!({"username": username, "password": PASSWORD}),
            ),
            (
                "/api/v1/auth/passkey/options",
                json!({"username": username}),
            ),
        ]
    };
    // Eighteen attempts plus one per handler use up "target"'s twenty.
    for _ in 0..18 {
        let (path, input) = inputs("target")[1].clone();
        error(
            call(&app, request("POST", path, None, Some(input))).await,
            StatusCode::UNAUTHORIZED,
            "unauthorized",
        )
        .await;
    }
    for (path, input) in inputs("target") {
        error(
            call(&app, request("POST", path, None, Some(input))).await,
            StatusCode::UNAUTHORIZED,
            "unauthorized",
        )
        .await;
    }
    for (path, input) in inputs("target") {
        error(
            call(&app, request("POST", path, None, Some(input))).await,
            StatusCode::TOO_MANY_REQUESTS,
            "rate_limited",
        )
        .await;
    }
    assert_eq!(app.accounts.login_attempts("target"), 20);
    // Another username, including the real administrator, is unaffected.
    for (path, input) in inputs("bystander") {
        error(
            call(&app, request("POST", path, None, Some(input))).await,
            StatusCode::UNAUTHORIZED,
            "unauthorized",
        )
        .await;
    }
    let admin = call(
        &app,
        request(
            "POST",
            "/api/v1/auth/login",
            None,
            Some(json!({"username": "admin", "password": PASSWORD})),
        ),
    )
    .await;
    assert_eq!(admin.status(), StatusCode::OK);
    // A random challenge has no user and spends nobody's budget.
    error(
        call(
            &app,
            request(
                "POST",
                "/api/v1/auth/passkey/verify",
                None,
                Some(json!({"challenge_id": Uuid::new_v4(), "response": assertion_response()})),
            ),
        )
        .await,
        StatusCode::UNAUTHORIZED,
        "unauthorized",
    )
    .await;
    // The fixture's own sign-in plus the one above.
    assert_eq!(app.accounts.login_attempts("admin"), 2);
}

#[tokio::test]
async fn public_password_login_rotates_cookie_without_extending_expiry_and_logout_revokes_it() {
    let db = TestDatabase::new().await;
    let (accounts, actor) = db.ready_admin().await;
    let app = app(accounts);
    let response = call(
        &app,
        request(
            "POST",
            "/api/v1/auth/login",
            None,
            Some(json!({"username": "admin", "password": PASSWORD})),
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let login_cookie = response_cookie(&response);
    assert_eq!(
        body(response).await,
        json!({"id": actor.user.id, "username": "admin", "is_admin": true, "active": true, "must_change_password": false})
    );
    assert_eq!(
        call(
            &app,
            request("GET", "/api/v1/me", Some(&login_cookie), None)
        )
        .await
        .status(),
        StatusCode::OK
    );

    let deadline = Utc::now() + Duration::seconds(120);
    let issued = Sessions::issue_until(&db.db, actor.user.id, deadline)
        .await
        .unwrap();
    let original_cookie = format!("{}={}", cookie_name(&app.config), issued.token);
    // Missing current password reaches shared business policy rather than DTO rejection.
    error(
        call(
            &app,
            request(
                "PUT",
                "/api/v1/me/password",
                Some(&original_cookie),
                Some(json!({"new_password": "another-permanent-password"})),
            ),
        )
        .await,
        StatusCode::UNAUTHORIZED,
        "unauthorized",
    )
    .await;
    let response = call(
        &app,
        request(
            "PUT",
            "/api/v1/me/password",
            Some(&original_cookie),
            Some(
                json!({"current_password": PASSWORD, "new_password": "another-permanent-password"}),
            ),
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let rotated = response_cookie(&response);
    assert_ne!(rotated, original_cookie);
    let max_age: i64 = response.headers()[header::SET_COOKIE]
        .to_str()
        .unwrap()
        .split(';')
        .map(str::trim)
        .find_map(|s| s.strip_prefix("Max-Age="))
        .unwrap()
        .parse()
        .unwrap();
    assert!((1..=120).contains(&max_age));
    let token = rotated.split_once('=').unwrap().1;
    let fresh = Sessions::authenticate(&db.db, &token_hash(token).unwrap())
        .await
        .unwrap();
    // PostgreSQL stores microsecond precision; the original deadline may have nanoseconds.
    assert!(fresh.expires_at <= deadline);
    assert!(deadline.signed_duration_since(fresh.expires_at) < Duration::milliseconds(1));
    error(
        call(
            &app,
            request("GET", "/api/v1/me", Some(&original_cookie), None),
        )
        .await,
        StatusCode::UNAUTHORIZED,
        "unauthorized",
    )
    .await;
    error(
        call(
            &app,
            request("GET", "/api/v1/me", Some(&login_cookie), None),
        )
        .await,
        StatusCode::UNAUTHORIZED,
        "unauthorized",
    )
    .await;
    let logout = call(
        &app,
        request("POST", "/api/v1/auth/logout", Some(&rotated), None),
    )
    .await;
    assert_eq!(logout.status(), StatusCode::NO_CONTENT);
    assert_eq!(
        logout.headers()[header::SET_COOKIE],
        "__Host-attos_session=; Path=/; HttpOnly; SameSite=Strict; Max-Age=0; Secure"
    );
    assert!(to_bytes(logout.into_body(), 1024).await.unwrap().is_empty());
    error(
        call(&app, request("GET", "/api/v1/me", Some(&rotated), None)).await,
        StatusCode::UNAUTHORIZED,
        "unauthorized",
    )
    .await;
}

#[tokio::test]
async fn body_limit_and_unknown_dto_fields_reject_before_handlers() {
    let db = TestDatabase::new().await;
    let (accounts, actor) = db.ready_admin().await;
    let app = app(accounts);
    let cookie = cookie(&app, actor.user.id).await;
    let id = Uuid::new_v4();
    let registration_response = json!({
        "id": "AQ", "rawId": "AQ", "type": "public-key", "extensions": {},
        "response": {"attestationObject": "AQ", "clientDataJSON": "AQ"}
    });
    serde_json::from_value::<webauthn_rs::prelude::RegisterPublicKeyCredential>(
        registration_response.clone(),
    )
    .unwrap();
    serde_json::from_value::<webauthn_rs::prelude::PublicKeyCredential>(assertion_response())
        .unwrap();
    let cases = [
        (
            "POST",
            "/api/v1/auth/login".to_owned(),
            json!({"username": "admin", "password": PASSWORD}),
        ),
        (
            "PUT",
            "/api/v1/me/password".to_owned(),
            json!({"current_password": PASSWORD, "new_password": "another-permanent-password"}),
        ),
        (
            "POST",
            "/api/v1/admin/users".to_owned(),
            json!({"username": "synthetic", "temporary_password": PASSWORD}),
        ),
        (
            "PATCH",
            format!("/api/v1/admin/users/{id}"),
            json!({"active": true}),
        ),
        (
            "PUT",
            format!("/api/v1/admin/users/{id}/temporary-password"),
            json!({"temporary_password": PASSWORD}),
        ),
        (
            "POST",
            "/api/v1/auth/passkey/options".to_owned(),
            json!({"username": "admin"}),
        ),
        (
            "POST",
            "/api/v1/auth/passkey/verify".to_owned(),
            json!({"challenge_id": id, "response": assertion_response()}),
        ),
        (
            "POST",
            "/api/v1/me/passkeys/options".to_owned(),
            json!({"label": "synthetic"}),
        ),
        (
            "POST",
            "/api/v1/me/passkeys/verify".to_owned(),
            json!({"challenge_id": id, "response": registration_response}),
        ),
    ];
    for (method, path, mut input) in cases {
        input
            .as_object_mut()
            .unwrap()
            .insert("unexpected".to_owned(), json!(true));
        let response = call(&app, request(method, &path, Some(&cookie), Some(input))).await;
        assert_eq!(
            response.status(),
            StatusCode::UNPROCESSABLE_ENTITY,
            "unknown fields must fail for {method} {path}"
        );
    }
    let oversized = json!({"username": "admin", "password": "x".repeat(16 * 1024)});
    assert_eq!(
        call(
            &app,
            request("POST", "/api/v1/auth/login", None, Some(oversized))
        )
        .await
        .status(),
        StatusCode::PAYLOAD_TOO_LARGE
    );
    let oversized = json!({"label": "x".repeat(16 * 1024)});
    assert_eq!(
        call(
            &app,
            request(
                "POST",
                "/api/v1/me/passkeys/options",
                Some(&cookie),
                Some(oversized)
            )
        )
        .await
        .status(),
        StatusCode::PAYLOAD_TOO_LARGE
    );
    // Only the fixture's own sign-in; rejected DTOs never reach the budget.
    assert_eq!(app.accounts.login_attempts("admin"), 1);
}
