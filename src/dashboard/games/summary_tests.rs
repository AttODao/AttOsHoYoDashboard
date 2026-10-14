use super::summary::summarize;
use attosgameapi::game::{GameKind, Genshin, Starrail};
use chrono::{TimeZone, Utc};
use serde_json::json;

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
    assert!(summary["expeditions"][0].get("avatar_side_icon").is_none());
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
    assert!(summary["expeditions"][0].get("item_url").is_none());
}
