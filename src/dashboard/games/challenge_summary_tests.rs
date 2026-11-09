//! Synthetic, genshin.py-shaped payloads; none are captured live responses.
use super::challenge_summary::{ModeSummary, failed, summarize};
use attoshoyodashboard::challenge::Challenge;
use chrono::{DateTime, Utc};
use serde_json::json;

fn at(text: &str) -> Option<DateTime<Utc>> {
    Some(
        DateTime::parse_from_rfc3339(text)
            .unwrap()
            .with_timezone(&Utc),
    )
}

#[test]
fn spiral_abyss_reads_stars_floor_battles_and_unix_period() {
    let summary = summarize(
        Challenge::SpiralAbyss,
        "os_asia",
        &json!({
            "is_unlock": true, "schedule_id": 90,
            "start_time": "1793548800", "end_time": "1794758399",
            "total_battle_times": 14, "total_win_times": 12,
            "max_floor": "12-3", "total_star": 33,
            "reveal_rank": [{"avatar_id": 1, "value": 9}],
            "floors": [{"index": 12, "star": 9}]
        }),
    );
    assert_eq!(
        summary,
        ModeSummary {
            mode: "spiral_abyss".into(),
            unlocked: Some(true),
            has_data: Some(true),
            starts_at: at("2026-11-01T16:00:00Z"),
            ends_at: at("2026-11-15T15:59:59Z"),
            stars: Some(33),
            max_floor: Some("12-3".into()),
            battles: Some(14),
            ..ModeSummary::default()
        }
    );
    // No per-character rank or floor detail reaches the stored summary.
    let stored = serde_json::to_string(&summary).unwrap();
    assert!(!stored.contains("avatar_id") && !stored.contains("reveal"));

    let untouched = summarize(
        Challenge::SpiralAbyss,
        "os_asia",
        &json!({"total_battle_times": 0, "start_time": "0"}),
    );
    assert_eq!(untouched.has_data, Some(false));
    assert_eq!(untouched.starts_at, None);
}

#[test]
fn missing_fields_stay_unknown_instead_of_locked_or_empty() {
    for challenge in [
        Challenge::SpiralAbyss,
        Challenge::ImaginariumTheater,
        Challenge::StygianOnslaught,
        Challenge::MemoryOfChaos,
        Challenge::PureFiction,
        Challenge::ApocalypticShadow,
        Challenge::AnomalyArbitration,
    ] {
        for raw in [json!({}), json!(null), json!([1, 2]), json!({"data": "x"})] {
            assert_eq!(
                summarize(challenge, "os_asia", &raw),
                ModeSummary {
                    mode: challenge.id().into(),
                    ..ModeSummary::default()
                },
                "{challenge:?} {raw}"
            );
        }
    }
    let wrong_types = summarize(
        Challenge::SpiralAbyss,
        "os_asia",
        &json!({"is_unlock": "yes", "total_star": "many", "max_floor": "  ", "end_time": -5}),
    );
    assert_eq!(wrong_types.unlocked, None);
    assert_eq!(wrong_types.stars, None);
    assert_eq!(wrong_types.max_floor, None);
    assert_eq!(wrong_types.ends_at, None);
}

#[test]
fn imaginarium_theater_prefers_the_current_schedule_entry() {
    let summary = summarize(
        Challenge::ImaginariumTheater,
        "os_euro",
        &json!({
            "is_unlock": true,
            "data": [
                {
                    "stat": {"difficulty_id": 3, "max_round_id": 8, "medal_num": 7},
                    "schedule": {"schedule_id": 11, "schedule_type": 2, "start_time": "1790000000", "end_time": "1791000000"},
                    "has_data": true
                },
                {
                    "rounds_data": [{"avatars": [{"avatar_id": 1}]}],
                    "stat": {"difficulty_id": 4, "max_round_id": 6, "medal_num": 5},
                    "schedule": {
                        "schedule_id": 12, "schedule_type": 1,
                        "start_date_time": {"year": 2026, "month": 11, "day": 1, "hour": 4, "minute": 0, "second": 0},
                        "end_date_time": {"year": 2026, "month": 12, "day": 1, "hour": 3, "minute": 59, "second": 59}
                    },
                    "has_data": true
                }
            ]
        }),
    );
    assert_eq!(summary.unlocked, Some(true));
    assert_eq!(summary.has_data, Some(true));
    assert_eq!(summary.max_round, Some(6));
    assert_eq!(summary.difficulty, Some(4));
    assert_eq!(summary.stars, Some(5));
    // Date objects are server time: the Europe server is UTC+1.
    assert_eq!(summary.starts_at, at("2026-11-01T03:00:00Z"));
    assert_eq!(summary.ends_at, at("2026-12-01T02:59:59Z"));
}

#[test]
fn stygian_onslaught_uses_the_valid_schedule_and_the_solo_best() {
    let raw = |single: bool, multi: bool| {
        json!({
            "is_unlock": true,
            "data": [{
                "schedule": {"schedule_id": 3, "name": "Season", "is_valid": true,
                    "start_time": "1793548800", "end_time": "1796198399"},
                "single": {"best": {"difficulty": 5, "second": 112, "icon": "x"}, "challenge": [{"teams": []}], "has_data": single},
                "mp": {"best": {"difficulty": 6, "second": 90}, "challenge": [], "has_data": multi}
            }]
        })
    };
    let solo = summarize(Challenge::StygianOnslaught, "os_usa", &raw(true, true));
    assert_eq!(solo.name.as_deref(), Some("Season"));
    assert_eq!(solo.has_data, Some(true));
    assert_eq!((solo.difficulty, solo.best_seconds), (Some(5), Some(112)));
    assert_eq!(solo.ends_at, at("2026-12-02T07:59:59Z"));
    let multi = summarize(Challenge::StygianOnslaught, "os_usa", &raw(false, true));
    assert_eq!((multi.difficulty, multi.best_seconds), (Some(6), Some(90)));
    let none = summarize(Challenge::StygianOnslaught, "os_usa", &raw(false, false));
    assert_eq!(none.has_data, Some(false));
    assert_eq!((none.difficulty, none.best_seconds), (None, None));
}

#[test]
fn starrail_floor_modes_read_the_matching_group_in_server_time() {
    let raw = json!({
        "schedule_id": 1012, "star_num": 30, "max_floor": "Memory of Chaos (12)",
        "battle_num": 15, "has_data": true,
        "begin_time": {"year": 2026, "month": 10, "day": 27, "hour": 4, "minute": 0},
        "end_time": {"year": 2026, "month": 12, "day": 8, "hour": 3, "minute": 59},
        "all_floor_detail": [{"node_1": {"avatars": [{"id": 1}]}}],
        "groups": [
            {"schedule_id": 1011, "name_mi18n": "Old",
             "begin_time": {"year": 2026, "month": 9, "day": 1, "hour": 4, "minute": 0},
             "end_time": {"year": 2026, "month": 10, "day": 13, "hour": 3, "minute": 59}},
            {"schedule_id": 1012, "name_mi18n": "Current",
             "begin_time": {"year": 2026, "month": 11, "day": 3, "hour": 4, "minute": 0},
             "end_time": {"year": 2026, "month": 12, "day": 15, "hour": 3, "minute": 59}}
        ]
    });
    for challenge in [
        Challenge::MemoryOfChaos,
        Challenge::PureFiction,
        Challenge::ApocalypticShadow,
    ] {
        let summary = summarize(challenge, "prod_official_asia", &raw);
        assert_eq!(summary.mode, challenge.id());
        assert_eq!(summary.name.as_deref(), Some("Current"));
        assert_eq!(summary.stars, Some(30));
        assert_eq!(summary.max_floor.as_deref(), Some("Memory of Chaos (12)"));
        assert_eq!(summary.battles, Some(15));
        assert_eq!(summary.has_data, Some(true));
        // Asia server time is UTC+8.
        assert_eq!(summary.starts_at, at("2026-11-02T20:00:00Z"));
        assert_eq!(summary.ends_at, at("2026-12-14T19:59:00Z"));
    }
    // Without groups the top-level period is used.
    let summary = summarize(
        Challenge::MemoryOfChaos,
        "prod_official_usa",
        &json!({"begin_time": raw["begin_time"], "end_time": raw["end_time"], "has_data": false}),
    );
    assert_eq!(summary.has_data, Some(false));
    assert_eq!(summary.ends_at, at("2026-12-08T08:59:00Z"));
    // An unknown server leaves wall-clock times unknown rather than guessing a zone.
    let unknown = summarize(Challenge::MemoryOfChaos, "test_server", &raw);
    assert_eq!((unknown.starts_at, unknown.ends_at), (None, None));
    assert_eq!(unknown.stars, Some(30));
    // Impossible dates are dropped.
    let invalid = summarize(
        Challenge::PureFiction,
        "prod_official_eur",
        &json!({"end_time": {"year": 2026, "month": 2, "day": 30, "hour": 4, "minute": 0}}),
    );
    assert_eq!(invalid.ends_at, None);
}

#[test]
fn anomaly_arbitration_adds_boss_and_mob_stars_of_the_current_record() {
    let summary = summarize(
        Challenge::AnomalyArbitration,
        "prod_official_eur",
        &json!({
            "challenge_peak_records": [{
                "group": {"group_id": 4, "name_mi18n": "Arbitration",
                    "begin_time": {"year": 2026, "month": 11, "day": 3, "hour": 4, "minute": 0},
                    "end_time": {"year": 2026, "month": 12, "day": 15, "hour": 3, "minute": 59}},
                "boss_info": {"name": "boss"}, "mob_records": [{"avatars": []}],
                "has_challenge_record": true, "battle_num": 4,
                "boss_stars": 2, "mob_stars": 9
            }],
            "role": {"nickname": "private"}
        }),
    );
    assert_eq!(summary.name.as_deref(), Some("Arbitration"));
    assert_eq!(summary.stars, Some(11));
    assert_eq!(summary.battles, Some(4));
    assert_eq!(summary.has_data, Some(true));
    assert_eq!(summary.ends_at, at("2026-12-15T02:59:00Z"));
    assert!(!serde_json::to_string(&summary).unwrap().contains("private"));
    let partial = summarize(
        Challenge::AnomalyArbitration,
        "prod_official_eur",
        &json!({"challenge_peak_records": [{"mob_stars": 3}]}),
    );
    assert_eq!(partial.stars, Some(3));
}

#[test]
fn a_failed_mode_carries_only_its_code() {
    assert_eq!(
        failed(Challenge::PureFiction, "network".into()),
        ModeSummary {
            mode: "pure_fiction".into(),
            error_code: Some("network".into()),
            ..ModeSummary::default()
        }
    );
    // The stored form names every field, so API clients see explicit nulls.
    let stored = serde_json::to_value(failed(Challenge::PureFiction, "network".into())).unwrap();
    for key in [
        "mode",
        "unlocked",
        "has_data",
        "name",
        "starts_at",
        "ends_at",
        "stars",
        "max_floor",
        "max_round",
        "difficulty",
        "best_seconds",
        "battles",
        "error_code",
    ] {
        assert!(stored.get(key).is_some(), "{key}");
    }
}
