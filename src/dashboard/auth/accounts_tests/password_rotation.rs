use super::{audit_count, identity, stub_passkey};
use crate::dashboard::{
    auth::{
        accounts::{Accounts, lock_user},
        password::token_hash,
        sessions::{Identity, Sessions},
    },
    db::entities::{passkey, session, user},
    error::Error,
    test_support::{PASSWORD, TEMP_PASSWORD, TestDatabase},
};
use chrono::{Duration, Utc};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseConnection, DbBackend, EntityTrait,
    PaginatorTrait, QueryFilter, Set, Statement, TransactionTrait,
};

async fn mandatory_admin(test: &TestDatabase) -> (Accounts, Identity) {
    let accounts = Accounts::new(test.db.clone(), Duration::days(7))
        .await
        .unwrap();
    accounts
        .bootstrap_admin("admin".into(), TEMP_PASSWORD.into())
        .await
        .unwrap();
    let (_, session) = accounts
        .login("admin".into(), TEMP_PASSWORD.into())
        .await
        .unwrap();
    (accounts, identity(&test.db, &session).await)
}

#[tokio::test]
async fn mandatory_password_setup_omits_current_but_rejects_reuse_and_preserves_deadline() {
    let test = TestDatabase::new().await;
    let (accounts, original) = mandatory_admin(&test).await;
    let deadline = chrono::DateTime::from_timestamp_micros(
        (Utc::now() + Duration::minutes(4)).timestamp_micros(),
    )
    .unwrap();
    let tx = test.db.begin().await.unwrap();
    lock_user(&tx, original.user.id).await.unwrap();
    let session = Sessions::issue_until(&tx, original.user.id, deadline)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    let actor = identity(&test.db, &session).await;
    assert!(matches!(
        accounts
            .change_password(&actor, None, TEMP_PASSWORD.into())
            .await,
        Err(Error::Invalid(
            "new password must differ from current password"
        ))
    ));
    // Optional does not mean an explicitly supplied incorrect value is ignored.
    assert!(matches!(
        accounts
            .change_password(&actor, Some("incorrect-current".into()), PASSWORD.into())
            .await,
        Err(Error::Unauthorized)
    ));
    let (user, issued) = accounts
        .change_password(&actor, None, PASSWORD.into())
        .await
        .unwrap();
    assert!(!user.must_change_password);
    assert_eq!(issued.expires_at, deadline);
    for stale in [&actor, &original] {
        assert!(matches!(
            Sessions::authenticate(&test.db, &stale.token_hash).await,
            Err(Error::Unauthorized)
        ));
        assert!(matches!(
            accounts
                .change_password(stale, None, "another-password".into())
                .await,
            Err(Error::Unauthorized)
        ));
    }
    let ready = identity(&test.db, &issued).await;
    assert_eq!(ready.session_expires_at().unwrap(), deadline);
    assert!(matches!(
        accounts
            .change_password(&ready, None, "another-password".into())
            .await,
        Err(Error::Unauthorized)
    ));
    assert_eq!(
        audit_count(&test.db, Some(user.id), user.id, "password_changed").await,
        1
    );
}

#[tokio::test]
async fn mandatory_password_setup_rechecks_session_hash_and_flag_after_waiting_for_user_lock() {
    for race in ["hash", "flag", "expired", "revoked"] {
        let test = TestDatabase::new().await;
        let (accounts, actor) = mandatory_admin(&test).await;
        let gate = test.db.begin().await.unwrap();
        lock_user(&gate, actor.user.id).await.unwrap();
        let (changed, ()) = tokio::join!(
            accounts.change_password(&actor, None, PASSWORD.into()),
            async {
                wait_for_lock_waiters(&test.db, 1).await;
                match race {
                    "hash" | "flag" => {
                        let mut update = user::ActiveModel {
                            id: Set(actor.user.id),
                            ..Default::default()
                        };
                        if race == "hash" {
                            update.password_hash = Set("synthetic-raced-hash".into());
                        } else {
                            update.must_change_password = Set(false);
                        }
                        update.update(&gate).await.unwrap();
                    }
                    "expired" => {
                        session::ActiveModel {
                            token_hash: Set(actor.token_hash.clone()),
                            expires_at: Set(Utc::now() - Duration::seconds(1)),
                            ..Default::default()
                        }
                        .update(&gate)
                        .await
                        .unwrap();
                    }
                    "revoked" => Sessions::revoke_all(&gate, actor.user.id).await.unwrap(),
                    _ => unreachable!(),
                }
                gate.commit().await.unwrap();
            }
        );
        assert!(matches!(changed, Err(Error::Unauthorized)), "race: {race}");
        let unchanged = user::Entity::find_by_id(actor.user.id)
            .one(&test.db)
            .await
            .unwrap()
            .unwrap();
        assert!(
            unchanged.password_hash
                == if race == "hash" {
                    "synthetic-raced-hash"
                } else {
                    &actor.user.password_hash
                }
        );
        assert_eq!(unchanged.must_change_password, race != "flag");
        assert_eq!(
            audit_count(
                &test.db,
                Some(actor.user.id),
                actor.user.id,
                "password_changed"
            )
            .await,
            0
        );
    }
}

#[tokio::test]
async fn reissued_password_revokes_old_mandatory_session_and_new_login_can_omit_current() {
    let test = TestDatabase::new().await;
    let (accounts, admin) = test.ready_admin().await;
    let target = accounts
        .create_user(&admin, "ordinary".into(), TEMP_PASSWORD.into())
        .await
        .unwrap();
    let (_, old) = accounts
        .login("ordinary".into(), TEMP_PASSWORD.into())
        .await
        .unwrap();
    let stale = identity(&test.db, &old).await;
    accounts
        .reset_password(&admin, target.id, "reissued-temporary-password".into())
        .await
        .unwrap();
    assert!(matches!(
        accounts
            .change_password(&stale, None, PASSWORD.into())
            .await,
        Err(Error::Unauthorized)
    ));
    let (_, login) = accounts
        .login("ordinary".into(), "reissued-temporary-password".into())
        .await
        .unwrap();
    let actor = identity(&test.db, &login).await;
    assert!(matches!(
        accounts
            .change_password(&actor, None, "reissued-temporary-password".into())
            .await,
        Err(Error::Invalid(_))
    ));
    let (user, issued) = accounts
        .change_password(&actor, None, PASSWORD.into())
        .await
        .unwrap();
    assert!(!user.must_change_password);
    assert_eq!(issued.expires_at, actor.session_expires_at().unwrap());
}

async fn wait_for_lock_waiters(db: &DatabaseConnection, expected: i64) {
    // Each fixture owns a cluster; only these password-change transactions can be lock waiters.
    tokio::time::timeout(std::time::Duration::from_secs(30), async {
        loop {
            let row = db.query_one_raw(Statement::from_string(
                DbBackend::Postgres,
                "SELECT count(*) AS waiting FROM pg_stat_activity WHERE datname = current_database() AND wait_event_type = 'Lock'",
            )).await.map_err(Error::from).unwrap().unwrap();
            let waiting: i64 = row.try_get("", "waiting").map_err(Error::from).unwrap();
            if waiting >= expected { break; }
            tokio::task::yield_now().await;
        }
    }).await.expect("password changes must reach the held user lock");
}

#[tokio::test]
async fn password_rotation_preserves_original_deadline_revokes_others_and_validates_input() {
    let test = TestDatabase::new().await;
    let (accounts, admin) = test.ready_admin().await;
    let deadline = chrono::DateTime::from_timestamp_micros(
        (Utc::now() + Duration::minutes(4)).timestamp_micros(),
    )
    .unwrap();
    let tx = test.db.begin().await.unwrap();
    lock_user(&tx, admin.user.id).await.unwrap();
    let original = Sessions::issue_until(&tx, admin.user.id, deadline)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    let actor = identity(&test.db, &original).await;
    let (_, another) = accounts
        .login("admin".to_owned(), PASSWORD.to_owned())
        .await
        .unwrap();
    let key = stub_passkey(&test.db, admin.user.id).await;
    assert!(matches!(
        accounts
            .change_password(&actor, None, "replacement-password".to_owned())
            .await,
        Err(Error::Unauthorized)
    ));
    assert!(matches!(
        accounts
            .change_password(
                &actor,
                Some("incorrect-current".to_owned()),
                "replacement-password".to_owned()
            )
            .await,
        Err(Error::Unauthorized)
    ));
    assert!(matches!(
        accounts
            .change_password(&actor, Some(PASSWORD.to_owned()), PASSWORD.to_owned())
            .await,
        Err(Error::Invalid(_))
    ));
    assert!(matches!(
        accounts
            .change_password(&actor, Some(PASSWORD.to_owned()), "short".to_owned())
            .await,
        Err(Error::Invalid(_))
    ));
    assert!(
        Sessions::authenticate(&test.db, &actor.token_hash)
            .await
            .is_ok()
    );
    let (updated, replacement) = accounts
        .change_password(
            &actor,
            Some(PASSWORD.to_owned()),
            "replacement-password".to_owned(),
        )
        .await
        .unwrap();
    assert!(!updated.must_change_password);
    assert_eq!(replacement.expires_at, deadline);
    for hash in [
        &actor.token_hash,
        &admin.token_hash,
        &token_hash(&another.token).unwrap(),
    ] {
        assert!(matches!(
            Sessions::authenticate(&test.db, hash).await,
            Err(Error::Unauthorized)
        ));
    }
    assert_eq!(
        identity(&test.db, &replacement)
            .await
            .session_expires_at()
            .unwrap(),
        deadline
    );
    assert_eq!(
        session::Entity::find()
            .filter(session::Column::UserId.eq(admin.user.id))
            .count(&test.db)
            .await
            .unwrap(),
        1
    );
    assert!(
        passkey::Entity::find_by_id(key)
            .one(&test.db)
            .await
            .unwrap()
            .is_some()
    );
    assert!(matches!(
        accounts
            .login("admin".to_owned(), PASSWORD.to_owned())
            .await,
        Err(Error::Unauthorized)
    ));
    assert!(
        accounts
            .login("admin".to_owned(), "replacement-password".to_owned())
            .await
            .is_ok()
    );
    assert_eq!(
        audit_count(
            &test.db,
            Some(admin.user.id),
            admin.user.id,
            "password_changed"
        )
        .await,
        2
    );
}

#[tokio::test]
async fn password_change_reauthenticates_deadline_instead_of_trusting_live_snapshot() {
    let test = TestDatabase::new().await;
    let (accounts, actor) = test.ready_admin().await;
    assert!(actor.session_expires_at().unwrap() > Utc::now());
    let tx = test.db.begin().await.unwrap();
    lock_user(&tx, actor.user.id).await.unwrap();
    let (changed, ()) = tokio::join!(
        accounts.change_password(
            &actor,
            Some(PASSWORD.to_owned()),
            "replacement-password".to_owned()
        ),
        async {
            wait_for_lock_waiters(&test.db, 1).await;
            // Expire the persisted session only after the password work reaches the lock.
            session::ActiveModel {
                token_hash: Set(actor.token_hash.clone()),
                expires_at: Set(Utc::now() - Duration::seconds(1)),
                ..Default::default()
            }
            .update(&tx)
            .await
            .map_err(Error::from)
            .unwrap();
            tx.commit().await.map_err(Error::from).unwrap();
        },
    );
    assert!(matches!(changed, Err(Error::Unauthorized)));
    let unchanged = user::Entity::find_by_id(actor.user.id)
        .one(&test.db)
        .await
        .unwrap()
        .unwrap();
    assert!(unchanged.password_hash == actor.user.password_hash);
    assert_eq!(
        audit_count(
            &test.db,
            Some(actor.user.id),
            actor.user.id,
            "password_changed"
        )
        .await,
        1
    );
}

#[tokio::test]
async fn concurrent_password_changes_from_same_authenticated_snapshot_have_one_winner() {
    let test = TestDatabase::new().await;
    let (accounts, admin) = test.ready_admin().await;
    let first = Sessions::authenticate(&test.db, &admin.token_hash)
        .await
        .unwrap();
    let second = Sessions::authenticate(&test.db, &admin.token_hash)
        .await
        .unwrap();
    let gate = test.db.begin().await.map_err(Error::from).unwrap();
    lock_user(&gate, admin.user.id).await.unwrap();
    let (a, b, ()) = tokio::join!(
        accounts.change_password(
            &first,
            Some(PASSWORD.to_owned()),
            "first-replacement-password".to_owned()
        ),
        accounts.change_password(
            &second,
            Some(PASSWORD.to_owned()),
            "second-replacement-password".to_owned()
        ),
        async {
            wait_for_lock_waiters(&test.db, 2).await;
            gate.commit().await.map_err(Error::from).unwrap();
        },
    );
    let (winner, loser, password) = if a.is_ok() {
        (a, b, "first-replacement-password")
    } else {
        (b, a, "second-replacement-password")
    };
    let (_, issued) = winner.unwrap();
    assert!(matches!(loser, Err(Error::Unauthorized)));
    assert_eq!(issued.expires_at, admin.session_expires_at().unwrap());
    assert!(identity(&test.db, &issued).await.user.id == admin.user.id);
    assert!(matches!(
        Sessions::authenticate(&test.db, &admin.token_hash).await,
        Err(Error::Unauthorized)
    ));
    assert_eq!(
        session::Entity::find()
            .filter(session::Column::UserId.eq(admin.user.id))
            .count(&test.db)
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        audit_count(
            &test.db,
            Some(admin.user.id),
            admin.user.id,
            "password_changed"
        )
        .await,
        2
    );
    assert!(
        accounts
            .login("admin".to_owned(), password.to_owned())
            .await
            .is_ok()
    );
}

/// Spends the whole in-session password budget without paying for Argon2 in debug builds.
fn exhaust_reauth(accounts: &Accounts, user: uuid::Uuid) {
    let now = std::time::Instant::now();
    for _ in 0..1000 {
        if accounts.reauth_limit.admit(user, now).is_err() {
            return;
        }
    }
    panic!("current-password budget never ran out");
}

#[tokio::test]
async fn current_password_checks_are_limited_per_user_without_touching_sign_in() {
    let test = TestDatabase::new().await;
    let (accounts, admin) = test.ready_admin().await;
    exhaust_reauth(&accounts, admin.user.id);
    // Even the correct current password is not checked once the budget is spent.
    assert!(matches!(
        accounts
            .change_password(&admin, Some(PASSWORD.into()), "another-password".into())
            .await,
        Err(Error::RateLimited)
    ));
    let stored = user::Entity::find_by_id(admin.user.id)
        .one(&test.db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored.password_hash, admin.user.password_hash);
    assert!(
        Sessions::authenticate(&test.db, &admin.token_hash)
            .await
            .is_ok()
    );
    // Sign-in keeps its own per-username budget.
    assert_eq!(accounts.login_attempts(&admin.user.username), 1);
    assert!(
        accounts
            .login(admin.user.username.clone(), PASSWORD.into())
            .await
            .is_ok()
    );
    // Another user's budget is untouched.
    accounts
        .create_user(&admin, "other-member".into(), TEMP_PASSWORD.into())
        .await
        .unwrap();
    let (_, issued) = accounts
        .login("other-member".into(), TEMP_PASSWORD.into())
        .await
        .unwrap();
    let other = identity(&test.db, &issued).await;
    assert!(
        accounts
            .change_password(&other, Some(TEMP_PASSWORD.into()), PASSWORD.into())
            .await
            .is_ok()
    );
}
