use super::Rewards;
use crate::dashboard::{
    auth::{
        password::token_hash,
        sessions::{Identity, Sessions},
    },
    db::entities::{hoyolab_account, reward_run, reward_setting, user},
    hoyolab::{
        credentials::{CookieKey, Credentials},
        gateway::{DailyAward, DailyState, Gateway, SignState},
        store::CookieStore,
    },
};
use attoshoyodashboard::{
    game::GameKind,
    hoyolab::{RecordAccount, Result as UpstreamResult},
    models::account::Account,
};
use chrono::{Duration, Utc};
use sea_orm::{ActiveModelTrait, Set};
use serde_json::Value;
use std::{
    collections::VecDeque,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
};
use uuid::Uuid;

/// A month's synthetic award list as (name, count).
type Awards = Option<Vec<(String, i64)>>;

#[derive(Clone)]
pub(super) struct MockGateway {
    daily: Arc<Mutex<VecDeque<UpstreamResult<DailyState>>>>,
    sign: Arc<Mutex<VecDeque<UpstreamResult<SignState>>>>,
    awards: Arc<Mutex<Awards>>,
    daily_calls: Arc<AtomicUsize>,
    sign_calls: Arc<AtomicUsize>,
    award_calls: Arc<AtomicUsize>,
}

impl MockGateway {
    pub(super) fn new(
        daily: impl IntoIterator<Item = UpstreamResult<DailyState>>,
        sign: impl IntoIterator<Item = UpstreamResult<SignState>>,
    ) -> Self {
        Self {
            daily: Arc::new(Mutex::new(daily.into_iter().collect())),
            sign: Arc::new(Mutex::new(sign.into_iter().collect())),
            awards: Arc::new(Mutex::new(None)),
            daily_calls: Arc::new(AtomicUsize::new(0)),
            sign_calls: Arc::new(AtomicUsize::new(0)),
            award_calls: Arc::new(AtomicUsize::new(0)),
        }
    }
    /// Serves a month's award list; without one the award GET fails like an upstream error.
    pub(super) fn with_awards(self, awards: &[(&str, i64)]) -> Self {
        *self.awards.lock().unwrap() = Some(
            awards
                .iter()
                .map(|(name, count)| ((*name).to_owned(), *count))
                .collect(),
        );
        self
    }
    pub(super) fn award_calls(&self) -> usize {
        self.award_calls.load(Ordering::SeqCst)
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
    async fn awards(&self, _: &Credentials, _: GameKind) -> UpstreamResult<Vec<DailyAward>> {
        self.award_calls.fetch_add(1, Ordering::SeqCst);
        let awards = self.awards.lock().unwrap().clone();
        let awards = awards.ok_or(attoshoyodashboard::hoyolab::Error::MissingData)?;
        Ok(awards
            .into_iter()
            .map(|(name, count)| DailyAward {
                name,
                count,
                icon: "https://act-webstatic.hoyoverse.com/synthetic.png".to_owned(),
            })
            .collect())
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

pub(super) async fn identity_with(
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
        encrypted_credentials: Set(Some(key.seal(owner, Some(uid), &cookie).unwrap())),
        credential_version: Set(1),
        auth_status: Set("verified".to_owned()),
        created_at: Set(now),
        updated_at: Set(now),
        verified_at: Set(Some(now)),
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

pub(super) fn service(
    db: sea_orm::DatabaseConnection,
    key: Arc<CookieKey>,
    gateway: MockGateway,
) -> Rewards<MockGateway> {
    Rewards::new(Arc::new(CookieStore::for_test(db, key, gateway)))
}
