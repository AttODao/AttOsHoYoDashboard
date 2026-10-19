use super::{AuthenticationChallenge, CHALLENGE_TTL, credential_fingerprint};
use crate::dashboard::{auth::sessions::Sessions, db::entities::passkey, error::Error};
use crate::dashboard::{
    auth::{passkeys::test_support::Fixture, password::token_hash},
    db::entities::session,
};
use sea_orm::PaginatorTrait;
use sea_orm::{
    ActiveModelTrait, ConnectionTrait, DbBackend, EntityTrait, IntoActiveModel, Set, Statement,
    TransactionTrait,
};
use serde_json::Value;
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

#[tokio::test]
async fn software_p256_authentication_issues_a_session_and_updates_counter() {
    let fixture = Fixture::new().await;
    let before = passkey::Entity::find_by_id(fixture.key_id)
        .one(&fixture.db.db)
        .await
        .unwrap()
        .unwrap();
    let challenge = fixture
        .passkeys
        .authentication_start(&fixture.accounts, fixture.actor.user.username.clone())
        .await
        .unwrap();
    let response = fixture.authenticator.authenticate(&challenge, 2);
    let (user, issued) = fixture
        .passkeys
        .authentication_finish(&fixture.accounts, challenge.challenge_id, response.clone())
        .await
        .unwrap();
    assert_eq!(user.id, fixture.actor.user.id);
    assert_eq!(
        Sessions::authenticate(&fixture.db.db, &token_hash(&issued.token).unwrap())
            .await
            .unwrap()
            .user
            .id,
        fixture.actor.user.id
    );
    let after = passkey::Entity::find_by_id(fixture.key_id)
        .one(&fixture.db.db)
        .await
        .unwrap()
        .unwrap();
    assert_ne!(
        credential_fingerprint(&before.credential).unwrap(),
        credential_fingerprint(&after.credential).unwrap()
    );
    assert!(matches!(
        fixture
            .passkeys
            .authentication_finish(&fixture.accounts, challenge.challenge_id, response)
            .await,
        Err(Error::Unauthorized)
    ));
    // A fresh assertion with a lower nonzero counter cannot roll persisted state back.
    let challenge = fixture
        .passkeys
        .authentication_start(&fixture.accounts, fixture.actor.user.username.clone())
        .await
        .unwrap();
    let response = fixture.authenticator.authenticate(&challenge, 1);
    assert!(matches!(
        fixture
            .passkeys
            .authentication_finish(&fixture.accounts, challenge.challenge_id, response)
            .await,
        Err(Error::Unauthorized)
    ));
    let final_row = passkey::Entity::find_by_id(fixture.key_id)
        .one(&fixture.db.db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        credential_fingerprint(&after.credential).unwrap(),
        credential_fingerprint(&final_row.credential).unwrap()
    );
}

#[tokio::test]
async fn expired_authentication_does_not_issue_a_session() {
    let fixture = Fixture::new().await;
    let before = session::Entity::find().count(&fixture.db.db).await.unwrap();
    let challenge = fixture
        .passkeys
        .authentication_start(&fixture.accounts, fixture.actor.user.username.clone())
        .await
        .unwrap();
    let response = fixture.authenticator.authenticate(&challenge, 2);
    fixture
        .passkeys
        .pending
        .lock()
        .unwrap()
        .get_mut(&challenge.challenge_id)
        .unwrap()
        .expires_at = Instant::now() - CHALLENGE_TTL;
    assert!(matches!(
        fixture
            .passkeys
            .authentication_finish(&fixture.accounts, challenge.challenge_id, response)
            .await,
        Err(Error::Unauthorized)
    ));
    assert_eq!(
        session::Entity::find().count(&fixture.db.db).await.unwrap(),
        before
    );
}

#[tokio::test]
async fn signed_wrong_origin_is_rejected_consumed_and_fresh_retry_succeeds() {
    let fixture = Fixture::new().await;
    let before = session::Entity::find().count(&fixture.db.db).await.unwrap();
    let challenge = fixture
        .passkeys
        .authentication_start(&fixture.accounts, fixture.actor.user.username.clone())
        .await
        .unwrap();
    let wrong =
        fixture
            .authenticator
            .authenticate_at_origin(&challenge, 2, "http://localhost:9999");
    assert!(matches!(
        fixture
            .passkeys
            .authentication_finish(&fixture.accounts, challenge.challenge_id, wrong)
            .await,
        Err(Error::Unauthorized)
    ));
    let valid = fixture.authenticator.authenticate(&challenge, 2);
    assert!(matches!(
        fixture
            .passkeys
            .authentication_finish(&fixture.accounts, challenge.challenge_id, valid)
            .await,
        Err(Error::Unauthorized)
    ));
    assert_eq!(
        session::Entity::find().count(&fixture.db.db).await.unwrap(),
        before
    );
    let fresh = fixture
        .passkeys
        .authentication_start(&fixture.accounts, fixture.actor.user.username.clone())
        .await
        .unwrap();
    assert!(
        fixture
            .passkeys
            .authentication_finish(
                &fixture.accounts,
                fresh.challenge_id,
                fixture.authenticator.authenticate(&fresh, 2)
            )
            .await
            .is_ok()
    );
}

#[tokio::test]
async fn invalid_signature_is_rejected_without_issuing_session() {
    use crate::dashboard::test_support::webauthn::TestAuthenticator;
    let fixture = Fixture::new().await;
    let before = session::Entity::find().count(&fixture.db.db).await.unwrap();
    let challenge = fixture
        .passkeys
        .authentication_start(&fixture.accounts, fixture.actor.user.username.clone())
        .await
        .unwrap();
    let mut response =
        serde_json::to_value(fixture.authenticator.authenticate(&challenge, 2)).unwrap();
    // Valid DER signature over the same transcript, but from a different P256 key.
    // This must fail cryptographic verification, not merely signature parsing.
    let other = serde_json::to_value(TestAuthenticator::new().authenticate(&challenge, 2)).unwrap();
    response["response"]["signature"] = other["response"]["signature"].clone();
    let response = serde_json::from_value(response).unwrap();
    assert!(matches!(
        fixture
            .passkeys
            .authentication_finish(&fixture.accounts, challenge.challenge_id, response)
            .await,
        Err(Error::Unauthorized)
    ));
    assert_eq!(
        session::Entity::find().count(&fixture.db.db).await.unwrap(),
        before
    );
}

#[tokio::test]
async fn challenge_b_started_before_a_counter_update_has_a_stale_fingerprint() {
    let fixture = Fixture::new().await;
    let a = fixture
        .passkeys
        .authentication_start(&fixture.accounts, fixture.actor.user.username.clone())
        .await
        .unwrap();
    let b = fixture
        .passkeys
        .authentication_start(&fixture.accounts, fixture.actor.user.username.clone())
        .await
        .unwrap();
    let response_a = fixture.authenticator.authenticate(&a, 2);
    let response_b = fixture.authenticator.authenticate(&b, 3);
    fixture
        .passkeys
        .authentication_finish(&fixture.accounts, a.challenge_id, response_a)
        .await
        .unwrap();
    let before = session::Entity::find().count(&fixture.db.db).await.unwrap();
    assert!(matches!(
        fixture
            .passkeys
            .authentication_finish(&fixture.accounts, b.challenge_id, response_b)
            .await,
        Err(Error::Unauthorized)
    ));
    assert_eq!(
        session::Entity::find().count(&fixture.db.db).await.unwrap(),
        before
    );
    let fresh = fixture
        .passkeys
        .authentication_start(&fixture.accounts, fixture.actor.user.username.clone())
        .await
        .unwrap();
    assert!(
        fixture
            .passkeys
            .authentication_finish(
                &fixture.accounts,
                fresh.challenge_id,
                fixture.authenticator.authenticate(&fresh, 3)
            )
            .await
            .is_ok()
    );
}

#[tokio::test]
async fn deleted_and_reregistered_identical_json_cannot_revive_old_row_uuid() {
    let mut fixture = Fixture::new().await;
    let old = passkey::Entity::find_by_id(fixture.key_id)
        .one(&fixture.db.db)
        .await
        .unwrap()
        .unwrap();
    let challenge = fixture
        .passkeys
        .authentication_start(&fixture.accounts, fixture.actor.user.username.clone())
        .await
        .unwrap();
    let response = fixture.authenticator.authenticate(&challenge, 2);
    fixture
        .passkeys
        .delete(&fixture.accounts, &fixture.actor, fixture.key_id)
        .await
        .unwrap();
    let register = fixture
        .passkeys
        .registration_start(&fixture.accounts, &fixture.actor, "same key again".into())
        .await
        .unwrap();
    let registration = fixture.authenticator.register(&register, 1);
    let new = fixture
        .passkeys
        .registration_finish(
            &fixture.accounts,
            &fixture.actor,
            register.challenge_id,
            registration,
        )
        .await
        .unwrap();
    let new_row = passkey::Entity::find_by_id(new.id)
        .one(&fixture.db.db)
        .await
        .unwrap()
        .unwrap();
    assert_ne!(old.id, new_row.id);
    assert_eq!(old.credential_id, new_row.credential_id);
    assert_eq!(
        credential_fingerprint(&old.credential).unwrap(),
        credential_fingerprint(&new_row.credential).unwrap()
    );
    assert!(matches!(
        fixture
            .passkeys
            .authentication_finish(&fixture.accounts, challenge.challenge_id, response)
            .await,
        Err(Error::Unauthorized)
    ));
    let fresh = fixture
        .passkeys
        .authentication_start(&fixture.accounts, fixture.actor.user.username.clone())
        .await
        .unwrap();
    assert!(
        fixture
            .passkeys
            .authentication_finish(
                &fixture.accounts,
                fresh.challenge_id,
                fixture.authenticator.authenticate(&fresh, 2)
            )
            .await
            .is_ok()
    );
}

#[tokio::test]
async fn simultaneous_finishes_of_two_snapshots_issue_only_one_session() {
    let fixture = Fixture::new().await;
    let a = fixture
        .passkeys
        .authentication_start(&fixture.accounts, fixture.actor.user.username.clone())
        .await
        .unwrap();
    let b = fixture
        .passkeys
        .authentication_start(&fixture.accounts, fixture.actor.user.username.clone())
        .await
        .unwrap();
    let before = session::Entity::find().count(&fixture.db.db).await.unwrap();
    let tx = fixture.accounts.db.begin().await.unwrap();
    crate::dashboard::auth::accounts::lock_user(&tx, fixture.actor.user.id)
        .await
        .unwrap();
    let fixture = Arc::new(fixture);
    let a_fixture = fixture.clone();
    let a_task = tokio::spawn(async move {
        a_fixture
            .passkeys
            .authentication_finish(
                &a_fixture.accounts,
                a.challenge_id,
                a_fixture.authenticator.authenticate(&a, 2),
            )
            .await
    });
    let b_fixture = fixture.clone();
    let b_task = tokio::spawn(async move {
        b_fixture
            .passkeys
            .authentication_finish(
                &b_fixture.accounts,
                b.challenge_id,
                b_fixture.authenticator.authenticate(&b, 3),
            )
            .await
    });
    // Each fixture has its own cluster. Observe both requests blocked on the real
    // database lock before releasing it; no production hooks or scheduling guesses.
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let row = fixture.db.db.query_one_raw(Statement::from_string(
                DbBackend::Postgres,
                "SELECT count(*) AS blocked FROM pg_stat_activity WHERE datname = current_database() AND backend_type = 'client backend' AND wait_event_type = 'Lock'",
            )).await.unwrap().unwrap();
            if row.try_get::<i64>("", "blocked").unwrap() == 2 { break; }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }).await.expect("both authentication transactions must reach the user lock");
    tx.rollback().await.unwrap();
    let (a_result, b_result) = tokio::time::timeout(Duration::from_secs(5), async {
        tokio::join!(a_task, b_task)
    })
    .await
    .unwrap();
    let a_result = a_result.unwrap();
    let b_result = b_result.unwrap();
    assert!(matches!(
        (&a_result, &b_result),
        (Ok(_), Err(Error::Unauthorized)) | (Err(Error::Unauthorized), Ok(_))
    ));
    assert_eq!(
        session::Entity::find().count(&fixture.db.db).await.unwrap(),
        before + 1
    );
}

#[tokio::test]
async fn current_temporary_password_policy_blocks_existing_keys_and_pending_login() {
    let fixture = Fixture::new().await;
    let challenge = fixture
        .passkeys
        .authentication_start(&fixture.accounts, fixture.actor.user.username.clone())
        .await
        .unwrap();
    let response = fixture.authenticator.authenticate(&challenge, 2);
    let mut user = fixture.actor.user.clone().into_active_model();
    user.must_change_password = Set(true);
    user.update(&fixture.db.db).await.unwrap();
    assert!(matches!(
        fixture
            .passkeys
            .authentication_finish(&fixture.accounts, challenge.challenge_id, response)
            .await,
        Err(Error::Unauthorized)
    ));
}

#[tokio::test]
async fn current_disabled_policy_blocks_existing_keys_and_pending_login() {
    let fixture = Fixture::new().await;
    let challenge = fixture
        .passkeys
        .authentication_start(&fixture.accounts, fixture.actor.user.username.clone())
        .await
        .unwrap();
    let response = fixture.authenticator.authenticate(&challenge, 2);
    let mut user = fixture.actor.user.clone().into_active_model();
    user.active = Set(false);
    user.update(&fixture.db.db).await.unwrap();
    assert!(matches!(
        fixture
            .passkeys
            .authentication_finish(&fixture.accounts, challenge.challenge_id, response)
            .await,
        Err(Error::Unauthorized)
    ));
}

#[tokio::test]
async fn malformed_usernames_are_unauthorized_without_a_challenge() {
    let fixture = Fixture::new().await;
    for username in [String::new(), "x".repeat(65), "bad name".into()] {
        assert!(matches!(
            fixture
                .passkeys
                .authentication_start(&fixture.accounts, username)
                .await,
            Err(Error::Unauthorized)
        ));
    }
    assert!(fixture.passkeys.pending.lock().unwrap().is_empty());
}

/// Options with the per-request random challenge and the credential ID blanked.
fn comparable(challenge: &AuthenticationChallenge) -> Value {
    let mut options = serde_json::to_value(&challenge.options).unwrap();
    let public_key = &mut options["publicKey"];
    assert!(
        public_key["challenge"]
            .as_str()
            .is_some_and(|c| !c.is_empty())
    );
    public_key["challenge"] = Value::Null;
    let allowed = public_key["allowCredentials"].as_array_mut().unwrap();
    assert_eq!(allowed.len(), 1);
    assert!(allowed[0]["id"].as_str().is_some_and(|id| !id.is_empty()));
    allowed[0]["id"] = Value::Null;
    options
}

#[tokio::test]
async fn names_without_a_usable_passkey_get_options_shaped_like_a_real_user() {
    let fixture = Fixture::new().await;
    let real = fixture
        .passkeys
        .authentication_start(&fixture.accounts, fixture.actor.user.username.clone())
        .await
        .unwrap();
    let expected = comparable(&real);
    let decoy = |username: &str| {
        let username = username.to_owned();
        let fixture = &fixture;
        async move {
            fixture
                .passkeys
                .authentication_start(&fixture.accounts, username)
                .await
                .unwrap()
        }
    };
    let unknown = decoy("unknown-synthetic").await;
    assert_eq!(comparable(&unknown), expected);
    assert_eq!(
        serde_json::to_value(&unknown)
            .unwrap()
            .as_object()
            .unwrap()
            .len(),
        serde_json::to_value(&real)
            .unwrap()
            .as_object()
            .unwrap()
            .len()
    );
    // Repeated probes of one name list the same credential; another name lists another.
    let id = |challenge: &AuthenticationChallenge| {
        serde_json::to_value(&challenge.options).unwrap()["publicKey"]["allowCredentials"][0]["id"]
            .clone()
    };
    assert_eq!(id(&unknown), id(&decoy("unknown-synthetic").await));
    assert_ne!(id(&unknown), id(&decoy("other-synthetic").await));
    assert_ne!(id(&unknown), id(&real));

    // A temporary-password user and a disabled user are decoys too.
    let mut user = fixture.actor.user.clone().into_active_model();
    user.must_change_password = Set(true);
    let user = user.update(&fixture.db.db).await.unwrap();
    assert_eq!(comparable(&decoy(&user.username).await), expected);
    let mut user = user.into_active_model();
    user.must_change_password = Set(false);
    user.active = Set(false);
    let user = user.update(&fixture.db.db).await.unwrap();
    assert_eq!(comparable(&decoy(&user.username).await), expected);

    // So is an active user whose last passkey was removed.
    let mut user = user.into_active_model();
    user.active = Set(true);
    user.update(&fixture.db.db).await.unwrap();
    passkey::Entity::delete_by_id(fixture.key_id)
        .exec(&fixture.db.db)
        .await
        .unwrap();
    let keyless = decoy(&fixture.actor.user.username).await;
    assert_eq!(comparable(&keyless), expected);
}

#[tokio::test]
async fn decoy_challenges_fail_like_a_wrong_assertion_and_spend_the_budget() {
    let fixture = Fixture::new().await;
    let before = session::Entity::find().count(&fixture.db.db).await.unwrap();
    let challenge = fixture
        .passkeys
        .authentication_start(&fixture.accounts, "unknown-synthetic".into())
        .await
        .unwrap();
    assert_eq!(fixture.accounts.login_attempts("unknown-synthetic"), 1);
    // A valid signature from a real key still cannot complete a decoy.
    let response = fixture.authenticator.authenticate(&challenge, 2);
    assert!(matches!(
        fixture
            .passkeys
            .authentication_finish(&fixture.accounts, challenge.challenge_id, response.clone())
            .await,
        Err(Error::Unauthorized)
    ));
    assert_eq!(fixture.accounts.login_attempts("unknown-synthetic"), 2);
    // Single use like any challenge.
    assert!(matches!(
        fixture
            .passkeys
            .authentication_finish(&fixture.accounts, challenge.challenge_id, response)
            .await,
        Err(Error::Unauthorized)
    ));
    assert_eq!(fixture.accounts.login_attempts("unknown-synthetic"), 2);
    assert_eq!(
        session::Entity::find().count(&fixture.db.db).await.unwrap(),
        before
    );
}

#[tokio::test]
async fn decoy_credentials_are_stable_across_restarts_with_the_cookie_key() {
    use crate::dashboard::{
        auth::passkeys::Passkeys,
        hoyolab::credentials::CookieKey,
        test_support::webauthn::{ORIGIN, RP_ID},
    };
    let fixture = Fixture::new().await;
    let seed = CookieKey::from_hex(&"22".repeat(32)).unwrap();
    let mut ids = Vec::new();
    for passkeys in [
        Passkeys::new(RP_ID, ORIGIN, Some(&seed)).unwrap(),
        Passkeys::new(RP_ID, ORIGIN, Some(&seed)).unwrap(),
        Passkeys::new(RP_ID, ORIGIN, None).unwrap(),
    ] {
        let challenge = passkeys
            .authentication_start(&fixture.accounts, "unknown-synthetic".into())
            .await
            .unwrap();
        ids.push(
            serde_json::to_value(&challenge.options).unwrap()["publicKey"]["allowCredentials"]
                [0]["id"]
                .clone(),
        );
    }
    assert_eq!(ids[0], ids[1]);
    assert_ne!(ids[0], ids[2]);
}

#[tokio::test]
async fn credential_deleted_after_verification_but_before_the_user_lock_is_rejected() {
    let fixture = Fixture::new().await;
    let challenge = fixture
        .passkeys
        .authentication_start(&fixture.accounts, fixture.actor.user.username.clone())
        .await
        .unwrap();
    let response = fixture.authenticator.authenticate(&challenge, 2);
    let before = session::Entity::find().count(&fixture.db.db).await.unwrap();
    // Hold the user lock so the finish verifies and computes its update, then waits.
    let tx = fixture.accounts.db.begin().await.unwrap();
    crate::dashboard::auth::accounts::lock_user(&tx, fixture.actor.user.id)
        .await
        .unwrap();
    let fixture = Arc::new(fixture);
    let task_fixture = fixture.clone();
    let task = tokio::spawn(async move {
        task_fixture
            .passkeys
            .authentication_finish(&task_fixture.accounts, challenge.challenge_id, response)
            .await
    });
    wait_for_lock_waiters(&fixture, 1).await;
    passkey::Entity::delete_by_id(fixture.key_id)
        .exec(&tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    assert!(matches!(
        tokio::time::timeout(Duration::from_secs(5), task)
            .await
            .unwrap()
            .unwrap(),
        Err(Error::Unauthorized)
    ));
    assert_eq!(
        session::Entity::find().count(&fixture.db.db).await.unwrap(),
        before
    );
}

#[tokio::test]
async fn no_webauthn_cpu_work_runs_after_the_user_lock() {
    let fixture = Fixture::new().await;
    let challenge = fixture
        .passkeys
        .authentication_start(&fixture.accounts, fixture.actor.user.username.clone())
        .await
        .unwrap();
    let response = fixture.authenticator.authenticate(&challenge, 2);
    let tx = fixture.accounts.db.begin().await.unwrap();
    crate::dashboard::auth::accounts::lock_user(&tx, fixture.actor.user.id)
        .await
        .unwrap();
    let fixture = Arc::new(fixture);
    let task_fixture = fixture.clone();
    let task = tokio::spawn(async move {
        task_fixture
            .passkeys
            .authentication_finish(&task_fixture.accounts, challenge.challenge_id, response)
            .await
    });
    wait_for_lock_waiters(&fixture, 1).await;
    // Once waiting on the lock, the finish needs no WebAuthn worker to complete.
    let permits = fixture
        .passkeys
        .cpu
        .clone()
        .acquire_many_owned(4)
        .await
        .unwrap();
    tx.rollback().await.unwrap();
    assert!(
        tokio::time::timeout(Duration::from_secs(5), task)
            .await
            .unwrap()
            .unwrap()
            .is_ok()
    );
    drop(permits);
}

async fn wait_for_lock_waiters(fixture: &Fixture, expected: i64) {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let row = fixture
                .db
                .db
                .query_one_raw(Statement::from_string(
                    DbBackend::Postgres,
                    "SELECT count(*) AS blocked FROM pg_stat_activity WHERE datname = current_database() AND backend_type = 'client backend' AND wait_event_type = 'Lock'",
                ))
                .await
                .unwrap()
                .unwrap();
            if row.try_get::<i64>("", "blocked").unwrap() == expected {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("authentication must reach the user lock");
}
