use super::{Passwords, random_secret, token_hash};
use crate::dashboard::error::Error;
use argon2::PasswordHash;
use std::{
    collections::HashSet,
    future::{Future, poll_fn},
    sync::mpsc,
    task::Poll,
    time::Duration,
};

#[tokio::test]
async fn hash_enforces_inclusive_utf8_byte_limits() {
    let passwords = Passwords::new().await.expect("initialize passwords");
    for password in [
        String::new(),
        "a".repeat(11),
        "a".repeat(257),
        format!("{}a", "é".repeat(5)),
        format!("{}a", "é".repeat(128)),
    ] {
        assert!(matches!(
            passwords.hash(password).await,
            Err(Error::Invalid(
                "password must contain 12 to 256 UTF-8 bytes"
            ))
        ));
    }
    // These Unicode inputs have fewer than 12 / 256 characters, but 12 / 256 bytes.
    for password in [
        "a".repeat(12),
        "a".repeat(256),
        "é".repeat(6),
        "é".repeat(128),
    ] {
        let hash = passwords
            .hash(password.clone())
            .await
            .expect("hash boundary password");
        assert!(
            passwords
                .verify(password, Some(&hash))
                .await
                .expect("verify boundary password")
        );
    }
    assert_eq!(passwords.workers.available_permits(), 4);
}

#[tokio::test]
async fn hashes_use_distinct_salts_and_verify_correct_and_wrong_passwords() {
    let passwords = Passwords::new().await.expect("initialize passwords");
    let password = "test password";
    let first = passwords.hash(password.into()).await.expect("first hash");
    let second = passwords.hash(password.into()).await.expect("second hash");
    let first_phc = PasswordHash::new(&first).expect("parse first hash");
    let second_phc = PasswordHash::new(&second).expect("parse second hash");
    assert!(first_phc.algorithm.as_str() == "argon2id");
    assert!(second_phc.algorithm.as_str() == "argon2id");
    assert!(first_phc.salt.is_some());
    assert!(second_phc.salt.is_some());
    assert!(first_phc.salt != second_phc.salt);
    assert!(first != second);
    for hash in [&first, &second] {
        assert!(
            passwords
                .verify(password.into(), Some(hash))
                .await
                .expect("verify correct password")
        );
        assert!(
            !passwords
                .verify("wrong password".into(), Some(hash))
                .await
                .expect("verify wrong password")
        );
    }
}

#[tokio::test]
async fn verify_uses_dummy_hash_and_reports_corrupt_hashes() {
    let mut passwords = Passwords::new().await.expect("initialize passwords");
    assert!(PasswordHash::new(&passwords.dummy_hash).is_ok());
    assert!(
        !passwords
            .verify("test password".into(), None)
            .await
            .expect("verify missing account")
    );
    // Malformed PHC syntax and valid PHC syntax with an unsupported algorithm
    // exercise both hash parsing and verifier errors.
    let unsupported = passwords.dummy_hash.replace("argon2id", "unsupported");
    assert!(PasswordHash::new(&unsupported).is_ok());
    for hash in ["not a PHC hash", unsupported.as_str()] {
        assert!(matches!(
            passwords.verify("test password".into(), Some(hash)).await,
            Err(Error::Password)
        ));
        assert_eq!(passwords.workers.available_permits(), 4);
    }
    passwords.dummy_hash = "not a PHC hash".into();
    assert!(matches!(
        passwords.verify("test password".into(), None).await,
        Err(Error::Password)
    ));
    assert_eq!(passwords.workers.available_permits(), 4);
}

#[tokio::test]
async fn verify_rejects_overlong_utf8_but_accepts_short_candidates() {
    let passwords = Passwords::new().await.expect("initialize passwords");
    for password in ["a".repeat(257), format!("{}a", "é".repeat(128))] {
        // A corrupt hash ensures the length guard runs before parsing/hashing.
        assert!(matches!(
            passwords.verify(password, Some("not a PHC hash")).await,
            Err(Error::Unauthorized)
        ));
    }
    for password in ["", "short"] {
        assert!(
            !passwords
                .verify(password.into(), None)
                .await
                .expect("verify short candidate")
        );
    }
    assert_eq!(passwords.workers.available_permits(), 4);
}

#[tokio::test]
async fn four_worker_permits_bound_both_hash_and_verify_admission() {
    let passwords = Passwords::new().await.expect("initialize passwords");
    assert_eq!(passwords.workers.available_permits(), 4);
    let permits: Vec<_> = (0..4)
        .map(|_| {
            passwords
                .workers
                .clone()
                .try_acquire_owned()
                .expect("reserve worker")
        })
        .collect();
    assert_eq!(passwords.workers.available_permits(), 0);
    assert!(passwords.workers.clone().try_acquire_owned().is_err());
    assert!(matches!(
        passwords.hash("test password".into()).await,
        Err(Error::Busy)
    ));
    for hash in [Some(passwords.dummy_hash.as_str()), None] {
        assert!(matches!(
            passwords.verify("test password".into(), hash).await,
            Err(Error::Busy)
        ));
    }
    drop(permits);
    assert_eq!(passwords.workers.available_permits(), 4);
    let hash = passwords
        .hash("test password".into())
        .await
        .expect("hash after releasing workers");
    assert!(
        passwords
            .verify("test password".into(), Some(&hash))
            .await
            .expect("verify after releasing workers")
    );
}

#[test]
fn cancelled_hash_retains_worker_until_queued_blocking_work_finishes() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .max_blocking_threads(1)
        .enable_all()
        .build()
        .expect("build runtime");
    // Complete startup dummy hashing before occupying the sole blocking thread.
    let passwords = runtime
        .block_on(Passwords::new())
        .expect("initialize passwords");
    let (started_tx, started_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let blocker = runtime.spawn_blocking(move || {
        started_tx.send(()).expect("signal blocking thread started");
        // Dropping the sender during unwinding also releases this thread.
        let _ = release_rx.recv();
    });
    started_rx.recv().expect("wait for blocking thread");

    runtime.block_on(async {
        let mut hash = Box::pin(passwords.hash("test password".into()));
        poll_fn(|cx| {
            assert!(hash.as_mut().poll(cx).is_pending());
            Poll::Ready(())
        })
        .await;
        assert_eq!(passwords.workers.available_permits(), 3);
        drop(hash);
        // The closure is queued behind the barrier: dropping its awaiting future
        // must not release the worker admission permit.
        assert_eq!(passwords.workers.available_permits(), 3);
        assert!(passwords.workers.clone().try_acquire_many_owned(4).is_err());
        release_tx.send(()).expect("release blocking thread");
        blocker.await.expect("join blocking barrier");
        let permits =
            tokio::time::timeout(Duration::from_secs(30), passwords.workers.acquire_many(4))
                .await
                .expect("cancelled hash must eventually release worker")
                .expect("acquire workers");
        drop(permits);
        assert_eq!(passwords.workers.available_permits(), 4);
    });
}

#[test]
fn random_secrets_are_unique_lowercase_hex_tokens() {
    let mut secrets = HashSet::new();
    for _ in 0..32 {
        let secret = random_secret().expect("generate secret");
        assert_eq!(secret.len(), 64);
        assert!(
            secret
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        );
        assert!(token_hash(&secret).is_ok());
        assert!(secrets.insert(secret));
    }
}

#[test]
fn token_hash_is_sha256_of_raw_hex_and_rejects_invalid_tokens() {
    // Public, synthetic known-answer vector; assertions never display tokens/hashes.
    let zeros = "0".repeat(64);
    assert!(
        token_hash(&zeros).expect("hash token")
            == "60e05bd1b195af2f94112fa7197a5c88289058840ce7c6df9693756bc6250f55"
    );
    let uppercase = "ABCDEF0123456789".repeat(4);
    assert!(token_hash(&uppercase).is_ok());
    assert!(
        token_hash(&uppercase).expect("hash uppercase token")
            != token_hash(&uppercase.to_ascii_lowercase()).expect("hash lowercase token")
    );
    for invalid in [
        String::new(),
        "0".repeat(63),
        "0".repeat(65),
        "g".repeat(64),
        "é".repeat(32),
        " ".repeat(64),
    ] {
        assert!(matches!(token_hash(&invalid), Err(Error::Unauthorized)));
    }
}
