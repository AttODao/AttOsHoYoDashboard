//! UnifiedPush delivery to app installations: Web Push requests (RFC 8030) with
//! aes128gcm payloads (RFC 8291) and VAPID (RFC 8292), using OpenSSL.
//!
//! A push only wakes the app, which then reads its inbox (`app.rs`); the payload carries no
//! notice text, and the inbox cursor keeps a notice from being shown twice. Endpoints are
//! user-supplied, so they follow the Webhook URL rules and are sealed like Webhook URLs.
use super::{
    channel::Notifications,
    delivery::validate_public_https_url,
    secret::{self, Purpose},
};
use crate::dashboard::{
    auth::sessions::Identity,
    db::entities::app_device,
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
use reqwest::{Client, Url};
use sea_orm::{ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, sea_query::Expr};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{
    sync::{
        Arc, OnceLock,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};
use uuid::Uuid;

const VAPID_PURPOSE: &[u8] = b"attoshoyodashboard:app-push-vapid:v1";
const RECORD_SIZE: u32 = 4096;
/// UnifiedPush endpoints are at most 1000 bytes.
const MAX_ENDPOINT: usize = 1000;
/// The wake-up the app acts on; it names no notice and holds no secret.
const PAYLOAD: &[u8] = br#"{"type":"inbox"}"#;
/// Undelivered wake-ups replace each other on the push server (RFC 8030 section 5.4).
const TOPIC: &str = "ahd-inbox";
/// A wake-up older than this is pointless: the app's 15-minute poll has read the inbox.
const TTL_SECONDS: &str = "3600";

/// Outcomes of push requests since the server started.
#[derive(Default)]
pub(super) struct Stats {
    sent: AtomicU64,
    gone: AtomicU64,
    failed: AtomicU64,
}

impl Stats {
    fn record(&self, outcome: Outcome) {
        let counter = match outcome {
            Outcome::Sent => &self.sent,
            Outcome::Gone => &self.gone,
            Outcome::Failed => &self.failed,
        };
        counter.fetch_add(1, Ordering::Relaxed);
    }

    fn snapshot(&self) -> [(&'static str, u64); 3] {
        [
            ("sent", self.sent.load(Ordering::Relaxed)),
            ("gone", self.gone.load(Ordering::Relaxed)),
            ("failed", self.failed.load(Ordering::Relaxed)),
        ]
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Outcome {
    /// Accepted by the push server.
    Sent,
    /// The push server reports the subscription as gone (404/410), or it is unusable.
    Gone,
    Failed,
}

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
    private_key(&group, d)
}

fn private_key(group: &EcGroup, d: BigNum) -> Result<EcKey<Private>> {
    let mut ctx = BigNumContext::new().map_err(|_| Error::Internal)?;
    let mut public = EcPoint::new(group).map_err(|_| Error::Internal)?;
    public
        .mul_generator2(group, &d, &mut ctx)
        .map_err(|_| Error::Internal)?;
    EcKey::from_private_components(group, &d, &public).map_err(|_| Error::Internal)
}

fn uncompressed(key: &EcKey<impl openssl::pkey::HasPublic>) -> Result<Vec<u8>> {
    let mut ctx = BigNumContext::new().map_err(|_| Error::Internal)?;
    key.public_key()
        .to_bytes(key.group(), PointConversionForm::UNCOMPRESSED, &mut ctx)
        .map_err(|_| Error::Internal)
}

/// The VAPID public key the app registers with its distributor (base64url, uncompressed P-256).
fn public_key(key: &CookieKey) -> Result<String> {
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

fn receiver_point(group: &EcGroup, receiver: &[u8]) -> Result<EcPoint> {
    let mut ctx = BigNumContext::new().map_err(|_| Error::Internal)?;
    if receiver.len() != 65 || receiver[0] != 4 {
        return Err(Error::Invalid("invalid push subscription keys"));
    }
    EcPoint::from_bytes(group, receiver, &mut ctx)
        .map_err(|_| Error::Invalid("invalid push subscription keys"))
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
    let point = receiver_point(&group, receiver)?;
    let peer = PKey::from_ec_key(
        EcKey::from_public_key(&group, &point)
            .map_err(|_| Error::Invalid("invalid push subscription keys"))?,
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

/// Encrypts with a fresh sender key and salt for every message.
fn encrypt(plaintext: &[u8], receiver: &[u8], auth: &[u8]) -> Result<Vec<u8>> {
    let group = group()?;
    let sender = EcKey::generate(&group).map_err(|_| Error::Internal)?;
    let mut salt = [0; 16];
    rand::rngs::OsRng
        .try_fill_bytes(&mut salt)
        .map_err(|_| Error::Random)?;
    encrypt_with(plaintext, receiver, auth, &sender, &salt)
}

/// RFC 8292 VAPID authorization for one push service origin; `subject` is a contact URI.
fn vapid_header(key: &CookieKey, endpoint: &str, subject: Option<&str>) -> Result<String> {
    let url = Url::parse(endpoint).map_err(|_| Error::Internal)?;
    let mut claims = json!({
        "aud": url.origin().ascii_serialization(),
        "exp": Utc::now().timestamp() + 12 * 3600,
    });
    if let Some(subject) = subject {
        claims["sub"] = json!(subject);
    }
    let header = b64url(br#"{"typ":"JWT","alg":"ES256"}"#);
    let signing_input = format!("{header}.{}", b64url(claims.to_string().as_bytes()));
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

/// What the app sends after its distributor gave it an endpoint; `p256dh` and `auth`
/// are the app's own Web Push keys (base64url), so only the app can read a push.
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(in crate::dashboard) struct PushInput {
    endpoint: String,
    p256dh: String,
    auth: String,
}

impl PushInput {
    /// The subscription to store: a public HTTPS endpoint and well-formed P-256 keys.
    fn validated(self) -> Result<Self> {
        let endpoint = validate_public_https_url(
            &self.endpoint,
            MAX_ENDPOINT,
            "push endpoint must be an HTTPS URL on a public host name",
        )?;
        receiver_point(&group()?, &b64url_decode(&self.p256dh)?)?;
        if b64url_decode(&self.auth)?.len() != 16 {
            return Err(Error::Invalid("invalid push subscription keys"));
        }
        Ok(Self { endpoint, ..self })
    }
}

#[derive(Serialize)]
pub(in crate::dashboard) struct PushView {
    /// The application server key to register with the distributor (base64url).
    vapid_public_key: String,
    /// Whether the server holds a push subscription for this installation.
    registered: bool,
}

fn client() -> Option<&'static Client> {
    static CLIENT: OnceLock<Option<Client>> = OnceLock::new();
    CLIENT
        .get_or_init(|| {
            Client::builder()
                .timeout(Duration::from_secs(10))
                .redirect(reqwest::redirect::Policy::none())
                .user_agent("AHD notifications")
                .build()
                .ok()
        })
        .as_ref()
}

/// One wake-up to one subscription. Errors never include the endpoint.
async fn send_one(key: &CookieKey, subject: Option<&str>, subscription: &PushInput) -> Outcome {
    let (Ok(receiver), Ok(auth)) = (
        b64url_decode(&subscription.p256dh),
        b64url_decode(&subscription.auth),
    ) else {
        return Outcome::Gone;
    };
    let body = match encrypt(PAYLOAD, &receiver, &auth) {
        Ok(body) => body,
        Err(Error::Invalid(_)) => return Outcome::Gone,
        Err(_) => return Outcome::Failed,
    };
    let (Ok(authorization), Some(client)) =
        (vapid_header(key, &subscription.endpoint, subject), client())
    else {
        return Outcome::Failed;
    };
    let response = client
        .post(&subscription.endpoint)
        .header("TTL", TTL_SECONDS)
        .header("Urgency", "normal")
        .header("Topic", TOPIC)
        .header("Content-Encoding", "aes128gcm")
        .header("Content-Type", "application/octet-stream")
        .header("Authorization", authorization)
        .body(body)
        .send()
        .await;
    match response.map(|response| response.status().as_u16()) {
        Ok(200..=299) => Outcome::Sent,
        Ok(404 | 410) => Outcome::Gone,
        _ => Outcome::Failed,
    }
}

/// Drops every push subscription of a user: account disable and deletion, next to the
/// session and API token revocation, in the caller's transaction.
pub(in crate::dashboard) async fn revoke_all(db: &impl ConnectionTrait, user: Uuid) -> Result<()> {
    app_device::Entity::update_many()
        .col_expr(
            app_device::Column::PushSubscription,
            Expr::value(Option::<Vec<u8>>::None),
        )
        .filter(app_device::Column::UserId.eq(user))
        .exec(db)
        .await?;
    Ok(())
}

impl Notifications {
    fn push_key(&self) -> Result<&Arc<CookieKey>> {
        self.key
            .as_ref()
            .ok_or(Error::Unavailable("push is not configured"))
    }

    /// The actor's own registered installation.
    async fn own_device(&self, actor: &Identity, device: Uuid) -> Result<app_device::Model> {
        let fresh = self.app_ready(actor).await?;
        app_device::Entity::find_by_id(device)
            .filter(app_device::Column::UserId.eq(fresh.user.id))
            .one(&self.db)
            .await?
            .ok_or(Error::NotFound)
    }

    async fn store_push(&self, device: &app_device::Model, sealed: Option<Vec<u8>>) -> Result<()> {
        app_device::Entity::update_many()
            .col_expr(app_device::Column::PushSubscription, Expr::value(sealed))
            .filter(app_device::Column::Id.eq(device.id))
            .filter(app_device::Column::UserId.eq(device.user_id))
            .exec(&self.db)
            .await?;
        Ok(())
    }

    /// Clears a subscription the push server reported as gone, unless the app replaced it
    /// while the request was in flight.
    async fn drop_gone_push(&self, device: &app_device::Model) -> Result<()> {
        let Some(sent) = device.push_subscription.clone() else {
            return Ok(());
        };
        app_device::Entity::update_many()
            .col_expr(
                app_device::Column::PushSubscription,
                Expr::value(Option::<Vec<u8>>::None),
            )
            .filter(app_device::Column::Id.eq(device.id))
            .filter(app_device::Column::PushSubscription.eq(sent))
            .exec(&self.db)
            .await?;
        Ok(())
    }

    pub(in crate::dashboard) async fn app_push_view(
        &self,
        actor: &Identity,
        device: Uuid,
    ) -> Result<PushView> {
        let key = self.push_key()?;
        let device = self.own_device(actor, device).await?;
        Ok(PushView {
            vapid_public_key: public_key(key)?,
            registered: device.push_subscription.is_some(),
        })
    }

    /// Stores (replaces) the installation's push subscription.
    pub(in crate::dashboard) async fn set_app_push(
        &self,
        actor: &Identity,
        device: Uuid,
        input: PushInput,
    ) -> Result<()> {
        let key = self.push_key()?;
        let device = self.own_device(actor, device).await?;
        let input = input.validated()?;
        let text = serde_json::to_string(&input).map_err(|_| Error::Internal)?;
        let sealed = secret::seal(key, Purpose::AppPush, device.id, &text)?;
        self.store_push(&device, Some(sealed)).await
    }

    pub(in crate::dashboard) async fn remove_app_push(
        &self,
        actor: &Identity,
        device: Uuid,
    ) -> Result<()> {
        let device = self.own_device(actor, device).await?;
        self.store_push(&device, None).await
    }

    /// Wakes every installation of `user` that has a push subscription. Best effort: the
    /// notice is already in the inbox, so failures only leave it to the app's poll.
    pub(super) async fn push_to_app(&self, user: Uuid) {
        let Some(key) = self.key.clone() else {
            return;
        };
        let devices = match app_device::Entity::find()
            .filter(app_device::Column::UserId.eq(user))
            .filter(app_device::Column::PushSubscription.is_not_null())
            .all(&self.db)
            .await
        {
            Ok(devices) => devices,
            Err(_) => {
                tracing::warn!("app push lookup failed");
                return;
            }
        };
        let subject = self
            .origin
            .starts_with("https://")
            .then(|| self.origin.clone());
        let mut sends = tokio::task::JoinSet::new();
        for device in devices {
            let subscription = device
                .push_subscription
                .as_deref()
                .and_then(|blob| secret::open(&key, Purpose::AppPush, device.id, blob).ok())
                .and_then(|text| serde_json::from_str::<PushInput>(&text).ok());
            let (key, subject) = (Arc::clone(&key), subject.clone());
            sends.spawn(async move {
                let outcome = match &subscription {
                    Some(subscription) => send_one(&key, subject.as_deref(), subscription).await,
                    // Unreadable after a key change: the app registers again on its next start.
                    None => Outcome::Gone,
                };
                (device, outcome)
            });
        }
        while let Some(joined) = sends.join_next().await {
            let Ok((device, outcome)) = joined else {
                continue;
            };
            self.push_stats.record(outcome);
            match outcome {
                Outcome::Sent => {}
                Outcome::Gone => {
                    if self.drop_gone_push(&device).await.is_err() {
                        tracing::warn!("app push cleanup failed");
                    }
                }
                Outcome::Failed => tracing::warn!("app push delivery failed"),
            }
        }
    }

    /// Push request outcomes since start (`sent`, `gone`, `failed`).
    pub(in crate::dashboard) fn push_outcomes(&self) -> [(&'static str, u64); 3] {
        self.push_stats.snapshot()
    }
}

#[cfg(test)]
#[path = "push_tests.rs"]
mod tests;
