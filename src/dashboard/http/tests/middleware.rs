use super::*;
use crate::dashboard::{
    auth::sessions::Sessions,
    db::entities::{hoyolab_account, reward_run, reward_setting, user},
    test_support::TestDatabase,
};
use sea_orm::{ActiveModelTrait, EntityTrait, Set};

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
    assert_eq!(
        body(call(&app, req).await).await,
        json!({"status": "ok", "worker": "ok"})
    );
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

#[tokio::test]
async fn health_reports_a_stalled_reward_worker_as_unavailable() {
    let db = TestDatabase::new().await;
    let (accounts, _) = db.ready_admin().await;
    let app = app(accounts);
    app.heartbeat
        .set_age_secs(crate::dashboard::http::runtime::STALLED_AFTER_SECS + 1);
    let response = call(&app, request("GET", "/health", None, None)).await;
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(
        body(response).await,
        json!({"status": "degraded", "worker": "stalled"})
    );
}

#[tokio::test]
async fn metrics_require_the_configured_bearer_token() {
    let db = TestDatabase::new().await;
    let (accounts, _) = db.ready_admin().await;
    let app = app(accounts);
    let scrape = |token: Option<&str>| {
        let mut builder = Request::builder().uri("/metrics");
        if let Some(token) = token {
            builder = builder.header(header::AUTHORIZATION, format!("Bearer {token}"));
        }
        builder.body(Body::empty()).unwrap()
    };
    for token in [None, Some("wrong-synthetic-token")] {
        assert_eq!(
            call(&app, scrape(token)).await.status(),
            StatusCode::UNAUTHORIZED
        );
    }
    let response = call(&app, scrape(Some("synthetic-metrics-token"))).await;
    assert_eq!(response.status(), StatusCode::OK);
    let text = String::from_utf8(
        to_bytes(response.into_body(), 64 * 1024)
            .await
            .unwrap()
            .to_vec(),
    )
    .unwrap();
    assert!(text.contains("ahd_up 1"));
    assert!(text.contains("ahd_users{state=\"active\"} 1"));
    assert!(text.contains("ahd_reward_worker_heartbeat_age_seconds "));
    for state in [
        "success",
        "already_claimed",
        "failed",
        "action_required",
        "unknown",
        "stopped",
    ] {
        assert!(text.contains(&format!(
            "ahd_reward_runs_ended_24h{{state=\"{state}\"}} 0\n"
        )));
    }
    assert!(!text.contains("ahd_api_tokens{"));

    // Only runs that ended within the last 24 hours count; tokens are counted by scope.
    let owner = app.accounts.db.clone();
    let admin = user::Entity::find().one(&owner).await.unwrap().unwrap();
    let link = hoyolab_account::ActiveModel {
        id: Set(Uuid::new_v4()),
        owner_user_id: Set(admin.id),
        hoyolab_uid: Set(Some("123456789".to_owned())),
        encrypted_credentials: Set(Some(vec![7; 30])),
        credential_version: Set(1),
        auth_status: Set("verified".to_owned()),
        created_at: Set(Utc::now()),
        updated_at: Set(Utc::now()),
        verified_at: Set(None),
    }
    .insert(&owner)
    .await
    .unwrap();
    let setting = reward_setting::ActiveModel {
        id: Set(Uuid::new_v4()),
        hoyolab_account_id: Set(link.id),
        game_kind: Set("genshin".to_owned()),
        ..Default::default()
    }
    .insert(&owner)
    .await
    .unwrap();
    for ended in [Duration::hours(1), Duration::days(2)] {
        reward_run::ActiveModel {
            id: Set(Uuid::new_v4()),
            setting_id: Set(setting.id),
            trigger: Set("manual".to_owned()),
            state: Set("failed".to_owned()),
            created_at: Set(Utc::now() - ended),
            updated_at: Set(Utc::now() - ended),
            ended_at: Set(Some(Utc::now() - ended)),
            attempt_count: Set(1),
            credential_version: Set(1),
            send_state: Set("not_sent".to_owned()),
            ..Default::default()
        }
        .insert(&owner)
        .await
        .unwrap();
    }
    let actor = Sessions::authenticate(
        &owner,
        &crate::dashboard::auth::password::token_hash(
            cookie(&app, admin.id).await.split_once('=').unwrap().1,
        )
        .unwrap(),
    )
    .await
    .unwrap();
    crate::dashboard::auth::api_tokens::ApiTokens::issue(
        &owner,
        &actor,
        "deck",
        crate::dashboard::auth::api_tokens::Scope::Run,
    )
    .await
    .unwrap();
    let response = call(&app, scrape(Some("synthetic-metrics-token"))).await;
    let text = String::from_utf8(
        to_bytes(response.into_body(), 64 * 1024)
            .await
            .unwrap()
            .to_vec(),
    )
    .unwrap();
    assert!(text.contains("ahd_reward_runs_ended_24h{state=\"failed\"} 1\n"));
    assert!(text.contains("ahd_reward_runs{state=\"failed\"} 2\n"));
    assert!(text.contains("ahd_api_tokens{scope=\"run\"} 1\n"));

    // Without a configured token the endpoint does not exist.
    let mut app = app;
    Arc::get_mut(&mut app).unwrap().config.metrics_token = None;
    let closed = call(&app, scrape(Some("synthetic-metrics-token")))
        .await
        .status();
    assert_eq!(closed, StatusCode::NOT_FOUND);
}
