use super::channel::{ChannelInput, Notifications, PersonalInput};
use crate::dashboard::{
    auth::sessions::Identity, hoyolab::credentials::CookieKey, test_support::TestDatabase,
};
use serde_json::{Value, json};
use std::sync::Arc;

#[tokio::test]
async fn personal_settings_round_trip_webhook_url_and_require_valid_fields() {
    let fixture = TestDatabase::new().await;
    let (_, actor) = fixture.ready_admin().await;
    let notifications = Notifications::new(
        fixture.db.clone(),
        Some(Arc::new(CookieKey::from_hex(&"11".repeat(32)).unwrap())),
    );
    let view = notifications.personal_view(&actor).await.unwrap();
    let json = serde_json::to_value(&view).unwrap();
    assert_eq!(json["webhook_configured"], false);
    assert_eq!(json["webhook_hint"], Value::Null);
    assert_eq!(json["resin_warn_hours"], 2);
    assert!(json.get("push_public_key").is_none() && json.get("push_subscriptions").is_none());

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
async fn app_event_choices_are_separate_from_the_webhook_and_kept_when_absent() {
    let fixture = TestDatabase::new().await;
    let (_, actor) = fixture.ready_admin().await;
    let notifications = Notifications::new(
        fixture.db.clone(),
        Some(Arc::new(CookieKey::from_hex(&"11".repeat(32)).unwrap())),
    );
    let personal = |extra: Value| -> PersonalInput {
        let mut body = json!({
            "webhook_format": "discord",
            "language": "ja",
            "events": {"reward_claimed": true, "resource_full": false},
            "resin_warn_hours": 2,
            "daily_reminder_time": "21:00",
            "weekly_reminder_time": "15:00",
        });
        body.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        serde_json::from_value(body).unwrap()
    };
    async fn app_events(notifications: &Notifications, actor: &Identity) -> Value {
        serde_json::to_value(notifications.personal_view(actor).await.unwrap()).unwrap()["app_events"]
            .clone()
    }

    // Unsaved: every personal kind with the webhook defaults.
    let defaults = app_events(&notifications, &actor).await;
    assert_eq!(
        defaults.as_object().unwrap().len(),
        super::PERSONAL_EVENTS.len()
    );
    assert_eq!(defaults["reward_failed"], true);
    assert_eq!(defaults["reward_claimed"], false);
    assert_eq!(defaults["signed_in"], false);

    // A first save without app_events (older clients) copies the webhook choices.
    notifications
        .update_personal(&actor, personal(json!({})))
        .await
        .unwrap();
    let copied = app_events(&notifications, &actor).await;
    assert_eq!(copied["reward_claimed"], true);
    assert_eq!(copied["resource_full"], false);

    // Explicit app choices are stored apart from the webhook ones.
    notifications
        .update_personal(
            &actor,
            personal(json!({"app_events": {"resource_full": true, "reward_claimed": false, "weekly_unfinished": false}})),
        )
        .await
        .unwrap();
    let view = serde_json::to_value(notifications.personal_view(&actor).await.unwrap()).unwrap();
    assert_eq!(view["app_events"]["resource_full"], true);
    assert_eq!(view["app_events"]["reward_claimed"], false);
    assert_eq!(view["app_events"]["weekly_unfinished"], false);
    assert_eq!(
        view["app_events"]["cookie_invalid"], true,
        "unknown kinds take defaults"
    );
    assert_eq!(view["events"]["resource_full"], false);
    assert_eq!(view["events"]["reward_claimed"], true);

    // Saving the webhook side alone keeps them.
    notifications
        .update_personal(&actor, personal(json!({"events": {}})))
        .await
        .unwrap();
    let kept = app_events(&notifications, &actor).await;
    assert_eq!(kept["resource_full"], true);
    assert_eq!(kept["weekly_unfinished"], false);

    // Administrator-only kinds and unknown kinds are rejected for the app.
    for bad in [
        json!({"app_events": {"worker_stalled": true}}),
        json!({"app_events": {"user_attention": true}}),
        json!({"app_events": {"test": true}}),
    ] {
        assert!(matches!(
            notifications.update_personal(&actor, personal(bad)).await,
            Err(crate::dashboard::error::Error::Invalid(_))
        ));
    }
    assert!(
        serde_json::from_value::<PersonalInput>(json!({
            "webhook_format": "json", "language": "ja", "events": {}, "app_events": [],
            "resin_warn_hours": 2, "daily_reminder_time": "06:00", "weekly_reminder_time": "15:00",
        }))
        .is_err()
    );
    let admin_view = serde_json::to_value(notifications.admin_view(&actor).await.unwrap()).unwrap();
    assert!(admin_view.get("app_events").is_none());
}

#[tokio::test]
async fn admin_channel_requires_an_administrator_and_shares_a_single_row() {
    let fixture = TestDatabase::new().await;
    let (_, admin) = fixture.ready_admin().await;
    let notifications = Notifications::new(
        fixture.db.clone(),
        Some(Arc::new(CookieKey::from_hex(&"11".repeat(32)).unwrap())),
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
