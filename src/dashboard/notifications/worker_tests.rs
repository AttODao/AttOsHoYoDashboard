//! Notifier ticks against an isolated database: the app inbox gets exactly the notices
//! the web channels (Webhook and browser push) are sent, under the same conditions.
use super::{
    channel::Notifications,
    worker::{Notifier, NotifierSchedule},
};
use crate::dashboard::{
    auth::{
        password::token_hash,
        sessions::{Identity, Sessions},
    },
    config::Tuning,
    db::entities::{
        app_notification, audit, game_account, game_status_cache, hoyolab_account,
        notification_cursor, notification_log, notification_setting, reward_run, reward_setting,
        user,
    },
    error::Error,
    games::service::Games,
    hoyolab::{
        credentials::{CookieKey, Credentials},
        gateway::{DailyAward, DailyState, Gateway, SignState},
        store::CookieStore,
    },
    rewards::schedule::ScheduleZone,
    test_support::TestDatabase,
};
use attoshoyodashboard::{
    game::GameKind,
    hoyolab::{RecordAccount, Result as UpstreamResult},
    models::account::Account,
};
use chrono::{DateTime, Duration, FixedOffset, Utc};
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, QueryOrder, Set};
use serde_json::{Value, json};
use std::{collections::BTreeSet, sync::Arc};
use uuid::Uuid;

/// Every status in these tests is cached and fresh: any upstream call is a bug.
#[derive(Clone)]
struct NoUpstream;

impl Gateway for NoUpstream {
    async fn identity(&self, _: &Credentials) -> UpstreamResult<(String, String)> {
        panic!("identity must not be called by notifier tests")
    }
    async fn roles(&self, _: &Credentials) -> UpstreamResult<Vec<Account>> {
        panic!("roles must not be called by notifier tests")
    }
    async fn note(&self, _: &Credentials, _: GameKind, _: &RecordAccount) -> UpstreamResult<Value> {
        panic!("note must not be called with a fresh status cache")
    }
    async fn daily(&self, _: &Credentials, _: GameKind) -> UpstreamResult<DailyState> {
        panic!("daily must not be called by notifier tests")
    }
    async fn sign(&self, _: &Credentials, _: GameKind) -> UpstreamResult<SignState> {
        panic!("sign must not be called by notifier tests")
    }
    async fn awards(&self, _: &Credentials, _: GameKind) -> UpstreamResult<Vec<DailyAward>> {
        panic!("awards must not be called by notifier tests")
    }
}

fn key() -> Arc<CookieKey> {
    Arc::new(CookieKey::from_hex(&"11".repeat(32)).unwrap())
}

fn zone() -> ScheduleZone {
    ScheduleZone::Fixed(FixedOffset::east_opt(9 * 3600).unwrap())
}

/// Sunday 2026-11-01 21:30 JST: after the daily and weekly reminder times.
fn sunday_evening() -> DateTime<Utc> {
    DateTime::parse_from_rfc3339("2026-11-01T12:30:00Z")
        .unwrap()
        .with_timezone(&Utc)
}

struct Fixture {
    fixture: TestDatabase,
    notifications: Arc<Notifications>,
    notifier: Notifier<NoUpstream>,
}

async fn fixture() -> Fixture {
    let fixture = TestDatabase::new().await;
    let key = key();
    let notifications = Arc::new(Notifications::new(
        fixture.db.clone(),
        Some(key.clone()),
        "https://ahd.example.test".to_owned(),
    ));
    let games = Arc::new(Games::new(Arc::new(CookieStore::for_test(
        fixture.db.clone(),
        key,
        NoUpstream,
    ))));
    let notifier = Notifier::new(
        notifications.clone(),
        games,
        NotifierSchedule::default(),
        zone(),
        Tuning::default(),
    );
    Fixture {
        fixture,
        notifications,
        notifier,
    }
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
        last_login_at: Set(None),
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

async fn settings(db: &sea_orm::DatabaseConnection, user: Uuid, events: Value) {
    notification_setting::ActiveModel {
        user_id: Set(user),
        webhook_url: Set(None),
        webhook_format: Set("discord".to_owned()),
        language: Set("ja".to_owned()),
        events: Set(events),
        last_delivery_at: Set(None),
        last_delivery_ok: Set(None),
        updated_at: Set(Utc::now()),
        resin_warn_hours: Set(2),
        daily_reminder_time: Set("21:00".to_owned()),
        weekly_reminder_time: Set("15:00".to_owned()),
    }
    .insert(db)
    .await
    .unwrap();
}

fn every_event() -> Value {
    Value::Object(
        super::PERSONAL_EVENTS
            .iter()
            .map(|(kind, _)| ((*kind).to_owned(), Value::Bool(true)))
            .collect(),
    )
}

async fn link(
    db: &sea_orm::DatabaseConnection,
    owner: Uuid,
    status: &str,
) -> hoyolab_account::Model {
    // HoYoLAB ids are unique across users.
    let uid = (owner.as_u128() % 1_000_000_000).to_string();
    let cookie =
        Credentials::parse(&format!("ltoken_v2=synthetictoken{uid}; ltuid_v2={uid}")).unwrap();
    hoyolab_account::ActiveModel {
        id: Set(Uuid::new_v4()),
        owner_user_id: Set(owner),
        hoyolab_uid: Set(Some(uid.clone())),
        encrypted_credentials: Set(Some(key().seal(owner, Some(&uid), &cookie).unwrap())),
        credential_version: Set(1),
        auth_status: Set(status.to_owned()),
        created_at: Set(Utc::now()),
        updated_at: Set(Utc::now()),
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
    fetched_at: DateTime<Utc>,
    payload: Value,
) -> Uuid {
    let account = game_account::ActiveModel {
        id: Set(Uuid::new_v4()),
        hoyolab_account_id: Set(link),
        game_kind: Set(kind.to_owned()),
        game_uid: Set(uid.to_owned()),
        region: Set("os_asia".to_owned()),
        visible: Set(true),
        detected: Set(true),
        last_detected_at: Set(Utc::now()),
    }
    .insert(db)
    .await
    .unwrap();
    game_status_cache::ActiveModel {
        game_account_id: Set(account.id),
        fetched_at: Set(Some(fetched_at)),
        updated_at: Set(fetched_at),
        payload: Set(Some(payload)),
        payload_version: Set(1),
        error_code: Set(None),
        error_at: Set(None),
    }
    .insert(db)
    .await
    .unwrap();
    account.id
}

async fn reward_runs(db: &sea_orm::DatabaseConnection, link: Uuid, ended: DateTime<Utc>) {
    let setting = reward_setting::ActiveModel {
        id: Set(Uuid::new_v4()),
        hoyolab_account_id: Set(link),
        game_kind: Set("genshin".to_owned()),
        enabled: Set(true),
        local_time: Set("06:00".to_owned()),
        auth_status: Set("ready".to_owned()),
        next_run_at: Set(None),
    }
    .insert(db)
    .await
    .unwrap();
    for state in ["success", "failed"] {
        reward_run::ActiveModel {
            id: Set(Uuid::new_v4()),
            setting_id: Set(setting.id),
            trigger: Set("scheduled".to_owned()),
            state: Set(state.to_owned()),
            created_at: Set(ended - Duration::minutes(1)),
            updated_at: Set(ended),
            ended_at: Set(Some(ended)),
            attempt_count: Set(1),
            credential_version: Set(1),
            send_state: Set("confirmed".to_owned()),
            next_retry_at: Set(None),
        }
        .insert(db)
        .await
        .unwrap();
    }
}

async fn sign_ins(db: &sea_orm::DatabaseConnection, user: Uuid, at: DateTime<Utc>) {
    for (action, actor) in [("signed_in", Some(user)), ("sign_in_failed", None)] {
        audit::ActiveModel {
            id: Set(Uuid::new_v4()),
            actor_user_id: Set(actor),
            target_user_id: Set(user),
            action: Set(action.to_owned()),
            created_at: Set(at),
        }
        .insert(db)
        .await
        .unwrap();
    }
}

async fn scanned_until(db: &sea_orm::DatabaseConnection, at: DateTime<Utc>) {
    notification_cursor::ActiveModel {
        id: Set(1),
        scanned_until: Set(at),
    }
    .insert(db)
    .await
    .unwrap();
}

async fn inbox(db: &sea_orm::DatabaseConnection, user: Uuid) -> Vec<app_notification::Model> {
    app_notification::Entity::find()
        .filter(app_notification::Column::UserId.eq(user))
        .order_by_asc(app_notification::Column::Id)
        .all(db)
        .await
        .unwrap()
}

async fn delivered_keys(db: &sea_orm::DatabaseConnection, user: Uuid) -> BTreeSet<String> {
    notification_log::Entity::find()
        .filter(notification_log::Column::Scope.eq(user))
        .all(db)
        .await
        .unwrap()
        .into_iter()
        .map(|row| row.event_key)
        .collect()
}

fn genshin_ready(now: DateTime<Utc>) -> Value {
    json!({
        "current_resin": 200, "max_resin": 200,
        "expeditions": [{"status": "Finished"}, {"status": "Finished"}],
        "transformer_ready": true,
        "finished_task_num": 2, "total_task_num": 4,
        "remain_resin_discount_num": 1,
        "fetched_at": now.to_rfc3339(),
    })
}

fn starrail_near_cap(now: DateTime<Utc>) -> Value {
    json!({
        "current_stamina": 230, "max_stamina": 240,
        "stamina_full_at": (now + Duration::minutes(80)).to_rfc3339(),
        "current_train_score": 500, "max_train_score": 500,
        "weekly_cocoon_cnt": 0, "current_rogue_score": 0, "max_rogue_score": 0,
    })
}

#[tokio::test]
async fn the_app_inbox_receives_every_personal_notice_the_web_channels_are_sent() {
    let Fixture {
        fixture,
        notifications,
        notifier,
    } = fixture().await;
    let db = &fixture.db;
    let now = sunday_evening();
    let owner = ready_identity(db, "owner").await;
    settings(db, owner.user.id, every_event()).await;
    let device = notifications.register_app_device(&owner).await.unwrap();
    let device_json = serde_json::to_value(&device).unwrap();
    assert_eq!(device_json["cursor"], 0);
    let device_id: Uuid = device_json["id"].as_str().unwrap().parse().unwrap();

    let verified = link(db, owner.user.id, "verified").await;
    let genshin = game(
        db,
        verified.id,
        "genshin",
        "800000001",
        now,
        genshin_ready(now),
    )
    .await;
    let starrail = game(
        db,
        verified.id,
        "starrail",
        "700000001",
        now,
        starrail_near_cap(now),
    )
    .await;
    reward_runs(db, verified.id, now - Duration::minutes(10)).await;
    sign_ins(db, owner.user.id, now - Duration::minutes(10)).await;
    scanned_until(db, now - Duration::hours(1)).await;

    notifier.tick(now, None).await.unwrap();

    let entries = inbox(db, owner.user.id).await;
    let kinds: BTreeSet<&str> = entries.iter().map(|row| row.kind.as_str()).collect();
    assert_eq!(
        kinds,
        BTreeSet::from([
            "reward_claimed",
            "reward_failed",
            "signed_in",
            "sign_in_failed",
            "resource_full",
            "resource_near_cap",
            "expeditions_done",
            "transformer_ready",
            "daily_unfinished",
            "weekly_unfinished",
        ])
    );
    // Same dedup keys as the web channels' delivery log, one entry per delivered notice.
    let tags: BTreeSet<String> = entries.iter().map(|row| row.tag.clone()).collect();
    assert_eq!(tags.len(), entries.len());
    assert_eq!(tags, delivered_keys(db, owner.user.id).await);
    // The rendered text is the web text, and each opens the screen that handles it.
    let full = entries
        .iter()
        .find(|row| row.kind == "resource_full")
        .unwrap();
    assert_eq!(full.title, "天然樹脂が上限に達しました");
    assert_eq!(
        full.body,
        "原神 UID 800000001: 200/200。これ以上は回復しません。"
    );
    assert_eq!(full.tag, format!("{genshin}:resource_full"));
    assert_eq!(full.url, "/dashboard");
    let near = entries
        .iter()
        .find(|row| row.kind == "resource_near_cap")
        .unwrap();
    assert_eq!(near.tag, format!("{starrail}:resource_near_cap"));
    let failed = entries
        .iter()
        .find(|row| row.kind == "reward_failed")
        .unwrap();
    assert_eq!(failed.url, "/settings/bonus");
    assert_eq!(failed.body, "原神: 失敗。実行履歴を確認してください。");
    let signed = entries.iter().find(|row| row.kind == "signed_in").unwrap();
    assert_eq!(signed.url, "/settings/security");

    // Reading through the device API returns them in order and advances the cursor.
    let page =
        serde_json::to_value(notifications.app_inbox(&owner, device_id, 0).await.unwrap()).unwrap();
    assert_eq!(page["items"].as_array().unwrap().len(), entries.len());
    assert_eq!(page["cursor"], entries.last().unwrap().id);
    assert_eq!(page["more"], false);
    let rest = serde_json::to_value(
        notifications
            .app_inbox(&owner, device_id, entries.last().unwrap().id)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(rest["items"], json!([]));

    // A second tick reports nothing again (the web dedup applies to the app too).
    notifier
        .tick(now + Duration::minutes(1), None)
        .await
        .unwrap();
    assert_eq!(inbox(db, owner.user.id).await.len(), entries.len());
}

#[tokio::test]
async fn the_app_follows_the_event_choices_and_needs_a_registered_device() {
    let Fixture {
        fixture,
        notifications,
        notifier,
    } = fixture().await;
    let db = &fixture.db;
    let now = sunday_evening();
    // Defaults (no settings row): reward_claimed and signed_in are off on the web too.
    let owner = ready_identity(db, "owner").await;
    notifications.register_app_device(&owner).await.unwrap();
    // A user without any channel: no inbox rows, and no upstream status reads either.
    let other = ready_identity(db, "other").await;
    for user in [owner.user.id, other.user.id] {
        let verified = link(db, user, "verified").await;
        reward_runs(db, verified.id, now - Duration::minutes(10)).await;
        sign_ins(db, user, now - Duration::minutes(10)).await;
    }
    settings(db, other.user.id, every_event()).await;
    scanned_until(db, now - Duration::hours(1)).await;

    notifier.tick(now, None).await.unwrap();

    let kinds: BTreeSet<String> = inbox(db, owner.user.id)
        .await
        .into_iter()
        .map(|row| row.kind)
        .collect();
    assert_eq!(
        kinds,
        BTreeSet::from(["reward_failed".to_owned(), "sign_in_failed".to_owned()])
    );
    assert!(inbox(db, other.user.id).await.is_empty());
}

#[tokio::test]
async fn cookie_failures_reach_the_app_once_per_credential_version() {
    let Fixture {
        fixture,
        notifications,
        notifier,
    } = fixture().await;
    let db = &fixture.db;
    let now = Utc::now();
    let owner = ready_identity(db, "owner").await;
    notifications.register_app_device(&owner).await.unwrap();
    let broken = link(db, owner.user.id, "cookie_invalid").await;
    notifier.tick(now, None).await.unwrap();
    notifier
        .tick(now + Duration::minutes(1), None)
        .await
        .unwrap();
    let entries = inbox(db, owner.user.id).await;
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].kind, "cookie_invalid");
    assert_eq!(entries[0].tag, format!("cookie:{}:1", broken.id));
    assert_eq!(entries[0].url, "/settings/link");
    assert_eq!(entries[0].title, "HoYoLABのCookieが使えなくなりました");
}

#[tokio::test]
async fn devices_are_owner_scoped_limited_and_expire_when_unseen() {
    let Fixture {
        fixture,
        notifications,
        ..
    } = fixture().await;
    let db = &fixture.db;
    let owner = ready_identity(db, "owner").await;
    let other = ready_identity(db, "other").await;
    let device =
        serde_json::to_value(notifications.register_app_device(&owner).await.unwrap()).unwrap();
    let id: Uuid = device["id"].as_str().unwrap().parse().unwrap();
    // Another user can neither read nor remove it.
    assert!(matches!(
        notifications.app_inbox(&other, id, 0).await,
        Err(Error::NotFound)
    ));
    notifications
        .unregister_app_device(&other, id)
        .await
        .unwrap();
    notifications.app_inbox(&owner, id, 0).await.unwrap();

    // The test notice goes to the app as well, and new devices start after it.
    let result = serde_json::to_value(notifications.test_personal(&owner).await.unwrap()).unwrap();
    assert_eq!(result["webhook"], "not_configured");
    assert_eq!(result["app"], true);
    let test = inbox(db, owner.user.id).await;
    assert_eq!(test.len(), 1);
    assert_eq!(test[0].kind, "test");
    assert_eq!(test[0].url, "/settings/notifications");
    let second =
        serde_json::to_value(notifications.register_app_device(&owner).await.unwrap()).unwrap();
    assert_eq!(second["cursor"], test[0].id);
    let other_result =
        serde_json::to_value(notifications.test_personal(&other).await.unwrap()).unwrap();
    assert_eq!(other_result["app"], false);

    for _ in 2..10 {
        notifications.register_app_device(&owner).await.unwrap();
    }
    assert!(matches!(
        notifications.register_app_device(&owner).await,
        Err(Error::Invalid(_))
    ));

    notifications
        .unregister_app_device(&owner, id)
        .await
        .unwrap();
    assert!(matches!(
        notifications.app_inbox(&owner, id, 0).await,
        Err(Error::NotFound)
    ));

    // Cleanup drops old entries and devices that stopped polling.
    notifications
        .cleanup_app(Utc::now() + Duration::days(31))
        .await
        .unwrap();
    assert!(inbox(db, owner.user.id).await.is_empty());
    assert!(!notifications.has_app_device(owner.user.id).await.unwrap());
}
