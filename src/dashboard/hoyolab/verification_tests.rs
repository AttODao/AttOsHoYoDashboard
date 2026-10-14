//! Synthetic principal contracts exercise policy, never claim live token binding proof.
use super::{
    credentials::{CookieKey, Credentials},
    gateway::{DailyState, Gateway, LiveGateway, SignState},
    store::CookieStore,
};
use crate::dashboard::{
    auth::{
        password::token_hash,
        sessions::{Identity, Sessions},
    },
    db::entities::{game_account, hoyolab_account, reward_guard, reward_setting},
    error::Error,
    test_support::TestDatabase,
};
use attosgameapi::{
    game::GameKind,
    hoyolab::{RecordAccount, Result as UpstreamResult},
    models::account::Account,
};
use chrono::Utc;
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, IntoActiveModel, QueryFilter, Set};
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};
use tokio::sync::Notify;

#[derive(Clone)]
struct Mock {
    state: Arc<Mutex<State>>,
    pause: Option<Arc<(Notify, Notify)>>,
}
struct State {
    account: String,
    community: String,
    roles: Value,
    headers: Vec<String>,
}
impl Mock {
    fn new(uid: &str) -> Self {
        Self {
            state: Arc::new(Mutex::new(State {
                account: uid.into(),
                community: uid.into(),
                roles: json!([{ "game_biz":"hk4e_global","region":"os_asia","game_uid":"1001"}]),
                headers: vec![],
            })),
            pause: None,
        }
    }
}
impl Gateway for Mock {
    async fn identity(&self, cookie: &Credentials) -> UpstreamResult<(String, String)> {
        let result = {
            let mut state = self.state.lock().unwrap();
            state.headers.push(cookie.as_str().into());
            (state.account.clone(), state.community.clone())
        };
        if let Some(pause) = &self.pause {
            pause.0.notify_one();
            pause.1.notified().await;
        }
        Ok(result)
    }
    async fn roles(&self, cookie: &Credentials) -> UpstreamResult<Vec<Account>> {
        let mut state = self.state.lock().unwrap();
        state.headers.push(cookie.as_str().into());
        Ok(serde_json::from_value(state.roles.clone()).unwrap())
    }
    async fn note(&self, _: &Credentials, _: GameKind, _: &RecordAccount) -> UpstreamResult<Value> {
        panic!("unexpected note request")
    }
    async fn daily(&self, _: &Credentials, _: GameKind) -> UpstreamResult<DailyState> {
        panic!("unexpected daily request")
    }
    async fn sign(&self, _: &Credentials, _: GameKind) -> UpstreamResult<SignState> {
        panic!("unexpected sign request")
    }
}
fn key() -> Arc<CookieKey> {
    Arc::new(CookieKey::from_hex(&"11".repeat(32)).unwrap())
}
fn cookie(uid: &str) -> String {
    format!(
        "ltuid_v2={uid}; ltoken_v2=v2_synthetic_token; ltmid_v2=synthetic_mid; unrelated=discarded"
    )
}
async fn row<G: Gateway>(store: &CookieStore<G>, owner: uuid::Uuid) -> hoyolab_account::Model {
    hoyolab_account::Entity::find()
        .filter(hoyolab_account::Column::OwnerUserId.eq(owner))
        .one(&store.db)
        .await
        .unwrap()
        .unwrap()
}

#[tokio::test]
async fn live_store_does_not_treat_pending_cookie_submission_as_verified_link() {
    let fixture = TestDatabase::new().await;
    let (_, actor) = fixture.ready_admin().await;
    let store = CookieStore::<LiveGateway>::new(fixture.db.clone(), key());
    store.submit(&actor, &cookie("42")).await.unwrap();
    // Readiness must still reject a Cookie claim before any upstream call.
    assert!(matches!(
        store.ready_link(&actor).await,
        Err(Error::InitialSetupRequired)
    ));
    let row = row(&store, actor.user.id).await;
    assert_eq!(row.hoyolab_uid, None);
    assert_eq!(row.auth_status, "pending_verification");
}
#[tokio::test]
async fn mock_consensus_seals_verified_context_and_creates_hidden_uids_off_settings() {
    let fixture = TestDatabase::new().await;
    let (_, actor) = fixture.ready_admin().await;
    let gateway = Mock::new("42");
    let store = CookieStore::for_test(fixture.db.clone(), key(), gateway.clone());
    store.submit(&actor, &cookie("42")).await.unwrap();
    store.verify(&actor).await.unwrap();
    let link = row(&store, actor.user.id).await;
    assert_eq!(link.hoyolab_uid.as_deref(), Some("42"));
    assert_eq!(link.credential_version, 2);
    assert!(
        store
            .key
            .open(actor.user.id, Some("42"), &link.encrypted_credentials)
            .is_ok()
    );
    assert!(
        store
            .key
            .open(actor.user.id, None, &link.encrypted_credentials)
            .is_err()
    );
    let roles = game_account::Entity::find()
        .filter(game_account::Column::HoyolabAccountId.eq(link.id))
        .all(&store.db)
        .await
        .unwrap();
    assert_eq!(roles.len(), 1);
    assert!(!roles[0].visible);
    assert!(roles[0].detected);
    let settings = reward_setting::Entity::find()
        .filter(reward_setting::Column::HoyolabAccountId.eq(link.id))
        .all(&store.db)
        .await
        .unwrap();
    assert_eq!(settings.len(), 2);
    for setting in settings {
        assert!(!setting.enabled);
        assert_eq!(setting.local_time, "06:00");
        assert_eq!(setting.next_run_at, None);
        assert!(
            !reward_guard::Entity::find_by_id(setting.id)
                .one(&store.db)
                .await
                .unwrap()
                .unwrap()
                .post_may_have_happened
        );
    }
    let headers = &gateway.state.lock().unwrap().headers;
    assert_eq!(headers.len(), 2);
    assert!(headers.iter().all(|header| !header.contains("unrelated")));
}
#[tokio::test]
async fn candidate_or_claim_mismatch_never_promotes_and_uid_collision_does_not_reveal_owner() {
    let fixture = TestDatabase::new().await;
    let (accounts, actor) = fixture.ready_admin().await;
    let gateway = Mock::new("42");
    let store = CookieStore::for_test(fixture.db.clone(), key(), gateway.clone());
    store.submit(&actor, &cookie("42")).await.unwrap();
    gateway.state.lock().unwrap().community = "43".into();
    assert!(matches!(store.verify(&actor).await, Err(Error::Forbidden)));
    assert_eq!(row(&store, actor.user.id).await.hoyolab_uid, None);
    gateway.state.lock().unwrap().community = "42".into();
    store.verify(&actor).await.unwrap();
    let other = accounts
        .create_user(
            &actor,
            "collision-user".into(),
            "temporary-password-123".into(),
        )
        .await
        .unwrap();
    let (_, session) = accounts
        .login(other.username.clone(), "temporary-password-123".into())
        .await
        .unwrap();
    let login = Sessions::authenticate(&accounts.db, &token_hash(&session.token).unwrap())
        .await
        .unwrap();
    let (_, session) = accounts
        .change_password(
            &login,
            "temporary-password-123".into(),
            "changed-password-123".into(),
        )
        .await
        .unwrap();
    let other = Sessions::authenticate(&accounts.db, &token_hash(&session.token).unwrap())
        .await
        .unwrap();
    store.submit(&other, &cookie("42")).await.unwrap();
    assert!(matches!(store.verify(&other).await, Err(Error::Forbidden)));
    assert_eq!(row(&store, other.user.id).await.hoyolab_uid, None);
}
#[tokio::test]
async fn same_uid_update_preserves_visibility_and_schedules_only_the_next_tick() {
    let fixture = TestDatabase::new().await;
    let (_, actor) = fixture.ready_admin().await;
    let gateway = Mock::new("42");
    let store = CookieStore::for_test(fixture.db.clone(), key(), gateway.clone());
    store.submit(&actor, &cookie("42")).await.unwrap();
    store.verify(&actor).await.unwrap();
    let link = row(&store, actor.user.id).await;
    let game = game_account::Entity::find()
        .one(&store.db)
        .await
        .unwrap()
        .unwrap();
    let mut game = game.into_active_model();
    game.visible = Set(true);
    let game = game.update(&store.db).await.unwrap();
    let setting = reward_setting::Entity::find()
        .one(&store.db)
        .await
        .unwrap()
        .unwrap();
    let mut setting = setting.into_active_model();
    setting.enabled = Set(true);
    setting.next_run_at = Set(Some(Utc::now() - chrono::Duration::days(1)));
    let setting = setting.update(&store.db).await.unwrap();
    gateway.state.lock().unwrap().roles = json!([
        { "game_biz":"hk4e_global","region":"os_asia","game_uid":"1001" },
        { "game_biz":"hk4e_global","region":"os_asia","game_uid":"001001" },
        { "game_biz":"hk4e_global","region":"os_usa","game_uid":"1001" },
        { "game_biz":"unsupported","region":"invalid region","game_uid":"not-a-uid" }
    ]);
    store.submit(&actor, &cookie("42")).await.unwrap();
    let updated = row(&store, actor.user.id).await;
    assert_eq!(updated.id, link.id);
    assert_eq!(updated.hoyolab_uid, link.hoyolab_uid);
    assert_eq!(updated.credential_version, link.credential_version + 1);
    let roles = game_account::Entity::find()
        .filter(game_account::Column::HoyolabAccountId.eq(link.id))
        .all(&store.db)
        .await
        .unwrap();
    assert_eq!(roles.len(), 2);
    let existing = roles.iter().find(|row| row.region == "os_asia").unwrap();
    assert_eq!(existing.id, game.id);
    assert_eq!(existing.game_uid, "1001");
    assert!(existing.visible);
    assert!(existing.detected);
    assert!(existing.last_detected_at >= game.last_detected_at);
    let new_region = roles.iter().find(|row| row.region == "os_usa").unwrap();
    assert_ne!(new_region.id, game.id);
    assert_eq!(new_region.game_uid, "1001");
    assert!(!new_region.visible);
    assert!(new_region.detected);
    assert!(
        reward_setting::Entity::find_by_id(setting.id)
            .one(&store.db)
            .await
            .unwrap()
            .unwrap()
            .next_run_at
            .unwrap()
            > Utc::now()
    );
    gateway.state.lock().unwrap().roles = json!([]);
    store.sync(&actor).await.unwrap();
    let missing = game_account::Entity::find_by_id(game.id)
        .one(&store.db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(missing.id, game.id);
    assert!(missing.visible);
    assert!(!missing.detected);
    gateway.state.lock().unwrap().roles =
        json!([{ "game_biz":"hk4e_global","region":"os_asia","game_uid":"1001"}]);
    store.sync(&actor).await.unwrap();
    let found = game_account::Entity::find_by_id(game.id)
        .one(&store.db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(found.id, game.id);
    assert!(found.visible);
    assert!(found.detected);
    gateway.state.lock().unwrap().account = "43".into();
    gateway.state.lock().unwrap().community = "43".into();
    assert!(matches!(
        store.submit(&actor, &cookie("43")).await,
        Err(Error::Forbidden)
    ));
    assert_eq!(
        row(&store, actor.user.id).await.credential_version,
        updated.credential_version
    );
}
#[tokio::test]
async fn disabling_during_identity_get_prevents_link_promotion() {
    let fixture = TestDatabase::new().await;
    let (accounts, admin) = fixture.ready_admin().await;
    let user = accounts
        .create_user(
            &admin,
            "paused-user".into(),
            "temporary-password-123".into(),
        )
        .await
        .unwrap();
    let (_, session) = accounts
        .login(user.username, "temporary-password-123".into())
        .await
        .unwrap();
    let first = Sessions::authenticate(&accounts.db, &token_hash(&session.token).unwrap())
        .await
        .unwrap();
    let (_, session) = accounts
        .change_password(
            &first,
            "temporary-password-123".into(),
            "changed-password-123".into(),
        )
        .await
        .unwrap();
    let actor = Sessions::authenticate(&accounts.db, &token_hash(&session.token).unwrap())
        .await
        .unwrap();
    let pause = Arc::new((Notify::new(), Notify::new()));
    let mut gateway = Mock::new("42");
    gateway.pause = Some(pause.clone());
    let store = Arc::new(CookieStore::for_test(fixture.db.clone(), key(), gateway));
    store.submit(&actor, &cookie("42")).await.unwrap();
    let task = {
        let store = store.clone();
        let actor = Identity {
            token_hash: actor.token_hash.clone(),
            user: actor.user.clone(),
            expires_at: actor.expires_at,
        };
        tokio::spawn(async move { store.verify(&actor).await })
    };
    pause.0.notified().await;
    accounts
        .set_active(&admin, actor.user.id, false)
        .await
        .unwrap();
    pause.1.notify_one();
    assert!(matches!(task.await.unwrap(), Err(Error::Unauthorized)));
    assert_eq!(row(&store, actor.user.id).await.hoyolab_uid, None);
}
