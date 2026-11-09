// Synthetic payloads shaped after genshin.py's calendar models, not captured live.
use super::calendar_summary::{CalendarItem, FeaturedItem, official_image, summarize};
use attoshoyodashboard::game::{Game, Genshin, Starrail};
use chrono::{DateTime, Duration, TimeZone, Utc};
use serde_json::json;

fn now() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 11, 9, 3, 0, 0).unwrap()
}

#[test]
fn genshin_banners_and_events_keep_periods_progress_and_featured_items() {
    let now = now();
    let end = now + Duration::days(2);
    let raw = json!({
        "avatar_card_pool_list": [{
            "pool_id": 11, "pool_name": "  Character  Event  Wish ", "version_name": "6.2",
            "start_timestamp": (now - Duration::days(19)).timestamp().to_string(),
            "end_timestamp": end.timestamp().to_string(),
            "countdown_seconds": 1, "pool_status": 1, "jump_url": "https://example.invalid/",
            "avatars": [
                {"id": 1, "name": "Synthetic Five", "rarity": 5, "element": "Hydro",
                 "icon": "https://upload-os-bbs.hoyolab.com/five.png"},
                {"id": 2, "name": "Synthetic Four", "rarity": 4,
                 "icon": "https://attacker.example/four.png"}
            ],
            "weapon": []
        }],
        "weapon_card_pool_list": [{
            "pool_id": 12, "pool_name": "Epitome Invocation",
            // No timestamps: the server-time text is read for the account's server.
            "start_time": "2026-10-21 18:00:00", "end_time": "2026-11-11 17:59:59",
            "avatars": [], "weapon": [{"name": "Synthetic Sword", "rarity": 5, "icon": ""}]
        }],
        "mixed_card_pool_list": [],
        "act_list": [
            // Only the countdown is known.
            {"id": 21, "name": "Double Drops", "countdown_seconds": 3600,
             "is_finished": false, "double_detail": {"total": 3, "left": 1},
             "desc": "private description", "reward_list": [{"item_id": 1}]},
            {"id": 22, "name": "Finished Event", "is_finished": true,
             "end_timestamp": (now - Duration::minutes(1)).timestamp()},
            {"id": 23, "name": "No Period", "is_finished": true},
            // Before its start the countdown may count to the start: not an end.
            {"id": 24, "name": "Announced", "countdown_seconds": 7200,
             "start_timestamp": (now + Duration::hours(1)).timestamp()}
        ],
        "fixed_act_list": [{"id": 31, "name": "Spiral Abyss"}]
    });
    let items = summarize(Genshin::KIND, "os_asia", &raw, now);
    let names: Vec<_> = items.iter().map(|item| item.name.as_deref()).collect();
    // Ended items and the endgame list are dropped; ends first, unknown ends last.
    assert_eq!(
        names,
        [
            Some("Double Drops"),
            Some("Character Event Wish"),
            Some("Epitome Invocation"),
            Some("No Period"),
            Some("Announced")
        ]
    );
    assert_eq!(items[4].starts_at, Some(now + Duration::hours(1)));
    assert_eq!(items[4].ends_at, None);
    let double = &items[0];
    assert_eq!(double.kind, "event");
    assert_eq!(double.ends_at, Some(now + Duration::hours(1)));
    assert_eq!(
        (double.finished, double.progress, double.progress_total),
        (Some(false), Some(2), Some(3))
    );
    let banner = &items[1];
    assert_eq!(banner.kind, "character_banner");
    assert_eq!(banner.id, Some(11));
    assert_eq!(banner.version.as_deref(), Some("6.2"));
    assert_eq!(
        banner.ends_at.map(|at| at.timestamp()),
        Some(end.timestamp())
    );
    assert_eq!(
        banner.featured,
        [
            FeaturedItem {
                name: Some("Synthetic Five".into()),
                rarity: Some(5),
                icon: Some("https://upload-os-bbs.hoyolab.com/five.png".into()),
            },
            // A non-CDN image is dropped; the name stays.
            FeaturedItem {
                name: Some("Synthetic Four".into()),
                rarity: Some(4),
                icon: None,
            }
        ]
    );
    let weapon = &items[2];
    assert_eq!(weapon.kind, "weapon_banner");
    assert_eq!(
        weapon.ends_at,
        Some(Utc.with_ymd_and_hms(2026, 11, 11, 9, 59, 59).unwrap())
    );
    assert_eq!(weapon.featured[0].name.as_deref(), Some("Synthetic Sword"));
    assert_eq!(items[3].ends_at, None);
    // Descriptions and reward lists are not kept.
    let stored = serde_json::to_string(&items).unwrap();
    assert!(!stored.contains("private description") && !stored.contains("item_id"));
}

/// Genshin `act_list` entries carry zero timestamps and their period only in the
/// `start_time`/`end_time` date mappings (genshin.py's `DateTime`), with or without a
/// `tzinfo` offset in hours, while the banners keep their Unix timestamps.
#[test]
fn genshin_events_read_their_period_from_date_mappings() {
    let now = now();
    let banner_end = now + Duration::days(2);
    let mapping = |month: u32, day: u32, hour: u32, minute: u32, second: u32| {
        json!({"year": 2026, "month": month, "day": day, "hour": hour, "minute": minute,
               "second": second})
    };
    let raw = json!({
        "avatar_card_pool_list": [{
            "pool_id": 11, "pool_name": "Synthetic Wish", "version_name": "7.2",
            "start_timestamp": (now - Duration::days(19)).timestamp().to_string(),
            "end_timestamp": banner_end.timestamp().to_string(),
            "start_time": mapping(10, 21, 18, 0, 0), "end_time": mapping(11, 11, 11, 0, 0),
            "countdown_seconds": 172_800, "pool_status": 2, "avatars": [], "weapon": []
        }],
        "weapon_card_pool_list": [],
        "mixed_card_pool_list": [],
        "act_list": [
            // Server time (os_asia, UTC+8) without tzinfo.
            {"id": 1, "name": "Synthetic Record", "type": "ActTypeOther",
             "start_timestamp": "0", "end_timestamp": "0",
             "start_time": mapping(11, 5, 10, 0, 0), "end_time": mapping(12, 7, 3, 59, 59),
             "status": 1, "countdown_seconds": 0, "is_finished": false, "reward_list": []},
            // An explicit offset wins over the server's zone.
            {"id": 2, "name": "Synthetic Leyline", "type": "ActTypeDouble",
             "start_timestamp": "0", "end_timestamp": "0",
             "start_time": {"year": 2026, "month": 11, "day": 1, "hour": 0, "minute": 0,
                            "second": 0, "tzinfo": 9},
             "end_time": {"year": 2026, "month": 12, "day": 15, "hour": 4, "minute": 0,
                          "second": 0, "tzinfo": 9},
             "countdown_seconds": 0, "is_finished": false,
             "double_detail": {"total": 3, "left": 3}},
            // Numeric timestamps still come first.
            {"id": 3, "name": "Synthetic Timestamped",
             "start_timestamp": (now - Duration::days(1)).timestamp(),
             "end_timestamp": (now + Duration::days(5)).timestamp(),
             "start_time": mapping(1, 1, 0, 0, 0), "end_time": mapping(1, 2, 0, 0, 0)},
            // Nothing known at all: the period stays unknown.
            {"id": 4, "name": "Synthetic Undated", "start_timestamp": "0", "end_timestamp": "0",
             "start_time": null, "end_time": null, "countdown_seconds": 0},
            // An announced event that starts after the fetch.
            {"id": 5, "name": "Synthetic Announced", "start_timestamp": "0", "end_timestamp": "0",
             "start_time": mapping(11, 27, 10, 0, 0), "end_time": mapping(12, 7, 3, 59, 59)}
        ]
    });
    let items = summarize(Genshin::KIND, "os_asia", &raw, now);
    let periods: Vec<_> = items
        .iter()
        .map(|item| (item.name.as_deref().unwrap(), item.starts_at, item.ends_at))
        .collect();
    let utc = |month, day, hour, minute, second| {
        Some(
            Utc.with_ymd_and_hms(2026, month, day, hour, minute, second)
                .unwrap(),
        )
    };
    assert_eq!(
        periods,
        [
            (
                "Synthetic Wish",
                Some(now - Duration::days(19)),
                Some(banner_end)
            ),
            (
                "Synthetic Timestamped",
                Some(now - Duration::days(1)),
                Some(now + Duration::days(5))
            ),
            (
                "Synthetic Record",
                utc(11, 5, 2, 0, 0),
                utc(12, 6, 19, 59, 59)
            ),
            (
                "Synthetic Announced",
                utc(11, 27, 2, 0, 0),
                utc(12, 6, 19, 59, 59)
            ),
            (
                "Synthetic Leyline",
                utc(10, 31, 15, 0, 0),
                utc(12, 14, 19, 0, 0)
            ),
            ("Synthetic Undated", None, None),
        ]
    );
    assert_eq!(
        (items[4].progress, items[4].progress_total),
        (Some(0), Some(3))
    );
    // Without tzinfo the mapping follows the account's server: America is UTC-5.
    let america = summarize(Genshin::KIND, "os_usa", &raw, now);
    let record = america.iter().find(|item| item.id == Some(1)).unwrap();
    assert_eq!(record.starts_at, utc(11, 5, 15, 0, 0));
    // With tzinfo it needs no known server.
    let unknown = summarize(Genshin::KIND, "unknown_server", &raw, now);
    let leyline = unknown.iter().find(|item| item.id == Some(2)).unwrap();
    assert_eq!(leyline.ends_at, utc(12, 14, 19, 0, 0));
    assert_eq!(
        unknown
            .iter()
            .find(|item| item.id == Some(1))
            .unwrap()
            .starts_at,
        None
    );
}

#[test]
fn star_rail_periods_fall_back_to_the_wall_clock_fields() {
    let now = now();
    let raw = json!({
        "act_list": [
            {"id": 1, "name": "Mapped", "act_time_type": "ActTimeTypeDefault",
             "time_info": {"start_ts": "0", "end_ts": "0", "now": now.timestamp().to_string(),
                 "start_time": {"year": 2026, "month": 11, "day": 5, "hour": 12, "minute": 0},
                 "end_time": {"year": 2026, "month": 11, "day": 26, "hour": 3, "minute": 59,
                              "second": 59}}},
            {"id": 2, "name": "Text", "act_time_type": "ActTimeTypeDefault",
             "time_info": {"start_time": "2026-11-05 12:00:00", "end_time": "2026-11-26 03:59"}}
        ]
    });
    let items = summarize(Starrail::KIND, "prod_official_asia", &raw, now);
    let start = Some(Utc.with_ymd_and_hms(2026, 11, 5, 4, 0, 0).unwrap());
    assert_eq!(
        items
            .iter()
            .map(|item| (item.id, item.starts_at))
            .collect::<Vec<_>>(),
        [(Some(2), start), (Some(1), start)]
    );
    // Text without seconds ends at the minute, one second before the mapped end.
    assert_eq!(
        items[0].ends_at,
        Some(Utc.with_ymd_and_hms(2026, 11, 25, 19, 59, 0).unwrap())
    );
    assert_eq!(
        items[1].ends_at,
        Some(Utc.with_ymd_and_hms(2026, 11, 25, 19, 59, 59).unwrap())
    );
}

#[test]
fn star_rail_skips_long_running_modes_and_reads_status_and_progress() {
    let now = now();
    let time = |start: DateTime<Utc>, end: DateTime<Utc>| {
        json!({"start_ts": start.timestamp().to_string(), "end_ts": end.timestamp().to_string(),
               "now": now.timestamp().to_string()})
    };
    let raw = json!({
        "avatar_card_pool_list": [],
        "equip_card_pool_list": [{
            "id": 3, "name": "Brilliant Fixation", "version": "3.7",
            "time_info": time(now + Duration::days(1), now + Duration::days(22)),
            "equip_list": [{"item_name": "Synthetic Cone", "rarity": 5,
                "item_url": "https://act-webstatic.hoyoverse.com/cone.png",
                "icon_url": "https://act-webstatic.hoyoverse.com/ignored.png"}]
        }],
        "act_list": [
            {"id": 1, "name": "Simulated\\nUniverse", "act_time_type": "ActTimeTypeLong",
             "time_info": time(now - Duration::days(1), now + Duration::days(1))},
            {"id": 2, "name": "Garden\\nof Plenty", "act_time_type": "ActTimeTypeDefault",
             "act_status": "DoubleActStatusUnopened", "all_finished": false,
             "current_progress": 9, "total_progress": 3,
             "time_info": time(now - Duration::days(1), now + Duration::days(5))},
            {"id": 3, "name": "Zero Times", "act_status": "SignActStatusUnclaimed",
             "current_progress": 0, "total_progress": 0,
             "time_info": {"start_ts": "0", "end_ts": "0", "start_time": ""}}
        ],
        "challenge_list": [{"group_id": 1, "name_mi18n": "Memory of Chaos"}],
        "cur_game_version": "3.7"
    });
    let items = summarize(Starrail::KIND, "prod_official_asia", &raw, now);
    assert_eq!(items.len(), 3);
    assert_eq!(
        items[0],
        CalendarItem {
            kind: "event".into(),
            id: Some(2),
            name: Some("Garden of Plenty".into()),
            starts_at: Some(now - Duration::days(1)),
            ends_at: Some(now + Duration::days(5)),
            finished: Some(false),
            locked: Some(true),
            progress: Some(3),
            progress_total: Some(3),
            ..CalendarItem::default()
        }
    );
    // An announced banner keeps its future start.
    let cone = &items[1];
    assert_eq!(cone.kind, "light_cone_banner");
    assert_eq!(cone.starts_at, Some(now + Duration::days(1)));
    assert_eq!(
        cone.featured[0].icon.as_deref(),
        Some("https://act-webstatic.hoyoverse.com/cone.png")
    );
    // No period and no total: unknown, not zero.
    let zero = &items[2];
    assert_eq!((zero.starts_at, zero.ends_at), (None, None));
    assert_eq!((zero.progress, zero.progress_total), (None, None));
    assert_eq!(zero.locked, Some(false));
}

#[test]
fn unexpected_shapes_yield_unknowns_and_bounded_output() {
    let now = now();
    for raw in [
        json!(null),
        json!([]),
        json!({"act_list": "x", "avatar_card_pool_list": {"a": 1}}),
    ] {
        assert!(summarize(Genshin::KIND, "os_asia", &raw, now).is_empty());
        assert!(summarize(Starrail::KIND, "os_asia", &raw, now).is_empty());
    }
    let item = summarize(
        Genshin::KIND,
        "unknown_server",
        &json!({"act_list": [{"name": 5, "is_finished": "yes", "start_time": "2026-11-01 00:00:00",
                              "end_timestamp": "soon"}]}),
        now,
    );
    assert_eq!(
        item,
        [CalendarItem {
            kind: "event".into(),
            ..CalendarItem::default()
        }]
    );
    let many: Vec<_> = (0..100)
        .map(|id| json!({"id": id, "name": "x".repeat(500), "countdown_seconds": 60 + id}))
        .collect();
    let items = summarize(Genshin::KIND, "os_asia", &json!({"act_list": many}), now);
    assert_eq!(items.len(), 60);
    assert_eq!(items[0].name.as_ref().unwrap().chars().count(), 120);
}

#[test]
fn only_https_urls_on_the_csp_image_hosts_are_kept() {
    for url in [
        "https://upload-os-bbs.hoyolab.com/a.png",
        "https://act-upload.hoyolab.com/a.png?x=1",
        "https://act-webstatic.hoyoverse.com/a.png",
        "https://fastcdn.hoyoverse.com/a.png",
        "https://Upload-OS-BBS.hoyolab.com/a.png",
    ] {
        assert_eq!(official_image(url).as_deref(), Some(url));
    }
    for url in [
        "http://upload-os-bbs.hoyolab.com/a.png",
        "https://upload-os-bbs.hoyolab.com.example/a.png",
        "https://user@upload-os-bbs.hoyolab.com/a.png",
        "https://upload-os-bbs.hoyolab.com:444/a.png",
        "https://upload-os-bbs.hoyolab.com/a b.png",
        "//upload-os-bbs.hoyolab.com/a.png",
        "",
    ] {
        assert_eq!(official_image(url), None, "{url}");
    }
}
