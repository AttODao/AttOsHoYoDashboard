use super::{Accounts, lock_user, require_admin, require_ready, validate_username};
use crate::dashboard::{
    auth::{
        password::token_hash,
        sessions::{Identity, IssuedSession, Sessions},
    },
    db::entities::{audit, passkey, session, user},
    error::Error,
    test_support::{PASSWORD, TEMP_PASSWORD, TestDatabase},
};
use chrono::{Duration, Utc};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseConnection, DbBackend, EntityTrait,
    PaginatorTrait, QueryFilter, Set, Statement, TransactionTrait,
};
use uuid::Uuid;

fn policy_user() -> user::Model {
    user::Model {
        id: Uuid::new_v4(),
        username: "policy-user".to_owned(),
        password_hash: String::new(),
        is_admin: false,
        active: true,
        must_change_password: false,
        created_at: Utc::now(),
    }
}

async fn audit_count(
    db: &DatabaseConnection,
    actor: Option<Uuid>,
    target: Uuid,
    action: &str,
) -> u64 {
    let rows = audit::Entity::find()
        .filter(audit::Column::TargetUserId.eq(target))
        .filter(audit::Column::Action.eq(action))
        .all(db)
        .await
        .unwrap();
    assert!(rows.iter().all(|row| row.actor_user_id == actor));
    rows.len() as u64
}

async fn stub_passkey(db: &DatabaseConnection, owner: Uuid) -> Uuid {
    // Recovery only deletes rows: WebAuthn cryptography is tested by its own responsibility.
    let id = Uuid::new_v4();
    passkey::ActiveModel {
        id: Set(id),
        user_id: Set(owner),
        credential_id: Set(id.as_bytes().to_vec()),
        credential: Set(serde_json::json!({"test_stub": true})),
        label: Set("test credential".to_owned()),
        created_at: Set(Utc::now()),
    }
    .insert(db)
    .await
    .map_err(Error::from)
    .unwrap();
    id
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

async fn identity(db: &DatabaseConnection, issued: &IssuedSession) -> Identity {
    Sessions::authenticate(db, &token_hash(&issued.token).unwrap())
        .await
        .unwrap()
}

#[test]
fn username_validation_has_exact_ascii_bounds() {
    for name in ["a", "Admin09._-", &"a".repeat(64)] {
        assert!(validate_username(name).is_ok());
    }
    for name in ["", " a", "a b", "a/b", "a@b", "a\n", "é", &"a".repeat(65)] {
        assert!(matches!(validate_username(name), Err(Error::Invalid(_))));
    }
}

#[test]
fn readiness_and_admin_policy_are_shared() {
    let mut user = policy_user();
    assert!(require_ready(&user).is_ok());
    assert!(matches!(require_admin(&user), Err(Error::Forbidden)));
    user.is_admin = true;
    assert!(require_admin(&user).is_ok());
    user.must_change_password = true;
    assert!(matches!(
        require_ready(&user),
        Err(Error::PasswordChangeRequired)
    ));
    assert!(matches!(
        require_admin(&user),
        Err(Error::PasswordChangeRequired)
    ));
    user.active = false;
    assert!(matches!(require_ready(&user), Err(Error::Unauthorized)));
    assert!(matches!(require_admin(&user), Err(Error::Unauthorized)));
}

#[tokio::test]
async fn bootstrap_requires_empty_table_and_temporary_admin_must_change_password() {
    let test = TestDatabase::new().await;
    let accounts = Accounts::new(test.db.clone(), Duration::hours(1))
        .await
        .unwrap();
    let admin = accounts
        .bootstrap_admin("admin".to_owned(), TEMP_PASSWORD.to_owned())
        .await
        .unwrap();
    assert!(admin.is_admin && admin.active && admin.must_change_password);
    assert_eq!(
        audit_count(&test.db, None, admin.id, "bootstrap_admin").await,
        1
    );
    assert!(matches!(
        accounts
            .bootstrap_admin("second".to_owned(), TEMP_PASSWORD.to_owned())
            .await,
        Err(Error::Invalid(_))
    ));
    assert_eq!(user::Entity::find().count(&test.db).await.unwrap(), 1);
    assert!(matches!(
        accounts
            .login("unknown".to_owned(), TEMP_PASSWORD.to_owned())
            .await,
        Err(Error::Unauthorized)
    ));
    assert!(matches!(
        accounts
            .login("admin".to_owned(), "wrong-password".to_owned())
            .await,
        Err(Error::Unauthorized)
    ));
    let (_, issued) = accounts
        .login("admin".to_owned(), TEMP_PASSWORD.to_owned())
        .await
        .unwrap();
    let actor = identity(&test.db, &issued).await;
    assert!(matches!(
        accounts.list_users(&actor).await,
        Err(Error::PasswordChangeRequired)
    ));
    assert!(matches!(
        accounts
            .create_user(&actor, "blocked".to_owned(), TEMP_PASSWORD.to_owned())
            .await,
        Err(Error::PasswordChangeRequired)
    ));
    // Temporary identities remain able to log out and change the password.
    let (_, second) = accounts
        .login("admin".to_owned(), TEMP_PASSWORD.to_owned())
        .await
        .unwrap();
    accounts.logout(&actor).await.unwrap();
    assert!(matches!(
        Sessions::authenticate(&test.db, &actor.token_hash).await,
        Err(Error::Unauthorized)
    ));
    assert!(matches!(
        accounts.logout(&actor).await,
        Err(Error::Unauthorized)
    ));
    // Logout revokes this token, not every session owned by the same user.
    let actor = identity(&test.db, &second).await;
    let (ready, replacement) = accounts
        .change_password(&actor, TEMP_PASSWORD.to_owned(), PASSWORD.to_owned())
        .await
        .unwrap();
    assert!(!ready.must_change_password);
    let ready_actor = identity(&test.db, &replacement).await;
    assert_eq!(accounts.list_users(&ready_actor).await.unwrap().len(), 1);
    assert_eq!(
        audit_count(&test.db, Some(admin.id), admin.id, "password_changed").await,
        1
    );
}

#[tokio::test]
async fn creation_and_recovery_audit_temporary_password_and_delete_only_target_passkeys() {
    let test = TestDatabase::new().await;
    let (accounts, admin) = test.ready_admin().await;
    let target = accounts
        .create_user(&admin, "member".to_owned(), TEMP_PASSWORD.to_owned())
        .await
        .unwrap();
    assert!(target.active && target.must_change_password && !target.is_admin);
    assert_eq!(
        audit_count(&test.db, Some(admin.user.id), target.id, "user_created").await,
        1
    );
    assert!(matches!(
        accounts
            .create_user(&admin, "member".to_owned(), TEMP_PASSWORD.to_owned())
            .await,
        Err(Error::Conflict)
    ));
    assert_eq!(
        audit_count(&test.db, Some(admin.user.id), target.id, "user_created").await,
        1
    );
    let (_, first) = accounts
        .login("member".to_owned(), TEMP_PASSWORD.to_owned())
        .await
        .unwrap();
    let actor = identity(&test.db, &first).await;
    assert!(matches!(
        require_ready(&actor.user),
        Err(Error::PasswordChangeRequired)
    ));
    let (_, changed) = accounts
        .change_password(&actor, TEMP_PASSWORD.to_owned(), PASSWORD.to_owned())
        .await
        .unwrap();
    let (_, other) = accounts
        .login("member".to_owned(), PASSWORD.to_owned())
        .await
        .unwrap();
    let target_key = stub_passkey(&test.db, target.id).await;
    let second_target_key = stub_passkey(&test.db, target.id).await;
    let admin_key = stub_passkey(&test.db, admin.user.id).await;
    let recovery = "recovery-temporary-password";
    accounts
        .reset_password(&admin, target.id, recovery.to_owned())
        .await
        .unwrap();
    for token in [&changed.token, &other.token] {
        assert!(matches!(
            Sessions::authenticate(&test.db, &token_hash(token).unwrap()).await,
            Err(Error::Unauthorized)
        ));
    }
    for key in [target_key, second_target_key] {
        assert!(
            passkey::Entity::find_by_id(key)
                .one(&test.db)
                .await
                .unwrap()
                .is_none()
        );
    }
    assert!(
        passkey::Entity::find_by_id(admin_key)
            .one(&test.db)
            .await
            .unwrap()
            .is_some()
    );
    assert!(
        Sessions::authenticate(&test.db, &admin.token_hash)
            .await
            .is_ok()
    );
    assert!(matches!(
        accounts
            .login("member".to_owned(), PASSWORD.to_owned())
            .await,
        Err(Error::Unauthorized)
    ));
    let (recovered, _) = accounts
        .login("member".to_owned(), recovery.to_owned())
        .await
        .unwrap();
    assert!(recovered.must_change_password && recovered.active);
    assert!(matches!(
        require_ready(&recovered),
        Err(Error::PasswordChangeRequired)
    ));
    assert_eq!(
        audit_count(&test.db, Some(admin.user.id), target.id, "password_reset").await,
        1
    );
}

#[tokio::test]
async fn disable_enable_keeps_password_and_passkeys_but_never_revives_old_sessions() {
    let test = TestDatabase::new().await;
    let (accounts, admin) = test.ready_admin().await;
    let target = accounts
        .create_user(&admin, "member".to_owned(), TEMP_PASSWORD.to_owned())
        .await
        .unwrap();
    let (_, issued) = accounts
        .login("member".to_owned(), TEMP_PASSWORD.to_owned())
        .await
        .unwrap();
    let key = stub_passkey(&test.db, target.id).await;
    accounts.set_active(&admin, target.id, false).await.unwrap();
    let disabled = user::Entity::find_by_id(target.id)
        .one(&test.db)
        .await
        .unwrap()
        .unwrap();
    assert!(!disabled.active);
    assert!(disabled.password_hash == target.password_hash);
    assert_eq!(disabled.must_change_password, target.must_change_password);
    assert!(
        passkey::Entity::find_by_id(key)
            .one(&test.db)
            .await
            .unwrap()
            .is_some()
    );
    assert!(matches!(
        accounts
            .login("member".to_owned(), TEMP_PASSWORD.to_owned())
            .await,
        Err(Error::Unauthorized)
    ));
    accounts.set_active(&admin, target.id, true).await.unwrap();
    assert!(matches!(
        Sessions::authenticate(&test.db, &token_hash(&issued.token).unwrap()).await,
        Err(Error::Unauthorized)
    ));
    assert!(
        accounts
            .login("member".to_owned(), TEMP_PASSWORD.to_owned())
            .await
            .is_ok()
    );
    assert!(
        passkey::Entity::find_by_id(key)
            .one(&test.db)
            .await
            .unwrap()
            .is_some()
    );
    assert!(matches!(
        accounts.set_active(&admin, admin.user.id, false).await,
        Err(Error::Invalid(_))
    ));
    assert!(matches!(
        accounts.set_active(&admin, Uuid::new_v4(), false).await,
        Err(Error::NotFound)
    ));
    assert_eq!(
        audit_count(&test.db, Some(admin.user.id), target.id, "user_disabled").await,
        1
    );
    assert_eq!(
        audit_count(&test.db, Some(admin.user.id), target.id, "user_enabled").await,
        1
    );
}

#[tokio::test]
async fn admin_mutations_reauthenticate_instead_of_trusting_snapshot_or_other_owner() {
    let test = TestDatabase::new().await;
    let (accounts, admin) = test.ready_admin().await;
    let target = accounts
        .create_user(&admin, "member".to_owned(), TEMP_PASSWORD.to_owned())
        .await
        .unwrap();
    let (_, issued) = accounts
        .login("member".to_owned(), TEMP_PASSWORD.to_owned())
        .await
        .unwrap();
    let temporary = identity(&test.db, &issued).await;
    let (_, issued) = accounts
        .change_password(&temporary, TEMP_PASSWORD.to_owned(), PASSWORD.to_owned())
        .await
        .unwrap();
    let member = identity(&test.db, &issued).await;
    assert!(matches!(
        accounts.list_users(&member).await,
        Err(Error::Forbidden)
    ));
    assert!(matches!(
        accounts
            .reset_password(&member, admin.user.id, TEMP_PASSWORD.to_owned())
            .await,
        Err(Error::Forbidden)
    ));
    assert!(matches!(
        accounts.set_active(&member, admin.user.id, false).await,
        Err(Error::Forbidden)
    ));
    assert!(matches!(
        accounts
            .create_user(&member, "no-admin".to_owned(), TEMP_PASSWORD.to_owned())
            .await,
        Err(Error::Forbidden)
    ));
    // A ready admin snapshot paired with a different, nonadmin owner's token is not authority.
    let mixed = Identity {
        user: admin.user.clone(),
        token_hash: member.token_hash.clone(),
        expires_at: member.expires_at,
    };
    assert!(matches!(
        accounts
            .create_user(&mixed, "mixed-owner".to_owned(), TEMP_PASSWORD.to_owned())
            .await,
        Err(Error::Unauthorized)
    ));
    assert!(matches!(
        accounts
            .reset_password(&mixed, target.id, TEMP_PASSWORD.to_owned())
            .await,
        Err(Error::Unauthorized)
    ));
    assert!(matches!(
        accounts.set_active(&mixed, target.id, false).await,
        Err(Error::Unauthorized)
    ));
    // A previously ready administrator snapshot cannot bypass current readiness.
    user::ActiveModel {
        id: Set(admin.user.id),
        must_change_password: Set(true),
        ..Default::default()
    }
    .update(&test.db)
    .await
    .map_err(Error::from)
    .unwrap();
    assert!(matches!(
        accounts
            .create_user(&admin, "not-ready".to_owned(), TEMP_PASSWORD.to_owned())
            .await,
        Err(Error::PasswordChangeRequired)
    ));
    assert!(matches!(
        accounts
            .reset_password(&admin, target.id, TEMP_PASSWORD.to_owned())
            .await,
        Err(Error::PasswordChangeRequired)
    ));
    assert!(matches!(
        accounts.set_active(&admin, target.id, false).await,
        Err(Error::PasswordChangeRequired)
    ));
    user::ActiveModel {
        id: Set(admin.user.id),
        must_change_password: Set(false),
        ..Default::default()
    }
    .update(&test.db)
    .await
    .map_err(Error::from)
    .unwrap();
    accounts.logout(&admin).await.unwrap();
    assert!(matches!(
        accounts.list_users(&admin).await,
        Err(Error::Unauthorized)
    ));
    assert!(matches!(
        accounts
            .create_user(&admin, "stale-admin".to_owned(), TEMP_PASSWORD.to_owned())
            .await,
        Err(Error::Unauthorized)
    ));
    assert!(matches!(
        accounts
            .reset_password(&admin, target.id, TEMP_PASSWORD.to_owned())
            .await,
        Err(Error::Unauthorized)
    ));
    assert!(matches!(
        accounts.set_active(&admin, target.id, false).await,
        Err(Error::Unauthorized)
    ));
    assert_eq!(user::Entity::find().count(&test.db).await.unwrap(), 2);
    assert_eq!(
        audit_count(&test.db, Some(admin.user.id), target.id, "password_reset").await,
        0
    );
    assert!(
        user::Entity::find_by_id(target.id)
            .one(&test.db)
            .await
            .unwrap()
            .unwrap()
            .active
    );
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
            .change_password(
                &actor,
                "incorrect-current".to_owned(),
                "replacement-password".to_owned()
            )
            .await,
        Err(Error::Unauthorized)
    ));
    assert!(matches!(
        accounts
            .change_password(&actor, PASSWORD.to_owned(), PASSWORD.to_owned())
            .await,
        Err(Error::Invalid(_))
    ));
    assert!(matches!(
        accounts
            .change_password(&actor, PASSWORD.to_owned(), "short".to_owned())
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
            PASSWORD.to_owned(),
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
    assert_eq!(identity(&test.db, &replacement).await.expires_at, deadline);
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
    assert!(actor.expires_at > Utc::now());
    let tx = test.db.begin().await.unwrap();
    lock_user(&tx, actor.user.id).await.unwrap();
    let (changed, ()) = tokio::join!(
        accounts.change_password(
            &actor,
            PASSWORD.to_owned(),
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
            PASSWORD.to_owned(),
            "first-replacement-password".to_owned()
        ),
        accounts.change_password(
            &second,
            PASSWORD.to_owned(),
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
    assert_eq!(issued.expires_at, admin.expires_at);
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
