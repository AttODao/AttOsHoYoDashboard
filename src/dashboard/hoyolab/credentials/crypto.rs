//! Versioned AES-256-GCM storage bound to owner and verification context.
use super::{CookieKey, Credentials, KEY_LEN, MAX_COOKIE_LEN};
use crate::dashboard::error::{Error, Result};
use openssl::symm::{Cipher, decrypt_aead, encrypt_aead};
use rand::TryRngCore;
use uuid::Uuid;

#[cfg(test)]
mod tests;

const VERSION: u8 = 1;
const NONCE_LEN: usize = 12;
const TAG_LEN: usize = 16;
const AAD_PREFIX: &[u8] = b"attosgameapi:dashboard:hoyolab-cookie:v1";

impl CookieKey {
    pub(in crate::dashboard) fn from_hex(raw: &str) -> Result<Self> {
        if raw.len() != KEY_LEN * 2 || !raw.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(Error::Invalid(
                "Cookie key must be exactly 64 ASCII hex characters",
            ));
        }
        let mut key = [0; KEY_LEN];
        for (index, chunk) in raw.as_bytes().as_chunks::<2>().0.iter().enumerate() {
            key[index] = (hex_value(chunk[0])? << 4) | hex_value(chunk[1])?;
        }
        Ok(Self(key))
    }

    pub(in crate::dashboard) fn seal(
        &self,
        owner: Uuid,
        verified_uid: Option<&str>,
        cookie: &Credentials,
    ) -> Result<Vec<u8>> {
        // Consistency after independent verification, never proof from a Cookie claim.
        if verified_uid.is_some_and(|uid| uid != cookie.claimed_uid()) {
            return Err(Error::Internal);
        }
        let mut nonce = [0; NONCE_LEN];
        rand::rngs::OsRng
            .try_fill_bytes(&mut nonce)
            .map_err(|_| Error::Random)?;

        let aad = build_aad(owner, verified_uid);
        let mut tag = [0; TAG_LEN];
        let ciphertext = encrypt_aead(
            Cipher::aes_256_gcm(),
            &self.0,
            Some(&nonce),
            &aad,
            cookie.as_str().as_bytes(),
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

    pub(in crate::dashboard) fn open(
        &self,
        owner: Uuid,
        verified_uid: Option<&str>,
        blob: &[u8],
    ) -> Result<Credentials> {
        if blob.len() <= 1 + NONCE_LEN + TAG_LEN
            || blob.len() > 1 + NONCE_LEN + MAX_COOKIE_LEN + TAG_LEN
        {
            return Err(Error::Internal);
        }
        if blob[0] != VERSION {
            return Err(Error::Internal);
        }

        let nonce = &blob[1..1 + NONCE_LEN];
        let tag_offset = blob.len() - TAG_LEN;
        let ciphertext = &blob[1 + NONCE_LEN..tag_offset];
        let tag = &blob[tag_offset..];
        let aad = build_aad(owner, verified_uid);
        let plaintext = decrypt_aead(
            Cipher::aes_256_gcm(),
            &self.0,
            Some(nonce),
            &aad,
            ciphertext,
            tag,
        )
        .map_err(|_| Error::Internal)?;
        let header = String::from_utf8(plaintext).map_err(|_| Error::Internal)?;
        let cookie = Credentials::parse(&header).map_err(|_| Error::Internal)?;
        if verified_uid.is_some_and(|uid| uid != cookie.claimed_uid()) {
            return Err(Error::Internal);
        }
        Ok(cookie)
    }
}

fn hex_value(byte: u8) -> Result<u8> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        _ => Err(Error::Invalid(
            "Cookie key must be exactly 64 ASCII hex characters",
        )),
    }
}

fn build_aad(owner: Uuid, verified_uid: Option<&str>) -> Vec<u8> {
    let uid = verified_uid.unwrap_or("").as_bytes();
    let mut aad = Vec::with_capacity(AAD_PREFIX.len() + 1 + 16 + 1 + 8 + uid.len());
    aad.extend_from_slice(AAD_PREFIX);
    aad.push(0);
    aad.extend_from_slice(owner.as_bytes());
    aad.push(u8::from(verified_uid.is_some()));
    aad.extend_from_slice(&(uid.len() as u64).to_be_bytes());
    aad.extend_from_slice(uid);
    aad
}
