use super::*;
use crate::dashboard::{auth::sessions::Sessions, test_support::TestDatabase};

#[tokio::test]
async fn origin_requires_one_exact_header_for_every_unsafe_method() {
    let app = disconnected_app().await;
    for method in ["POST", "PUT", "PATCH", "DELETE"] {
        for origins in [
            vec![],
            vec!["https://other.example.test"],
            vec!["null"],
            vec!["https://dashboard.example.test/"],
            vec!["https://dashboard.example.test.attacker.test"],
            vec![ORIGIN, ORIGIN],
            vec![ORIGIN, "https://other.example.test"],
            vec!["https://other.example.test", ORIGIN],
        ] {
            let mut req = Request::builder().method(method).uri("/unknown");
            for origin in origins {
                req = req.header(header::ORIGIN, origin);
            }
            error(
                call(&app, req.body(Body::empty()).unwrap()).await,
                StatusCode::FORBIDDEN,
                "forbidden",
            )
            .await;
        }
        // The same unsafe request with one exact Origin reaches the router's 404.
        assert_eq!(
            call(&app, request(method, "/unknown", None, None))
                .await
                .status(),
            StatusCode::NOT_FOUND
        );
    }
    let req = Request::builder()
        .method("POST")
        .uri("/unknown")
        .header(
            header::ORIGIN,
            axum::http::HeaderValue::from_bytes(&[0xff]).unwrap(),
        )
        .body(Body::empty())
        .unwrap();
    error(call(&app, req).await, StatusCode::FORBIDDEN, "forbidden").await;
}

#[tokio::test]
async fn safe_methods_do_not_require_origin_and_all_responses_are_hardened() {
    let db = TestDatabase::new().await;
    let (accounts, _) = db.ready_admin().await;
    let app = app(accounts);
    for method in ["GET", "HEAD", "OPTIONS"] {
        let req = Request::builder()
            .method(method)
            .uri("/health")
            .body(Body::empty())
            .unwrap();
        let response = call(&app, req).await;
        assert_eq!(
            response.status(),
            if method == "OPTIONS" {
                StatusCode::METHOD_NOT_ALLOWED
            } else {
                StatusCode::OK
            }
        );
    }
    let req = Request::builder()
        .uri("/health")
        .header(header::ORIGIN, "https://other.example.test")
        .body(Body::empty())
        .unwrap();
    assert_eq!(body(call(&app, req).await).await, json!({"status": "ok"}));
    for (method, path, input) in [
        ("GET", "/api/v1/me", None),
        ("GET", "/api/v1/server-settings", None),
        (
            "PUT",
            "/api/v1/me/password",
            Some(
                json!({"current_password": "synthetic-password", "new_password": "another-synthetic-password"}),
            ),
        ),
        ("POST", "/api/v1/auth/logout", None),
    ] {
        error(
            call(&app, request(method, path, None, input)).await,
            StatusCode::UNAUTHORIZED,
            "unauthorized",
        )
        .await;
    }
    let req = Request::builder()
        .method("POST")
        .uri("/api/v1/auth/logout")
        .body(Body::empty())
        .unwrap();
    // Boundary is outermost: a missing Origin wins over a missing protected-route session.
    error(call(&app, req).await, StatusCode::FORBIDDEN, "forbidden").await;
}

#[tokio::test]
async fn authentication_rejects_missing_expired_inactive_and_duplicate_cookies() {
    let db = TestDatabase::new().await;
    let (accounts, actor) = db.ready_admin().await;
    let app = app(accounts);
    let valid = cookie(&app, actor.user.id).await;
    assert_eq!(
        call(&app, request("GET", "/api/v1/me", Some(&valid), None))
            .await
            .status(),
        StatusCode::OK
    );
    let expired = Sessions::issue_until(&db.db, actor.user.id, Utc::now() - Duration::seconds(1))
        .await
        .unwrap();
    let expired = format!("{}={}", cookie_name(&app.config), expired.token);
    for supplied in [
        None,
        Some(expired.as_str()),
        Some("__Host-attos_session=not-a-token"),
    ] {
        error(
            call(&app, request("GET", "/api/v1/me", supplied, None)).await,
            StatusCode::UNAUTHORIZED,
            "unauthorized",
        )
        .await;
    }
    let mut duplicate = request("GET", "/api/v1/me", Some(&valid), None);
    duplicate
        .headers_mut()
        .append(header::COOKIE, valid.parse().unwrap());
    error(
        call(&app, duplicate).await,
        StatusCode::UNAUTHORIZED,
        "unauthorized",
    )
    .await;
    let duplicate = format!("{valid}; {valid}");
    error(
        call(&app, request("GET", "/api/v1/me", Some(&duplicate), None)).await,
        StatusCode::UNAUTHORIZED,
        "unauthorized",
    )
    .await;

    use crate::dashboard::db::entities::user;
    use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
    user::Entity::update_many()
        .col_expr(user::Column::Active, sea_orm::sea_query::Expr::value(false))
        .filter(user::Column::Id.eq(actor.user.id))
        .exec(&db.db)
        .await
        .unwrap();
    // Retain the stored session to exercise the authentication JOIN's active-user check.
    error(
        call(&app, request("GET", "/api/v1/me", Some(&valid), None)).await,
        StatusCode::UNAUTHORIZED,
        "unauthorized",
    )
    .await;
}
