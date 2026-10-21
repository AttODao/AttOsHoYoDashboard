use super::channel::{ChannelInput, Notifications, PersonalInput};
use crate::dashboard::{hoyolab::credentials::CookieKey, test_support::TestDatabase};
use serde_json::{Value, json};
use std::sync::Arc;

#[tokio::test]
async fn personal_settings_round_trip_webhook_url_and_require_valid_fields() {
    let fixture = TestDatabase::new().await;
    let (_, actor) = fixture.ready_admin().await;
    let notifications = Notifications::new(
        fixture.db.clone(),
        Some(Arc::new(CookieKey::from_hex(&"11".repeat(32)).unwrap())),
        "https://ahd.example.test".to_owned(),
    );
    let view = notifications.personal_view(&actor).await.unwrap();
    let json = serde_json::to_value(&view).unwrap();
    assert_eq!(json["webhook_configured"], false);
    assert_eq!(json["webhook_hint"], Value::Null);
    assert_eq!(json["resin_warn_hours"], 2);
    assert!(json["push_public_key"].is_string());

    let input: PersonalInput = serde_json::from_value(json!({
        "webhook_url": "https://discord.com/api/webhooks/1/synthetic-token",
        "webhook_format": "discord",
        "language": "ja",
        "events": {"reward_failed": true},
        "resin_warn_hours": 6,
        "daily_reminder_time": "22:30",
        "weekly_reminder_time": "15:00",
    }))
    .unwrap();
    notifications
        .update_personal(&actor, input)
        .await
        .map_err(|e| format!("update_personal: {:?}", e))
        .unwrap();
    let json = serde_json::to_value(notifications.personal_view(&actor).await.unwrap()).unwrap();
    assert_eq!(json["webhook_configured"], true);
    assert_eq!(json["webhook_hint"].as_str().unwrap(), "discord.com/…/oken");
    assert_eq!(json["resin_warn_hours"], 6);
    assert_eq!(json["daily_reminder_time"], "22:30");
    assert_eq!(json["events"]["reward_failed"], true);
    assert_eq!(
        json["events"]["cookie_invalid"], true,
        "unknown kinds take defaults"
    );

    // Keeping the URL across unrelated updates.
    let keep: PersonalInput = serde_json::from_value(json!({
        "webhook_format": "json",
        "language": "en",
        "events": {},
        "resin_warn_hours": 4,
        "daily_reminder_time": "08:00",
        "weekly_reminder_time": "15:00",
    }))
    .unwrap();
    notifications.update_personal(&actor, keep).await.unwrap();
    assert_eq!(
        serde_json::to_value(notifications.personal_view(&actor).await.unwrap()).unwrap()["webhook_configured"],
        true
    );

    // Null removes the URL.
    let clear: PersonalInput = serde_json::from_value(json!({
        "webhook_url": null,
        "webhook_format": "json",
        "language": "en",
        "events": {},
        "resin_warn_hours": 4,
        "daily_reminder_time": "08:00",
        "weekly_reminder_time": "15:00",
    }))
    .unwrap();
    notifications.update_personal(&actor, clear).await.unwrap();
    assert_eq!(
        serde_json::to_value(notifications.personal_view(&actor).await.unwrap()).unwrap()["webhook_configured"],
        false
    );

    for bad in [
        json!({"webhook_url": "http://discord.com/x", "webhook_format": "discord", "language": "ja", "events": {}, "resin_warn_hours": 2, "daily_reminder_time": "06:00", "weekly_reminder_time": "15:00"}),
        json!({"webhook_format": "slack", "language": "ja", "events": {}, "resin_warn_hours": 2, "daily_reminder_time": "06:00", "weekly_reminder_time": "15:00"}),
        json!({"webhook_format": "json", "language": "ja", "events": {"unknown_event": true}, "resin_warn_hours": 2, "daily_reminder_time": "06:00", "weekly_reminder_time": "15:00"}),
        json!({"webhook_format": "json", "language": "ja", "events": {}, "resin_warn_hours": 0, "daily_reminder_time": "06:00", "weekly_reminder_time": "15:00"}),
        json!({"webhook_format": "json", "language": "ja", "events": {}, "resin_warn_hours": 2, "daily_reminder_time": "25:00", "weekly_reminder_time": "15:00"}),
    ] {
        let Ok(input) = serde_json::from_value::<PersonalInput>(bad) else {
            continue;
        };
        assert!(notifications.update_personal(&actor, input).await.is_err());
    }
}

#[tokio::test]
async fn admin_channel_requires_an_administrator_and_shares_a_single_row() {
    let fixture = TestDatabase::new().await;
    let (_, admin) = fixture.ready_admin().await;
    let notifications = Notifications::new(
        fixture.db.clone(),
        Some(Arc::new(CookieKey::from_hex(&"11".repeat(32)).unwrap())),
        "https://ahd.example.test".to_owned(),
    );
    let input: ChannelInput = serde_json::from_value(json!({
        "webhook_url": "https://discord.com/api/webhooks/9/admin-token",
        "webhook_format": "json",
        "language": "en",
        "events": {"worker_stalled": true, "deletion_stuck": false},
    }))
    .unwrap();
    notifications.update_admin(&admin, input).await.unwrap();
    let view = serde_json::to_value(notifications.admin_view(&admin).await.unwrap()).unwrap();
    assert_eq!(view["webhook_hint"].as_str().unwrap(), "discord.com/…/oken");
    assert_eq!(view["events"]["deletion_stuck"], false);
    assert_eq!(view["events"]["worker_stalled"], true);
    assert_eq!(
        view["events"]["user_attention"], true,
        "unknown kinds take defaults"
    );
}
