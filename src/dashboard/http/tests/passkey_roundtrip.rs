//! Software-generated credentials validate HTTP DTOs/composition, not physical authenticators.
use super::*;
use crate::dashboard::{
    auth::passkeys::{AuthenticationChallenge, RegistrationChallenge},
    test_support::{
        TestDatabase,
        webauthn::{self, TestAuthenticator},
    },
};

fn local_request(
    method: &str,
    path: &str,
    cookie: Option<&str>,
    body: Option<Value>,
) -> Request<Body> {
    let mut request = request(method, path, cookie, body);
    request
        .headers_mut()
        .insert(header::ORIGIN, webauthn::ORIGIN.parse().unwrap());
    request
}

#[tokio::test]
async fn registration_public_authentication_metadata_and_delete_roundtrip() {
    let db = TestDatabase::new().await;
    let (accounts, actor) = db.ready_admin().await;
    let mut app = app(accounts);
    let state = Arc::get_mut(&mut app).unwrap();
    state.config.origin = webauthn::ORIGIN.to_owned();
    state.config.rp_id = webauthn::RP_ID.to_owned();
    state.config.secure_cookie = false;
    state.passkeys = Passkeys::new(webauthn::RP_ID, webauthn::ORIGIN).unwrap();
    let admin = cookie(&app, actor.user.id).await;
    let mut authenticator = TestAuthenticator::new();
    let options = call(
        &app,
        local_request(
            "POST",
            "/api/v1/me/passkeys/options",
            Some(&admin),
            Some(json!({"label": "software fixture"})),
        ),
    )
    .await;
    assert_eq!(options.status(), StatusCode::OK);
    let options = body(options).await;
    let challenge = RegistrationChallenge {
        challenge_id: options["challenge_id"].as_str().unwrap().parse().unwrap(),
        options: serde_json::from_value(options["options"].clone()).unwrap(),
    };
    let credential = authenticator.register(&challenge, 0);
    let registered = call(
        &app,
        local_request(
            "POST",
            "/api/v1/me/passkeys/verify",
            Some(&admin),
            Some(json!({"challenge_id": challenge.challenge_id, "response": credential})),
        ),
    )
    .await;
    assert_eq!(registered.status(), StatusCode::CREATED);
    let metadata = body(registered).await;
    assert_eq!(metadata.as_object().unwrap().len(), 3);
    assert_eq!(metadata["label"], "software fixture");
    assert!(metadata["created_at"].is_string());
    let id: Uuid = metadata["id"].as_str().unwrap().parse().unwrap();
    let listing = call(
        &app,
        local_request("GET", "/api/v1/me/passkeys", Some(&admin), None),
    )
    .await;
    assert_eq!(listing.status(), StatusCode::OK);
    assert_eq!(body(listing).await, json!([metadata]));

    let options = call(
        &app,
        local_request(
            "POST",
            "/api/v1/auth/passkey/options",
            None,
            Some(json!({"username": "admin"})),
        ),
    )
    .await;
    assert_eq!(options.status(), StatusCode::OK);
    let options = body(options).await;
    let challenge = AuthenticationChallenge {
        challenge_id: options["challenge_id"].as_str().unwrap().parse().unwrap(),
        options: serde_json::from_value(options["options"].clone()).unwrap(),
    };
    let proof = authenticator.authenticate(&challenge, 1);
    let login = call(
        &app,
        local_request(
            "POST",
            "/api/v1/auth/passkey/verify",
            None,
            Some(json!({"challenge_id": challenge.challenge_id, "response": proof})),
        ),
    )
    .await;
    assert_eq!(login.status(), StatusCode::OK);
    let passkey_cookie = response_cookie(&login);
    assert!(passkey_cookie.starts_with("attos_session="));
    assert_eq!(
        body(login).await,
        json!({"id": actor.user.id, "username": "admin", "is_admin": true, "active": true, "must_change_password": false})
    );
    // The fixture sign-in, then passkey options and verify.
    assert_eq!(app.accounts.login_attempts("admin"), 3);
    assert_eq!(
        call(
            &app,
            local_request("GET", "/api/v1/me", Some(&passkey_cookie), None)
        )
        .await
        .status(),
        StatusCode::OK
    );
    let deleted = call(
        &app,
        local_request(
            "DELETE",
            &format!("/api/v1/me/passkeys/{id}"),
            Some(&admin),
            None,
        ),
    )
    .await;
    assert_eq!(deleted.status(), StatusCode::NO_CONTENT);
    assert!(
        to_bytes(deleted.into_body(), 1024)
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        body(
            call(
                &app,
                local_request("GET", "/api/v1/me/passkeys", Some(&admin), None)
            )
            .await
        )
        .await,
        json!([])
    );
    error(
        call(
            &app,
            local_request(
                "POST",
                "/api/v1/auth/passkey/options",
                None,
                Some(json!({"username": "admin"})),
            ),
        )
        .await,
        StatusCode::UNAUTHORIZED,
        "unauthorized",
    )
    .await;
}
