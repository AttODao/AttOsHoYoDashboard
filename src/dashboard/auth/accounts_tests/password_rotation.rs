use super::{audit_count, identity, stub_passkey};
use crate::dashboard::{
    auth::{accounts::lock_user, password::token_hash, sessions::Sessions},
    db::entities::{passkey, session, user},
    error::Error,
    test_support::{PASSWORD, TestDatabase},
};
use chrono::{Duration, Utc};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseConnection, DbBackend, EntityTrait,
    PaginatorTrait, QueryFilter, Set, Statement, TransactionTrait,
};

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
