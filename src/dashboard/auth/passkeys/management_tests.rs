use super::{Passkeys, validate_label};
use crate::dashboard::{auth::sessions::Sessions, db::entities::passkey, error::Error};
use crate::dashboard::{
    auth::{
        passkeys::test_support::{Fixture, ready_user},
        password::token_hash,
    },
    test_support::{
        PASSWORD, TEMP_PASSWORD,
        webauthn::{ORIGIN, RP_ID, TestAuthenticator},
    },
};
use sea_orm::PaginatorTrait;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
use std::sync::Arc;
use std::time::Duration;

#[test]
fn webauthn_configuration_accepts_localhost_and_rejects_invalid_rp_relationships() {
    assert!(Passkeys::new(RP_ID, ORIGIN, None).is_ok());
    assert!(Passkeys::new("example.com", "https://dashboard.example.com", None).is_ok());
    for (rp, origin) in [
        (RP_ID, "not a URL"),
        ("other.example", ORIGIN),
        ("127.0.0.1", "http://127.0.0.1:8080"),
    ] {
        assert!(matches!(
            Passkeys::new(rp, origin, None),
            Err(Error::Invalid(_))
        ));
    }
}

#[test]
fn labels_are_trimmed_and_bounded_in_utf8_bytes_without_controls() {
    assert_eq!(validate_label("  laptop  ".into()).unwrap(), "laptop");
    assert!(validate_label("x".repeat(100)).is_ok());
    assert!(validate_label("é".repeat(50)).is_ok());
    for label in [
        "".into(),
        " \t ".into(),
        "x".repeat(101),
        "é".repeat(51),
        "a\nb".into(),
        "a\u{7f}b".into(),
    ] {
        assert!(matches!(validate_label(label), Err(Error::Invalid(_))));
    }
}

#[tokio::test]
async fn cpu_admission_is_busy_and_cancellation_keeps_the_real_work_permit() {
    let passkeys = Arc::new(Passkeys::new(RP_ID, ORIGIN, None).unwrap());
    let held = passkeys.cpu.clone().acquire_many_owned(3).await.unwrap();
    let (started_tx, started_rx) = tokio::sync::oneshot::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let worker = passkeys.clone();
    let task = tokio::spawn(async move {
        worker
            .run_cpu(move |_| {
                let _ = started_tx.send(());
                // Dropping the sender also releases this wait if an assertion panics.
                let _ = release_rx.recv();
                Ok(())
            })
            .await
    });
    tokio::time::timeout(Duration::from_secs(5), started_rx)
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(
        tokio::time::timeout(Duration::from_secs(5), passkeys.run_cpu(|_| Ok(())))
            .await
            .unwrap(),
        Err(Error::Busy)
    ));
    task.abort();
    assert!(
        tokio::time::timeout(Duration::from_secs(5), task)
            .await
            .unwrap()
            .unwrap_err()
            .is_cancelled()
    );
    assert_eq!(passkeys.cpu.available_permits(), 0);
    assert!(matches!(
        tokio::time::timeout(Duration::from_secs(5), passkeys.run_cpu(|_| Ok(())))
            .await
            .unwrap(),
        Err(Error::Busy)
    ));
    release_tx.send(()).unwrap();
    let returned =
        tokio::time::timeout(Duration::from_secs(5), passkeys.cpu.clone().acquire_owned())
            .await
            .unwrap()
            .unwrap();
    drop(returned);
    drop(held);
    assert_eq!(passkeys.cpu.available_permits(), 4);
    assert!(passkeys.run_cpu(|_| Ok(())).await.is_ok());
}

#[tokio::test]
async fn list_exposes_only_metadata_and_delete_is_owner_scoped() {
    let fixture = Fixture::new().await;
    let other = ready_user(&fixture.accounts, &fixture.actor, "non-owner").await;
    assert!(
        fixture
            .passkeys
            .list(&fixture.accounts, &other)
            .await
            .unwrap()
            .is_empty()
    );
    let keys = fixture
        .passkeys
        .list(&fixture.accounts, &fixture.actor)
        .await
        .unwrap();
    assert_eq!(keys.len(), 1);
    assert_eq!(keys[0].id, fixture.key_id);
    let json = serde_json::to_value(&keys[0]).unwrap();
    let mut fields = json
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect::<Vec<_>>();
    fields.sort_unstable();
    assert_eq!(fields, ["created_at", "id", "label"]);
    assert!(matches!(
        fixture
            .passkeys
            .delete(&fixture.accounts, &other, fixture.key_id)
            .await,
        Err(Error::NotFound)
    ));
    assert_eq!(
        fixture
            .passkeys
            .list(&fixture.accounts, &fixture.actor)
            .await
            .unwrap()
            .len(),
        1
    );
    fixture
        .passkeys
        .delete(&fixture.accounts, &fixture.actor, fixture.key_id)
        .await
        .unwrap();
    assert!(
        fixture
            .passkeys
            .list(&fixture.accounts, &fixture.actor)
            .await
            .unwrap()
            .is_empty()
    );
    assert!(matches!(
        fixture
            .passkeys
            .delete(&fixture.accounts, &fixture.actor, fixture.key_id)
            .await,
        Err(Error::NotFound)
    ));
}

#[tokio::test]
async fn recovery_removes_keys_and_sessions_and_invalidates_both_pending_ceremonies() {
    let fixture = Fixture::new().await;
    let target = ready_user(&fixture.accounts, &fixture.actor, "recovery-target").await;
    let mut authenticator = TestAuthenticator::new();
    let registration = fixture
        .passkeys
        .registration_start(&fixture.accounts, &target, "target key".into())
        .await
        .unwrap();
    let response = authenticator.register(&registration, 1);
    fixture
        .passkeys
        .registration_finish(
            &fixture.accounts,
            &target,
            registration.challenge_id,
            response,
        )
        .await
        .unwrap();
    let login = fixture
        .passkeys
        .authentication_start(&fixture.accounts, target.user.username.clone())
        .await
        .unwrap();
    let login_response = authenticator.authenticate(&login, 2);
    let registration = fixture
        .passkeys
        .registration_start(&fixture.accounts, &target, "pending key".into())
        .await
        .unwrap();
    let registration_response = TestAuthenticator::new().register(&registration, 1);
    fixture
        .accounts
        .reset_password(&fixture.actor, target.user.id, TEMP_PASSWORD.into())
        .await
        .unwrap();
    assert_eq!(
        passkey::Entity::find()
            .filter(passkey::Column::UserId.eq(target.user.id))
            .count(&fixture.db.db)
            .await
            .unwrap(),
        0
    );
    assert!(matches!(
        Sessions::authenticate(&fixture.db.db, &target.token_hash).await,
        Err(Error::Unauthorized)
    ));
    assert_eq!(
        crate::dashboard::db::entities::session::Entity::find()
            .filter(crate::dashboard::db::entities::session::Column::UserId.eq(target.user.id))
            .count(&fixture.db.db)
            .await
            .unwrap(),
        0
    );
    // Clear temporary-password policy before old authentication finish, isolating key deletion.
    let (_, issued) = fixture
        .accounts
        .login(target.user.username.clone(), TEMP_PASSWORD.into())
        .await
        .unwrap();
    let temporary = Sessions::authenticate(&fixture.db.db, &token_hash(&issued.token).unwrap())
        .await
        .unwrap();
    fixture
        .accounts
        .change_password(&temporary, Some(TEMP_PASSWORD.into()), PASSWORD.into())
        .await
        .unwrap();
    assert!(matches!(
        fixture
            .passkeys
            .authentication_finish(&fixture.accounts, login.challenge_id, login_response)
            .await,
        Err(Error::Unauthorized)
    ));
    assert!(matches!(
        fixture
            .passkeys
            .registration_finish(
                &fixture.accounts,
                &target,
                registration.challenge_id,
                registration_response
            )
            .await,
        Err(Error::Unauthorized)
    ));
    assert_eq!(
        fixture
            .passkeys
            .list(&fixture.accounts, &fixture.actor)
            .await
            .unwrap()
            .len(),
        1
    );
}
