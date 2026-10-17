use super::summary::summarize;
use attoshoyodashboard::game::{GameKind, Genshin, Starrail};
use chrono::{TimeZone, Utc};
use serde_json::{Value, json};

#[test]
fn typed_genshin_note_preserves_explicit_names_and_locations_without_inventing_labels() {
    let mut raw: Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/hoyolab/genshin-note.json"
    ))
    .unwrap();
    let expedition = raw["expeditions"][0].clone();
    raw["expeditions"] = json!([
        {"name":"Assignment", "location":"Destination"},
        {"location":"Destination"},
        {"name":"  ", "location":"Destination"},
        {"name":42, "location":"Destination"},
        {}
    ]);
    for item in raw["expeditions"].as_array_mut().unwrap() {
        for (key, value) in expedition.as_object().unwrap() {
            item[key] = value.clone();
        }
    }
    let note: attoshoyodashboard::models::genshin::DailyNote = serde_json::from_value(raw).unwrap();
    let summary = summarize(
        GameKind::Genshin(Genshin),
        Utc.with_ymd_and_hms(2026, 1, 2, 3, 4, 5).unwrap(),
        &serde_json::to_value(note).unwrap(),
    )
    .unwrap();
    assert_eq!(summary["expeditions"][0]["name"], "Assignment");
    for index in 1..4 {
        assert_eq!(summary["expeditions"][index]["name"], "Destination");
    }
    assert!(summary["expeditions"][4].get("name").is_none());
    for item in summary["expeditions"].as_array().unwrap() {
        assert_eq!(item["avatar_side_icon"], expedition["avatar_side_icon"]);
    }
}

#[test]
fn typed_starrail_note_reaches_summary_with_assignment_icon_name_status_and_numeric_time() {
    let note: attoshoyodashboard::models::starrail::DailyNote = serde_json::from_str(include_str!(
        "../../../tests/fixtures/hoyolab/starrail-note.json"
    ))
    .unwrap();
    let summary = summarize(
        GameKind::Starrail(Starrail),
        Utc.with_ymd_and_hms(2026, 1, 2, 3, 4, 5).unwrap(),
        &serde_json::to_value(note).unwrap(),
    )
    .unwrap();
    assert_eq!(
        summary["expeditions"],
        json!([
            {"name":"test", "status":"Ongoing", "remaining_time":456, "item_url":"item"}
        ])
    );
}

#[test]
fn expedition_icons_are_game_specific_nonempty_strings_and_never_invented() {
    let now = Utc.with_ymd_and_hms(2026, 1, 2, 3, 4, 5).unwrap();
    for (game, icon_key, other_icon_key) in [
        (GameKind::Genshin(Genshin), "avatar_side_icon", "item_url"),
        (GameKind::Starrail(Starrail), "item_url", "avatar_side_icon"),
    ] {
        for icon in [Value::Null, json!(42), json!({}), json!(""), json!("  ")] {
            let summary = summarize(
                game,
                now,
                &json!({"expeditions":[{"status":"Ongoing", (icon_key):icon, (other_icon_key):"other-game-icon"}]}),
            )
            .unwrap();
            assert_eq!(summary["expeditions"], json!([{"status":"Ongoing"}]));
        }
        let summary = summarize(
            game,
            now,
            &json!({"expeditions":[{(icon_key):"opaque-upstream-icon"}]}),
        )
        .unwrap();
        assert_eq!(
            summary["expeditions"],
            json!([{(icon_key):"opaque-upstream-icon"}])
        );
    }
}

#[test]
fn reported_empty_expeditions_are_distinct_from_missing_or_invalid_expeditions() {
    let now = Utc.with_ymd_and_hms(2026, 1, 2, 3, 4, 5).unwrap();
    for game in [GameKind::Genshin(Genshin), GameKind::Starrail(Starrail)] {
        let summary = summarize(game, now, &json!({"expeditions":[]})).unwrap();
        assert_eq!(summary["expeditions"], json!([]));
        for raw in [
            json!({}),
            json!({"expeditions":null}),
            json!({"expeditions":{}}),
            json!({"expeditions":[null, 42, {}]}),
        ] {
            assert!(summarize(game, now, &raw).is_none());
        }
    }
}

#[test]
fn enormous_recovery_seconds_never_panic_or_fabricate_a_full_time() {
    let now = Utc.with_ymd_and_hms(2026, 1, 2, 3, 4, 5).unwrap();
    for seconds in [json!(i64::MAX), json!(i64::MAX.to_string())] {
        let summary = summarize(
            GameKind::Genshin(Genshin),
            now,
            &json!({"current_resin":1,"resin_recovery_time":seconds}),
        )
        .unwrap();
        assert!(summary.get("resin_full_at").is_none());
    }
}

#[test]
fn genshin_summary_curates_current_note_and_projects_confirmed_recovery_seconds() {
    let fetched_at = Utc.with_ymd_and_hms(2026, 1, 2, 3, 4, 5).unwrap();
    let summary = summarize(
        GameKind::Genshin(Genshin),
        fetched_at,
        &json!({
            "current_resin": 42,
            "max_resin": 200,
            "resin_recovery_time": "60",
            "current_home_coin": 100,
            "max_home_coin": 2400,
            "home_coin_recovery_time": "120",
            "finished_task_num": 3,
            "total_task_num": 4,
            "is_extra_task_reward_received": false,
            "expeditions": [{"status":"Ongoing", "remained_time":"1800", "avatar_side_icon":"private-url"}],
            "transformer": {"unsupported": true},
            "weekly": 99
        }),
    )
    .unwrap();

    assert_eq!(summary["current_resin"], 42);
    assert_eq!(summary["max_resin"], 200);
    assert_eq!(summary["resin_full_at"], "2026-01-02T03:05:05+00:00");
    assert_eq!(summary["home_coin_full_at"], "2026-01-02T03:06:05+00:00");
    assert_eq!(summary["expeditions"][0]["status"], "Ongoing");
    assert_eq!(summary["expeditions"][0]["remained_time"], "1800");
    assert!(summary.get("transformer").is_none());
    assert_eq!(summary["expeditions"][0]["avatar_side_icon"], "private-url");
}

#[test]
fn invalid_or_negative_genshin_recovery_does_not_fabricate_projection_or_zeroes() {
    let fetched_at = Utc.with_ymd_and_hms(2026, 1, 2, 3, 4, 5).unwrap();
    let summary = summarize(
        GameKind::Genshin(Genshin),
        fetched_at,
        &json!({
            "current_resin": 0,
            "resin_recovery_time": "-1",
            "home_coin_recovery_time": "not-seconds"
        }),
    )
    .unwrap();

    assert_eq!(summary["current_resin"], 0);
    assert!(summary.get("max_resin").is_none());
    assert!(summary.get("resin_full_at").is_none());
    assert!(summary.get("home_coin_full_at").is_none());
}

#[test]
fn starrail_summary_keeps_supported_current_fields_without_weekly_or_made_up_reserve_limits() {
    let fetched_at = Utc.with_ymd_and_hms(2026, 1, 2, 3, 4, 5).unwrap();
    let summary = summarize(
        GameKind::Starrail(Starrail),
        fetched_at,
        &json!({
            "current_stamina": 120,
            "max_stamina": 240,
            "stamina_recover_time": 30,
            "current_reserve_stamina": 900,
            "current_train_score": 100,
            "max_train_score": 500,
            "weekly_cocoon_cnt": 1,
            "weekly_cocoon_limit": 3,
            "expeditions": [{"name":"assignment", "status":"Finished", "remaining_time": 0, "item_url":"private-url"}]
        }),
    )
    .unwrap();

    assert_eq!(summary["stamina_full_at"], "2026-01-02T03:04:35+00:00");
    assert_eq!(summary["current_reserve_stamina"], 900);
    assert_eq!(summary["expeditions"][0]["remaining_time"], 0);
    assert!(summary.get("max_reserve_stamina").is_none());
    assert!(summary.get("weekly_cocoon_cnt").is_none());
    assert_eq!(summary["expeditions"][0]["item_url"], "private-url");
}
