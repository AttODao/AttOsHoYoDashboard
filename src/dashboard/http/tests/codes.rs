//! Redemption-code routes: session and token reachability, administrator controls, metrics.
use super::*;
use crate::dashboard::{
    codes::test_support::{game, link, ready_user, redeem_link},
    test_support::TestDatabase,
};

fn bearer(method: &str, path: &str, token: &str, body: Option<Value>) -> Request<Body> {
    let mut builder = Request::builder()
        .method(method)
        .uri(path)
        .header(header::AUTHORIZATION, format!("Bearer {token}"));
    let body = match body {
        Some(body) => {
            builder = builder.header(header::CONTENT_TYPE, "application/json");
            Body::from(body.to_string())
        }
        None => Body::empty(),
    };
    builder.body(body).unwrap()
}

async fn token(app: &App, session: &str, scope: &str) -> String {
    let response = call(
        app,
        request(
            "POST",
            "/api/v1/me/api-tokens",
            Some(session),
            Some(json!({"name": format!("codes {scope}"), "scope": scope})),
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    body(response).await["token"].as_str().unwrap().to_owned()
}

#[tokio::test]
async fn users_read_codes_and_change_modes_only_over_their_session() {
    let db = TestDatabase::new().await;
    let (accounts, admin) = db.ready_admin().await;
    let app = app(accounts);
    let session = cookie(&app, admin.user.id).await;

    let response = call(&app, request("GET", "/api/v1/codes", Some(&session), None)).await;
    assert_eq!(response.status(), StatusCode::OK);
    let catalogue = body(response).await;
    assert_eq!(catalogue["enabled"], true);
    assert_eq!(catalogue["last_success_at"], Value::Null);
    assert_eq!(catalogue["codes"], json!([]));
    assert_eq!(catalogue["consecutive_failures"], 0);
    // Never fetched yet: the next fetch is due now.
    assert!(catalogue["next_fetch_at"].is_string());
    error(
        call(&app, request("GET", "/api/v1/codes", None, None)).await,
        StatusCode::UNAUTHORIZED,
        "unauthorized",
    )
    .await;
    // Modes and history belong to the verified HoYoLAB link.
    for path in ["/api/v1/code-settings", "/api/v1/code-history"] {
        error(
            call(&app, request("GET", path, Some(&session), None)).await,
            StatusCode::FORBIDDEN,
            "initial_setup_required",
        )
        .await;
    }
    let owner_link = link(&app.accounts.db, admin.user.id, "100000001", "verified").await;
    let uid = game(&app.accounts.db, owner_link, "genshin", "800000001").await;
    let response = call(
        &app,
        request("GET", "/api/v1/code-settings", Some(&session), None),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let listed = body(response).await;
    assert_eq!(listed[0]["game_account_id"], uid.to_string());
    assert_eq!(listed[0]["mode"], "off");

    let path = format!("/api/v1/code-settings/{uid}");
    let response = call(
        &app,
        request(
            "PATCH",
            &path,
            Some(&session),
            Some(json!({"mode": "notify"})),
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    error(
        call(
            &app,
            request(
                "PATCH",
                &path,
                Some(&session),
                Some(json!({"mode": "auto"})),
            ),
        )
        .await,
        StatusCode::BAD_REQUEST,
        "invalid_input",
    )
    .await;
    error(
        call(
            &app,
            request(
                "PATCH",
                &format!("/api/v1/code-settings/{}", Uuid::new_v4()),
                Some(&session),
                Some(json!({"mode": "notify"})),
            ),
        )
        .await,
        StatusCode::NOT_FOUND,
        "not_found",
    )
    .await;
    // Unsafe methods need the Origin, like every other cookie route.
    let mut missing_origin = request("PATCH", &path, Some(&session), Some(json!({"mode": "off"})));
    missing_origin.headers_mut().remove(header::ORIGIN);
    error(
        call(&app, missing_origin).await,
        StatusCode::FORBIDDEN,
        "forbidden",
    )
    .await;
    let response = call(
        &app,
        request("GET", "/api/v1/code-settings", Some(&session), None),
    )
    .await;
    assert_eq!(body(response).await[0]["mode"], "notify");
    let response = call(
        &app,
        request("GET", "/api/v1/code-history", Some(&session), None),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(body(response).await, json!([]));

    // Both token scopes read; neither changes a mode or reaches the administrator routes.
    for scope in ["read", "run"] {
        let token = token(&app, &session, scope).await;
        for path in [
            "/api/v1/codes",
            "/api/v1/code-settings",
            "/api/v1/code-history",
            "/api/v1/code-redemption",
        ] {
            let response = call(&app, bearer("GET", path, &token, None)).await;
            assert_eq!(response.status(), StatusCode::OK, "{scope} {path}");
        }
        error(
            call(
                &app,
                bearer("PATCH", &path, &token, Some(json!({"mode": "off"}))),
            )
            .await,
            StatusCode::FORBIDDEN,
            "forbidden",
        )
        .await;
        error(
            call(&app, bearer("GET", "/api/v1/admin/codes", &token, None)).await,
            StatusCode::FORBIDDEN,
            "forbidden",
        )
        .await;
        error(
            call(
                &app,
                bearer(
                    "PUT",
                    "/api/v1/admin/codes",
                    &token,
                    Some(json!({"enabled": false, "interval_minutes": 180})),
                ),
            )
            .await,
            StatusCode::FORBIDDEN,
            "forbidden",
        )
        .await;
    }
}

#[tokio::test]
async fn administrators_stop_the_feature_and_set_the_interval() {
    let db = TestDatabase::new().await;
    let (accounts, admin) = db.ready_admin().await;
    let app = app(accounts);
    let session = cookie(&app, admin.user.id).await;
    let response = call(
        &app,
        request("GET", "/api/v1/admin/codes", Some(&session), None),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let view = body(response).await;
    assert_eq!(view["enabled"], true);
    assert_eq!(view["interval_minutes"], 180);
    assert_eq!(view["interval_minutes_min"], 60);
    assert_eq!(view["interval_minutes_max"], 1440);
    assert_eq!(view["source_host"], "hoyo-codes.seria.moe");
    assert_eq!(view["consecutive_failures"], 0);
    assert_eq!(view["last_error"], Value::Null);
    assert!(view["next_fetch_at"].is_string(), "never fetched: due now");
    assert_eq!(
        view["catalogue"],
        json!({"genshin": {"listed": 0, "unlisted": 0}, "starrail": {"listed": 0, "unlisted": 0}})
    );

    for interval in [59, 1441] {
        error(
            call(
                &app,
                request(
                    "PUT",
                    "/api/v1/admin/codes",
                    Some(&session),
                    Some(json!({"enabled": true, "interval_minutes": interval})),
                ),
            )
            .await,
            StatusCode::BAD_REQUEST,
            "invalid_input",
        )
        .await;
    }
    let response = call(
        &app,
        request(
            "PUT",
            "/api/v1/admin/codes",
            Some(&session),
            Some(json!({"enabled": false, "interval_minutes": 360})),
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    let view = body(
        call(
            &app,
            request("GET", "/api/v1/admin/codes", Some(&session), None),
        )
        .await,
    )
    .await;
    assert_eq!(view["enabled"], false);
    assert_eq!(view["interval_minutes"], 360);
    assert_eq!(view["next_fetch_at"], Value::Null);
    let response = call(&app, request("GET", "/api/v1/codes", Some(&session), None)).await;
    assert_eq!(body(response).await["enabled"], false);

    // A user who is not an administrator gets neither route.
    let user = ready_user(&app.accounts.db, "codes-user").await;
    let user_session = cookie(&app, user.user.id).await;
    error(
        call(
            &app,
            request("GET", "/api/v1/admin/codes", Some(&user_session), None),
        )
        .await,
        StatusCode::FORBIDDEN,
        "forbidden",
    )
    .await;
    error(
        call(
            &app,
            request(
                "PUT",
                "/api/v1/admin/codes",
                Some(&user_session),
                Some(json!({"enabled": true, "interval_minutes": 180})),
            ),
        )
        .await,
        StatusCode::FORBIDDEN,
        "forbidden",
    )
    .await;

    // `/metrics` reports every code series, with zeros before the first run.
    let response = call(
        &app,
        Request::builder()
            .uri("/metrics")
            .header(header::AUTHORIZATION, "Bearer synthetic-metrics-token")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let text = String::from_utf8(
        to_bytes(response.into_body(), 1_048_576)
            .await
            .unwrap()
            .to_vec(),
    )
    .unwrap();
    for line in [
        "ahd_codes_catalogue{game=\"genshin\",state=\"listed\"} 0",
        "ahd_codes_catalogue{game=\"starrail\",state=\"unlisted\"} 0",
        "ahd_codes_fetches_total{game=\"genshin\",outcome=\"ok\"} 0",
        "ahd_codes_fetches_total{game=\"starrail\",outcome=\"invalid_payload\"} 0",
        "ahd_codes_rejected_rows_total{game=\"starrail\"} 0",
        "ahd_codes_announcements_total 0",
    ] {
        assert!(text.lines().any(|candidate| candidate == line), "{line}");
    }
}

#[tokio::test]
async fn redemption_requests_need_a_session_redemption_credentials_and_respect_budgets() {
    let db = TestDatabase::new().await;
    let (accounts, admin) = db.ready_admin().await;
    let app = app(accounts);
    let session = cookie(&app, admin.user.id).await;
    let enter = |code: &str| Some(json!({"game_kind": "genshin", "code": code}));

    let response = call(
        &app,
        request("GET", "/api/v1/code-redemption", Some(&session), None),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        body(response).await,
        json!({"available": true, "redemption_credentials": false, "action_required": null,
               "action_at": null, "running": false, "pending": 0})
    );
    // A link whose Cookie lacks cookie_token_v2 can neither enter codes nor take `auto`.
    let plain = link(&app.accounts.db, admin.user.id, "100000001", "verified").await;
    let uid = game(&app.accounts.db, plain, "genshin", "800000001").await;
    let status = body(
        call(
            &app,
            request("GET", "/api/v1/me/hoyolab", Some(&session), None),
        )
        .await,
    )
    .await;
    assert_eq!(status["redemption_credentials"], false);
    error(
        call(
            &app,
            request(
                "POST",
                "/api/v1/code-redemption/codes",
                Some(&session),
                enter("GENSHINGIFT"),
            ),
        )
        .await,
        StatusCode::BAD_REQUEST,
        "invalid_input",
    )
    .await;

    // The same owner with a Cookie that carries cookie_token_v2.
    let user = ready_user(&app.accounts.db, "redeeming-user").await;
    let user_session = cookie(&app, user.user.id).await;
    let owner_link = redeem_link(&app.accounts.db, user.user.id, "100000002").await;
    let genshin = game(&app.accounts.db, owner_link, "genshin", "800000002").await;
    let status = body(
        call(
            &app,
            request("GET", "/api/v1/me/hoyolab", Some(&user_session), None),
        )
        .await,
    )
    .await;
    assert_eq!(status["redemption_credentials"], true);
    assert!(
        !status.to_string().contains("syntheticcookietoken"),
        "only a boolean leaves the server"
    );
    let response = call(
        &app,
        request(
            "PATCH",
            &format!("/api/v1/code-settings/{genshin}"),
            Some(&user_session),
            Some(json!({"mode": "auto"})),
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    let response = call(
        &app,
        request(
            "POST",
            "/api/v1/code-redemption/codes",
            Some(&user_session),
            enter("genshingift"),
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::ACCEPTED);
    let entered = body(response).await;
    assert_eq!(
        entered["entries"][0]["game_account_id"],
        genshin.to_string()
    );
    assert_eq!(entered["entries"][0]["code"], "GENSHINGIFT");
    // One chosen UID of the game; a UID that is not one of the owner's is not found.
    let response = call(
        &app,
        request(
            "POST",
            "/api/v1/code-redemption/codes",
            Some(&user_session),
            Some(json!({"game_kind": "genshin", "code": "CHOSENUID", "game_account_id": genshin})),
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::ACCEPTED);
    let entered = body(response).await;
    assert_eq!(entered["entries"].as_array().unwrap().len(), 1);
    assert_eq!(
        entered["entries"][0]["game_account_id"],
        genshin.to_string()
    );
    error(
        call(
            &app,
            request(
                "POST",
                "/api/v1/code-redemption/codes",
                Some(&user_session),
                Some(json!({"game_kind": "genshin", "code": "CHOSENUID", "game_account_id": uuid::Uuid::from_u128(9)})),
            ),
        )
        .await,
        StatusCode::NOT_FOUND,
        "not_found",
    )
    .await;
    assert_eq!(entered["entries"][0]["state"], "pending");
    // Unknown fields are refused before any budget is spent.
    let response = call(
        &app,
        request(
            "POST",
            "/api/v1/code-redemption/codes",
            Some(&user_session),
            Some(json!({"game_kind": "genshin", "code": "GENSHINGIFT", "uid": "1"})),
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);

    // Tokens read the state but never send anything, whatever their scope.
    for scope in ["read", "run"] {
        let token = token(&app, &user_session, scope).await;
        let response = call(&app, bearer("GET", "/api/v1/code-redemption", &token, None)).await;
        assert_eq!(response.status(), StatusCode::OK);
        for (path, input) in [
            ("/api/v1/code-redemption/run", json!({})),
            (
                "/api/v1/code-redemption/codes",
                json!({"game_kind": "genshin", "code": "TOKENGIFT"}),
            ),
        ] {
            error(
                call(&app, bearer("POST", path, &token, Some(input))).await,
                StatusCode::FORBIDDEN,
                "forbidden",
            )
            .await;
        }
    }
    // Unsafe methods need the Origin.
    let mut missing_origin = request(
        "POST",
        "/api/v1/code-redemption/run",
        Some(&user_session),
        Some(json!({})),
    );
    missing_origin.headers_mut().remove(header::ORIGIN);
    error(
        call(&app, missing_origin).await,
        StatusCode::FORBIDDEN,
        "forbidden",
    )
    .await;

    // Redeem now: three per ten minutes, then 429.
    for _ in 0..3 {
        let response = call(
            &app,
            request(
                "POST",
                "/api/v1/code-redemption/run",
                Some(&user_session),
                Some(json!({})),
            ),
        )
        .await;
        assert_eq!(response.status(), StatusCode::ACCEPTED);
        assert_eq!(body(response).await, json!({"started": true}));
    }
    error(
        call(
            &app,
            request(
                "POST",
                "/api/v1/code-redemption/run",
                Some(&user_session),
                Some(json!({})),
            ),
        )
        .await,
        StatusCode::TOO_MANY_REQUESTS,
        "rate_limited",
    )
    .await;
    // Entering codes: ten per ten minutes (three were used above).
    for index in 0..7 {
        let response = call(
            &app,
            request(
                "POST",
                "/api/v1/code-redemption/codes",
                Some(&user_session),
                enter(&format!("BUDGET{index:02}")),
            ),
        )
        .await;
        assert_eq!(response.status(), StatusCode::ACCEPTED, "{index}");
    }
    error(
        call(
            &app,
            request(
                "POST",
                "/api/v1/code-redemption/codes",
                Some(&user_session),
                enter("ONEMORE"),
            ),
        )
        .await,
        StatusCode::TOO_MANY_REQUESTS,
        "rate_limited",
    )
    .await;
    let _ = uid;

    // The administrator sees counts only.
    let view = body(
        call(
            &app,
            request("GET", "/api/v1/admin/codes", Some(&session), None),
        )
        .await,
    )
    .await;
    assert_eq!(view["redemption"]["auto_users"], 1);
    assert_eq!(view["redemption"]["auto_uids"], 1);
    assert_eq!(view["redemption"]["action_required_users"], 0);
    let text = String::from_utf8(
        to_bytes(
            call(
                &app,
                Request::builder()
                    .uri("/metrics")
                    .header(header::AUTHORIZATION, "Bearer synthetic-metrics-token")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .into_body(),
            1_048_576,
        )
        .await
        .unwrap()
        .to_vec(),
    )
    .unwrap();
    for line in [
        "ahd_code_auto_users 1",
        "ahd_code_action_required_users 0",
        "ahd_code_redemptions_total{game=\"starrail\",outcome=\"captcha_required\"} 0",
    ] {
        assert!(text.lines().any(|candidate| candidate == line), "{line}");
    }
    for state in crate::dashboard::codes::ledger::STATES {
        assert!(
            text.lines().any(|candidate| candidate
                .starts_with(&format!("ahd_code_ledger{{state=\"{state}\"}} "))),
            "{state}"
        );
    }
}
