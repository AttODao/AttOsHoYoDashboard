use super::document::{FieldError, parse, render};
use serde_json::{Value, json};

fn file() -> Value {
    json!({
        "format": "ahd-settings",
        "version": 1,
        "exported_at": "2026-11-09T01:02:03Z",
        "rewards": [
            {"game_kind": "genshin", "enabled": true, "local_time": "06:00"},
            {"game_kind": "starrail", "enabled": false, "local_time": "07:30"},
        ],
        "game_accounts": [
            {"game_kind": "genshin", "game_uid": "800000001", "visible": false},
        ],
        "notifications": {
            "webhook_configured": true,
            "webhook_format": "json",
            "language": "en",
            "events": {"reward_claimed": true},
            "app_events": {"resource_full": false},
            "resin_warn_hours": 6,
            "daily_reminder_time": "22:00",
            "weekly_reminder_time": "16:00",
        },
    })
}

fn errors(value: &Value) -> Vec<(String, &'static str)> {
    parse(value)
        .unwrap_err()
        .into_iter()
        .map(|FieldError { field, code }| (field, code))
        .collect()
}

fn error(field: &str, code: &'static str) -> (String, &'static str) {
    (field.to_owned(), code)
}

#[test]
fn a_rendered_file_parses_back_to_the_same_document() {
    let document = parse(&file()).unwrap();
    assert_eq!(document.rewards.len(), 2);
    assert_eq!(document.game_accounts[0].game_uid, "800000001");
    assert!(document.notifications.webhook_configured);
    let rendered = render(&document, chrono::Utc::now());
    assert_eq!(rendered["format"], "ahd-settings");
    assert_eq!(rendered["version"], 1);
    assert_eq!(parse(&rendered).unwrap(), document);
    // `exported_at` is informational and may be left out.
    let mut without = file();
    without.as_object_mut().unwrap().remove("exported_at");
    assert_eq!(parse(&without).unwrap(), document);
}

#[test]
fn another_format_or_version_is_rejected_before_anything_else() {
    for (change, expected) in [
        (json!({"format": "other"}), error("format", "unsupported")),
        (json!({"format": 1}), error("format", "type")),
        (json!({"version": 2}), error("version", "unsupported")),
        (json!({"version": "1"}), error("version", "type")),
        (json!({"version": 1.5}), error("version", "unsupported")),
    ] {
        let mut value = file();
        for (key, field) in change.as_object().unwrap() {
            value[key] = field.clone();
        }
        value["rewards"] = json!("not checked");
        assert_eq!(errors(&value), vec![expected]);
    }
    let mut missing = file();
    missing.as_object_mut().unwrap().remove("version");
    assert_eq!(errors(&missing), vec![error("version", "missing")]);
    assert_eq!(errors(&json!([])), vec![error("$", "type")]);
}

#[test]
fn every_problem_is_reported_with_its_field() {
    let value = json!({
        "format": "ahd-settings",
        "version": 1,
        "exported_at": "yesterday",
        "cookie": "ltoken_v2=synthetic",
        "rewards": [
            {"game_kind": "zzz", "enabled": "yes", "local_time": "6:00"},
            {"game_kind": "genshin", "enabled": true, "local_time": "06:00", "extra": 1},
            {"game_kind": "genshin", "enabled": true, "local_time": "06:00"},
        ],
        "game_accounts": [
            {"game_kind": "genshin", "game_uid": "0123", "visible": true},
            {"game_kind": "genshin", "game_uid": "800000001", "visible": true},
            {"game_kind": "genshin", "game_uid": "800000001", "visible": false},
            "800000002",
        ],
        "notifications": {
            "webhook_url": "https://discord.com/api/webhooks/1/synthetic",
            "webhook_configured": "no",
            "webhook_format": "slack",
            "language": "de",
            "events": {"reward_claimed": 1, "unknown_kind": true},
            "app_events": [],
            "resin_warn_hours": 25,
            "daily_reminder_time": "24:00",
        },
    });
    assert_eq!(
        errors(&value),
        vec![
            error("cookie", "unknown"),
            error("exported_at", "invalid"),
            error("rewards", "too_many"),
            error("game_accounts[0].game_uid", "invalid"),
            error("game_accounts[2].game_uid", "duplicate"),
            error("game_accounts[3]", "type"),
            error("notifications.webhook_url", "unknown"),
            error("notifications.webhook_configured", "type"),
            error("notifications.webhook_format", "invalid"),
            error("notifications.language", "invalid"),
            error("notifications.events.reward_claimed", "type"),
            error("notifications.events.unknown_kind", "unknown"),
            error("notifications.app_events", "type"),
            error("notifications.resin_warn_hours", "invalid"),
            error("notifications.daily_reminder_time", "invalid"),
            error("notifications.weekly_reminder_time", "missing"),
        ]
    );
    let mut rewards = file();
    rewards["rewards"] = json!([
        {"game_kind": "zzz", "enabled": "yes", "local_time": "6:00"},
        {"game_kind": "genshin", "enabled": true, "local_time": "06:00", "extra": 1},
    ]);
    assert_eq!(
        errors(&rewards),
        vec![
            error("rewards[0].game_kind", "invalid"),
            error("rewards[0].enabled", "type"),
            error("rewards[0].local_time", "invalid"),
            error("rewards[1].extra", "unknown"),
        ]
    );
    rewards["rewards"] = json!([
        {"game_kind": "genshin", "enabled": true, "local_time": "06:00"},
        {"game_kind": "genshin", "enabled": false, "local_time": "07:00"},
    ]);
    assert_eq!(
        errors(&rewards),
        vec![error("rewards[1].game_kind", "duplicate")]
    );
}

#[test]
fn sections_are_required_and_sizes_are_bounded() {
    for section in ["rewards", "game_accounts", "notifications"] {
        let mut value = file();
        value.as_object_mut().unwrap().remove(section);
        assert_eq!(errors(&value), vec![error(section, "missing")]);
    }
    let mut value = file();
    value["game_accounts"] = Value::Array(
        (0..65)
            .map(|n| json!({"game_kind": "genshin", "game_uid": format!("{}", 800_000_000 + n), "visible": true}))
            .collect(),
    );
    assert_eq!(errors(&value), vec![error("game_accounts", "too_many")]);
    let mut value = file();
    value["notifications"]["resin_warn_hours"] = json!(2.5);
    assert_eq!(
        errors(&value),
        vec![error("notifications.resin_warn_hours", "type")]
    );
    // A huge number of problems is cut short instead of growing the response.
    let mut value = file();
    value["notifications"]["events"] = Value::Object(
        (0..80)
            .map(|n| (format!("unknown_{n:02}"), json!(true)))
            .collect(),
    );
    assert_eq!(errors(&value).len(), 50);
}
