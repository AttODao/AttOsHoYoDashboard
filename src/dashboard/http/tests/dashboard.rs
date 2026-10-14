//! Actual mounted endpoints and setup requirements, without any upstream request.
use super::*;
use crate::dashboard::{
    db::entities::{deletion_request, user},
    test_support::{PASSWORD, TestDatabase},
};
use sea_orm::EntityTrait;
#[tokio::test]
async fn compiled_dioxus_assets_are_public_local_and_hardened() {
    struct Bundle(std::path::PathBuf);
    impl Drop for Bundle {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    // Static-serving fixture, not a substitute for compiling/running the real UI.
    let bundle = Bundle(std::env::temp_dir().join(format!("attos-ui-{}", Uuid::new_v4())));
    std::fs::create_dir_all(bundle.0.join("assets")).unwrap();
    std::fs::write(
        bundle.0.join("index.html"),
        "<!doctype html><div id=\"main\"></div><script type=\"module\" src=\"/assets/app.js\"></script>",
    )
    .unwrap();
    std::fs::write(bundle.0.join("assets/app.js"), "export function boot() {}").unwrap();
    std::fs::write(bundle.0.join("assets/style.css"), "body{color:black}").unwrap();
    std::fs::write(bundle.0.join("assets/app.wasm"), b"\0asm\x01\0\0\0").unwrap();
    std::fs::write(bundle.0.join("private.txt"), "synthetic-private-marker").unwrap();
    let mut app = disconnected_app().await;
    Arc::get_mut(&mut app).unwrap().config.ui_dir = bundle.0.clone();
    for (path, content_type) in [
        ("/", "text/html"),
        ("/docs", "text/html"),
        ("/assets/app.js", "text/javascript"),
        ("/assets/style.css", "text/css"),
        ("/assets/app.wasm", "application/wasm"),
        ("/openapi.json", "application/json"),
    ] {
        let response = call(&app, request("GET", path, None, None)).await;
        assert_eq!(response.status(), StatusCode::OK);
        assert!(
            response.headers()[header::CONTENT_TYPE]
                .to_str()
                .unwrap()
                .starts_with(content_type)
        );
        if content_type == "text/html" {
            let csp = response.headers()["content-security-policy"]
                .to_str()
                .unwrap();
            assert!(csp.contains("script-src 'self' 'wasm-unsafe-eval';"));
            assert!(csp.contains("frame-ancestors 'none'"));
            assert!(!csp.contains("'unsafe-eval'"));
            assert!(!csp.contains("'unsafe-inline'"));
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
        "/private.txt",
        "/assets/docs.js",
    ] {
        let response = call(&app, request("GET", path, None, None)).await;
        assert!(!response.status().is_success());
        let bytes = to_bytes(response.into_body(), 128 * 1024).await.unwrap();
        assert!(!String::from_utf8_lossy(&bytes).contains("synthetic-private-marker"));
    }
}
#[tokio::test]
async fn current_game_reward_verify_routes_require_authentication_and_verified_setup() {
    let fixture = TestDatabase::new().await;
    let (accounts, actor) = fixture.ready_admin().await;
    let app = app(accounts);
    let cookie = cookie(&app, actor.user.id).await;
    let id = Uuid::new_v4();
    for (method, path, input) in [
        ("GET", "/api/v1/game-accounts".into(), None),
        ("GET", format!("/api/v1/game-accounts/{id}/status"), None),
        (
            "PATCH",
            format!("/api/v1/game-accounts/{id}"),
            Some(json!({"visible":true})),
        ),
        ("GET", "/api/v1/reward-settings".into(), None),
        (
            "PATCH",
            format!("/api/v1/reward-settings/{id}"),
            Some(json!({"enabled":true,"local_time":"06:00"})),
        ),
        (
            "POST",
            format!("/api/v1/reward-settings/{id}/run"),
            Some(json!({})),
        ),
        ("GET", "/api/v1/reward-runs?limit=100".into(), None),
        ("POST", "/api/v1/me/hoyolab/verify".into(), Some(json!({}))),
        ("POST", "/api/v1/me/hoyolab/sync".into(), Some(json!({}))),
    ] {
        error(
            call(&app, request(method, &path, None, input.clone())).await,
            StatusCode::UNAUTHORIZED,
            "unauthorized",
        )
        .await;
        error(
            call(&app, request(method, &path, Some(&cookie), input)).await,
            StatusCode::FORBIDDEN,
            "initial_setup_required",
        )
        .await;
    }
    let me = body(call(&app, request("GET", "/api/v1/me", Some(&cookie), None)).await).await;
    assert_eq!(me["onboarding"]["cookie_setup_complete"], false);
    assert_eq!(me["onboarding"]["verification_available"], true);
    assert_eq!(me["capabilities"]["reward_posting_available"], true);
}
#[tokio::test]
async fn delete_endpoint_queues_owner_deletion_revokes_session_and_rejects_last_admin() {
    let fixture = TestDatabase::new().await;
    let (accounts, admin) = fixture.ready_admin().await;
    let target = accounts
        .create_user(&admin, "delete-http-user".into(), PASSWORD.into())
        .await
        .unwrap();
    let (_, issued) = accounts
        .login(target.username.clone(), PASSWORD.into())
        .await
        .unwrap();
    let actor = Sessions::authenticate(
        &accounts.db,
        &crate::dashboard::auth::password::token_hash(&issued.token).unwrap(),
    )
    .await
    .unwrap();
    accounts
        .change_password(&actor, PASSWORD.into(), "changed-http-password-123".into())
        .await
        .unwrap();
    let app = app(accounts);
    let cookie = cookie(&app, admin.user.id).await;
    error(
        call(
            &app,
            request(
                "DELETE",
                "/api/v1/me",
                Some(&cookie),
                Some(json!({"current_password":PASSWORD,"confirmation":admin.user.username})),
            ),
        )
        .await,
        StatusCode::BAD_REQUEST,
        "invalid_input",
    )
    .await;
    let response = call(
        &app,
        request(
            "DELETE",
            &format!("/api/v1/admin/users/{}", target.id),
            Some(&cookie),
            Some(json!({"current_password":PASSWORD,"confirmation":target.username})),
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::ACCEPTED);
    assert!(
        deletion_request::Entity::find_by_id(target.id)
            .one(&fixture.db)
            .await
            .unwrap()
            .is_some()
    );
    assert!(
        !user::Entity::find_by_id(target.id)
            .one(&fixture.db)
            .await
            .unwrap()
            .unwrap()
            .active
    );
    app.accounts.finish_deletion(target.id).await.unwrap();
    assert!(
        user::Entity::find_by_id(target.id)
            .one(&fixture.db)
            .await
            .unwrap()
            .is_none()
    );
}
