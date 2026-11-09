use super::upstream_json::{moment, server_offset, wall_clock, wall_clock_text};
use chrono::{DateTime, TimeZone, Utc};
use serde_json::{Value, json};

fn utc(day: u32, hour: u32, minute: u32, second: u32) -> Option<DateTime<Utc>> {
    Some(
        Utc.with_ymd_and_hms(2026, 11, day, hour, minute, second)
            .unwrap(),
    )
}

#[test]
fn moments_are_read_from_unix_seconds_date_mappings_and_text() {
    let asia = server_offset("os_asia");
    let read = |value: Value| moment(Some(&value), asia);
    // Unix seconds as a number or as text; zero is unknown, not 1970.
    assert_eq!(read(json!(1_794_225_600)), utc(9, 12, 0, 0));
    assert_eq!(read(json!(" 1794225600 ")), utc(9, 12, 0, 0));
    for unknown in [
        json!(0),
        json!("0"),
        json!(-5),
        json!(null),
        json!(""),
        json!({}),
    ] {
        assert_eq!(read(unknown.clone()), None, "{unknown}");
    }
    // A date mapping in the server's zone, with an optional second.
    let mapped = json!({"year": 2026, "month": 11, "day": 27, "hour": 10, "minute": 0});
    assert_eq!(read(mapped.clone()), utc(27, 2, 0, 0));
    // An explicit tzinfo (hours, possibly fractional or as text) wins.
    let mut offset = mapped.clone();
    offset["tzinfo"] = json!(-5);
    assert_eq!(read(offset.clone()), utc(27, 15, 0, 0));
    offset["tzinfo"] = json!(5.5);
    assert_eq!(read(offset.clone()), utc(27, 4, 30, 0));
    offset["tzinfo"] = json!("9");
    assert_eq!(read(offset.clone()), utc(27, 1, 0, 0));
    offset["tzinfo"] = json!(null);
    assert_eq!(read(offset.clone()), utc(27, 2, 0, 0));
    // A tzinfo that is not an offset leaves the moment unknown instead of guessing.
    for invalid in [json!("Asia/Shanghai"), json!(30), json!([8])] {
        offset["tzinfo"] = invalid;
        assert_eq!(read(offset.clone()), None);
    }
    // Server-time text, with or without seconds.
    assert_eq!(read(json!("2026-11-27 10:00:30")), utc(27, 2, 0, 30));
    assert_eq!(read(json!("2026-11-27 10:00")), utc(27, 2, 0, 0));
    assert_eq!(read(json!("27/11/2026")), None);
}

#[test]
fn wall_clock_forms_need_a_known_zone_unless_they_carry_one() {
    let mapped = json!({"year": 2026, "month": 11, "day": 27, "hour": 10, "minute": 0});
    assert_eq!(wall_clock(Some(&mapped), None), None);
    assert_eq!(
        wall_clock_text(Some(&json!("2026-11-27 10:00:00")), None),
        None
    );
    let offset = json!({"year": 2026, "month": 11, "day": 27, "hour": 10, "minute": 0,
                        "second": 0, "tzinfo": 8});
    assert_eq!(wall_clock(Some(&offset), None), utc(27, 2, 0, 0));
    // Impossible dates and missing parts stay unknown.
    for broken in [
        json!({"year": 2026, "month": 2, "day": 30, "hour": 0, "minute": 0}),
        json!({"year": 0, "month": 1, "day": 1, "hour": 0, "minute": 0}),
        json!({"year": 2026, "month": 1, "day": 1, "hour": 0}),
    ] {
        assert_eq!(wall_clock(Some(&broken), server_offset("os_asia")), None);
    }
}
