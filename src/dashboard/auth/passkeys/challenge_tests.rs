use super::{CHALLENGE_TTL, Ceremony, OfferedCredential, Passkeys, Pending};
use crate::dashboard::error::Error;
use crate::dashboard::test_support::webauthn::{ORIGIN, RP_ID};
use std::time::{Duration, Instant};
use uuid::Uuid;

fn pending(passkeys: &Passkeys, expires_at: Instant) -> Pending {
    let owner = Uuid::new_v4();
    let (_, state) =
        tracing::subscriber::with_default(tracing::subscriber::NoSubscriber::default(), || {
            passkeys
                .webauthn
                .start_passkey_registration(owner, "synthetic", "synthetic", None)
        })
        .unwrap();
    Pending {
        owner,
        expires_at,
        ceremony: Ceremony::Registration {
            session_hash: "synthetic-session".into(),
            label: "key".into(),
            state,
        },
    }
}

#[test]
fn pending_capacity_is_128_and_consumption_frees_a_slot() {
    let passkeys = Passkeys::new(RP_ID, ORIGIN, None).unwrap();
    let mut ids = Vec::new();
    for _ in 0..128 {
        ids.push(
            passkeys
                .insert_pending(pending(&passkeys, Instant::now() + CHALLENGE_TTL))
                .unwrap(),
        );
    }
    assert!(matches!(
        passkeys.insert_pending(pending(&passkeys, Instant::now() + CHALLENGE_TTL)),
        Err(Error::Busy)
    ));
    assert!(passkeys.take_pending(ids[0]).is_ok());
    assert!(matches!(
        passkeys.take_pending(ids[0]),
        Err(Error::Unauthorized)
    ));
    assert!(
        passkeys
            .insert_pending(pending(&passkeys, Instant::now() + CHALLENGE_TTL))
            .is_ok()
    );
}

#[test]
fn expired_entries_are_cleaned_on_insert_and_take() {
    assert_eq!(CHALLENGE_TTL, Duration::from_secs(300));
    let passkeys = Passkeys::new(RP_ID, ORIGIN, None).unwrap();
    let expired = Uuid::new_v4();
    passkeys.pending.lock().unwrap().insert(
        expired,
        pending(&passkeys, Instant::now() - Duration::from_secs(1)),
    );
    let live = passkeys
        .insert_pending(pending(&passkeys, Instant::now() + CHALLENGE_TTL))
        .unwrap();
    assert!(!passkeys.pending.lock().unwrap().contains_key(&expired));
    passkeys
        .pending
        .lock()
        .unwrap()
        .get_mut(&live)
        .unwrap()
        .expires_at = Instant::now() - Duration::from_secs(1);
    assert!(matches!(
        passkeys.take_pending(live),
        Err(Error::Unauthorized)
    ));
    assert!(passkeys.pending.lock().unwrap().is_empty());
}

#[test]
fn process_local_challenges_do_not_survive_service_recreation() {
    let original = Passkeys::new(RP_ID, ORIGIN, None).unwrap();
    let id = original
        .insert_pending(pending(&original, Instant::now() + CHALLENGE_TTL))
        .unwrap();
    let replacement = Passkeys::new(RP_ID, ORIGIN, None).unwrap();
    assert!(matches!(
        replacement.take_pending(id),
        Err(Error::Unauthorized)
    ));
    assert!(original.take_pending(id).is_ok());
}

#[test]
fn pending_state_does_not_implement_serialize_or_debug() {
    // Inference becomes ambiguous if either sensitive type gains the forbidden trait.
    trait AmbiguousIfSerialize<A> {
        fn check() {}
    }
    impl<T: ?Sized> AmbiguousIfSerialize<()> for T {}
    impl<T: ?Sized + serde::Serialize> AmbiguousIfSerialize<u8> for T {}
    trait AmbiguousIfDebug<A> {
        fn check() {}
    }
    impl<T: ?Sized> AmbiguousIfDebug<()> for T {}
    impl<T: ?Sized + std::fmt::Debug> AmbiguousIfDebug<u8> for T {}
    let _ = <Pending as AmbiguousIfSerialize<_>>::check;
    let _ = <Ceremony as AmbiguousIfSerialize<_>>::check;
    let _ = <OfferedCredential as AmbiguousIfSerialize<_>>::check;
    let _ = <Pending as AmbiguousIfDebug<_>>::check;
    let _ = <Ceremony as AmbiguousIfDebug<_>>::check;
    let _ = <OfferedCredential as AmbiguousIfDebug<_>>::check;
}
