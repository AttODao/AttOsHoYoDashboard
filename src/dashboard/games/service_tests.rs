use super::service::Games;
use crate::dashboard::{
    auth::{
        password::token_hash,
        sessions::{Identity, Sessions},
    },
    db::entities::{game_account, game_status_cache, hoyolab_account, reward_setting, user},
    error::Error,
    hoyolab::{
        credentials::{CookieKey, Credentials},
        gateway::{DailyAward, DailyState, Gateway, SignState},
        store::CookieStore,
    },
    test_support::TestDatabase,
};
use attoshoyodashboard::{
    game::GameKind,
    hoyolab::{Error as UpstreamError, RecordAccount, Result as UpstreamResult},
    models::account::Account,
};
use chrono::{Duration, Utc};
use sea_orm::{ActiveModelTrait, EntityTrait, Set};
use serde_json::{Value, json};
use std::{
    collections::VecDeque,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
};
use uuid::Uuid;

#[derive(Clone)]
struct MockGateway {
    notes: Arc<Mutex<VecDeque<UpstreamResult<Value>>>>,
    calls: Arc<AtomicUsize>,
}

impl MockGateway {
    fn new(notes: impl IntoIterator<Item = UpstreamResult<Value>>) -> Self {
        Self {
            notes: Arc::new(Mutex::new(notes.into_iter().collect())),
            calls: Arc::new(AtomicUsize::new(0)),
        }
    }

    fn empty() -> Self {
        Self::new(Vec::<UpstreamResult<Value>>::new())
    }

    fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}

impl Gateway for MockGateway {
    async fn identity(&self, _: &Credentials) -> UpstreamResult<(String, String)> {
        panic!("identity must not be called by games service tests")
    }

    async fn roles(&self, _: &Credentials) -> UpstreamResult<Vec<Account>> {
        panic!("roles must not be called by games service tests")
    }

    async fn note(&self, _: &Credentials, _: GameKind, _: &RecordAccount) -> UpstreamResult<Value> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.notes
            .lock()
            .unwrap()
            .pop_front()
            .expect("unexpected note call")
    }

    async fn daily(&self, _: &Credentials, _: GameKind) -> UpstreamResult<DailyState> {
        panic!("daily must not be called by games service tests")
    }

    async fn sign(&self, _: &Credentials, _: GameKind) -> UpstreamResult<SignState> {
        panic!("sign must not be called by games service tests")
    }

    async fn awards(&self, _: &Credentials, _: GameKind) -> UpstreamResult<Vec<DailyAward>> {
        panic!("awards must not be called by games service tests")
    }
}

fn key() -> Arc<CookieKey> {
    Arc::new(CookieKey::from_hex(&"11".repeat(32)).unwrap())
}

fn raw_cookie(uid: &str) -> String {
    format!("ltoken_v2=synthetictoken{uid}; ltuid_v2={uid}")
}

async fn ready_identity(db: &sea_orm::DatabaseConnection, name: &str) -> Identity {
    let user = user::ActiveModel {
        id: Set(Uuid::new_v4()),
        username: Set(name.to_owned()),
        password_hash: Set("synthetic-private-hash".to_owned()),
        is_admin: Set(false),
        active: Set(true),
        must_change_password: Set(false),
        created_at: Set(Utc::now()),
    }
    .insert(db)
    .await
    .unwrap();
    let issued = Sessions::new(Duration::days(7))
        .issue(db, user.id)
        .await
        .unwrap();
    Sessions::authenticate(db, &token_hash(&issued.token).unwrap())
        .await
        .unwrap()
}

async fn verified_link(
    db: &sea_orm::DatabaseConnection,
    key: &CookieKey,
    owner: Uuid,
    uid: &str,
) -> hoyolab_account::Model {
    let now = Utc::now();
    let cookie = Credentials::parse(&raw_cookie(uid)).unwrap();
    hoyolab_account::ActiveModel {
        id: Set(Uuid::new_v4()),
        owner_user_id: Set(owner),
        hoyolab_uid: Set(Some(uid.to_owned())),
        encrypted_credentials: Set(Some(key.seal(owner, Some(uid), &cookie).unwrap())),
        credential_version: Set(1),
        auth_status: Set("verified".to_owned()),
        created_at: Set(now),
        updated_at: Set(now),
        verified_at: Set(None),
    }
    .insert(db)
    .await
    .unwrap()
}

async fn game(
    db: &sea_orm::DatabaseConnection,
    link: Uuid,
    kind: &str,
    uid: &str,
    visible: bool,
    detected: bool,
) -> game_account::Model {
    game_account::ActiveModel {
        id: Set(Uuid::new_v4()),
        hoyolab_account_id: Set(link),
        game_kind: Set(kind.to_owned()),
        game_uid: Set(uid.to_owned()),
        region: Set("os_usa".to_owned()),
        visible: Set(visible),
        detected: Set(detected),
        last_detected_at: Set(Utc::now()),
    }
    .insert(db)
    .await
    .unwrap()
}

async fn cache(
    db: &sea_orm::DatabaseConnection,
    game_id: Uuid,
    fetched_at: Option<chrono::DateTime<Utc>>,
    payload: Option<Value>,
    version: i32,
) -> game_status_cache::Model {
    game_status_cache::ActiveModel {
        game_account_id: Set(game_id),
        fetched_at: Set(fetched_at),
        updated_at: Set(Utc::now()),
        payload: Set(payload),
        payload_version: Set(version),
        error_code: Set(None),
        error_at: Set(None),
    }
    .insert(db)
    .await
    .unwrap()
}

fn service(
    db: sea_orm::DatabaseConnection,
    key: Arc<CookieKey>,
    gateway: MockGateway,
) -> Games<MockGateway> {
    Games::new(Arc::new(CookieStore::for_test(db, key, gateway)))
}

#[tokio::test]
async fn list_is_owner_scoped_and_visibility_toggle_persists_for_missing_accounts() {
    let fixture = TestDatabase::new().await;
    let owner = ready_identity(&fixture.db, "owner").await;
    let other = ready_identity(&fixture.db, "other").await;
    let key = key();
    let owner_link = verified_link(&fixture.db, &key, owner.user.id, "123").await;
    let other_link = verified_link(&fixture.db, &key, other.user.id, "456").await;
    let owned = game(
        &fixture.db,
        owner_link.id,
        "genshin",
        "800000001",
        false,
        false,
    )
    .await;
    let foreign = game(
        &fixture.db,
        other_link.id,
        "starrail",
        "900000001",
        true,
        true,
    )
    .await;
    let gateway = MockGateway::empty();
    let games = service(fixture.db.clone(), key, gateway.clone());

    let listed = games.list(&owner).await.unwrap();
    assert_eq!(
        listed.iter().map(|row| row.id).collect::<Vec<_>>(),
        vec![owned.id]
    );
    assert!(matches!(
        games.set_visible(&owner, foreign.id, false).await,
        Err(Error::NotFound)
    ));
    games.set_visible(&owner, owned.id, true).await.unwrap();
    let updated = game_account::Entity::find_by_id(owned.id)
        .one(&fixture.db)
        .await
        .unwrap()
        .unwrap();
    assert!(updated.visible);
    assert!(!updated.detected);
    assert_eq!(gateway.calls(), 0);
}

#[tokio::test]
async fn hidden_and_undetected_status_never_fetch_and_never_display_cached_payload() {
    let fixture = TestDatabase::new().await;
    let actor = ready_identity(&fixture.db, "owner").await;
    let key = key();
    let link = verified_link(&fixture.db, &key, actor.user.id, "123").await;
    let hidden = game(&fixture.db, link.id, "genshin", "800000001", false, true).await;
    let missing = game(&fixture.db, link.id, "starrail", "900000001", true, false).await;
    cache(
        &fixture.db,
        hidden.id,
        Some(Utc::now()),
        Some(json!({"private":"old"})),
        3,
    )
    .await;
    cache(
        &fixture.db,
        missing.id,
        Some(Utc::now()),
        Some(json!({"private":"old"})),
        3,
    )
    .await;
    let gateway = MockGateway::empty();
    let games = service(fixture.db.clone(), key, gateway.clone());

    let hidden_status = games.status(&actor, hidden.id).await.unwrap();
    let missing_status = games.status(&actor, missing.id).await.unwrap();

    assert_eq!(gateway.calls(), 0);
    assert!(hidden_status.payload.is_none());
    assert!(missing_status.payload.is_none());
    assert!(!hidden_status.visible);
    assert!(!missing_status.detected);
}

#[tokio::test]
async fn fresh_cache_is_reused_without_fetch_or_credential_leakage() {
    let fixture = TestDatabase::new().await;
    let actor = ready_identity(&fixture.db, "owner").await;
    let key = key();
    let link = verified_link(&fixture.db, &key, actor.user.id, "123").await;
    let account = game(&fixture.db, link.id, "genshin", "800000001", true, true).await;
    let payload = json!({"current_resin": 42});
    cache(
        &fixture.db,
        account.id,
        Some(Utc::now()),
        Some(payload.clone()),
        3,
    )
    .await;
    let gateway = MockGateway::empty();
    let games = service(fixture.db.clone(), key, gateway.clone());

    let status = games.status(&actor, account.id).await.unwrap();

    assert_eq!(gateway.calls(), 0);
    assert_eq!(status.payload, Some(payload));
    assert!(!status.stale);
    let debug = format!("{status:?}");
    assert!(!debug.contains("synthetictoken"));
    assert!(!debug.contains("ltoken"));
}

#[tokio::test]
async fn fresh_legacy_summary_is_refetched_to_populate_starrail_expedition_icons() {
    let fixture = TestDatabase::new().await;
    let actor = ready_identity(&fixture.db, "owner").await;
    let key = key();
    let link = verified_link(&fixture.db, &key, actor.user.id, "123").await;
    let account = game(&fixture.db, link.id, "starrail", "900000001", true, true).await;
    cache(
        &fixture.db,
        account.id,
        Some(Utc::now()),
        Some(json!({"expeditions":[]})),
        1,
    )
    .await;
    let raw: Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/hoyolab/starrail-note.json"
    ))
    .unwrap();
    let expected = raw["expeditions"][0].clone();
    let gateway = MockGateway::new([Ok(raw)]);
    let games = service(fixture.db.clone(), key, gateway.clone());

    let status = games.status(&actor, account.id).await.unwrap();

    assert_eq!(gateway.calls(), 1);
    assert!(!status.stale);
    let expedition = &status.payload.as_ref().unwrap()["expeditions"][0];
    for field in ["name", "status", "remaining_time", "item_url"] {
        assert_eq!(expedition[field], expected[field]);
    }
    let stored = game_status_cache::Entity::find_by_id(account.id)
        .one(&fixture.db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored.payload_version, 3);
    assert_eq!(stored.payload, status.payload);

    let cached = games.status(&actor, account.id).await.unwrap();
    assert_eq!(cached.payload, status.payload);
    assert_eq!(gateway.calls(), 1);
}

#[tokio::test]
async fn stale_visible_detected_status_fetches_summary_and_updates_cache() {
    let fixture = TestDatabase::new().await;
    let actor = ready_identity(&fixture.db, "owner").await;
    let key = key();
    let link = verified_link(&fixture.db, &key, actor.user.id, "123").await;
    let account = game(&fixture.db, link.id, "genshin", "800000001", true, true).await;
    cache(
        &fixture.db,
        account.id,
        Some(Utc::now() - Duration::minutes(10)),
        Some(json!({"current_resin": 1})),
        3,
    )
    .await;
    let gateway = MockGateway::new([Ok(json!({
        "current_resin": 50,
        "max_resin": 200,
        "resin_recovery_time": "60",
        "expeditions": []
    }))]);
    let games = service(fixture.db.clone(), key, gateway.clone());

    let status = games.status(&actor, account.id).await.unwrap();

    assert_eq!(gateway.calls(), 1);
    assert_eq!(status.payload.as_ref().unwrap()["current_resin"], 50);
    assert!(
        status
            .payload
            .as_ref()
            .unwrap()
            .get("resin_full_at")
            .is_some()
    );
    assert!(!status.stale);
    let stored = game_status_cache::Entity::find_by_id(account.id)
        .one(&fixture.db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored.payload, status.payload);
    assert!(stored.error_code.is_none());
}

#[tokio::test]
async fn upstream_failure_retains_supported_old_payload_and_records_safe_error() {
    let fixture = TestDatabase::new().await;
    let actor = ready_identity(&fixture.db, "owner").await;
    let key = key();
    let link = verified_link(&fixture.db, &key, actor.user.id, "123").await;
    let account = game(&fixture.db, link.id, "genshin", "800000001", true, true).await;
    let fetched_at = Utc::now() - Duration::minutes(10);
    let old = json!({"current_resin": 10});
    cache(
        &fixture.db,
        account.id,
        Some(fetched_at),
        Some(old.clone()),
        3,
    )
    .await;
    let gateway = MockGateway::new([Err(UpstreamError::Network), Ok(json!({}))]);
    let games = service(fixture.db.clone(), key, gateway.clone());

    let status = games.status(&actor, account.id).await.unwrap();

    assert_eq!(gateway.calls(), 1);
    assert_eq!(status.payload, Some(old.clone()));
    assert_eq!(
        status.fetched_at.map(|time| time.timestamp_micros()),
        Some(fetched_at.timestamp_micros())
    );
    assert_eq!(status.error_code.as_deref(), Some("network"));
    assert!(status.error_at.is_some());
    assert!(status.stale);
    let stored = game_status_cache::Entity::find_by_id(account.id)
        .one(&fixture.db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored.payload, Some(old));
    assert_eq!(stored.fetched_at, status.fetched_at);
    assert_eq!(stored.payload_version, 3);
    assert_eq!(stored.error_code.as_deref(), Some("network"));
    assert!(stored.error_at.is_some());

    let refreshed = games.status(&actor, account.id).await.unwrap();
    assert_eq!(gateway.calls(), 2);
    assert!(!refreshed.stale);
    assert!(refreshed.payload.is_none());
    let stored = game_status_cache::Entity::find_by_id(account.id)
        .one(&fixture.db)
        .await
        .unwrap()
        .unwrap();
    assert!(stored.payload.is_none());
    assert!(stored.fetched_at.unwrap() > fetched_at);
    assert_eq!(stored.payload_version, 3);
    assert!(stored.error_code.is_none());
    assert!(stored.error_at.is_none());
}

#[tokio::test]
async fn first_failed_fetch_has_no_payload_or_fetched_at() {
    let fixture = TestDatabase::new().await;
    let actor = ready_identity(&fixture.db, "owner").await;
    let key = key();
    let link = verified_link(&fixture.db, &key, actor.user.id, "123").await;
    let account = game(&fixture.db, link.id, "starrail", "900000001", true, true).await;
    let gateway = MockGateway::new([Err(UpstreamError::Http(500))]);
    let games = service(fixture.db.clone(), key, gateway.clone());

    let status = games.status(&actor, account.id).await.unwrap();

    assert_eq!(gateway.calls(), 1);
    assert!(status.payload.is_none());
    assert!(status.fetched_at.is_none());
    assert_eq!(status.error_code.as_deref(), Some("http:500"));
    let stored = game_status_cache::Entity::find_by_id(account.id)
        .one(&fixture.db)
        .await
        .unwrap()
        .unwrap();
    assert!(stored.payload.is_none());
    assert!(stored.fetched_at.is_none());
    assert_eq!(stored.payload_version, 3);
    assert_eq!(stored.error_code.as_deref(), Some("http:500"));
    assert!(stored.error_at.is_some());
}

#[tokio::test]
async fn unsupported_cache_version_is_not_displayed_and_is_replaced_by_refresh() {
    let fixture = TestDatabase::new().await;
    let actor = ready_identity(&fixture.db, "owner").await;
    let key = key();
    let link = verified_link(&fixture.db, &key, actor.user.id, "123").await;
    let account = game(&fixture.db, link.id, "starrail", "900000001", true, true).await;
    cache(
        &fixture.db,
        account.id,
        Some(Utc::now()),
        Some(json!({"old":"unsupported"})),
        99,
    )
    .await;
    let gateway = MockGateway::new([
        Err(UpstreamError::Network),
        Ok(json!({
            "current_stamina": 120,
            "max_stamina": 240,
            "stamina_recover_time": 0,
            "current_reserve_stamina": 9,
            "current_train_score": 100,
            "max_train_score": 500,
            "expeditions": []
        })),
    ]);
    let games = service(fixture.db.clone(), key, gateway.clone());

    let failed = games.status(&actor, account.id).await.unwrap();
    assert!(failed.payload.is_none());
    assert!(failed.fetched_at.is_none());
    let stored = game_status_cache::Entity::find_by_id(account.id)
        .one(&fixture.db)
        .await
        .unwrap()
        .unwrap();
    assert!(stored.payload.is_none());
    assert!(stored.fetched_at.is_none());
    assert_eq!(stored.payload_version, 3);

    let status = games.status(&actor, account.id).await.unwrap();

    assert_eq!(gateway.calls(), 2);
    assert_eq!(status.payload.as_ref().unwrap()["current_stamina"], 120);
    assert!(status.payload.as_ref().unwrap().get("old").is_none());
    let stored = game_status_cache::Entity::find_by_id(account.id)
        .one(&fixture.db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored.payload_version, 3);
}

#[tokio::test]
async fn game_privacy_error_backs_off_note_fetches_without_touching_reward_settings() {
    let fixture = TestDatabase::new().await;
    let actor = ready_identity(&fixture.db, "owner").await;
    let key = key();
    let link = verified_link(&fixture.db, &key, actor.user.id, "123").await;
    let account = game(&fixture.db, link.id, "genshin", "800000001", true, true).await;
    let next_run_at = Utc::now() + Duration::hours(1);
    let setting = reward_setting::ActiveModel {
        id: Set(Uuid::new_v4()),
        hoyolab_account_id: Set(link.id),
        game_kind: Set("genshin".into()),
        enabled: Set(true),
        local_time: Set("06:00".into()),
        auth_status: Set("ready".into()),
        next_run_at: Set(Some(next_run_at)),
    }
    .insert(&fixture.db)
    .await
    .unwrap();
    let gateway = MockGateway::new([
        Err(UpstreamError::Upstream { code: 10102 }),
        Ok(json!({
            "current_resin": 50,
            "max_resin": 200,
            "resin_recovery_time": "60",
            "expeditions": []
        })),
    ]);
    let games = service(fixture.db.clone(), key, gateway.clone());
    let status = games.status(&actor, account.id).await.unwrap();
    assert_eq!(status.error_code.as_deref(), Some("upstream:10102"));
    let status = games.status(&actor, account.id).await.unwrap();
    assert_eq!(status.error_code.as_deref(), Some("upstream:10102"));
    assert!(status.stale);
    assert_eq!(gateway.calls(), 1);
    let stored = reward_setting::Entity::find_by_id(setting.id)
        .one(&fixture.db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored, setting);

    // Once the backoff window passes, the record is fetched again and can recover.
    let mut cached: game_status_cache::ActiveModel =
        game_status_cache::Entity::find_by_id(account.id)
            .one(&fixture.db)
            .await
            .unwrap()
            .unwrap()
            .into();
    cached.error_at = Set(Some(Utc::now() - Duration::minutes(6)));
    cached.update(&fixture.db).await.unwrap();
    let recovered = games.status(&actor, account.id).await.unwrap();
    assert_eq!(gateway.calls(), 2);
    assert!(recovered.error_code.is_none());
    assert_eq!(recovered.payload.as_ref().unwrap()["current_resin"], 50);
}

#[tokio::test]
async fn cookie_invalid_upstream_error_marks_link_invalid_after_safe_cache_record() {
    let fixture = TestDatabase::new().await;
    let actor = ready_identity(&fixture.db, "owner").await;
    let key = key();
    let link = verified_link(&fixture.db, &key, actor.user.id, "123").await;
    let account = game(&fixture.db, link.id, "genshin", "800000001", true, true).await;
    let gateway = MockGateway::new([Err(UpstreamError::Upstream { code: -100 })]);
    let games = service(fixture.db.clone(), key, gateway.clone());

    let status = games.status(&actor, account.id).await.unwrap();

    assert_eq!(gateway.calls(), 1);
    assert_eq!(status.error_code.as_deref(), Some("upstream:-100"));
    let link = hoyolab_account::Entity::find_by_id(link.id)
        .one(&fixture.db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(link.auth_status, "cookie_invalid");
}
