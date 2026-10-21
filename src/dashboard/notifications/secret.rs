//! Webhook URLs are bearer secrets: sealed at rest with a purpose-derived key.
use crate::dashboard::{
    error::{Error, Result},
    hoyolab::credentials::CookieKey,
};
use openssl::symm::{Cipher, decrypt_aead, encrypt_aead};
use rand::TryRngCore;
use uuid::Uuid;

const VERSION: u8 = 1;
const NONCE_LEN: usize = 12;
const TAG_LEN: usize = 16;
const PURPOSE: &[u8] = b"attoshoyodashboard:notification-webhook:v1";

/// Seals a URL to its channel (a user id, or nil for the administrator channel).
pub(in crate::dashboard) fn seal(key: &CookieKey, scope: Uuid, url: &str) -> Result<Vec<u8>> {
    let subkey = key.derive(PURPOSE)?;
    let mut nonce = [0; NONCE_LEN];
    rand::rngs::OsRng
        .try_fill_bytes(&mut nonce)
        .map_err(|_| Error::Random)?;
    let mut tag = [0; TAG_LEN];
    let ciphertext = encrypt_aead(
        Cipher::aes_256_gcm(),
        &subkey,
        Some(&nonce),
        scope.as_bytes(),
        url.as_bytes(),
        &mut tag,
    )
    .map_err(|_| Error::Internal)?;
    let mut blob = Vec::with_capacity(1 + NONCE_LEN + ciphertext.len() + TAG_LEN);
    blob.push(VERSION);
    blob.extend_from_slice(&nonce);
    blob.extend_from_slice(&ciphertext);
    blob.extend_from_slice(&tag);
    Ok(blob)
}

pub(in crate::dashboard) fn open(key: &CookieKey, scope: Uuid, blob: &[u8]) -> Result<String> {
    if blob.len() <= 1 + NONCE_LEN + TAG_LEN || blob[0] != VERSION {
        return Err(Error::Internal);
    }
    let subkey = key.derive(PURPOSE)?;
    let tag_offset = blob.len() - TAG_LEN;
    let plaintext = decrypt_aead(
        Cipher::aes_256_gcm(),
        &subkey,
        Some(&blob[1..1 + NONCE_LEN]),
        scope.as_bytes(),
        &blob[1 + NONCE_LEN..tag_offset],
        &blob[tag_offset..],
    )
    .map_err(|_| Error::Internal)?;
    String::from_utf8(plaintext).map_err(|_| Error::Internal)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls_open_only_with_the_same_key_and_scope() {
        let key = CookieKey::from_hex(&"11".repeat(32)).unwrap();
        let other = CookieKey::from_hex(&"22".repeat(32)).unwrap();
        let scope = Uuid::new_v4();
        let url = "https://discord.com/api/webhooks/1/synthetic-token";
        let blob = seal(&key, scope, url).unwrap();
        assert!(!blob.windows(9).any(|window| window == b"synthetic"));
        assert_eq!(open(&key, scope, &blob).unwrap(), url);
        assert!(open(&other, scope, &blob).is_err());
        assert!(open(&key, Uuid::new_v4(), &blob).is_err());
        assert!(open(&key, scope, &blob[..10]).is_err());
    }
}
