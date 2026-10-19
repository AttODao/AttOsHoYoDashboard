use super::*;
use crate::dashboard::test_support::webauthn::{ORIGIN, RP_ID};
use webauthn_rs::prelude::{Url, WebauthnBuilder};

fn webauthn() -> Webauthn {
    WebauthnBuilder::new(RP_ID, &Url::parse(ORIGIN).unwrap())
        .unwrap()
        .build()
        .unwrap()
}

#[test]
fn credential_ids_are_keyed_per_name_and_stable_for_one_seed() {
    let seed = CookieKey::from_hex(&"33".repeat(32)).unwrap();
    let a = DecoyKey::new(Some(&seed)).unwrap();
    let b = DecoyKey::new(Some(&seed)).unwrap();
    let id = a.credential_id("alice").unwrap();
    assert_eq!(id.len(), 32);
    assert_eq!(id, b.credential_id("alice").unwrap());
    assert_ne!(id, a.credential_id("bob").unwrap());
    // Exact lookup, exact input: a differently cased name is another name.
    assert_ne!(id, a.credential_id("Alice").unwrap());
    // Domain separation: the decoy key is not the Cookie key itself.
    assert_ne!(a.0, seed.derive(b"").unwrap());
    // Without a configured seed each process draws its own key.
    let first = DecoyKey::new(None).unwrap();
    let second = DecoyKey::new(None).unwrap();
    assert_ne!(
        first.credential_id("alice").unwrap(),
        second.credential_id("alice").unwrap()
    );
}

#[test]
fn options_carry_a_fresh_challenge_and_one_credential_without_discoverable_extras() {
    let webauthn = webauthn();
    let first = serde_json::to_value(options(&webauthn, vec![1; 32]).unwrap()).unwrap();
    let second = serde_json::to_value(options(&webauthn, vec![1; 32]).unwrap()).unwrap();
    assert_ne!(
        first["publicKey"]["challenge"],
        second["publicKey"]["challenge"]
    );
    assert!(first.get("mediation").is_none());
    assert!(first["publicKey"].get("extensions").is_none());
    assert_eq!(first["publicKey"]["userVerification"], "required");
    assert_eq!(first["publicKey"]["rpId"], RP_ID);
    let allowed = first["publicKey"]["allowCredentials"].as_array().unwrap();
    assert_eq!(allowed.len(), 1);
    assert_eq!(allowed[0]["type"], "public-key");
    assert!(allowed[0].get("transports").is_none());
    assert_eq!(
        allowed[0]["id"],
        serde_json::to_value(Base64UrlSafeData::from(vec![1; 32])).unwrap()
    );
}
