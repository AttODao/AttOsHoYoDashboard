use super::*;
use crate::dashboard::{
    db::entities::{audit, user},
    test_support::TestDatabase,
};
use sea_orm::{ConnectionTrait, PaginatorTrait, SqlErr};

const RAW: &str = "ltuid_v2=123; ltoken_v2=synthetic-cookie-token; ltmid_v2=synthetic-mid";
fn key() -> Arc<CookieKey> {
    Arc::new(CookieKey::from_hex(&"11".repeat(32)).unwrap())
}

#[tokio::test]
async fn pending_setup_is_encrypted_owned_audited_and_never_identity_proof() {
    let fixture = TestDatabase::new().await;
    let (_, actor) = fixture.ready_admin().await;
    let key = key();
    let store = CookieStore::new(fixture.db.clone(), Arc::clone(&key));
    assert!(store.status(&actor).await.unwrap().is_none());
    store.submit(&actor, RAW).await.unwrap();
    let first = store.status(&actor).await.unwrap().unwrap();
    assert_eq!(first.owner_user_id, actor.user.id);
    assert_eq!(first.hoyolab_uid, None);
    assert_eq!(first.auth_status, "pending_verification");
    assert_eq!(first.credential_version, 1);
    assert!(
        !first
            .sealed_credentials()
            .unwrap()
            .windows(b"synthetic-cookie-token".len())
            .any(|w| w == b"synthetic-cookie-token")
    );
    assert!(!format!("{first:?}").contains("123"));
    assert_eq!(
        key.open(actor.user.id, None, first.sealed_credentials().unwrap())
            .unwrap()
            .as_str(),
        Credentials::parse(RAW).unwrap().as_str()
    );
    // Pending claims can be corrected, but never populate the verified UID column.
    let updated = "ltuid_v2=456; ltoken_v2=synthetic-new-token; ltmid_v2=synthetic-mid";
    store.submit(&actor, updated).await.unwrap();
    let second = store.status(&actor).await.unwrap().unwrap();
    assert_eq!(second.id, first.id);
    assert_eq!(second.credential_version, 2);
    assert_eq!(second.hoyolab_uid, None);
    assert_eq!(
        key.open(actor.user.id, None, second.sealed_credentials().unwrap())
            .unwrap()
            .as_str(),
        Credentials::parse(updated).unwrap().as_str()
    );
    assert_eq!(
        audit::Entity::find()
            .filter(audit::Column::Action.eq("hoyolab_cookie_submitted"))
            .count(&fixture.db)
            .await
            .unwrap(),
        2
    );
}

#[tokio::test]
async fn forged_owner_with_another_users_session_cannot_read_or_write_credentials() {
    let fixture = TestDatabase::new().await;
    let (accounts, actor) = fixture.ready_admin().await;
    let other = accounts
        .create_user(
            &actor,
            "other-owner".into(),
            "temporary-test-password".into(),
        )
        .await
        .unwrap();
    let token = accounts
        .sessions
        .issue(&fixture.db, other.id)
        .await
        .unwrap();
    let hash = crate::dashboard::auth::password::token_hash(&token.token).unwrap();
    let forged = Identity {
        token_hash: hash,
        user: actor.user.clone(),
        expires_at: token.expires_at,
    };
    let store = CookieStore::new(fixture.db.clone(), key());
    store.submit(&actor, RAW).await.unwrap();
    let initial = store.status(&actor).await.unwrap().unwrap();
    assert!(matches!(
        store.status(&forged).await,
        Err(Error::Unauthorized)
    ));
    assert!(matches!(
        store.submit(&forged, RAW).await,
        Err(Error::Unauthorized)
    ));
    assert_eq!(store.status(&actor).await.unwrap().unwrap(), initial);
}

#[tokio::test]
async fn invalid_submission_keeps_prior_ciphertext_version_and_audit() {
    let fixture = TestDatabase::new().await;
    let (_, actor) = fixture.ready_admin().await;
    let store = CookieStore::new(fixture.db.clone(), key());
    store.submit(&actor, RAW).await.unwrap();
    let prior = store.status(&actor).await.unwrap().unwrap();
    assert!(matches!(
        store
            .submit(&actor, "ltuid_v2=123; ltoken_v2=one; ltoken_v2=two")
            .await,
        Err(Error::Invalid(_))
    ));
    assert_eq!(store.status(&actor).await.unwrap().unwrap(), prior);
    assert_eq!(
        audit::Entity::find()
            .filter(audit::Column::Action.eq("hoyolab_cookie_submitted"))
            .count(&fixture.db)
            .await
            .unwrap(),
        1
    );
}

#[tokio::test]
async fn reauthentication_rejects_recovery_disabled_and_changed_password_states() {
    let fixture = TestDatabase::new().await;
    let (accounts, actor) = fixture.ready_admin().await;
    let store = CookieStore::new(fixture.db.clone(), key());
    let mut row = actor.user.clone().into_active_model();
    row.must_change_password = Set(true);
    row.update(&fixture.db).await.unwrap();
    assert!(matches!(
        store.submit(&actor, RAW).await,
        Err(Error::PasswordChangeRequired)
    ));
    let mut row = actor.user.clone().into_active_model();
    row.active = Set(false);
    row.update(&fixture.db).await.unwrap();
    assert!(matches!(
        store.submit(&actor, RAW).await,
        Err(Error::Unauthorized)
    ));
    let mut row = actor.user.clone().into_active_model();
    row.active = Set(true);
    row.must_change_password = Set(false);
    row.update(&fixture.db).await.unwrap();
    accounts
        .reset_password(&actor, actor.user.id, "temporary-reset-password".into())
        .await
        .unwrap();
    assert!(matches!(
        store.submit(&actor, RAW).await,
        Err(Error::Unauthorized)
    ));
    assert!(matches!(
        store.status(&actor).await,
        Err(Error::Unauthorized)
    ));
    assert_eq!(
        hoyolab_account::Entity::find()
            .count(&fixture.db)
            .await
            .unwrap(),
        0
    );
}

#[tokio::test]
async fn stored_uid_cannot_be_replaced_by_a_cookie_claim_and_ciphertext_is_row_bound() {
    let fixture = TestDatabase::new().await;
    let (_, actor) = fixture.ready_admin().await;
    let store = CookieStore::new(fixture.db.clone(), key());
    store.submit(&actor, RAW).await.unwrap();
    // Legacy syntax still opens from storage but can no longer be submitted;
    // this fixture never calls live identity APIs.
    let raw = "ltuid=123; ltoken=synthetic-cookie-token";
    let pending = store.status(&actor).await.unwrap().unwrap();
    // Synthetic verified row only: this is not upstream evidence.
    let mut verified = pending.clone().into_active_model();
    verified.hoyolab_uid = Set(Some("123".into()));
    verified.auth_status = Set("verified".into());
    verified.encrypted_credentials = Set(Some(
        key()
            .seal(
                actor.user.id,
                Some("123"),
                &Credentials::parse(raw).unwrap(),
            )
            .unwrap(),
    ));
    let verified = verified.update(&fixture.db).await.unwrap();
    assert!(matches!(
        store
            .submit(&actor, "ltuid_v2=456; ltoken_v2=other; ltmid_v2=mid")
            .await,
        Err(Error::Forbidden)
    ));
    assert!(matches!(
        store.submit(&actor, raw).await,
        Err(Error::Invalid(_))
    ));
    assert_eq!(store.status(&actor).await.unwrap().unwrap(), verified);
    let mut corrupted = verified.into_active_model();
    corrupted.hoyolab_uid = Set(Some("456".into()));
    corrupted.update(&fixture.db).await.unwrap();
    assert!(matches!(store.status(&actor).await, Err(Error::Internal)));
}

#[tokio::test]
async fn simultaneous_pending_submissions_remain_one_row_with_monotonic_versions() {
    let fixture = TestDatabase::new().await;
    let (_, actor) = fixture.ready_admin().await;
    let store = CookieStore::new(fixture.db.clone(), key());
    let (a, b) = tokio::join!(store.submit(&actor, RAW), store.submit(&actor, RAW));
    a.unwrap();
    b.unwrap();
    assert_eq!(
        hoyolab_account::Entity::find()
            .count(&fixture.db)
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        store
            .status(&actor)
            .await
            .unwrap()
            .unwrap()
            .credential_version,
        2
    );
}

async fn user_without_audit(db: &DatabaseConnection, name: &str) -> user::Model {
    user::ActiveModel {
        id: Set(Uuid::new_v4()),
        username: Set(name.into()),
        password_hash: Set("synthetic-only".into()),
        is_admin: Set(false),
        active: Set(true),
        must_change_password: Set(false),
        created_at: Set(Utc::now()),
        last_login_at: Set(None),
    }
    .insert(db)
    .await
    .unwrap()
}
fn stored(owner: Uuid, uid: Option<&str>) -> hoyolab_account::ActiveModel {
    let now = Utc::now();
    hoyolab_account::ActiveModel {
        id: Set(Uuid::new_v4()),
        owner_user_id: Set(owner),
        hoyolab_uid: Set(uid.map(str::to_owned)),
        encrypted_credentials: Set(Some(
            key()
                .seal(owner, uid, &Credentials::parse(RAW).unwrap())
                .unwrap(),
        )),
        credential_version: Set(1),
        auth_status: Set(if uid.is_some() {
            "verified"
        } else {
            "pending_verification"
        }
        .into()),
        created_at: Set(now),
        updated_at: Set(now),
        verified_at: Set(None),
    }
}

#[tokio::test]
async fn database_enforces_unique_owner_verified_uid_state_and_cascade() {
    let fixture = TestDatabase::new().await;
    let a = user_without_audit(&fixture.db, "owner-a").await;
    let b = user_without_audit(&fixture.db, "owner-b").await;
    stored(a.id, Some("123")).insert(&fixture.db).await.unwrap();
    for row in [stored(a.id, None), stored(b.id, Some("123"))] {
        assert!(matches!(
            row.insert(&fixture.db).await.unwrap_err().sql_err(),
            Some(SqlErr::UniqueConstraintViolation(_))
        ));
    }
    assert!(matches!(
        stored(Uuid::new_v4(), None)
            .insert(&fixture.db)
            .await
            .unwrap_err()
            .sql_err(),
        Some(SqlErr::ForeignKeyConstraintViolation(_))
    ));
    // Pending storage can never assert verification by status alone.
    let mut bad = stored(b.id, None);
    bad.auth_status = Set("verified".into());
    assert!(bad.insert(&fixture.db).await.is_err());
    assert!(
        fixture
            .db
            .execute_unprepared("UPDATE hoyolab_accounts SET credential_version = 0")
            .await
            .is_err()
    );
    stored(b.id, None).insert(&fixture.db).await.unwrap();
    user::Entity::delete_by_id(a.id)
        .exec(&fixture.db)
        .await
        .unwrap();
    assert_eq!(
        hoyolab_account::Entity::find()
            .count(&fixture.db)
            .await
            .unwrap(),
        1
    );
    // The verified UID is released only with owner deletion, no unlink method exists.
    hoyolab_account::Entity::delete_many()
        .exec(&fixture.db)
        .await
        .unwrap();
    stored(b.id, Some("123")).insert(&fixture.db).await.unwrap();
}
