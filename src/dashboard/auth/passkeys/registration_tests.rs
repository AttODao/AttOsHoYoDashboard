use super::{CHALLENGE_TTL, Passkeys};
use crate::dashboard::{auth::sessions::Sessions, db::entities::passkey, error::Error};
use crate::dashboard::{
    auth::{
        passkeys::test_support::{Fixture, ready_user},
        password::token_hash,
    },
    test_support::{
        PASSWORD, TEMP_PASSWORD, TestDatabase,
        webauthn::{ORIGIN, RP_ID, TestAuthenticator},
    },
};
use chrono::Utc;
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter, Set};
use std::time::Instant;
use uuid::Uuid;

#[tokio::test]
async fn software_p256_registration_persists_trimmed_label_and_is_single_use() {
    let db = TestDatabase::new().await;
    let (accounts, actor) = db.ready_admin().await;
    let passkeys = Passkeys::new(RP_ID, ORIGIN).unwrap();
    let challenge = passkeys
        .registration_start(&accounts, &actor, "  laptop  ".into())
        .await
        .unwrap();
    let mut authenticator = TestAuthenticator::new();
    let response = authenticator.register(&challenge, 1);
    let key = passkeys
        .registration_finish(&accounts, &actor, challenge.challenge_id, response.clone())
        .await
        .unwrap();
    assert_eq!(key.label, "laptop");
    assert_eq!(
        passkey::Entity::find_by_id(key.id)
            .one(&db.db)
            .await
            .unwrap()
            .unwrap()
            .user_id,
        actor.user.id
    );
    assert!(matches!(
        passkeys
            .registration_finish(&accounts, &actor, challenge.challenge_id, response)
            .await,
        Err(Error::Unauthorized)
    ));
}

#[tokio::test]
async fn temporary_password_blocks_registration_and_passkey_login() {
    let db = TestDatabase::new().await;
    let (accounts, admin) = db.ready_admin().await;
    accounts
        .create_user(&admin, "temporary-user".into(), TEMP_PASSWORD.into())
        .await
        .unwrap();
    let (_, session) = accounts
        .login("temporary-user".into(), TEMP_PASSWORD.into())
        .await
        .unwrap();
    let actor = Sessions::authenticate(&db.db, &token_hash(&session.token).unwrap())
        .await
        .unwrap();
    let passkeys = Passkeys::new(RP_ID, ORIGIN).unwrap();
    assert!(matches!(
        passkeys
            .registration_start(&accounts, &actor, "key".into())
            .await,
        Err(Error::PasswordChangeRequired)
    ));
    assert!(matches!(
        passkeys
            .authentication_start(&accounts, "temporary-user".into())
            .await,
        Err(Error::Unauthorized)
    ));
}

#[tokio::test]
async fn different_owner_cannot_finish_and_consumes_the_registration() {
    let fixture = Fixture::new().await;
    let other = ready_user(&fixture.accounts, &fixture.actor, "other-owner").await;
    let challenge = fixture
        .passkeys
        .registration_start(&fixture.accounts, &fixture.actor, "new key".into())
        .await
        .unwrap();
    let response = TestAuthenticator::new().register(&challenge, 1);
    assert!(matches!(
        fixture
            .passkeys
            .registration_finish(
                &fixture.accounts,
                &other,
                challenge.challenge_id,
                response.clone()
            )
            .await,
        Err(Error::Unauthorized)
    ));
    assert!(matches!(
        fixture
            .passkeys
            .registration_finish(
                &fixture.accounts,
                &fixture.actor,
                challenge.challenge_id,
                response
            )
            .await,
        Err(Error::Unauthorized)
    ));
}

#[tokio::test]
async fn another_session_for_the_same_owner_cannot_finish_registration() {
    let fixture = Fixture::new().await;
    let (_, issued) = fixture
        .accounts
        .login(fixture.actor.user.username.clone(), PASSWORD.into())
        .await
        .unwrap();
    let other_session = Sessions::authenticate(&fixture.db.db, &token_hash(&issued.token).unwrap())
        .await
        .unwrap();
    let challenge = fixture
        .passkeys
        .registration_start(&fixture.accounts, &fixture.actor, "new key".into())
        .await
        .unwrap();
    let response = TestAuthenticator::new().register(&challenge, 1);
    assert!(matches!(
        fixture
            .passkeys
            .registration_finish(
                &fixture.accounts,
                &other_session,
                challenge.challenge_id,
                response.clone()
            )
            .await,
        Err(Error::Unauthorized)
    ));
    assert!(matches!(
        fixture
            .passkeys
            .registration_finish(
                &fixture.accounts,
                &fixture.actor,
                challenge.challenge_id,
                response
            )
            .await,
        Err(Error::Unauthorized)
    ));
}

#[tokio::test]
async fn logout_of_original_session_invalidates_registration() {
    let fixture = Fixture::new().await;
    let challenge = fixture
        .passkeys
        .registration_start(&fixture.accounts, &fixture.actor, "new key".into())
        .await
        .unwrap();
    let response = TestAuthenticator::new().register(&challenge, 1);
    fixture.accounts.logout(&fixture.actor).await.unwrap();
    assert!(matches!(
        fixture
            .passkeys
            .registration_finish(
                &fixture.accounts,
                &fixture.actor,
                challenge.challenge_id,
                response
            )
            .await,
        Err(Error::Unauthorized)
    ));
}

#[tokio::test]
async fn registration_limit_is_rechecked_at_finish_and_start() {
    let fixture = Fixture::new().await;
    let challenge = fixture
        .passkeys
        .registration_start(&fixture.accounts, &fixture.actor, "last key".into())
        .await
        .unwrap();
    let response = TestAuthenticator::new().register(&challenge, 1);
    let template = passkey::Entity::find_by_id(fixture.key_id)
        .one(&fixture.db.db)
        .await
        .unwrap()
        .unwrap();
    // Synthetic rows exercise the count guard, not credential decoding.
    for n in 0..19u8 {
        passkey::ActiveModel {
            id: Set(Uuid::new_v4()),
            user_id: Set(fixture.actor.user.id),
            credential_id: Set(vec![n]),
            label: Set("count fixture".into()),
            credential: Set(template.credential.clone()),
            created_at: Set(Utc::now()),
        }
        .insert(&fixture.db.db)
        .await
        .unwrap();
    }
    assert!(matches!(
        fixture
            .passkeys
            .registration_finish(
                &fixture.accounts,
                &fixture.actor,
                challenge.challenge_id,
                response
            )
            .await,
        Err(Error::Invalid("passkey limit reached"))
    ));
    assert!(matches!(
        fixture
            .passkeys
            .registration_start(&fixture.accounts, &fixture.actor, "over limit".into())
            .await,
        Err(Error::Invalid("passkey limit reached"))
    ));
    assert_eq!(
        passkey::Entity::find()
            .filter(passkey::Column::UserId.eq(fixture.actor.user.id))
            .count(&fixture.db.db)
            .await
            .unwrap(),
        20
    );
}

#[tokio::test]
async fn lost_or_expired_registration_challenge_is_rejected() {
    let fixture = Fixture::new().await;
    let challenge = fixture
        .passkeys
        .registration_start(&fixture.accounts, &fixture.actor, "new key".into())
        .await
        .unwrap();
    let response = TestAuthenticator::new().register(&challenge, 1);
    let restarted = Passkeys::new(RP_ID, ORIGIN).unwrap();
    assert!(matches!(
        restarted
            .registration_finish(
                &fixture.accounts,
                &fixture.actor,
                challenge.challenge_id,
                response.clone()
            )
            .await,
        Err(Error::Unauthorized)
    ));
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
            .registration_finish(
                &fixture.accounts,
                &fixture.actor,
                challenge.challenge_id,
                response
            )
            .await,
        Err(Error::Unauthorized)
    ));
}
