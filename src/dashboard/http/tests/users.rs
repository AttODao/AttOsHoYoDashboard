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
        json!({"id": id, "username": "ordinary", "is_admin": false, "active": true, "must_change_password": true, "last_login_at": null})
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
    let mut me = body(call(&app, request("GET", "/api/v1/me", Some(&temporary), None)).await).await;
    assert!(
        me["last_login_at"].is_string(),
        "the first sign-in is recorded"
    );
    me["last_login_at"] = serde_json::Value::Null;
    assert_eq!(me, expected);
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
        assert_eq!(view.as_object().unwrap().len(), 11);
        assert!(view.get("password_hash").is_none());
        assert!(view["created_at"].is_string());
        assert_eq!(view["link_status"], "not_configured");
        assert_eq!(view["automatic_claims"], 0);
        assert_eq!(view["needs_attention"], false);
        assert_eq!(
            view["hoyolab"],
            json!({"verified_at": null, "status_fetched_at": null, "games": []})
        );
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

#[tokio::test]
async fn admin_audit_and_role_routes_validate_their_input() {
    let fixture = TestDatabase::new().await;
    let (accounts, actor) = fixture.ready_admin().await;
    let app = app(accounts);
    let admin = cookie(&app, actor.user.id).await;
    let created = body(
        call(
            &app,
            request(
                "POST",
                "/api/v1/admin/users",
                Some(&admin),
                Some(json!({"username": "member", "temporary_password": TEMP_PASSWORD})),
            ),
        )
        .await,
    )
    .await;
    let path = format!("/api/v1/admin/users/{}", created["id"].as_str().unwrap());
    for input in [json!({}), json!({"active": true, "is_admin": true})] {
        error(
            call(&app, request("PATCH", &path, Some(&admin), Some(input))).await,
            StatusCode::BAD_REQUEST,
            "invalid_input",
        )
        .await;
    }
    let response = call(
        &app,
        request(
            "PATCH",
            &path,
            Some(&admin),
            Some(json!({"is_admin": true})),
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    let audit = body(
        call(
            &app,
            request("GET", "/api/v1/admin/audit?limit=2", Some(&admin), None),
        )
        .await,
    )
    .await;
    let entries = audit.as_array().unwrap();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0]["action"], "admin_granted");
    assert_eq!(entries[0]["actor"], "admin");
    assert_eq!(entries[0]["target"], "member");
    assert_eq!(entries[0].as_object().unwrap().len(), 5);
    assert!(
        call(
            &app,
            request("GET", "/api/v1/admin/audit?limit=x", Some(&admin), None),
        )
        .await
        .status()
        .is_client_error()
    );
}

#[tokio::test]
async fn admin_listing_reports_link_health_without_identifiers() {
    use crate::dashboard::db::entities::{
        hoyolab_account, reward_attempt, reward_run, reward_setting,
    };
    use sea_orm::{ActiveModelTrait, Set};
    let db = TestDatabase::new().await;
    let (accounts, actor) = db.ready_admin().await;
    let member = accounts
        .create_user(&actor, "member".to_owned(), TEMP_PASSWORD.to_owned())
        .await
        .unwrap();
    let now = Utc::now();
    let link = hoyolab_account::ActiveModel {
        id: Set(Uuid::new_v4()),
        owner_user_id: Set(member.id),
        hoyolab_uid: Set(Some("424242".to_owned())),
        encrypted_credentials: Set(Some(vec![7; 32])),
        credential_version: Set(1),
        auth_status: Set("cookie_invalid".to_owned()),
        created_at: Set(now),
        updated_at: Set(now),
        verified_at: Set(None),
    }
    .insert(&db.db)
    .await
    .unwrap();
    let setting = reward_setting::ActiveModel {
        id: Set(Uuid::new_v4()),
        hoyolab_account_id: Set(link.id),
        game_kind: Set("starrail".to_owned()),
        enabled: Set(false),
        local_time: Set("06:00".to_owned()),
        auth_status: Set("action_required".to_owned()),
        next_run_at: Set(None),
    }
    .insert(&db.db)
    .await
    .unwrap();
    let run = reward_run::ActiveModel {
        id: Set(Uuid::new_v4()),
        setting_id: Set(setting.id),
        trigger: Set("manual".to_owned()),
        state: Set("action_required".to_owned()),
        created_at: Set(now),
        updated_at: Set(now),
        ended_at: Set(Some(now)),
        attempt_count: Set(1),
        credential_version: Set(1),
        send_state: Set("not_sent".to_owned()),
        next_retry_at: Set(None),
    }
    .insert(&db.db)
    .await
    .unwrap();
    reward_attempt::ActiveModel {
        id: Set(Uuid::new_v4()),
        run_id: Set(run.id),
        number: Set(1),
        started_at: Set(now),
        finished_at: Set(Some(now)),
        outcome: Set("action_required".to_owned()),
        error_code: Set(Some("action_required".to_owned())),
        send_state: Set("not_sent".to_owned()),
    }
    .insert(&db.db)
    .await
    .unwrap();

    let app = app(accounts);
    let admin_cookie = cookie(&app, actor.user.id).await;
    let listing = body(
        call(
            &app,
            request("GET", "/api/v1/admin/users", Some(&admin_cookie), None),
        )
        .await,
    )
    .await;
    let row = listing
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["username"] == "member")
        .unwrap();
    assert_eq!(row["link_status"], "cookie_invalid");
    assert_eq!(row["automatic_claims"], 0);
    assert_eq!(row["needs_attention"], true);
    let games = row["hoyolab"]["games"].as_array().unwrap();
    assert_eq!(games.len(), 1);
    assert_eq!(
        games[0].as_object().unwrap().keys().collect::<Vec<_>>(),
        ["automatic", "game_kind", "last_run", "setting_status"]
    );
    assert_eq!(games[0]["game_kind"], "starrail");
    assert_eq!(games[0]["setting_status"], "action_required");
    assert_eq!(games[0]["last_run"]["state"], "action_required");
    assert_eq!(games[0]["last_run"]["error_code"], "action_required");
    assert!(games[0]["last_run"]["ended_at"].is_string());
    assert_eq!(games[0]["last_run"].as_object().unwrap().len(), 3);
    // Neither the HoYoLAB UID nor any setting/run identifier leaves the server.
    let text = row.to_string();
    for secret in [
        "424242",
        &setting.id.to_string(),
        &run.id.to_string(),
        &link.id.to_string(),
    ] {
        assert!(!text.contains(secret), "{secret}");
    }
}
