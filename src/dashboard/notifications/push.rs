//! Browser push (RFC 8030/8291/8292): aes128gcm payloads and VAPID, using OpenSSL.
use super::{Notice, channel::Notifications, delivery::validate_webhook_url, messages::render};
use crate::dashboard::{
    auth::{
        accounts::require_ready,
        sessions::{Identity, Sessions},
    },
    db::entities::push_subscription,
    error::{Error, Result},
    hoyolab::credentials::CookieKey,
};
use chrono::Utc;
use openssl::{
    bn::{BigNum, BigNumContext},
    derive::Deriver,
    ec::{EcGroup, EcKey, EcPoint, PointConversionForm},
    ecdsa::EcdsaSig,
    hash::{MessageDigest, hash},
    nid::Nid,
    pkey::{PKey, Private},
    sign::Signer,
    symm::{Cipher, encrypt_aead},
};
use rand::TryRngCore;
use reqwest::Url;
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter, Set};
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

const VAPID_PURPOSE: &[u8] = b"attoshoyodashboard:vapid:v1";
const MAX_SUBSCRIPTIONS: u64 = 10;
const RECORD_SIZE: u32 = 4096;

fn b64url(bytes: &[u8]) -> String {
    openssl::base64::encode_block(bytes)
        .replace('+', "-")
        .replace('/', "_")
        .trim_end_matches('=')
        .to_owned()
}

fn b64url_decode(text: &str) -> Result<Vec<u8>> {
    let invalid = || Error::Invalid("push keys must be base64url");
    if text.is_empty()
        || !text
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return Err(invalid());
    }
    let mut standard = text.replace('-', "+").replace('_', "/");
    while !standard.len().is_multiple_of(4) {
        standard.push('=');
    }
    openssl::base64::decode_block(&standard).map_err(|_| invalid())
}

fn group() -> Result<EcGroup> {
    EcGroup::from_curve_name(Nid::X9_62_PRIME256V1).map_err(|_| Error::Internal)
}

/// The server's VAPID key, derived from the Cookie key so it survives restarts.
fn vapid_key(key: &CookieKey) -> Result<EcKey<Private>> {
    let group = group()?;
    let mut ctx = BigNumContext::new().map_err(|_| Error::Internal)?;
    let mut order = BigNum::new().map_err(|_| Error::Internal)?;
    group
        .order(&mut order, &mut ctx)
        .map_err(|_| Error::Internal)?;
    // d = seed mod (n - 1) + 1 lies in [1, n - 1].
    let seed = BigNum::from_slice(&key.derive(VAPID_PURPOSE)?).map_err(|_| Error::Internal)?;
    let mut range = BigNum::new().map_err(|_| Error::Internal)?;
    range
        .checked_sub(
            &order,
            BigNum::from_u32(1).map_err(|_| Error::Internal)?.as_ref(),
        )
        .map_err(|_| Error::Internal)?;
    let mut d = BigNum::new().map_err(|_| Error::Internal)?;
    d.nnmod(&seed, &range, &mut ctx)
        .map_err(|_| Error::Internal)?;
    d.add_word(1).map_err(|_| Error::Internal)?;
    let mut public = EcPoint::new(&group).map_err(|_| Error::Internal)?;
    public
        .mul_generator2(&group, &d, &mut ctx)
        .map_err(|_| Error::Internal)?;
    EcKey::from_private_components(&group, &d, &public).map_err(|_| Error::Internal)
}

fn uncompressed(key: &EcKey<impl openssl::pkey::HasPublic>) -> Result<Vec<u8>> {
    let mut ctx = BigNumContext::new().map_err(|_| Error::Internal)?;
    key.public_key()
        .to_bytes(key.group(), PointConversionForm::UNCOMPRESSED, &mut ctx)
        .map_err(|_| Error::Internal)
}

/// The application server key browsers subscribe with (base64url, uncompressed P-256).
pub(in crate::dashboard) fn public_key(key: &CookieKey) -> Result<String> {
    Ok(b64url(&uncompressed(&vapid_key(key)?)?))
}

fn hmac(key: &[u8], data: &[&[u8]]) -> Result<Vec<u8>> {
    let key = PKey::hmac(key).map_err(|_| Error::Internal)?;
    let mut signer = Signer::new(MessageDigest::sha256(), &key).map_err(|_| Error::Internal)?;
    for part in data {
        signer.update(part).map_err(|_| Error::Internal)?;
    }
    signer.sign_to_vec().map_err(|_| Error::Internal)
}

/// HKDF-SHA256 for outputs of at most one hash length.
fn hkdf(salt: &[u8], ikm: &[u8], info: &[u8], len: usize) -> Result<Vec<u8>> {
    let prk = hmac(salt, &[ikm])?;
    let mut okm = hmac(&prk, &[info, &[1]])?;
    okm.truncate(len);
    Ok(okm)
}

/// RFC 8291 aes128gcm body for one record, from an explicit sender key and salt.
fn encrypt_with(
    plaintext: &[u8],
    receiver: &[u8],
    auth: &[u8],
    sender: &EcKey<Private>,
    salt: &[u8; 16],
) -> Result<Vec<u8>> {
    let group = group()?;
    let mut ctx = BigNumContext::new().map_err(|_| Error::Internal)?;
    let point = EcPoint::from_bytes(&group, receiver, &mut ctx)
        .map_err(|_| Error::Invalid("invalid push key"))?;
    let peer = PKey::from_ec_key(
        EcKey::from_public_key(&group, &point).map_err(|_| Error::Invalid("invalid push key"))?,
    )
    .map_err(|_| Error::Internal)?;
    let own = PKey::from_ec_key(sender.clone()).map_err(|_| Error::Internal)?;
    let mut deriver = Deriver::new(&own).map_err(|_| Error::Internal)?;
    deriver.set_peer(&peer).map_err(|_| Error::Internal)?;
    let shared = deriver.derive_to_vec().map_err(|_| Error::Internal)?;
    let sender_public = uncompressed(sender)?;
    let mut key_info = b"WebPush: info\0".to_vec();
    key_info.extend_from_slice(receiver);
    key_info.extend_from_slice(&sender_public);
    let ikm = hkdf(auth, &shared, &key_info, 32)?;
    let cek = hkdf(salt, &ikm, b"Content-Encoding: aes128gcm\0", 16)?;
    let nonce = hkdf(salt, &ikm, b"Content-Encoding: nonce\0", 12)?;
    let mut padded = plaintext.to_vec();
    padded.push(2); // last-record delimiter
    let mut tag = [0; 16];
    let ciphertext = encrypt_aead(
        Cipher::aes_128_gcm(),
        &cek,
        Some(&nonce),
        &[],
        &padded,
        &mut tag,
    )
    .map_err(|_| Error::Internal)?;
    let mut body = Vec::with_capacity(86 + ciphertext.len() + 16);
    body.extend_from_slice(salt);
    body.extend_from_slice(&RECORD_SIZE.to_be_bytes());
    body.push(sender_public.len() as u8);
    body.extend_from_slice(&sender_public);
    body.extend_from_slice(&ciphertext);
    body.extend_from_slice(&tag);
    Ok(body)
}

fn encrypt(plaintext: &[u8], receiver: &[u8], auth: &[u8]) -> Result<Vec<u8>> {
    let group = group()?;
    let sender = EcKey::generate(&group).map_err(|_| Error::Internal)?;
    let mut salt = [0; 16];
    rand::rngs::OsRng
        .try_fill_bytes(&mut salt)
        .map_err(|_| Error::Random)?;
    encrypt_with(plaintext, receiver, auth, &sender, &salt)
}

/// RFC 8292 VAPID authorization for one push service origin.
fn vapid_header(key: &CookieKey, endpoint: &str, subject: &str) -> Result<String> {
    let url = Url::parse(endpoint).map_err(|_| Error::Invalid("invalid push endpoint"))?;
    let audience = url.origin().ascii_serialization();
    let header = b64url(br#"{"typ":"JWT","alg":"ES256"}"#);
    let claims = b64url(
        json!({"aud": audience, "exp": Utc::now().timestamp() + 12 * 3600, "sub": subject})
            .to_string()
            .as_bytes(),
    );
    let signing_input = format!("{header}.{claims}");
    let vapid = vapid_key(key)?;
    let digest =
        hash(MessageDigest::sha256(), signing_input.as_bytes()).map_err(|_| Error::Internal)?;
    let signature = EcdsaSig::sign(&digest, &vapid).map_err(|_| Error::Internal)?;
    let mut raw = signature
        .r()
        .to_vec_padded(32)
        .map_err(|_| Error::Internal)?;
    raw.extend(
        signature
            .s()
            .to_vec_padded(32)
            .map_err(|_| Error::Internal)?,
    );
    Ok(format!(
        "vapid t={signing_input}.{}, k={}",
        b64url(&raw),
        b64url(&uncompressed(&vapid)?)
    ))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::dashboard) struct SubscriptionKeys {
    p256dh: String,
    auth: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::dashboard) struct SubscriptionInput {
    endpoint: String,
    keys: SubscriptionKeys,
    #[serde(default, rename = "expirationTime")]
    _expiration_time: Option<serde_json::Value>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::dashboard) struct Unsubscription {
    endpoint: String,
}

impl Notifications {
    async fn push_ready(&self, actor: &Identity) -> Result<Identity> {
        if self.key.is_none() {
            return Err(Error::Unavailable("push is not configured"));
        }
        let fresh = Sessions::authenticate(&self.db, &actor.token_hash).await?;
        require_ready(&fresh.user)?;
        Ok(fresh)
    }

    /// Stores (or moves to this user) a browser's subscription.
    pub(in crate::dashboard) async fn subscribe(
        &self,
        actor: &Identity,
        input: SubscriptionInput,
    ) -> Result<()> {
        let fresh = self.push_ready(actor).await?;
        let endpoint = validate_webhook_url(&input.endpoint)?;
        let p256dh = b64url_decode(&input.keys.p256dh)?;
        let auth = b64url_decode(&input.keys.auth)?;
        if p256dh.len() != 65 || p256dh[0] != 4 || auth.len() != 16 {
            return Err(Error::Invalid("invalid push subscription keys"));
        }
        push_subscription::Entity::delete_many()
            .filter(push_subscription::Column::Endpoint.eq(&endpoint))
            .exec(&self.db)
            .await?;
        let count = push_subscription::Entity::find()
            .filter(push_subscription::Column::UserId.eq(fresh.user.id))
            .count(&self.db)
            .await?;
        if count >= MAX_SUBSCRIPTIONS {
            return Err(Error::Invalid("too many push subscriptions"));
        }
        push_subscription::ActiveModel {
            id: Set(Uuid::new_v4()),
            user_id: Set(fresh.user.id),
            endpoint: Set(endpoint),
            p256dh: Set(input.keys.p256dh),
            auth: Set(input.keys.auth),
            created_at: Set(Utc::now()),
        }
        .insert(&self.db)
        .await?;
        Ok(())
    }

    pub(in crate::dashboard) async fn unsubscribe(
        &self,
        actor: &Identity,
        input: Unsubscription,
    ) -> Result<()> {
        let fresh = self.push_ready(actor).await?;
        push_subscription::Entity::delete_many()
            .filter(push_subscription::Column::UserId.eq(fresh.user.id))
            .filter(push_subscription::Column::Endpoint.eq(input.endpoint.trim()))
            .exec(&self.db)
            .await?;
        Ok(())
    }
}

/// Sends a notice to every browser of a user; gone subscriptions are removed.
pub(super) async fn send_to_user(
    notifications: &Notifications,
    user: Uuid,
    notice: &Notice,
    language: &str,
) -> Result<usize> {
    let Some(key) = notifications.key.as_deref() else {
        return Ok(0);
    };
    let rows = push_subscription::Entity::find()
        .filter(push_subscription::Column::UserId.eq(user))
        .all(&notifications.db)
        .await?;
    if rows.is_empty() {
        return Ok(0);
    }
    let (title, body) = render(notice, language);
    let payload =
        json!({"title": title, "body": body, "url": "/dashboard", "tag": notice.key}).to_string();
    let mut sent = 0;
    for row in rows {
        match send_one(key, &notifications.origin, &row, payload.as_bytes()).await {
            Ok(true) => sent += 1,
            Ok(false) => {
                push_subscription::Entity::delete_by_id(row.id)
                    .exec(&notifications.db)
                    .await?;
            }
            Err(_) => tracing::warn!("browser push delivery failed"),
        }
    }
    Ok(sent)
}

/// Ok(false) when the push service reports the subscription as gone.
async fn send_one(
    key: &CookieKey,
    origin: &str,
    row: &push_subscription::Model,
    payload: &[u8],
) -> Result<bool> {
    let body = encrypt(
        payload,
        &b64url_decode(&row.p256dh)?,
        &b64url_decode(&row.auth)?,
    )?;
    let authorization = vapid_header(key, &row.endpoint, origin)?;
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| Error::Internal)?;
    let response = client
        .post(&row.endpoint)
        .header("TTL", "86400")
        .header("Urgency", "normal")
        .header("Content-Encoding", "aes128gcm")
        .header("Content-Type", "application/octet-stream")
        .header("Authorization", authorization)
        .body(body)
        .send()
        .await
        .map_err(|_| Error::Unavailable("push service unreachable"))?;
    match response.status().as_u16() {
        200..=299 => Ok(true),
        404 | 410 => Ok(false),
        _ => Err(Error::Unavailable("push service rejected the message")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openssl::symm::decrypt_aead;

    #[test]
    fn vapid_key_is_stable_per_cookie_key_and_signs_verifiable_tokens() {
        let key = CookieKey::from_hex(&"11".repeat(32)).unwrap();
        let public = public_key(&key).unwrap();
        assert_eq!(public, public_key(&key).unwrap());
        assert_ne!(
            public,
            public_key(&CookieKey::from_hex(&"22".repeat(32)).unwrap()).unwrap()
        );
        assert_eq!(b64url_decode(&public).unwrap().len(), 65);
        let header = vapid_header(
            &key,
            "https://push.example.test/send/abc",
            "https://ahd.example.test",
        )
        .unwrap();
        let token = header
            .strip_prefix("vapid t=")
            .unwrap()
            .split(", k=")
            .next()
            .unwrap();
        let parts: Vec<&str> = token.split('.').collect();
        let claims: serde_json::Value =
            serde_json::from_slice(&b64url_decode(parts[1]).unwrap()).unwrap();
        assert_eq!(claims["aud"], "https://push.example.test");
        let raw = b64url_decode(parts[2]).unwrap();
        let signature = EcdsaSig::from_private_components(
            BigNum::from_slice(&raw[..32]).unwrap(),
            BigNum::from_slice(&raw[32..]).unwrap(),
        )
        .unwrap();
        let digest = hash(
            MessageDigest::sha256(),
            format!("{}.{}", parts[0], parts[1]).as_bytes(),
        )
        .unwrap();
        assert!(
            signature
                .verify(&digest, &vapid_key(&key).unwrap())
                .unwrap()
        );
    }

    #[test]
    fn payloads_decrypt_on_the_receiving_side_per_rfc_8291() {
        // The receiver (browser) side, reimplemented to check the sender.
        let curve = group().unwrap();
        let receiver = EcKey::generate(&curve).unwrap();
        let receiver_public = uncompressed(&receiver).unwrap();
        let auth = [7u8; 16];
        let body = encrypt(b"{\"title\":\"t\"}", &receiver_public, &auth).unwrap();
        let salt = &body[..16];
        assert_eq!(
            u32::from_be_bytes(body[16..20].try_into().unwrap()),
            RECORD_SIZE
        );
        let id_len = body[20] as usize;
        let sender_public = &body[21..21 + id_len];
        let mut ctx = BigNumContext::new().unwrap();
        let point = EcPoint::from_bytes(&group().unwrap(), sender_public, &mut ctx).unwrap();
        let peer =
            PKey::from_ec_key(EcKey::from_public_key(&group().unwrap(), &point).unwrap()).unwrap();
        let own = PKey::from_ec_key(receiver).unwrap();
        let mut deriver = Deriver::new(&own).unwrap();
        deriver.set_peer(&peer).unwrap();
        let shared = deriver.derive_to_vec().unwrap();
        let mut key_info = b"WebPush: info\0".to_vec();
        key_info.extend_from_slice(&receiver_public);
        key_info.extend_from_slice(sender_public);
        let ikm = hkdf(&auth, &shared, &key_info, 32).unwrap();
        let cek = hkdf(salt, &ikm, b"Content-Encoding: aes128gcm\0", 16).unwrap();
        let nonce = hkdf(salt, &ikm, b"Content-Encoding: nonce\0", 12).unwrap();
        let record = &body[21 + id_len..];
        let (ciphertext, tag) = record.split_at(record.len() - 16);
        let plain = decrypt_aead(
            Cipher::aes_128_gcm(),
            &cek,
            Some(&nonce),
            &[],
            ciphertext,
            tag,
        )
        .unwrap();
        assert_eq!(&plain, b"{\"title\":\"t\"}\x02");
    }

    #[test]
    fn hkdf_matches_rfc_5869_case_1() {
        let ikm = [0x0b; 22];
        let salt: Vec<u8> = (0..=0x0c).collect();
        let info: Vec<u8> = (0xf0..=0xf9).collect();
        let okm = hkdf(&salt, &ikm, &info, 32).unwrap();
        assert_eq!(
            okm.iter().map(|b| format!("{b:02x}")).collect::<String>(),
            "3cb25f25faacd57a90434f64d0362f2a2d2d0a90cf1a5a4c5db02d56ecc4c5bf"
        );
    }
}
