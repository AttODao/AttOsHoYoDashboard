use super::*;
use crate::dashboard::test_support::TestDatabase;

#[tokio::test]
async fn protected_passkey_routes_require_sessions_and_ready_users() {
    let db = TestDatabase::new().await;
    let (accounts, actor) = db.ready_admin().await;
    let app = app(accounts);
    let credential = json!({
        "id": "AQ", "rawId": "AQ", "type": "public-key", "extensions": {},
        "response": {"attestationObject": "AQ", "clientDataJSON": "AQ"}
    });
    let id = Uuid::new_v4();
    let routes = [
        ("GET", "/api/v1/me/passkeys".to_owned(), None),
        (
            "POST",
            "/api/v1/me/passkeys/options".to_owned(),
            Some(json!({"label": "synthetic"})),
        ),
        (
            "POST",
            "/api/v1/me/passkeys/verify".to_owned(),
            Some(json!({"challenge_id": id, "response": credential})),
        ),
        ("DELETE", format!("/api/v1/me/passkeys/{id}"), None),
    ];
    for (method, path, input) in routes {
        error(
            call(&app, request(method, &path, None, input)).await,
            StatusCode::UNAUTHORIZED,
            "unauthorized",
        )
        .await;
    }
    let cookie = cookie(&app, actor.user.id).await;
    let listing = call(
        &app,
        request("GET", "/api/v1/me/passkeys", Some(&cookie), None),
    )
    .await;
    assert_eq!(listing.status(), StatusCode::OK);
    assert_eq!(body(listing).await, json!([]));
    let options = call(
        &app,
        request(
            "POST",
            "/api/v1/me/passkeys/options",
            Some(&cookie),
            Some(json!({"label": "synthetic"})),
        ),
    )
    .await;
    assert_eq!(options.status(), StatusCode::OK);
    let options = body(options).await;
    assert!(Uuid::parse_str(options["challenge_id"].as_str().unwrap()).is_ok());
    assert_eq!(
        options["options"]["publicKey"]["rp"]["id"],
        "dashboard.example.test"
    );
    assert_eq!(options["options"]["publicKey"]["user"]["name"], "admin");
    assert!(
        options["options"]["publicKey"]["challenge"]
            .as_str()
            .is_some_and(|s| !s.is_empty())
    );
    error(
        call(
            &app,
            request(
                "DELETE",
                &format!("/api/v1/me/passkeys/{id}"),
                Some(&cookie),
                None,
            ),
        )
        .await,
        StatusCode::NOT_FOUND,
        "not_found",
    )
    .await;
    assert_eq!(
        call(
            &app,
            request(
                "DELETE",
                "/api/v1/me/passkeys/not-a-uuid",
                Some(&cookie),
                None
            )
        )
        .await
        .status(),
        StatusCode::BAD_REQUEST
    );
    // Keep this real session and issued challenge, but change readiness before completion.
    // An unknown challenge would return 401 before the readiness check and prove nothing.
    use crate::dashboard::db::entities::user;
    use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
    user::Entity::update_many()
        .col_expr(
            user::Column::MustChangePassword,
            sea_orm::sea_query::Expr::value(true),
        )
        .filter(user::Column::Id.eq(actor.user.id))
        .exec(&db.db)
        .await
        .unwrap();
    let credential = json!({
        "id": "AQ", "rawId": "AQ", "type": "public-key", "extensions": {},
        "response": {"attestationObject": "AQ", "clientDataJSON": "AQ"}
    });
    error(
        call(
            &app,
            request(
                "POST",
                "/api/v1/me/passkeys/verify",
                Some(&cookie),
                Some(json!({"challenge_id": options["challenge_id"], "response": credential})),
            ),
        )
        .await,
        StatusCode::FORBIDDEN,
        "password_change_required",
    )
    .await;
}

#[tokio::test]
async fn public_login_routes_still_require_exact_origin_before_login_budget() {
    let app = disconnected_app().await;
    for (path, input) in [
        (
            "/api/v1/auth/login",
            json!({"username": "synthetic", "password": "synthetic-password"}),
        ),
        (
            "/api/v1/auth/passkey/options",
            json!({"username": "synthetic"}),
        ),
        (
            "/api/v1/auth/passkey/verify",
            json!({"challenge_id": Uuid::new_v4(), "response": assertion_response()}),
        ),
    ] {
        let mut req = request("POST", path, None, Some(input));
        req.headers_mut().remove(header::ORIGIN);
        error(call(&app, req).await, StatusCode::FORBIDDEN, "forbidden").await;
    }
    assert_eq!(app.accounts.login_attempts("synthetic"), 0);
}

#[tokio::test]
async fn discoverable_options_accept_empty_object_but_reject_unknown_fields() {
    let app = disconnected_app().await;
    let response = call(
        &app,
        request(
            "POST",
            "/api/v1/auth/passkey/options",
            None,
            Some(json!({})),
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let options = body(response).await;
    assert!(Uuid::parse_str(options["challenge_id"].as_str().unwrap()).is_ok());
    assert!(options["options"].get("mediation").is_none());
    let response = call(
        &app,
        request(
            "POST",
            "/api/v1/auth/passkey/options",
            None,
            Some(json!({"unknown": true})),
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
}
