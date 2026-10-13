use super::{IssuedSession, Sessions};
use crate::dashboard::{
    auth::{accounts::lock_user, password},
    db::entities::{session, user},
    error::Error,
    test_support::TestDatabase,
};
use chrono::{DateTime, Duration, Utc};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, IntoActiveModel,
    PaginatorTrait, QueryFilter, Set, TransactionTrait,
};
use uuid::Uuid;

async fn issue_locked(db: &DatabaseConnection, sessions: &Sessions, owner: Uuid) -> IssuedSession {
    let tx = db.begin().await.unwrap();
    lock_user(&tx, owner).await.unwrap();
    let issued = sessions.issue(&tx, owner).await.unwrap();
    tx.commit().await.unwrap();
    issued
}

async fn issue_until_locked(
    db: &DatabaseConnection,
    owner: Uuid,
    deadline: DateTime<Utc>,
) -> IssuedSession {
    let tx = db.begin().await.unwrap();
    lock_user(&tx, owner).await.unwrap();
    let issued = Sessions::issue_until(&tx, owner, deadline).await.unwrap();
    tx.commit().await.unwrap();
    issued
}

async fn other_user(db: &DatabaseConnection, template: &user::Model) -> user::Model {
    user::ActiveModel {
        id: Set(Uuid::new_v4()),
        username: Set("session-owner".to_owned()),
        password_hash: Set(template.password_hash.clone()),
        is_admin: Set(false),
        active: Set(true),
        must_change_password: Set(false),
        created_at: Set(Utc::now()),
    }
    .insert(db)
    .await
    .map_err(Error::from)
    .unwrap()
}

#[tokio::test]
async fn issuance_stores_only_hash_and_authentication_keeps_fixed_ttl() {
    let test = TestDatabase::new().await;
    let (_, admin) = test.ready_admin().await;
    let ttl = Duration::minutes(7);
    let sessions = Sessions::new(ttl);
    let before = Utc::now();
    let issued = issue_locked(&test.db, &sessions, admin.user.id).await;
    let after = Utc::now();
    assert!(issued.expires_at >= before + ttl);
    assert!(issued.expires_at <= after + ttl);
    let hash = password::token_hash(&issued.token).unwrap();
    assert!(hash != issued.token);
    assert!(
        session::Entity::find_by_id(&issued.token)
            .one(&test.db)
            .await
            .unwrap()
            .is_none()
    );
    let stored = session::Entity::find_by_id(&hash)
        .one(&test.db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored.user_id, admin.user.id);
    assert_eq!(
        stored.expires_at.timestamp_micros(),
        issued.expires_at.timestamp_micros()
    );
    let first = Sessions::authenticate(&test.db, &hash).await.unwrap();
    let second = Sessions::authenticate(&test.db, &hash).await.unwrap();
    assert_eq!(first.user.id, admin.user.id);
    assert_eq!(second.expires_at, first.expires_at);
    // A different configured TTL applies only to new issuance.
    let later = issue_locked(&test.db, &Sessions::new(Duration::hours(2)), admin.user.id).await;
    assert!(later.expires_at > issued.expires_at);
    assert_eq!(
        Sessions::authenticate(&test.db, &hash)
            .await
            .unwrap()
            .expires_at,
        first.expires_at
    );
    assert!(matches!(
        Sessions::authenticate(&test.db, &"0".repeat(64)).await,
        Err(Error::Unauthorized)
    ));
}

#[tokio::test]
async fn expiry_purge_and_joined_inactive_user_are_enforced() {
    let test = TestDatabase::new().await;
    let (_, admin) = test.ready_admin().await;
    let other = other_user(&test.db, &admin.user).await;
    let expired =
        issue_until_locked(&test.db, admin.user.id, Utc::now() - Duration::seconds(1)).await;
    let boundary = issue_until_locked(&test.db, admin.user.id, Utc::now()).await;
    let live = issue_locked(&test.db, &Sessions::new(Duration::hours(1)), admin.user.id).await;
    let inactive = issue_locked(&test.db, &Sessions::new(Duration::hours(1)), other.id).await;
    let mut update = other.into_active_model();
    update.active = Set(false);
    update.update(&test.db).await.map_err(Error::from).unwrap();
    for token in [&expired.token, &boundary.token, &inactive.token] {
        assert!(matches!(
            Sessions::authenticate(&test.db, &password::token_hash(token).unwrap()).await,
            Err(Error::Unauthorized)
        ));
    }
    Sessions::purge_expired(&test.db).await.unwrap();
    Sessions::purge_expired(&test.db).await.unwrap();
    for token in [&expired.token, &boundary.token] {
        assert!(
            session::Entity::find_by_id(password::token_hash(token).unwrap())
                .one(&test.db)
                .await
                .unwrap()
                .is_none()
        );
    }
    assert!(
        session::Entity::find_by_id(password::token_hash(&inactive.token).unwrap())
            .one(&test.db)
            .await
            .unwrap()
            .is_some()
    );
    assert!(
        Sessions::authenticate(&test.db, &password::token_hash(&live.token).unwrap())
            .await
            .is_ok()
    );
}

#[tokio::test]
async fn revoke_all_is_owner_scoped_and_preserves_other_sessions() {
    let test = TestDatabase::new().await;
    let (_, admin) = test.ready_admin().await;
    let other = other_user(&test.db, &admin.user).await;
    let sessions = Sessions::new(Duration::hours(1));
    let one = issue_locked(&test.db, &sessions, admin.user.id).await;
    let two = issue_locked(&test.db, &sessions, admin.user.id).await;
    assert!(one.token != two.token);
    let independent = issue_locked(&test.db, &sessions, other.id).await;
    let tx = test.db.begin().await.unwrap();
    lock_user(&tx, admin.user.id).await.unwrap();
    Sessions::revoke_all(&tx, admin.user.id).await.unwrap();
    tx.commit().await.unwrap();
    for token in [&one.token, &two.token] {
        assert!(matches!(
            Sessions::authenticate(&test.db, &password::token_hash(token).unwrap()).await,
            Err(Error::Unauthorized)
        ));
    }
    let identity =
        Sessions::authenticate(&test.db, &password::token_hash(&independent.token).unwrap())
            .await
            .unwrap();
    assert_eq!(identity.user.id, other.id);
    assert_eq!(
        session::Entity::find()
            .filter(session::Column::UserId.eq(admin.user.id))
            .count(&test.db)
            .await
            .unwrap(),
        0
    );
}

#[tokio::test]
async fn issue_until_preserves_deadline_and_sql_owner_fk_cascades() {
    let test = TestDatabase::new().await;
    let (_, admin) = test.ready_admin().await;
    let other = other_user(&test.db, &admin.user).await;
    // PostgreSQL stores microsecond precision; use an exactly representable deadline.
    let deadline =
        DateTime::from_timestamp_micros((Utc::now() + Duration::minutes(3)).timestamp_micros())
            .unwrap();
    let issued = issue_until_locked(&test.db, other.id, deadline).await;
    assert_eq!(issued.expires_at, deadline);
    let hash = password::token_hash(&issued.token).unwrap();
    assert_eq!(
        Sessions::authenticate(&test.db, &hash)
            .await
            .unwrap()
            .expires_at,
        deadline
    );
    assert!(matches!(
        Sessions::issue_until(&test.db, Uuid::new_v4(), deadline).await,
        Err(Error::Database(_))
    ));
    user::Entity::delete_by_id(other.id)
        .exec(&test.db)
        .await
        .unwrap();
    assert!(
        session::Entity::find_by_id(&hash)
            .one(&test.db)
            .await
            .unwrap()
            .is_none()
    );
    assert!(matches!(
        Sessions::authenticate(&test.db, &hash).await,
        Err(Error::Unauthorized)
    ));
    assert!(
        Sessions::authenticate(&test.db, &admin.token_hash)
            .await
            .is_ok()
    );
}
