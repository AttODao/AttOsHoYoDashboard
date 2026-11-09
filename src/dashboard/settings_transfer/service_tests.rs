use super::service::{Outcome, SettingsTransfer};
use crate::dashboard::{
    auth::{
        password::token_hash,
        sessions::{Identity, Sessions},
    },
    db::entities::{
        game_account, hoyolab_account, notification_setting, reward_run, reward_setting, user,
    },
    error::Error,
    hoyolab::{
        credentials::{CookieKey, Credentials},
        operations::Operations,
    },
    notifications::channel::{Notifications, PersonalInput},
    test_support::TestDatabase,
};
use chrono::{Duration, FixedOffset, Utc};
use sea_orm::{ActiveModelTrait, DatabaseConnection, EntityTrait, Set};
use serde_json::{Value, json};
use std::sync::Arc;
use uuid::Uuid;

const WEBHOOK: &str = "https://discord.com/api/webhooks/1/synthetic-webhook-secret";

fn key() -> Arc<CookieKey> {
    Arc::new(CookieKey::from_hex(&"11".repeat(32)).unwrap())
}

fn transfer(db: &DatabaseConnection) -> SettingsTransfer {
    SettingsTransfer::new(
        db.clone(),
        FixedOffset::east_opt(9 * 3600).unwrap().into(),
        Operations::default(),
        || false,
    )
}

async fn ready_identity(db: &DatabaseConnection, name: &str) -> Identity {
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

async fn verified_link(db: &DatabaseConnection, owner: Uuid, uid: &str) -> Uuid {
    let cookie =
        Credentials::parse(&format!("ltoken_v2=synthetictoken{uid}; ltuid_v2={uid}")).unwrap();
    hoyolab_account::ActiveModel {
        id: Set(Uuid::new_v4()),
        owner_user_id: Set(owner),
        hoyolab_uid: Set(Some(uid.to_owned())),
        encrypted_credentials: Set(Some(key().seal(owner, Some(uid), &cookie).unwrap())),
        credential_version: Set(1),
        auth_status: Set("verified".to_owned()),
        created_at: Set(Utc::now()),
        updated_at: Set(Utc::now()),
        verified_at: Set(None),
    }
    .insert(db)
    .await
    .unwrap()
    .id
}

async fn reward(
    db: &DatabaseConnection,
    link: Uuid,
    kind: &str,
    enabled: bool,
    time: &str,
    auth_status: &str,
) -> Uuid {
    reward_setting::ActiveModel {
        id: Set(Uuid::new_v4()),
        hoyolab_account_id: Set(link),
        game_kind: Set(kind.to_owned()),
        enabled: Set(enabled),
        local_time: Set(time.to_owned()),
        auth_status: Set(auth_status.to_owned()),
        next_run_at: Set(None),
    }
    .insert(db)
    .await
    .unwrap()
    .id
}

async fn game(db: &DatabaseConnection, link: Uuid, kind: &str, uid: &str, visible: bool) -> Uuid {
    game_account::ActiveModel {
        id: Set(Uuid::new_v4()),
        hoyolab_account_id: Set(link),
        game_kind: Set(kind.to_owned()),
        game_uid: Set(uid.to_owned()),
        region: Set("os_asia".to_owned()),
        visible: Set(visible),
        detected: Set(true),
        last_detected_at: Set(Utc::now()),
    }
    .insert(db)
    .await
    .unwrap()
    .id
}

fn planned(outcome: Outcome) -> Value {
    match outcome {
        Outcome::Planned(report) => serde_json::to_value(report).unwrap(),
        Outcome::Invalid(errors) => panic!("unexpected problems: {errors:?}"),
    }
}

/// Everything a later comparison needs from the database, secrets excluded.
async fn snapshot(db: &DatabaseConnection, user: Uuid) -> Value {
    let rewards: Vec<Value> = reward_setting::Entity::find()
        .all(db)
        .await
        .unwrap()
        .into_iter()
        .map(|row| {
            json!([
                row.game_kind,
                row.enabled,
                row.local_time,
                row.next_run_at.is_some()
            ])
        })
        .collect();
    let games: Vec<Value> = game_account::Entity::find()
        .all(db)
        .await
        .unwrap()
        .into_iter()
        .map(|row| json!([row.game_uid, row.visible]))
        .collect();
    let notifications = notification_setting::Entity::find_by_id(user)
        .one(db)
        .await
        .unwrap()
        .map(|row| {
            json!([
                row.language,
                row.events,
                row.app_events,
                row.daily_reminder_time,
                row.webhook_url.is_some()
            ])
        });
    json!([rewards, games, notifications])
}

#[tokio::test]
async fn export_round_trips_and_import_previews_then_applies_in_one_transaction() {
    let fixture = TestDatabase::new().await;
    let db = &fixture.db;
    let actor = ready_identity(db, "transfer-user").await;
    let link = verified_link(db, actor.user.id, "123456789").await;
    let genshin = reward(db, link, "genshin", true, "06:00", "ready").await;
    reward(db, link, "starrail", false, "07:00", "action_required").await;
    game(db, link, "genshin", "800000001", true).await;
    game(db, link, "starrail", "900000001", false).await;
    let notifications = Notifications::new(db.clone(), Some(key()));
    let input: PersonalInput = serde_json::from_value(json!({
        "webhook_url": WEBHOOK,
        "webhook_format": "discord",
        "language": "ja",
        "events": {"reward_claimed": false},
        "app_events": {"resource_full": true},
        "resin_warn_hours": 2,
        "daily_reminder_time": "21:00",
        "weekly_reminder_time": "15:00",
    }))
    .unwrap();
    notifications.update_personal(&actor, input).await.unwrap();
    let service = transfer(db);

    let exported = service.export(&actor).await.unwrap();
    let text = exported.to_string();
    for secret in [
        "synthetic-webhook-secret",
        "synthetictoken",
        "webhook_url",
        "123456789",
    ] {
        assert!(!text.contains(secret), "export leaked {secret}");
    }
    assert_eq!(exported["format"], "ahd-settings");
    assert_eq!(exported["version"], 1);
    assert_eq!(
        exported["rewards"],
        json!([
            {"game_kind": "genshin", "enabled": true, "local_time": "06:00"},
            {"game_kind": "starrail", "enabled": false, "local_time": "07:00"},
        ])
    );
    assert_eq!(
        exported["game_accounts"],
        json!([
            {"game_kind": "genshin", "game_uid": "800000001", "visible": true},
            {"game_kind": "starrail", "game_uid": "900000001", "visible": false},
        ])
    );
    assert_eq!(exported["notifications"]["webhook_configured"], true);
    assert_eq!(exported["notifications"]["events"]["cookie_invalid"], true);
    assert_eq!(
        exported["notifications"]["app_events"]["resource_full"],
        true
    );

    // Re-importing an unchanged export changes nothing.
    assert_eq!(
        planned(service.preview(&actor, &exported).await.unwrap()),
        json!({"changes": [], "skipped": [], "webhook": {"in_file": true, "configured": true}})
    );

    // A pending scheduled run must stop when the schedule changes.
    let now = Utc::now();
    reward_run::ActiveModel {
        id: Set(Uuid::new_v4()),
        setting_id: Set(genshin),
        trigger: Set("scheduled".to_owned()),
        state: Set("pending".to_owned()),
        created_at: Set(now),
        updated_at: Set(now),
        ended_at: Set(None),
        attempt_count: Set(0),
        credential_version: Set(1),
        send_state: Set("not_sent".to_owned()),
        next_retry_at: Set(None),
    }
    .insert(db)
    .await
    .unwrap();

    let mut file = exported.clone();
    file["rewards"][0]["local_time"] = json!("08:30");
    file["rewards"][1]["enabled"] = json!(true);
    file["game_accounts"][0]["visible"] = json!(false);
    file["game_accounts"]
        .as_array_mut()
        .unwrap()
        .push(json!({"game_kind": "genshin", "game_uid": "800000099", "visible": true}));
    file["notifications"]["language"] = json!("en");
    file["notifications"]["events"] = json!({"reward_claimed": true});
    let expected = json!({
        "changes": [
            {"section": "rewards", "game_kind": "genshin", "setting": "local_time", "from": "06:00", "to": "08:30"},
            {"section": "game_accounts", "game_kind": "genshin", "game_uid": "800000001", "setting": "visible", "from": true, "to": false},
            {"section": "notifications", "setting": "language", "from": "ja", "to": "en"},
            {"section": "notifications", "setting": "events.reward_claimed", "from": false, "to": true},
        ],
        "skipped": [
            {"section": "rewards", "game_kind": "starrail", "reason": "reward_not_ready"},
            {"section": "game_accounts", "game_kind": "genshin", "game_uid": "800000099", "reason": "uid_not_found"},
        ],
        "webhook": {"in_file": true, "configured": true},
    });
    let before = snapshot(db, actor.user.id).await;
    assert_eq!(
        planned(service.preview(&actor, &file).await.unwrap()),
        expected
    );
    assert_eq!(snapshot(db, actor.user.id).await, before, "preview wrote");

    assert_eq!(
        planned(service.import(&actor, &file).await.unwrap()),
        expected
    );
    let setting = reward_setting::Entity::find_by_id(genshin)
        .one(db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(setting.local_time, "08:30");
    assert!(setting.enabled && setting.next_run_at.is_some_and(|at| at > now));
    let runs = reward_run::Entity::find().all(db).await.unwrap();
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].state, "stopped");
    let games = game_account::Entity::find().all(db).await.unwrap();
    assert_eq!(games.len(), 2, "an unknown UID is never created");
    assert!(games.iter().all(|row| !row.visible));
    let view = serde_json::to_value(notifications.personal_view(&actor).await.unwrap()).unwrap();
    assert_eq!(view["webhook_configured"], true, "the Webhook URL is kept");
    assert_eq!(view["language"], "en");
    assert_eq!(view["events"]["reward_claimed"], true);
    assert_eq!(
        view["events"]["cookie_invalid"], true,
        "unlisted kinds keep their choice"
    );
    assert_eq!(view["app_events"]["resource_full"], true);

    // Applying the same file again finds nothing left to change.
    let again = planned(service.import(&actor, &file).await.unwrap());
    assert_eq!(again["changes"], json!([]));
}

#[tokio::test]
async fn imports_reject_bad_files_skip_unlinked_entries_and_need_a_live_session() {
    let fixture = TestDatabase::new().await;
    let db = &fixture.db;
    let owner = ready_identity(db, "transfer-owner").await;
    let link = verified_link(db, owner.user.id, "223456789").await;
    reward(db, link, "genshin", false, "06:00", "ready").await;
    game(db, link, "genshin", "800000001", true).await;
    let service = transfer(db);
    let mut file = service.export(&owner).await.unwrap();
    assert_eq!(file["notifications"]["webhook_configured"], false);
    file["notifications"]["webhook_configured"] = json!(true);
    file["notifications"]["resin_warn_hours"] = json!(5);

    // Another user with no HoYoLAB account: rewards and UIDs are reported, never created.
    let other = ready_identity(db, "transfer-other").await;
    let before = snapshot(db, other.user.id).await;
    let report = planned(service.import(&other, &file).await.unwrap());
    assert_eq!(
        report,
        json!({
            "changes": [
                {"section": "notifications", "setting": "resin_warn_hours", "from": 2, "to": 5},
            ],
            "skipped": [
                {"section": "rewards", "game_kind": "genshin", "reason": "not_linked"},
                {"section": "game_accounts", "game_kind": "genshin", "game_uid": "800000001", "reason": "not_linked"},
            ],
            "webhook": {"in_file": true, "configured": false},
        })
    );
    let after = snapshot(db, other.user.id).await;
    assert_eq!(after[0], before[0]);
    assert_eq!(after[1], before[1]);
    assert_eq!(
        after[2],
        json!(["ja", after[2][1], after[2][2], "21:00", false])
    );
    assert_eq!(
        reward_setting::Entity::find().all(db).await.unwrap().len(),
        1
    );

    // A file with problems changes nothing and lists every problem.
    let mut bad = file.clone();
    bad["notifications"]["language"] = json!("de");
    bad["rewards"][0]["local_time"] = json!("25:00");
    let before = snapshot(db, owner.user.id).await;
    for outcome in [
        service.preview(&owner, &bad).await.unwrap(),
        service.import(&owner, &bad).await.unwrap(),
    ] {
        let Outcome::Invalid(errors) = outcome else {
            panic!("a bad file was accepted");
        };
        let fields: Vec<_> = errors.iter().map(|e| (e.field.as_str(), e.code)).collect();
        assert_eq!(
            fields,
            vec![
                ("rewards[0].local_time", "invalid"),
                ("notifications.language", "invalid"),
            ]
        );
    }
    assert_eq!(snapshot(db, owner.user.id).await, before);

    // A server that is shutting down takes no import.
    let closing = SettingsTransfer::new(
        db.clone(),
        FixedOffset::east_opt(9 * 3600).unwrap().into(),
        Operations::default(),
        || true,
    );
    assert!(matches!(
        closing.import(&owner, &file).await,
        Err(Error::Unavailable(_))
    ));

    // A signed-out or locked user can neither read nor write.
    Sessions::revoke_all(db, owner.user.id).await.unwrap();
    for result in [
        service.export(&owner).await.err(),
        service.preview(&owner, &file).await.err(),
        service.import(&owner, &file).await.err(),
        service.import(&owner, &bad).await.err(),
    ] {
        assert!(matches!(result, Some(Error::Unauthorized)), "{result:?}");
    }
}
