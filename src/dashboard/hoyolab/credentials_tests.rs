use super::*;
use crate::dashboard::error::Error;
use openssl::symm::{Cipher, encrypt_aead};
use uuid::Uuid;

const LEGACY_TOKEN: &str = "fake.legacy-token_ABC-123==";
const V2_TOKEN: &str = "fake_v2.token-ABC123==";
const MID: &str = "fake_mid-ABC_123==";

#[test]
fn parse_enforces_cookie_size_ascii_controls_and_pair_syntax() {
    let max_token = "a".repeat(MAX_COOKIE_LEN - "ltoken=; ltuid=1".len());
    let max_raw = format!("ltoken={max_token}; ltuid=1");
    assert_eq!(max_raw.len(), MAX_COOKIE_LEN);
    assert_eq!(
        Credentials::parse(&max_raw)
            .expect("max-size Cookie parses")
            .as_str()
            .len(),
        MAX_COOKIE_LEN
    );

    let too_long = format!("{max_raw}a");
    for raw in [
        "",
        too_long.as_str(),
        "ltoken=abcé; ltuid=1",
        "ltoken=abc\r\nInjected: yes; ltuid=1",
        "ltoken=abc\u{7f}; ltuid=1",
        "ltoken; ltuid=1",
        "ltoken=abc; ; ltuid=1",
        "ltoken=abc; bad name=value; ltuid=1",
    ] {
        assert!(
            matches!(Credentials::parse(raw), Err(Error::Invalid(_))),
            "{raw:?} should be rejected"
        );
    }
}

#[test]
fn parse_rejects_duplicates_alias_conflicts_and_primary_family_mixing() {
    for raw in [
        "ltoken=fake; ltuid=1; ltuid=1",
        "ltoken=fake; ltuid=1; account_id=2",
        "ltoken_v2=fake; ltuid_v2=1; ltmid_v2=mid; account_mid_v2=other",
        "ltoken=fake; ltoken_v2=fake2; ltuid=1; ltuid_v2=1",
        "ltoken=fake; ltuid=1; account_id_v2=2",
        "ltoken=fake; ltuid_v2=1",
        "ltoken_v2=fake; ltuid=1",
    ] {
        assert!(
            matches!(Credentials::parse(raw), Err(Error::Invalid(_))),
            "{raw:?} should be rejected"
        );
    }
}

#[test]
fn parse_drops_unrelated_and_nonselected_token_cookies() {
    let raw = format!(
        "_ga=GA1; stoken=drop-me; cookie_token=drop-me-too; account_id=000123; ltoken={LEGACY_TOKEN}; ltuid=123; other=value"
    );
    let credentials = Credentials::parse(&raw).expect("parse legacy with droppable cookies");
    assert_eq!(
        credentials.as_str(),
        format!("ltoken={LEGACY_TOKEN}; ltuid=123")
    );
    assert_eq!(credentials.claimed_uid(), "123");
}

#[test]
fn parse_normalizes_legacy_v2_and_mid_aliases() {
    let legacy = Credentials::parse(&format!("account_id=000042; ltoken={LEGACY_TOKEN}"))
        .expect("parse legacy alias");
    assert_eq!(legacy.as_str(), format!("ltoken={LEGACY_TOKEN}; ltuid=42"));
    assert_eq!(legacy.claimed_uid(), "42");

    let v2 = Credentials::parse(&format!(
        "account_mid_v2={MID}; ltoken_v2={V2_TOKEN}; account_id_v2=000042"
    ))
    .expect("parse v2 aliases");
    assert_eq!(
        v2.as_str(),
        format!("ltoken_v2={V2_TOKEN}; ltuid_v2=42; ltmid_v2={MID}")
    );
    assert_eq!(v2.claimed_uid(), "42");

    let v2_matching_mid = Credentials::parse(&format!(
        "ltmid_v2={MID}; account_mid_v2={MID}; ltoken_v2={V2_TOKEN}; ltuid_v2=42"
    ))
    .expect("parse matching MID aliases");
    assert_eq!(
        v2_matching_mid.as_str(),
        format!("ltoken_v2={V2_TOKEN}; ltuid_v2=42; ltmid_v2={MID}")
    );
}

#[test]
fn parse_rejects_placeholders_bad_uids_and_bad_credential_tokens() {
    for raw in [
        "ltoken=YOUR_LTOKEN; ltuid=1",
        "ltoken=...; ltuid=1",
        "ltoken=fake; ltuid=0",
        "ltoken=fake; ltuid=-1",
        "ltoken=fake; ltuid=abc",
        "ltoken=fake; ltuid=18446744073709551616",
        "ltoken=; ltuid=1",
        "ltoken=bad token; ltuid=1",
        "ltoken=bad\"token; ltuid=1",
        "ltoken=bad,token; ltuid=1",
        "ltoken=bad\\token; ltuid=1",
        "ltoken_v2=fake; ltuid_v2=1; ltmid_v2=",
        "ltoken_v2=fake; ltuid_v2=1; ltmid_v2=bad token",
    ] {
        assert!(
            matches!(Credentials::parse(raw), Err(Error::Invalid(_))),
            "{raw:?} should be rejected"
        );
    }
}

#[test]
fn cookie_key_requires_exact_ascii_hex() {
    let key =
        CookieKey::from_hex("000102030405060708090a0b0c0d0e0f101112131415161718191A1B1C1D1E1F")
            .expect("parse mixed-case key");
    assert_eq!(
        key.0,
        [
            0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23,
            24, 25, 26, 27, 28, 29, 30, 31,
        ]
    );

    for raw in [
        "0",
        "000102030405060708090a0b0c0d0e0f101112131415161718191A1B1C1D1E",
        "000102030405060708090a0b0c0d0e0f101112131415161718191A1B1C1D1E1G",
        "000102030405060708090a0b0c0d0e0f101112131415161718191A1B1C1D1é",
    ] {
        assert!(matches!(CookieKey::from_hex(raw), Err(Error::Invalid(_))));
    }
}

#[test]
fn seal_uses_random_nonces_and_roundtrips_with_bound_context() {
    let key = test_key(7);
    let owner = Uuid::new_v4();
    let cookie = Credentials::parse(&format!("ltoken={LEGACY_TOKEN}; ltuid=42")).unwrap();

    let first = key.seal(owner, Some("42"), &cookie).expect("first seal");
    let second = key.seal(owner, Some("42"), &cookie).expect("second seal");
    assert_ne!(first, second);
    assert_eq!(first[0], VERSION);
    assert_eq!(second[0], VERSION);
    assert_ne!(&first[1..1 + NONCE_LEN], &second[1..1 + NONCE_LEN]);
    assert_eq!(first.len(), 1 + NONCE_LEN + cookie.as_str().len() + TAG_LEN);

    for blob in [&first, &second] {
        let opened = key
            .open(owner, Some("42"), blob)
            .expect("open sealed Cookie");
        assert_eq!(opened.as_str(), cookie.as_str());
        assert_eq!(opened.claimed_uid(), "42");
    }
}

#[test]
fn open_rejects_wrong_key_owner_verified_uid_version_and_short_blobs() {
    let key = test_key(1);
    let wrong_key = test_key(2);
    let owner = Uuid::new_v4();
    let cookie = Credentials::parse(&format!("ltoken={LEGACY_TOKEN}; ltuid=42")).unwrap();
    let blob = key.seal(owner, Some("42"), &cookie).unwrap();

    assert_internal(wrong_key.open(owner, Some("42"), &blob));
    assert_internal(key.open(Uuid::new_v4(), Some("42"), &blob));
    assert_internal(key.open(owner, Some("43"), &blob));
    assert_internal(key.open(owner, None, &blob));

    let mut wrong_version = blob.clone();
    wrong_version[0] = VERSION + 1;
    assert_internal(key.open(owner, Some("42"), &wrong_version));

    for len in [0, 1, 1 + NONCE_LEN + TAG_LEN] {
        assert_internal(key.open(owner, Some("42"), &blob[..len]));
    }
}

#[test]
fn open_rejects_nonce_ciphertext_and_tag_tampering() {
    let key = test_key(3);
    let owner = Uuid::new_v4();
    let cookie = Credentials::parse(&format!("ltoken={LEGACY_TOKEN}; ltuid=42")).unwrap();
    let blob = key.seal(owner, Some("42"), &cookie).unwrap();

    for index in [1, 1 + NONCE_LEN, blob.len() - 1] {
        let mut tampered = blob.clone();
        tampered[index] ^= 0x55;
        assert_internal(key.open(owner, Some("42"), &tampered));
    }
}

#[test]
fn open_rejects_decrypted_bad_utf8_and_invalid_cookie_plaintext() {
    let key = test_key(4);
    let owner = Uuid::new_v4();

    let bad_utf8 = seal_plaintext(&key, owner, None, b"\xff");
    assert_internal(key.open(owner, None, &bad_utf8));

    let invalid_cookie = seal_plaintext(&key, owner, None, b"ltoken=bad token; ltuid=1");
    assert_internal(key.open(owner, None, &invalid_cookie));
}

#[test]
fn verified_uid_and_plaintext_claim_must_match_without_promoting_pending_claims() {
    let key = test_key(5);
    let owner = Uuid::new_v4();
    let cookie = Credentials::parse("ltuid_v2=43; ltoken_v2=synthetic").unwrap();
    assert_internal(key.seal(owner, Some("42"), &cookie));
    let inconsistent = seal_plaintext(&key, owner, Some("42"), cookie.as_str().as_bytes());
    assert_internal(key.open(owner, Some("42"), &inconsistent));
    let pending = key.seal(owner, None, &cookie).unwrap();
    assert_eq!(key.open(owner, None, &pending).unwrap().claimed_uid(), "43");
}

fn test_key(byte: u8) -> CookieKey {
    CookieKey([byte; KEY_LEN])
}

fn seal_plaintext(
    key: &CookieKey,
    owner: Uuid,
    verified_uid: Option<&str>,
    plaintext: &[u8],
) -> Vec<u8> {
    let nonce = [9; NONCE_LEN];
    let aad = build_aad(owner, verified_uid);
    let mut tag = [0; TAG_LEN];
    let ciphertext = encrypt_aead(
        Cipher::aes_256_gcm(),
        &key.0,
        Some(&nonce),
        &aad,
        plaintext,
        &mut tag,
    )
    .expect("encrypt test plaintext");
    let mut blob = Vec::new();
    blob.push(VERSION);
    blob.extend_from_slice(&nonce);
    blob.extend_from_slice(&ciphertext);
    blob.extend_from_slice(&tag);
    blob
}

fn assert_internal<T>(result: Result<T>) {
    assert!(matches!(result, Err(Error::Internal)));
}
