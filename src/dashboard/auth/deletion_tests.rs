#[path = "deletion_tests/completion.rs"]
mod completion;

use super::*;
use crate::dashboard::{
    auth::{
        password::token_hash,
        sessions::{Credential, Identity, IssuedSession, Sessions},
    },
    db::entities::{audit, deletion_request, passkey, user},
    error::Error,
    test_support::{PASSWORD, TEMP_PASSWORD, TestDatabase},
};
use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, PaginatorTrait, QueryFilter,
    Set,
};
use uuid::Uuid;

async fn identity(db: &DatabaseConnection, issued: &IssuedSession) -> Identity {
    Sessions::authenticate(db, &token_hash(&issued.token).unwrap())
        .await
        .unwrap()
}

async fn ready_user(accounts: &Accounts, admin: &Identity, username: &str) -> Identity {
    accounts
        .create_user(admin, username.to_owned(), TEMP_PASSWORD.to_owned())
        .await
        .unwrap();
    let (_, issued) = accounts
        .login(username.to_owned(), TEMP_PASSWORD.to_owned())
        .await
        .unwrap();
    let temporary = identity(&accounts.db, &issued).await;
    let (_, issued) = accounts
        .change_password(
            &temporary,
            Some(TEMP_PASSWORD.to_owned()),
            PASSWORD.to_owned(),
        )
        .await
        .unwrap();
    identity(&accounts.db, &issued).await
}

async fn promote_admin(db: &DatabaseConnection, id: Uuid) {
    user::ActiveModel {
        id: Set(id),
        is_admin: Set(true),
        ..Default::default()
    }
    .update(db)
    .await
    .unwrap();
}

async fn stub_passkey(db: &DatabaseConnection, owner: Uuid) -> Uuid {
    let id = Uuid::new_v4();
    passkey::ActiveModel {
        id: Set(id),
        user_id: Set(owner),
        credential_id: Set(id.as_bytes().to_vec()),
        credential: Set(serde_json::json!({"synthetic": true})),
        label: Set("delete me".to_owned()),
        created_at: Set(Utc::now()),
        last_used_at: Set(None),
    }
    .insert(db)
    .await
    .unwrap();
    id
}

async fn pending_row(db: &DatabaseConnection, owner: Uuid) -> deletion_request::Model {
    deletion_request::Entity::find_by_id(owner)
        .one(db)
        .await
        .unwrap()
        .unwrap()
}

async fn assert_no_request(db: &DatabaseConnection, owner: Uuid) {
    assert!(
        deletion_request::Entity::find_by_id(owner)
            .one(db)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        user::Entity::find_by_id(owner)
            .one(db)
            .await
            .unwrap()
            .unwrap()
            .active
    );
}

#[tokio::test]
async fn administrator_can_delete_disabled_users_and_disabled_administrators() {
    let fixture = TestDatabase::new().await;
    let (accounts, admin) = fixture.ready_admin().await;
    for (name, is_admin) in [("disabled-user", false), ("disabled-admin", true)] {
        let target = ready_user(&accounts, &admin, name).await;
        if is_admin {
            promote_admin(&fixture.db, target.user.id).await;
        }
        accounts
            .set_active(&admin, target.user.id, false)
            .await
            .unwrap();
        accounts
            .request_deletion(&admin, target.user.id, PASSWORD.into(), name.into())
            .await
            .unwrap();
        accounts.finish_deletion(target.user.id).await.unwrap();
        assert!(
            user::Entity::find_by_id(target.user.id)
                .one(&fixture.db)
                .await
                .unwrap()
                .is_none()
        );
    }
    assert!(
        user::Entity::find_by_id(admin.user.id)
            .one(&fixture.db)
            .await
            .unwrap()
            .unwrap()
            .active
    );
}
#[tokio::test]
async fn deletion_advisory_lock_does_not_deadlock_a_password_row_writer() {
    use sea_orm::{DbBackend, Statement};
    let fixture = TestDatabase::new().await;
    let (accounts, admin) = fixture.ready_admin().await;
    let target = ready_user(&accounts, &admin, "row-writer").await;
    let writer = fixture.db.begin().await.unwrap();
    lock_user(&writer, target.user.id).await.unwrap();
    let accounts = std::sync::Arc::new(accounts);
    let target_id = target.user.id;
    let deletion = {
        let accounts = accounts.clone();
        tokio::spawn(async move {
            accounts
                .request_deletion(&admin, target_id, PASSWORD.into(), "row-writer".into())
                .await
        })
    };
    tokio::time::timeout(std::time::Duration::from_secs(10),async {loop{
  let row=fixture.db.query_one_raw(Statement::from_string(DbBackend::Postgres,"SELECT EXISTS(SELECT 1 FROM pg_locks WHERE locktype='advisory' AND objid=198642015 AND granted) AS locked")).await.unwrap().unwrap();
  if row.try_get::<bool>("","locked").unwrap(){break;}tokio::task::yield_now().await;
 }}).await.unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        user::ActiveModel {
            id: Set(target_id),
            must_change_password: Set(false),
            ..Default::default()
        }
        .update(&writer)
        .await
        .unwrap();
        writer.commit().await.unwrap();
        deletion.await.unwrap().unwrap();
    })
    .await
    .unwrap();
    assert!(
        !user::Entity::find_by_id(target_id)
            .one(&fixture.db)
            .await
            .unwrap()
            .unwrap()
            .active
    );
}

#[tokio::test]
async fn self_request_disables_revokes_and_finishes_later() {
    let test = TestDatabase::new().await;
    let (accounts, admin) = test.ready_admin().await;
    let member = ready_user(&accounts, &admin, "member").await;
    let (_, extra_session) = accounts
        .login("member".to_owned(), PASSWORD.to_owned())
        .await
        .unwrap();
    let passkey = stub_passkey(&test.db, member.user.id).await;

    accounts
        .request_deletion(
            &member,
            member.user.id,
            PASSWORD.to_owned(),
            "member".to_owned(),
        )
        .await
        .unwrap();

    let disabled = user::Entity::find_by_id(member.user.id)
        .one(&test.db)
        .await
        .unwrap()
        .unwrap();
    assert!(!disabled.active);
    assert_eq!(
        accounts
            .pending_deletions(&[], Instant::now())
            .await
            .unwrap(),
        vec![member.user.id]
    );
    assert!(
        pending_row(&test.db, member.user.id)
            .await
            .requested_by_user_id
            == Some(member.user.id)
    );
    for hash in [
        &member.token_hash,
        &token_hash(&extra_session.token).unwrap(),
    ] {
        assert!(matches!(
            Sessions::authenticate(&test.db, hash).await,
            Err(Error::Unauthorized)
        ));
    }
    assert!(
        passkey::Entity::find_by_id(passkey)
            .one(&test.db)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        user::Entity::find_by_id(member.user.id)
            .one(&test.db)
            .await
            .unwrap()
            .is_some()
    );

    accounts.finish_deletion(member.user.id).await.unwrap();
    accounts.finish_deletion(member.user.id).await.unwrap();
    assert!(
        user::Entity::find_by_id(member.user.id)
            .one(&test.db)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        accounts
            .pending_deletions(&[], Instant::now())
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn admin_can_request_other_user_and_repeat_preserves_original_request() {
    let test = TestDatabase::new().await;
    let (accounts, admin) = test.ready_admin().await;
    let member = ready_user(&accounts, &admin, "member").await;

    accounts
        .request_deletion(
            &admin,
            member.user.id,
            PASSWORD.to_owned(),
            "member".to_owned(),
        )
        .await
        .unwrap();
    let first = pending_row(&test.db, member.user.id).await;
    assert_eq!(first.requested_by_user_id, Some(admin.user.id));
    assert!(
        Sessions::authenticate(&test.db, &admin.token_hash)
            .await
            .is_ok()
    );

    accounts
        .request_deletion(
            &admin,
            member.user.id,
            PASSWORD.to_owned(),
            "member".to_owned(),
        )
        .await
        .unwrap();
    let repeated = pending_row(&test.db, member.user.id).await;
    assert_eq!(repeated.created_at, first.created_at);
    assert_eq!(repeated.requested_by_user_id, first.requested_by_user_id);
    assert_eq!(
        audit::Entity::find()
            .filter(audit::Column::TargetUserId.eq(member.user.id))
            .filter(audit::Column::Action.eq("user_deletion_requested"))
            .count(&test.db)
            .await
            .unwrap(),
        1
    );
}

#[tokio::test]
async fn request_failures_do_not_queue_or_disable_users() {
    let test = TestDatabase::new().await;
    let (accounts, admin) = test.ready_admin().await;

    let wrong_confirmation = ready_user(&accounts, &admin, "wrong-confirmation").await;
    assert!(matches!(
        accounts
            .request_deletion(
                &wrong_confirmation,
                wrong_confirmation.user.id,
                PASSWORD.to_owned(),
                "not-the-username".to_owned()
            )
            .await,
        Err(Error::Invalid(_))
    ));
    assert_no_request(&test.db, wrong_confirmation.user.id).await;

    let bad_password = ready_user(&accounts, &admin, "bad-password").await;
    assert!(matches!(
        accounts
            .request_deletion(
                &bad_password,
                bad_password.user.id,
                "incorrect-password".to_owned(),
                "bad-password".to_owned()
            )
            .await,
        Err(Error::Unauthorized)
    ));
    assert_no_request(&test.db, bad_password.user.id).await;

    let forged_target = ready_user(&accounts, &admin, "forged-target").await;
    let token_owner = ready_user(&accounts, &admin, "token-owner").await;
    let forged = Identity {
        user: admin.user.clone(),
        token_hash: token_owner.token_hash.clone(),
        credential: Credential::Session {
            expires_at: token_owner.session_expires_at().unwrap(),
        },
    };
    assert!(matches!(
        accounts
            .request_deletion(
                &forged,
                forged_target.user.id,
                PASSWORD.to_owned(),
                "forged-target".to_owned()
            )
            .await,
        Err(Error::Unauthorized)
    ));
    assert_no_request(&test.db, forged_target.user.id).await;

    let recovered = ready_user(&accounts, &admin, "recovered").await;
    accounts
        .reset_password(&admin, recovered.user.id, TEMP_PASSWORD.to_owned())
        .await
        .unwrap();
    assert!(matches!(
        accounts
            .request_deletion(
                &recovered,
                recovered.user.id,
                PASSWORD.to_owned(),
                "recovered".to_owned()
            )
            .await,
        Err(Error::Unauthorized)
    ));
    assert_no_request(&test.db, recovered.user.id).await;

    // A non-admin cannot delete another user, whether or not the password is right.
    let member = ready_user(&accounts, &admin, "member").await;
    let other = ready_user(&accounts, &admin, "other").await;
    for password in [PASSWORD, "incorrect-password"] {
        assert!(matches!(
            accounts
                .request_deletion(
                    &member,
                    other.user.id,
                    password.to_owned(),
                    "other".to_owned()
                )
                .await,
            Err(Error::Forbidden)
        ));
    }
    assert_no_request(&test.db, other.user.id).await;

    let disabled = ready_user(&accounts, &admin, "disabled").await;
    accounts
        .set_active(&admin, disabled.user.id, false)
        .await
        .unwrap();
    assert!(matches!(
        accounts
            .request_deletion(
                &disabled,
                disabled.user.id,
                PASSWORD.to_owned(),
                "disabled".to_owned()
            )
            .await,
        Err(Error::Unauthorized)
    ));
    assert!(
        deletion_request::Entity::find_by_id(disabled.user.id)
            .one(&test.db)
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn last_active_admin_is_never_deleted_even_under_concurrent_self_requests() {
    let test = TestDatabase::new().await;
    let (accounts, admin) = test.ready_admin().await;
    assert!(matches!(
        accounts
            .request_deletion(
                &admin,
                admin.user.id,
                PASSWORD.to_owned(),
                "admin".to_owned()
            )
            .await,
        Err(Error::Invalid(_))
    ));
    assert_no_request(&test.db, admin.user.id).await;

    let second = ready_user(&accounts, &admin, "second-admin").await;
    promote_admin(&test.db, second.user.id).await;
    let second = Sessions::authenticate(&test.db, &second.token_hash)
        .await
        .unwrap();

    let (first, second_result) = tokio::join!(
        accounts.request_deletion(
            &admin,
            admin.user.id,
            PASSWORD.to_owned(),
            "admin".to_owned()
        ),
        accounts.request_deletion(
            &second,
            second.user.id,
            PASSWORD.to_owned(),
            "second-admin".to_owned()
        ),
    );
    assert_eq!(
        usize::from(first.is_ok()) + usize::from(second_result.is_ok()),
        1
    );
    assert!(matches!(
        first
            .as_ref()
            .err()
            .or_else(|| second_result.as_ref().err()),
        Some(Error::Invalid(_))
    ));
    assert_eq!(
        user::Entity::find()
            .filter(user::Column::Active.eq(true))
            .filter(user::Column::IsAdmin.eq(true))
            .count(&test.db)
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        accounts
            .pending_deletions(&[], Instant::now())
            .await
            .unwrap()
            .len(),
        1
    );
}

#[tokio::test]
async fn pending_deletion_rejects_recovery_and_reactivation_without_changes() {
    let fixture = TestDatabase::new().await;
    let (accounts, admin) = fixture.ready_admin().await;
    let target = ready_user(&accounts, &admin, "pending-recovery").await;
    accounts
        .request_deletion(
            &admin,
            target.user.id,
            PASSWORD.into(),
            "pending-recovery".into(),
        )
        .await
        .unwrap();
    let before = user::Entity::find_by_id(target.user.id)
        .one(&fixture.db)
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(
        accounts
            .reset_password(&admin, target.user.id, TEMP_PASSWORD.into())
            .await,
        Err(Error::Invalid("user deletion is already in progress"))
    ));
    assert!(matches!(
        accounts.set_active(&admin, target.user.id, true).await,
        Err(Error::Invalid("user deletion is already in progress"))
    ));
    let after = user::Entity::find_by_id(target.user.id)
        .one(&fixture.db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(after.password_hash, before.password_hash);
    assert_eq!(after.must_change_password, before.must_change_password);
    assert!(!after.active);
    assert_eq!(
        audit::Entity::find()
            .filter(audit::Column::TargetUserId.eq(target.user.id))
            .filter(audit::Column::Action.is_in(["password_reset", "user_enabled"]))
            .count(&fixture.db)
            .await
            .unwrap(),
        0
    );
    pending_row(&fixture.db, target.user.id).await;
}

#[tokio::test]
async fn deletion_password_checks_spend_the_actor_budget() {
    let fixture = TestDatabase::new().await;
    let (accounts, admin) = fixture.ready_admin().await;
    let target = ready_user(&accounts, &admin, "limited-target").await;
    let now = std::time::Instant::now();
    while accounts.reauth_limit.admit(admin.user.id, now).is_ok() {}
    // Even the correct password is not checked once the actor's budget is spent.
    assert!(matches!(
        accounts
            .request_deletion(
                &admin,
                target.user.id,
                PASSWORD.into(),
                "limited-target".into(),
            )
            .await,
        Err(Error::RateLimited)
    ));
    assert_no_request(&fixture.db, target.user.id).await;
    // The budget belongs to the actor, not the target.
    accounts
        .request_deletion(
            &target,
            target.user.id,
            PASSWORD.into(),
            "limited-target".into(),
        )
        .await
        .unwrap();
    pending_row(&fixture.db, target.user.id).await;
}
