use super::*;
use crate::dashboard::{
    auth::{password::token_hash, sessions::Sessions},
    db::entities::{hoyolab_account, reward_guard, reward_run, reward_setting, user},
    hoyolab::{
        credentials::{CookieKey, Credentials},
        gateway::SignState,
    },
    test_support::TestDatabase,
};
use attosgameapi::{
    game::GameKind,
    hoyolab::{Error as UpstreamError, RecordAccount, Result as UpstreamResult},
    models::account::Account,
};
use chrono::{Duration, Utc};
use sea_orm::{ActiveModelTrait, EntityTrait, IntoActiveModel, Set};
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
pub(super) struct MockGateway {
    daily: Arc<Mutex<VecDeque<UpstreamResult<DailyState>>>>,
    sign: Arc<Mutex<VecDeque<UpstreamResult<SignState>>>>,
    daily_calls: Arc<AtomicUsize>,
    sign_calls: Arc<AtomicUsize>,
}

impl MockGateway {
    pub(super) fn new(
        daily: impl IntoIterator<Item = UpstreamResult<DailyState>>,
        sign: impl IntoIterator<Item = UpstreamResult<SignState>>,
    ) -> Self {
        Self {
            daily: Arc::new(Mutex::new(daily.into_iter().collect())),
            sign: Arc::new(Mutex::new(sign.into_iter().collect())),
            daily_calls: Arc::new(AtomicUsize::new(0)),
            sign_calls: Arc::new(AtomicUsize::new(0)),
        }
    }
    pub(super) fn daily_calls(&self) -> usize {
        self.daily_calls.load(Ordering::SeqCst)
    }
    pub(super) fn sign_calls(&self) -> usize {
        self.sign_calls.load(Ordering::SeqCst)
    }
}

impl Gateway for MockGateway {
    async fn identity(&self, _: &Credentials) -> UpstreamResult<(String, String)> {
        panic!("identity unused")
    }
    async fn roles(&self, _: &Credentials) -> UpstreamResult<Vec<Account>> {
        panic!("roles unused")
    }
    async fn note(&self, _: &Credentials, _: GameKind, _: &RecordAccount) -> UpstreamResult<Value> {
        panic!("note unused")
    }
    async fn daily(&self, _: &Credentials, _: GameKind) -> UpstreamResult<DailyState> {
        self.daily_calls.fetch_add(1, Ordering::SeqCst);
        self.daily
            .lock()
            .unwrap()
            .pop_front()
            .expect("unexpected daily call")
    }
    async fn sign(&self, _: &Credentials, _: GameKind) -> UpstreamResult<SignState> {
        self.sign_calls.fetch_add(1, Ordering::SeqCst);
        self.sign
            .lock()
            .unwrap()
            .pop_front()
            .expect("unexpected sign call")
    }
}

pub(super) fn key() -> Arc<CookieKey> {
    Arc::new(CookieKey::from_hex(&"11".repeat(32)).unwrap())
}
fn raw_cookie(uid: &str) -> String {
    format!("ltoken_v2=synthetictoken{uid}; ltuid_v2={uid}")
}

pub(super) async fn ready_identity(db: &sea_orm::DatabaseConnection, name: &str) -> Identity {
    identity_with(db, name, false, false).await
}

async fn identity_with(
    db: &sea_orm::DatabaseConnection,
    name: &str,
    is_admin: bool,
    must_change_password: bool,
) -> Identity {
    let user = user::ActiveModel {
        id: Set(Uuid::new_v4()),
        username: Set(name.to_owned()),
        password_hash: Set("synthetic-private-hash".to_owned()),
        is_admin: Set(is_admin),
        active: Set(true),
        must_change_password: Set(must_change_password),
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

pub(super) async fn inserted_run(
    db: &sea_orm::DatabaseConnection,
    setting_id: Uuid,
    state: &str,
    send_state: &str,
    created_at: chrono::DateTime<Utc>,
) -> reward_run::Model {
    reward_run::ActiveModel {
        id: Set(Uuid::new_v4()),
        setting_id: Set(setting_id),
        trigger: Set("manual".to_owned()),
        state: Set(state.to_owned()),
        created_at: Set(created_at),
        updated_at: Set(created_at),
        ended_at: Set(None),
        attempt_count: Set(0),
        credential_version: Set(1),
        send_state: Set(send_state.to_owned()),
        next_retry_at: Set(None),
    }
    .insert(db)
    .await
    .unwrap()
}

pub(super) async fn verified_link(
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
        encrypted_credentials: Set(key.seal(owner, Some(uid), &cookie).unwrap()),
        credential_version: Set(1),
        auth_status: Set("verified".to_owned()),
        created_at: Set(now),
        updated_at: Set(now),
    }
    .insert(db)
    .await
    .unwrap()
}

pub(super) async fn setting(
    db: &sea_orm::DatabaseConnection,
    link: Uuid,
    kind: &str,
    enabled: bool,
) -> reward_setting::Model {
    reward_setting::ActiveModel {
        id: Set(Uuid::new_v4()),
        hoyolab_account_id: Set(link),
        game_kind: Set(kind.to_owned()),
        enabled: Set(enabled),
        local_time: Set("06:00".to_owned()),
        auth_status: Set("ready".to_owned()),
        next_run_at: Set(None),
    }
    .insert(db)
    .await
    .unwrap()
}

async fn cache(
    db: &sea_orm::DatabaseConnection,
    setting_id: Uuid,
    fetched_at: chrono::DateTime<Utc>,
    is_sign: bool,
) {
    reward_status_cache::ActiveModel {
        setting_id: Set(setting_id),
        fetched_at: Set(Some(fetched_at)),
        updated_at: Set(fetched_at),
        payload: Set(Some(json!({"is_sign": is_sign, "total_sign_day": 9}))),
        payload_version: Set(1),
        error_code: Set(None),
        error_at: Set(None),
    }
    .insert(db)
    .await
    .unwrap();
}

pub(super) fn service(
    db: sea_orm::DatabaseConnection,
    key: Arc<CookieKey>,
    gateway: MockGateway,
) -> Rewards<MockGateway> {
    Rewards::new(Arc::new(CookieStore::for_test(db, key, gateway)))
}

#[tokio::test]
async fn manual_run_works_when_off_and_duplicate_enqueue_coalesces() {
    let fixture = TestDatabase::new().await;
    let actor = ready_identity(&fixture.db, "owner").await;
    let key = key();
    let link = verified_link(&fixture.db, &key, actor.user.id, "123").await;
    let setting = setting(&fixture.db, link.id, "genshin", false).await;
    let gateway = MockGateway::new(
        [
            Ok(DailyState {
                is_sign: false,
                total_sign_day: 1,
            }),
            Ok(DailyState {
                is_sign: true,
                total_sign_day: 2,
            }),
        ],
        [Ok(SignState {
            requires_verification: None,
        })],
    );
    let rewards = service(fixture.db.clone(), key, gateway.clone());

    let first = rewards.enqueue(&actor, setting.id).await.unwrap();
    let second = rewards.enqueue(&actor, setting.id).await.unwrap();
    assert_eq!(first.id, second.id);
    assert_eq!(gateway.sign_calls(), 0);

    rewards.tick().await.unwrap();
    let run = reward_run::Entity::find_by_id(first.id)
        .one(&fixture.db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(run.state, "success");
    assert_eq!(run.send_state, "confirmed");
    assert_eq!(gateway.daily_calls(), 2);
    assert_eq!(gateway.sign_calls(), 1);
}

#[tokio::test]
async fn already_signed_never_posts() {
    let fixture = TestDatabase::new().await;
    let actor = ready_identity(&fixture.db, "owner").await;
    let key = key();
    let link = verified_link(&fixture.db, &key, actor.user.id, "123").await;
    let setting = setting(&fixture.db, link.id, "starrail", true).await;
    let gateway = MockGateway::new(
        [Ok(DailyState {
            is_sign: true,
            total_sign_day: 7,
        })],
        Vec::<UpstreamResult<SignState>>::new(),
    );
    let rewards = service(fixture.db.clone(), key, gateway.clone());
    let run = rewards.enqueue(&actor, setting.id).await.unwrap();

    rewards.tick().await.unwrap();

    let run = reward_run::Entity::find_by_id(run.id)
        .one(&fixture.db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(run.state, "already_claimed");
    assert_eq!(gateway.sign_calls(), 0);
}

#[tokio::test]
async fn ambiguous_post_error_is_not_resent_on_later_ticks() {
    let fixture = TestDatabase::new().await;
    let actor = ready_identity(&fixture.db, "owner").await;
    let key = key();
    let link = verified_link(&fixture.db, &key, actor.user.id, "123").await;
    let setting = setting(&fixture.db, link.id, "genshin", true).await;
    let gateway = MockGateway::new(
        [
            Ok(DailyState {
                is_sign: false,
                total_sign_day: 1,
            }),
            Ok(DailyState {
                is_sign: false,
                total_sign_day: 1,
            }),
        ],
        [Err(UpstreamError::Network)],
    );
    let rewards = service(fixture.db.clone(), key, gateway.clone());
    let run = rewards.enqueue(&actor, setting.id).await.unwrap();

    rewards.tick().await.unwrap();
    rewards.tick().await.unwrap();

    let run = reward_run::Entity::find_by_id(run.id)
        .one(&fixture.db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(run.state, "unknown");
    assert_eq!(run.send_state, "may_have_sent");
    assert_eq!(gateway.sign_calls(), 1);
}

#[tokio::test]
async fn list_uses_fresh_cache_and_retains_old_payload_on_stale_failure() {
    let fixture = TestDatabase::new().await;
    let actor = ready_identity(&fixture.db, "owner").await;
    let key = key();
    let link = verified_link(&fixture.db, &key, actor.user.id, "123").await;
    let stale = setting(&fixture.db, link.id, "genshin", true).await;
    let fetched_at = Utc::now() - Duration::minutes(10);
    cache(&fixture.db, stale.id, fetched_at, true).await;
    let gateway = MockGateway::new(
        [
            Err(UpstreamError::Network),
            Ok(DailyState {
                is_sign: false,
                total_sign_day: 10,
            }),
        ],
        Vec::<UpstreamResult<SignState>>::new(),
    );
    let rewards = service(fixture.db.clone(), key, gateway.clone());

    let views = rewards.list(&actor).await.unwrap();

    assert_eq!(views.len(), 1);
    assert_eq!(views[0].is_sign, Some(true));
    assert_eq!(views[0].error_code.as_deref(), Some("network"));
    assert_eq!(
        views[0].last_checked_at.unwrap().timestamp_micros(),
        fetched_at.timestamp_micros()
    );
    assert!(views[0].error_at.is_some());
    assert_eq!(gateway.daily_calls(), 1);
    let stored = reward_status_cache::Entity::find_by_id(stale.id)
        .one(&fixture.db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored.fetched_at, views[0].last_checked_at);
    assert_eq!(stored.payload.as_ref().unwrap()["is_sign"], true);
    assert_eq!(stored.payload_version, 1);

    let views = rewards.list(&actor).await.unwrap();
    assert_eq!(views[0].is_sign, Some(false));
    assert!(views[0].last_checked_at.unwrap() > fetched_at);
    assert!(views[0].error_code.is_none());
    assert!(views[0].error_at.is_none());
    let stored = reward_status_cache::Entity::find_by_id(stale.id)
        .one(&fixture.db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored.fetched_at, views[0].last_checked_at);
    assert_eq!(stored.payload.as_ref().unwrap()["total_sign_day"], 10);
    assert!(stored.error_code.is_none());
    assert!(stored.error_at.is_none());
    rewards.list(&actor).await.unwrap();
    assert_eq!(gateway.daily_calls(), 2);
}

#[tokio::test]
async fn cache_upsert_returns_persisted_nullable_overwrites_in_a_transaction() {
    let fixture = TestDatabase::new().await;
    let actor = ready_identity(&fixture.db, "owner").await;
    let key = key();
    let link = verified_link(&fixture.db, &key, actor.user.id, "123").await;
    let setting = setting(&fixture.db, link.id, "genshin", false).await;
    let tx = fixture.db.begin().await.unwrap();
    let failed = upsert_cache(&tx, setting.id, CacheWrite::failure("network".into(), None))
        .await
        .unwrap();
    assert_eq!(failed.setting_id, setting.id);
    assert!(failed.payload.is_none());
    assert!(failed.fetched_at.is_none());
    assert_eq!(failed.payload_version, 1);
    assert_eq!(failed.error_code.as_deref(), Some("network"));
    assert!(failed.error_at.is_some());

    let success = upsert_cache(
        &tx,
        setting.id,
        CacheWrite::success(DailyState {
            is_sign: true,
            total_sign_day: 1,
        }),
    )
    .await
    .unwrap();
    assert!(success.payload.is_some());
    assert!(success.fetched_at.is_some());
    assert!(success.error_code.is_none());
    assert!(success.error_at.is_none());
    let mut unsupported = success.clone().into_active_model();
    unsupported.payload_version = Set(99);
    unsupported.update(&tx).await.unwrap();
    let cleared = upsert_cache(
        &tx,
        setting.id,
        CacheWrite {
            fetched_at: None,
            payload: None,
            error_code: None,
            error_at: None,
        },
    )
    .await
    .unwrap();
    assert!(cleared.payload.is_none());
    assert!(cleared.fetched_at.is_none());
    assert!(cleared.error_code.is_none());
    assert!(cleared.error_at.is_none());
    assert_eq!(cleared.payload_version, 1);
    assert!(cleared.updated_at >= success.updated_at);
    let persisted = reward_status_cache::Entity::find_by_id(setting.id)
        .one(&tx)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(cleared, persisted);
    tx.commit().await.unwrap();
}

#[tokio::test]
async fn enabling_schedules_strict_future_without_immediate_run() {
    let fixture = TestDatabase::new().await;
    let actor = ready_identity(&fixture.db, "owner").await;
    let key = key();
    let link = verified_link(&fixture.db, &key, actor.user.id, "123").await;
    let setting = setting(&fixture.db, link.id, "genshin", false).await;
    let gateway = MockGateway::new(
        Vec::<UpstreamResult<DailyState>>::new(),
        Vec::<UpstreamResult<SignState>>::new(),
    );
    let rewards = service(fixture.db.clone(), key, gateway.clone());

    rewards
        .update(&actor, setting.id, true, "06:00".to_owned())
        .await
        .unwrap();
    rewards.tick().await.unwrap();

    let setting = reward_setting::Entity::find_by_id(setting.id)
        .one(&fixture.db)
        .await
        .unwrap()
        .unwrap();
    assert!(setting.enabled);
    assert!(setting.next_run_at.unwrap() > Utc::now());
    assert_eq!(
        reward_run::Entity::find()
            .all(&fixture.db)
            .await
            .unwrap()
            .len(),
        0
    );
}

#[tokio::test]
async fn future_cache_is_not_fresh_and_action_required_does_not_refresh() {
    let fixture = TestDatabase::new().await;
    let actor = ready_identity(&fixture.db, "owner").await;
    let key = key();
    let link = verified_link(&fixture.db, &key, actor.user.id, "123").await;
    let future = setting(&fixture.db, link.id, "genshin", true).await;
    cache(
        &fixture.db,
        future.id,
        Utc::now() + Duration::minutes(1),
        true,
    )
    .await;
    let halted = setting(&fixture.db, link.id, "starrail", true).await;
    cache(
        &fixture.db,
        halted.id,
        Utc::now() - Duration::minutes(10),
        true,
    )
    .await;
    let halted_id = halted.id;
    let mut halted_row = halted.into_active_model();
    halted_row.auth_status = Set("action_required".to_owned());
    halted_row.update(&fixture.db).await.unwrap();
    let gateway = MockGateway::new(
        [Ok(DailyState {
            is_sign: false,
            total_sign_day: 1,
        })],
        Vec::<UpstreamResult<SignState>>::new(),
    );
    let rewards = service(fixture.db.clone(), key, gateway.clone());

    let views = rewards.list(&actor).await.unwrap();

    assert_eq!(gateway.daily_calls(), 1);
    let future_view = views
        .iter()
        .find(|view| view.setting.id == future.id)
        .unwrap();
    let halted_view = views
        .iter()
        .find(|view| view.setting.id == halted_id)
        .unwrap();
    assert_eq!(future_view.is_sign, Some(false));
    assert_eq!(halted_view.is_sign, Some(true));
}

#[tokio::test]
async fn requires_verification_stops_only_that_game_before_success() {
    let fixture = TestDatabase::new().await;
    let actor = ready_identity(&fixture.db, "owner").await;
    let key = key();
    let link = verified_link(&fixture.db, &key, actor.user.id, "123").await;
    let setting = setting(&fixture.db, link.id, "genshin", true).await;
    let gateway = MockGateway::new(
        [Ok(DailyState {
            is_sign: false,
            total_sign_day: 1,
        })],
        [Ok(SignState {
            requires_verification: Some(true),
        })],
    );
    let rewards = service(fixture.db.clone(), key, gateway.clone());
    let run = rewards.enqueue(&actor, setting.id).await.unwrap();

    rewards.tick().await.unwrap();

    let run = reward_run::Entity::find_by_id(run.id)
        .one(&fixture.db)
        .await
        .unwrap()
        .unwrap();
    let setting = reward_setting::Entity::find_by_id(setting.id)
        .one(&fixture.db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(run.state, "action_required");
    assert_eq!(setting.auth_status, "action_required");
    assert!(!setting.enabled);
    assert_eq!(gateway.sign_calls(), 1);
}

#[tokio::test]
async fn initial_get_retries_at_most_four_attempts_and_never_posts() {
    let fixture = TestDatabase::new().await;
    let actor = ready_identity(&fixture.db, "owner").await;
    let key = key();
    let link = verified_link(&fixture.db, &key, actor.user.id, "123").await;
    let setting = setting(&fixture.db, link.id, "genshin", true).await;
    let gateway = MockGateway::new(
        [
            Err(UpstreamError::Network),
            Err(UpstreamError::Network),
            Err(UpstreamError::Network),
            Err(UpstreamError::Network),
        ],
        Vec::<UpstreamResult<SignState>>::new(),
    );
    let rewards = service(fixture.db.clone(), key, gateway.clone());
    let run = rewards.enqueue(&actor, setting.id).await.unwrap();

    for _ in 0..4 {
        rewards.tick().await.unwrap();
        if let Some(run) = reward_run::Entity::find_by_id(run.id)
            .one(&fixture.db)
            .await
            .unwrap()
            && run.state == "retry_wait"
        {
            let mut row = run.into_active_model();
            row.next_retry_at = Set(Some(Utc::now() - Duration::seconds(1)));
            row.update(&fixture.db).await.unwrap();
        }
    }

    let run = reward_run::Entity::find_by_id(run.id)
        .one(&fixture.db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(run.state, "failed");
    assert_eq!(run.attempt_count, 4);
    assert_eq!(gateway.daily_calls(), 4);
    assert_eq!(gateway.sign_calls(), 0);
}

#[tokio::test]
async fn stale_credential_version_stops_before_post() {
    let fixture = TestDatabase::new().await;
    let actor = ready_identity(&fixture.db, "owner").await;
    let key = key();
    let link = verified_link(&fixture.db, &key, actor.user.id, "123").await;
    let setting = setting(&fixture.db, link.id, "genshin", true).await;
    let gateway = MockGateway::new(
        Vec::<UpstreamResult<DailyState>>::new(),
        Vec::<UpstreamResult<SignState>>::new(),
    );
    let rewards = service(fixture.db.clone(), key, gateway.clone());
    let run = rewards.enqueue(&actor, setting.id).await.unwrap();
    let mut link_row = link.into_active_model();
    link_row.credential_version = Set(2);
    link_row.update(&fixture.db).await.unwrap();

    rewards.tick().await.unwrap();

    let run = reward_run::Entity::find_by_id(run.id)
        .one(&fixture.db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(run.state, "stopped");
    assert_eq!(gateway.daily_calls(), 0);
    assert_eq!(gateway.sign_calls(), 0);
}

#[tokio::test]
async fn confirmed_post_clears_durable_guard() {
    let fixture = TestDatabase::new().await;
    let actor = ready_identity(&fixture.db, "owner").await;
    let key = key();
    let link = verified_link(&fixture.db, &key, actor.user.id, "123").await;
    let setting = setting(&fixture.db, link.id, "genshin", true).await;
    let gateway = MockGateway::new(
        [
            Ok(DailyState {
                is_sign: false,
                total_sign_day: 1,
            }),
            Ok(DailyState {
                is_sign: true,
                total_sign_day: 2,
            }),
        ],
        [Ok(SignState {
            requires_verification: Some(false),
        })],
    );
    let rewards = service(fixture.db.clone(), key, gateway.clone());
    let run = rewards.enqueue(&actor, setting.id).await.unwrap();

    rewards.tick().await.unwrap();

    let run = reward_run::Entity::find_by_id(run.id)
        .one(&fixture.db)
        .await
        .unwrap()
        .unwrap();
    let guard = reward_guard::Entity::find_by_id(setting.id)
        .one(&fixture.db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(run.state, "success");
    assert_eq!(run.send_state, "confirmed");
    assert!(!guard.post_may_have_happened);
    assert_eq!(gateway.sign_calls(), 1);
}

#[tokio::test]
async fn restart_guard_never_posts_and_survives_history_cleanup() {
    let fixture = TestDatabase::new().await;
    let actor = ready_identity(&fixture.db, "owner").await;
    let key = key();
    let link = verified_link(&fixture.db, &key, actor.user.id, "123").await;
    let setting = setting(&fixture.db, link.id, "genshin", true).await;
    reward_guard::ActiveModel {
        setting_id: Set(setting.id),
        post_may_have_happened: Set(true),
        updated_at: Set(Utc::now()),
    }
    .insert(&fixture.db)
    .await
    .unwrap();
    let active = inserted_run(&fixture.db, setting.id, "pending", "not_sent", Utc::now()).await;
    let old = inserted_run(
        &fixture.db,
        setting.id,
        "unknown",
        "may_have_sent",
        Utc::now() - Duration::days(6),
    )
    .await;
    let gateway = MockGateway::new(
        [Ok(DailyState {
            is_sign: false,
            total_sign_day: 1,
        })],
        Vec::<UpstreamResult<SignState>>::new(),
    );
    let rewards = service(fixture.db.clone(), key, gateway.clone());

    rewards.tick().await.unwrap();

    let active = reward_run::Entity::find_by_id(active.id)
        .one(&fixture.db)
        .await
        .unwrap()
        .unwrap();
    assert!(
        reward_run::Entity::find_by_id(old.id)
            .one(&fixture.db)
            .await
            .unwrap()
            .is_none()
    );
    let guard = reward_guard::Entity::find_by_id(setting.id)
        .one(&fixture.db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(active.state, "unknown");
    assert!(guard.post_may_have_happened);
    assert_eq!(gateway.daily_calls(), 1);
    assert_eq!(gateway.sign_calls(), 0);
}

#[tokio::test]
async fn disabled_user_stops_before_post() {
    let fixture = TestDatabase::new().await;
    let actor = ready_identity(&fixture.db, "owner").await;
    let key = key();
    let link = verified_link(&fixture.db, &key, actor.user.id, "123").await;
    let setting = setting(&fixture.db, link.id, "genshin", true).await;
    let gateway = MockGateway::new(
        Vec::<UpstreamResult<DailyState>>::new(),
        Vec::<UpstreamResult<SignState>>::new(),
    );
    let rewards = service(fixture.db.clone(), key, gateway.clone());
    let run = rewards.enqueue(&actor, setting.id).await.unwrap();
    let mut user = actor.user.clone().into_active_model();
    user.active = Set(false);
    user.update(&fixture.db).await.unwrap();

    rewards.tick().await.unwrap();

    let run = reward_run::Entity::find_by_id(run.id)
        .one(&fixture.db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(run.state, "stopped");
    assert_eq!(gateway.daily_calls(), 0);
    assert_eq!(gateway.sign_calls(), 0);
}

#[tokio::test]
async fn owner_scoping_prevents_admin_peeking_or_cross_setting_enqueue() {
    let fixture = TestDatabase::new().await;
    let owner = ready_identity(&fixture.db, "owner").await;
    let admin = identity_with(&fixture.db, "admin", true, false).await;
    let key = key();
    let owner_link = verified_link(&fixture.db, &key, owner.user.id, "123").await;
    let admin_link = verified_link(&fixture.db, &key, admin.user.id, "456").await;
    let owner_setting = setting(&fixture.db, owner_link.id, "genshin", true).await;
    let admin_setting = setting(&fixture.db, admin_link.id, "starrail", true).await;
    inserted_run(
        &fixture.db,
        owner_setting.id,
        "success",
        "confirmed",
        Utc::now(),
    )
    .await;
    inserted_run(
        &fixture.db,
        admin_setting.id,
        "failed",
        "not_sent",
        Utc::now(),
    )
    .await;
    let gateway = MockGateway::new(
        Vec::<UpstreamResult<DailyState>>::new(),
        Vec::<UpstreamResult<SignState>>::new(),
    );
    let rewards = service(fixture.db.clone(), key, gateway);

    assert!(matches!(
        rewards.enqueue(&admin, owner_setting.id).await,
        Err(Error::NotFound)
    ));
    let history = rewards.history(&admin, 100).await.unwrap();
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].run.setting_id, admin_setting.id);
}

#[tokio::test]
async fn initial_password_and_production_proof_gates_reject_work() {
    let fixture = TestDatabase::new().await;
    let must_change = identity_with(&fixture.db, "must-change", false, true).await;
    let ready = ready_identity(&fixture.db, "ready").await;
    let key = key();
    let link = verified_link(&fixture.db, &key, must_change.user.id, "123").await;
    let setting = setting(&fixture.db, link.id, "genshin", true).await;
    let gateway = MockGateway::new(
        Vec::<UpstreamResult<DailyState>>::new(),
        Vec::<UpstreamResult<SignState>>::new(),
    );
    let rewards = service(fixture.db.clone(), key.clone(), gateway.clone());

    assert!(matches!(
        rewards.enqueue(&must_change, setting.id).await,
        Err(Error::PasswordChangeRequired)
    ));
    assert_eq!(gateway.daily_calls(), 0);
    assert_eq!(gateway.sign_calls(), 0);

    let production = Rewards::new(Arc::new(CookieStore::new(fixture.db.clone(), key)));
    assert!(matches!(
        production.enqueue(&ready, setting.id).await,
        Err(Error::Unavailable(_))
    ));
    production.tick().await.unwrap();
}
