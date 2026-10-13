//! Private HoYoLAB Cookie credential parsing and at-rest sealing.
use crate::dashboard::error::{Error, Result};
use openssl::symm::{Cipher, decrypt_aead, encrypt_aead};
use rand::TryRngCore;
use std::collections::HashMap;
use uuid::Uuid;

#[cfg(test)]
#[path = "credentials_tests.rs"]
mod tests;

const MAX_COOKIE_LEN: usize = 8192;
const VERSION: u8 = 1;
const NONCE_LEN: usize = 12;
const TAG_LEN: usize = 16;
const KEY_LEN: usize = 32;
const AAD_PREFIX: &[u8] = b"attosgameapi:dashboard:hoyolab-cookie:v1";

pub(in crate::dashboard) struct Credentials {
    header: String,
    claimed_uid: String,
}

impl Credentials {
    pub(in crate::dashboard) fn parse(raw: &str) -> Result<Self> {
        if raw.is_empty() || raw.len() > MAX_COOKIE_LEN {
            return Err(Error::Invalid("Cookie header must contain 1 to 8192 bytes"));
        }
        if !raw.is_ascii()
            || raw
                .bytes()
                .any(|byte| byte.is_ascii_control() || byte == 0x7f)
        {
            return Err(Error::Invalid("Cookie header must be printable ASCII"));
        }

        let pairs = parse_pairs(raw)?;
        let has_legacy_token = pairs.contains_key("ltoken");
        let has_v2_token = pairs.contains_key("ltoken_v2");
        if has_legacy_token && has_v2_token {
            return Err(Error::Invalid(
                "Cookie header mixes HoYoLAB credential families",
            ));
        }
        if has_legacy_token {
            return build_legacy(&pairs);
        }
        if has_v2_token {
            return build_v2(&pairs);
        }
        Err(Error::Invalid(
            "Cookie header is missing HoYoLAB credentials",
        ))
    }

    pub(in crate::dashboard) fn as_str(&self) -> &str {
        &self.header
    }

    // This is only the UID claimed inside the Cookie. It is not ownership proof.
    pub(in crate::dashboard) fn claimed_uid(&self) -> &str {
        &self.claimed_uid
    }
}

pub(in crate::dashboard) struct CookieKey([u8; KEY_LEN]);

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

fn parse_pairs(raw: &str) -> Result<HashMap<&str, &str>> {
    let mut pairs = HashMap::new();
    for part in raw.split(';') {
        let part = part.trim_matches(' ');
        let (name, value) = part
            .split_once('=')
            .ok_or(Error::Invalid("Cookie header contains a malformed pair"))?;
        if name.is_empty() || !name.bytes().all(is_cookie_name_byte) {
            return Err(Error::Invalid("Cookie header contains an invalid name"));
        }
        if pairs.insert(name, value).is_some() {
            return Err(Error::Invalid("Cookie header contains duplicate names"));
        }
    }
    Ok(pairs)
}

fn build_legacy(pairs: &HashMap<&str, &str>) -> Result<Credentials> {
    let token = parse_cookie_token(pairs.get("ltoken").copied().ok_or(Error::Invalid(
        "Cookie header is missing HoYoLAB credentials",
    ))?)?;
    let claimed_uid = resolve_uid_aliases(pairs, &["ltuid", "account_id"])?;
    reject_conflicting_uid_aliases(pairs, &claimed_uid, &["ltuid", "account_id"])?;
    let header = format!("ltoken={token}; ltuid={claimed_uid}");
    Ok(Credentials {
        header,
        claimed_uid,
    })
}

fn build_v2(pairs: &HashMap<&str, &str>) -> Result<Credentials> {
    let token = parse_cookie_token(pairs.get("ltoken_v2").copied().ok_or(Error::Invalid(
        "Cookie header is missing HoYoLAB credentials",
    ))?)?;
    let claimed_uid = resolve_uid_aliases(pairs, &["ltuid_v2", "account_id_v2"])?;
    reject_conflicting_uid_aliases(pairs, &claimed_uid, &["ltuid_v2", "account_id_v2"])?;
    let mid = resolve_optional_token_aliases(pairs, &["ltmid_v2", "account_mid_v2"])?;

    let mut header = format!("ltoken_v2={token}; ltuid_v2={claimed_uid}");
    if let Some(mid) = mid {
        header.push_str("; ltmid_v2=");
        header.push_str(&mid);
    }
    Ok(Credentials {
        header,
        claimed_uid,
    })
}

fn resolve_uid_aliases(pairs: &HashMap<&str, &str>, aliases: &[&str]) -> Result<String> {
    let mut uid = None;
    for alias in aliases {
        if let Some(value) = pairs.get(alias).copied() {
            let parsed = parse_uid(value)?;
            if uid.as_deref().is_some_and(|uid| uid != parsed) {
                return Err(Error::Invalid(
                    "Cookie header contains conflicting UID aliases",
                ));
            }
            uid = Some(parsed);
        }
    }
    uid.ok_or(Error::Invalid("Cookie header is missing HoYoLAB UID"))
}

fn reject_conflicting_uid_aliases(
    pairs: &HashMap<&str, &str>,
    selected_uid: &str,
    selected_aliases: &[&str],
) -> Result<()> {
    for alias in [
        "ltuid",
        "account_id",
        "ltuid_v2",
        "account_id_v2",
        "stuid",
        "stuid_v2",
    ] {
        if selected_aliases.contains(&alias) {
            continue;
        }
        if let Some(value) = pairs.get(alias).copied() {
            let parsed = parse_uid(value)?;
            if parsed != selected_uid {
                return Err(Error::Invalid(
                    "Cookie header contains conflicting UID aliases",
                ));
            }
        }
    }
    Ok(())
}

fn resolve_optional_token_aliases(
    pairs: &HashMap<&str, &str>,
    aliases: &[&str],
) -> Result<Option<String>> {
    let mut token = None;
    for alias in aliases {
        if let Some(value) = pairs.get(alias).copied() {
            let parsed = parse_cookie_token(value)?.to_owned();
            if token.as_deref().is_some_and(|token| token != parsed) {
                return Err(Error::Invalid(
                    "Cookie header contains conflicting MID aliases",
                ));
            }
            token = Some(parsed);
        }
    }
    Ok(token)
}

fn parse_uid(value: &str) -> Result<String> {
    reject_placeholder(value)?;
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(Error::Invalid(
            "HoYoLAB UID must be a positive decimal integer",
        ));
    }
    let uid: u64 = value
        .parse()
        .map_err(|_| Error::Invalid("HoYoLAB UID must be a positive decimal integer"))?;
    if uid == 0 {
        return Err(Error::Invalid(
            "HoYoLAB UID must be a positive decimal integer",
        ));
    }
    Ok(uid.to_string())
}

fn parse_cookie_token(value: &str) -> Result<&str> {
    reject_placeholder(value)?;
    if value.is_empty() || !value.bytes().all(is_cookie_value_token_byte) {
        return Err(Error::Invalid(
            "HoYoLAB Cookie token contains invalid characters",
        ));
    }
    Ok(value)
}

fn reject_placeholder(value: &str) -> Result<()> {
    if value.starts_with("YOUR_") || value.contains("...") {
        return Err(Error::Invalid(
            "HoYoLAB Cookie contains a placeholder value",
        ));
    }
    Ok(())
}

fn is_cookie_name_byte(byte: u8) -> bool {
    matches!(
        byte,
        b'!' | b'#'
            | b'$'
            | b'%'
            | b'&'
            | b'\''
            | b'*'
            | b'+'
            | b'-'
            | b'.'
            | b'^'
            | b'_'
            | b'`'
            | b'|'
            | b'~'
            | b'0'..=b'9'
            | b'A'..=b'Z'
            | b'a'..=b'z'
    )
}

fn is_cookie_value_token_byte(byte: u8) -> bool {
    byte.is_ascii_graphic() && !matches!(byte, b';' | b'"' | b',' | b'\\')
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
