//! Web Push crypto against the RFC vectors, subscription rules, and wake-ups sent to local
//! mock push servers (404/410 drop the subscription).
use super::*;
use crate::dashboard::{
    db::entities::app_notification, notifications::Notice, test_support::TestDatabase,
};
use openssl::symm::decrypt_aead;
use sea_orm::{ActiveModelTrait, PaginatorTrait, Set};
use std::sync::Mutex;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};

fn key() -> CookieKey {
    CookieKey::from_hex(&"11".repeat(32)).unwrap()
}

fn decode(text: &str) -> Vec<u8> {
    b64url_decode(&text.replace(char::is_whitespace, "")).unwrap()
}

#[test]
fn vapid_key_is_stable_per_cookie_key_and_signs_verifiable_tokens() {
    let key = key();
    let public = public_key(&key).unwrap();
    assert_eq!(public, public_key(&key).unwrap());
    assert_ne!(
        public,
        public_key(&CookieKey::from_hex(&"22".repeat(32)).unwrap()).unwrap()
    );
    // UnifiedPush expects the 87-character base64url form of an uncompressed P-256 key.
    assert_eq!(public.len(), 87);
    assert_eq!(b64url_decode(&public).unwrap()[0], 4);
    for subject in [Some("https://ahd.example.test"), None] {
        let header = vapid_header(&key, "https://push.example.test/up/abc?up=1", subject).unwrap();
        let (token, k) = header
            .strip_prefix("vapid t=")
            .unwrap()
            .split_once(", k=")
            .unwrap();
        assert_eq!(k, public);
        let parts: Vec<&str> = token.split('.').collect();
        let claims: serde_json::Value =
            serde_json::from_slice(&b64url_decode(parts[1]).unwrap()).unwrap();
        assert_eq!(claims["aud"], "https://push.example.test");
        assert_eq!(claims.get("sub").and_then(|s| s.as_str()), subject);
        assert!(claims["exp"].as_i64().unwrap() > Utc::now().timestamp());
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
}

#[test]
fn encryption_matches_rfc_8291_appendix_a() {
    let group = group().unwrap();
    let as_private =
        BigNum::from_slice(&decode("yfWPiYE-n46HLnH0KqZOF1fJJU3MYrct3AELtAQ-oRw")).unwrap();
    let sender = private_key(&group, as_private).unwrap();
    assert_eq!(
        b64url(&uncompressed(&sender).unwrap()),
        "BP4z9KsN6nGRTbVYI_c7VJSPQTBtkgcy27mlmlMoZIIgDll6e3vCYLocInmYWAmS6TlzAC8wEqKK6PBru3jl7A8"
    );
    let ua_public = decode(
        "BCVxsr7N_eNgVRqvHtD0zTZsEc6-VV-JvLexhqUzORcx aOzi6-AYWXvTBHm4bjyPjs7Vd8pZGH6SRpkNtoIAiw4",
    );
    let salt: [u8; 16] = decode("DGv6ra1nlYgDCS1FRnbzlw").try_into().unwrap();
    let body = encrypt_with(
        &decode("V2hlbiBJIGdyb3cgdXAsIEkgd2FudCB0byBiZSBhIHdhdGVybWVsb24"),
        &ua_public,
        &decode("BTBZMqHH6r4Tts7J_aSIgg"),
        &sender,
        &salt,
    )
    .unwrap();
    let mut expected = decode(
        "DGv6ra1nlYgDCS1FRnbzlwAAEABBBP4z 9KsN6nGRTbVYI_c7VJSPQTBtkgcy27ml
         mlMoZIIgDll6e3vCYLocInmYWAmS6Tlz AC8wEqKK6PBru3jl7A8",
    );
    expected.extend(decode(
        "8pfeW0KbunFT06SuDKoJH9Ql87S1QUrd irN6GcG7sFz1y1sqLgVi1VhjVkHsUoEs bI_0LpXMuGvnzQ",
    ));
    assert_eq!(body, expected);
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

/// A user agent's Web Push key pair and auth secret, as the app creates them.
struct Receiver {
    key: EcKey<Private>,
    auth: [u8; 16],
}

impl Receiver {
    fn new() -> Self {
        Self {
            key: EcKey::generate(&group().unwrap()).unwrap(),
            auth: [7; 16],
        }
    }

    fn input(&self, endpoint: &str) -> PushInput {
        PushInput {
            endpoint: endpoint.to_owned(),
            p256dh: b64url(&uncompressed(&self.key).unwrap()),
            auth: b64url(&self.auth),
        }
    }

    /// The app side of RFC 8291, reimplemented to check the sender.
    fn decrypt(&self, body: &[u8]) -> Vec<u8> {
        let salt = &body[..16];
        assert_eq!(
            u32::from_be_bytes(body[16..20].try_into().unwrap()),
            RECORD_SIZE
        );
        let id_len = body[20] as usize;
        let sender_public = &body[21..21 + id_len];
        let group = group().unwrap();
        let mut ctx = BigNumContext::new().unwrap();
        let point = EcPoint::from_bytes(&group, sender_public, &mut ctx).unwrap();
        let peer = PKey::from_ec_key(EcKey::from_public_key(&group, &point).unwrap()).unwrap();
        let own = PKey::from_ec_key(self.key.clone()).unwrap();
        let mut deriver = Deriver::new(&own).unwrap();
        deriver.set_peer(&peer).unwrap();
        let shared = deriver.derive_to_vec().unwrap();
        let mut key_info = b"WebPush: info\0".to_vec();
        key_info.extend_from_slice(&uncompressed(&self.key).unwrap());
        key_info.extend_from_slice(sender_public);
        let ikm = hkdf(&self.auth, &shared, &key_info, 32).unwrap();
        let cek = hkdf(salt, &ikm, b"Content-Encoding: aes128gcm\0", 16).unwrap();
        let nonce = hkdf(salt, &ikm, b"Content-Encoding: nonce\0", 12).unwrap();
        let record = &body[21 + id_len..];
        let (ciphertext, tag) = record.split_at(record.len() - 16);
        decrypt_aead(
            Cipher::aes_128_gcm(),
            &cek,
            Some(&nonce),
            &[],
            ciphertext,
            tag,
        )
        .unwrap()
    }
}

#[test]
fn subscriptions_need_public_https_endpoints_and_p256_keys() {
    let receiver = Receiver::new();
    let ok = receiver
        .input(" https://ntfy.example.test/upAbC123?up=1 ")
        .validated()
        .unwrap();
    assert_eq!(ok.endpoint, "https://ntfy.example.test/upAbC123?up=1");
    for endpoint in [
        "http://ntfy.example.test/up",
        "https://127.0.0.1/up",
        "https://[::1]/up",
        "https://localhost/up",
        "https://push.lan/up",
        "https://user:pass@ntfy.example.test/up",
        "https://intranet/up",
        "",
    ] {
        assert!(receiver.input(endpoint).validated().is_err(), "{endpoint}");
    }
    let long = format!("https://ntfy.example.test/{}", "a".repeat(1000));
    assert!(receiver.input(&long).validated().is_err());
    let base = receiver.input("https://ntfy.example.test/up");
    let mut off_curve = vec![4u8];
    off_curve.extend([1u8; 64]);
    for (p256dh, auth) in [
        (b64url(&off_curve), base.auth.clone()),
        (b64url(&[4u8; 33]), base.auth.clone()),
        ("not base64!".to_owned(), base.auth.clone()),
        (base.p256dh.clone(), b64url(&[1u8; 15])),
        (base.p256dh.clone(), "".to_owned()),
    ] {
        let input = PushInput {
            p256dh,
            auth,
            ..base.clone()
        };
        assert!(input.validated().is_err());
    }
}

/// What a mock push server saw: request head (lowercased) and body.
type Seen = Arc<Mutex<Vec<(String, Vec<u8>)>>>;

/// A plain-HTTP push server answering every request with `status`.
async fn push_server(status: u16) -> (String, Seen) {
    let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
    let url = format!(
        "http://{}/up/synthetic?up=1",
        listener.local_addr().unwrap()
    );
    let seen: Seen = Arc::default();
    let log = Arc::clone(&seen);
    tokio::spawn(async move {
        while let Ok((mut stream, _)) = listener.accept().await {
            let mut data = Vec::new();
            let mut buffer = [0; 4096];
            let (head, body) = loop {
                let n = stream.read(&mut buffer).await.unwrap();
                if n == 0 {
                    break (String::new(), Vec::new());
                }
                data.extend_from_slice(&buffer[..n]);
                if let Some(end) = data.windows(4).position(|w| w == b"\r\n\r\n") {
                    let head = String::from_utf8_lossy(&data[..end]).to_ascii_lowercase();
                    let length: usize = head
                        .lines()
                        .find_map(|line| line.strip_prefix("content-length: "))
                        .map_or(0, |value| value.trim().parse().unwrap());
                    while data.len() < end + 4 + length {
                        let n = stream.read(&mut buffer).await.unwrap();
                        data.extend_from_slice(&buffer[..n]);
                    }
                    break (head, data[end + 4..end + 4 + length].to_vec());
                }
            };
            log.lock().unwrap().push((head, body));
            let reply =
                format!("HTTP/1.1 {status} X\r\ncontent-length: 0\r\nconnection: close\r\n\r\n");
            let _ = stream.write_all(reply.as_bytes()).await;
        }
    });
    (url, seen)
}

async fn device(db: &sea_orm::DatabaseConnection, user: Uuid, sealed: Option<Vec<u8>>) -> Uuid {
    let id = Uuid::new_v4();
    app_device::ActiveModel {
        id: Set(id),
        user_id: Set(user),
        created_at: Set(Utc::now()),
        last_seen_at: Set(Utc::now()),
        push_subscription: Set(sealed),
    }
    .insert(db)
    .await
    .unwrap();
    id
}

/// Seals a subscription the way `set_app_push` does; local test URLs skip validation.
fn sealed(device: Uuid, input: &PushInput) -> Vec<u8> {
    secret::seal(
        &key(),
        Purpose::AppPush,
        device,
        &serde_json::to_string(input).unwrap(),
    )
    .unwrap()
}

async fn subscription(db: &sea_orm::DatabaseConnection, device: Uuid) -> Option<Vec<u8>> {
    app_device::Entity::find_by_id(device)
        .one(db)
        .await
        .unwrap()
        .unwrap()
        .push_subscription
}

#[tokio::test]
async fn notices_wake_subscribed_installations_and_gone_subscriptions_are_dropped() {
    let fixture = TestDatabase::new().await;
    let (accounts, admin) = fixture.ready_admin().await;
    let db = &fixture.db;
    let user = admin.user.id;
    let notifications = Notifications::new(
        db.clone(),
        Some(Arc::new(key())),
        "https://ahd.example.test",
    );
    let receiver = Receiver::new();
    let (ok_url, ok_seen) = push_server(201).await;
    let (gone_url, gone_seen) = push_server(410).await;
    let (missing_url, _) = push_server(404).await;
    let (broken_url, broken_seen) = push_server(500).await;

    let polling = device(db, user, None).await;
    let ok = device(db, user, None).await;
    let gone = device(db, user, None).await;
    let missing = device(db, user, None).await;
    let broken = device(db, user, None).await;
    let unreadable = device(db, user, Some(vec![1, 2, 3])).await;
    for (id, url) in [
        (ok, &ok_url),
        (gone, &gone_url),
        (missing, &missing_url),
        (broken, &broken_url),
    ] {
        let row = app_device::Entity::find_by_id(id)
            .one(db)
            .await
            .unwrap()
            .unwrap();
        notifications
            .store_push(&row, Some(sealed(id, &receiver.input(url))))
            .await
            .unwrap();
    }

    assert!(
        notifications
            .send_to_app(user, &Notice::new("cookie_invalid", "cookie:1"), "ja")
            .await
            .unwrap()
    );
    // The notice is in the inbox before any push, so the woken app finds it.
    assert_eq!(
        app_notification::Entity::find()
            .filter(app_notification::Column::UserId.eq(user))
            .count(db)
            .await
            .unwrap(),
        1
    );
    let requests = ok_seen.lock().unwrap().clone();
    assert_eq!(requests.len(), 1);
    let (head, body) = &requests[0];
    assert!(head.starts_with("post /up/synthetic?up=1 "));
    for expected in [
        "content-encoding: aes128gcm",
        "ttl: 3600",
        "urgency: normal",
        "topic: ahd-inbox",
    ] {
        assert!(head.lines().any(|line| line == expected), "{expected}");
    }
    let authorization = head
        .lines()
        .find_map(|line| line.strip_prefix("authorization: vapid t="))
        .unwrap();
    assert!(
        authorization
            .ends_with(&format!(", k={}", public_key(&key()).unwrap()).to_ascii_lowercase())
    );
    // Only the app can read the wake-up, and it names no notice.
    assert_eq!(receiver.decrypt(body), b"{\"type\":\"inbox\"}\x02");
    assert!(!body.windows(6).any(|w| w == b"cookie"));
    assert_eq!(gone_seen.lock().unwrap().len(), 1);
    assert_eq!(broken_seen.lock().unwrap().len(), 1);

    assert!(subscription(db, ok).await.is_some());
    assert!(subscription(db, broken).await.is_some());
    for dropped in [gone, missing, unreadable] {
        assert_eq!(subscription(db, dropped).await, None);
    }
    assert_eq!(subscription(db, polling).await, None);
    assert_eq!(
        notifications.push_outcomes(),
        [("sent", 1), ("gone", 3), ("failed", 1)]
    );

    // A gone report for a subscription the app has replaced meanwhile keeps the new one.
    let before = app_device::Entity::find_by_id(ok)
        .one(db)
        .await
        .unwrap()
        .unwrap();
    let replaced = sealed(ok, &receiver.input(&ok_url));
    notifications
        .store_push(&before, Some(replaced.clone()))
        .await
        .unwrap();
    notifications.drop_gone_push(&before).await.unwrap();
    assert_eq!(subscription(db, ok).await, Some(replaced));

    // Account lifecycle: every subscription of the user goes, other users keep theirs.
    let other_user = accounts
        .create_user(
            &admin,
            "other".to_owned(),
            crate::dashboard::test_support::TEMP_PASSWORD.to_owned(),
        )
        .await
        .unwrap();
    let other = device(db, other_user.id, Some(vec![9])).await;
    revoke_all(db, user).await.unwrap();
    for id in [ok, broken] {
        assert_eq!(subscription(db, id).await, None);
    }
    assert_eq!(subscription(db, other).await, Some(vec![9]));
}
