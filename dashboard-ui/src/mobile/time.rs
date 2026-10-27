//! Device-local time for the app; the browser build uses JavaScript `Date`.
use chrono::{DateTime, Local, Utc};

/// The device's UTC offset in minutes east, e.g. 540 for JST.
pub(crate) fn offset_minutes() -> i32 {
    Local::now().offset().local_minus_utc() / 60
}

/// "MM/DD HH:MM" in device time, with the zone label the browser's ja-JP format gives.
pub(crate) fn compact_datetime(value: &str) -> Option<String> {
    let local = DateTime::parse_from_rfc3339(value.trim())
        .ok()?
        .with_timezone(&Local);
    let offset = local.offset().local_minus_utc() / 60;
    Some(format!(
        "{} {}",
        local.format("%m/%d %H:%M"),
        zone_label(offset)
    ))
}

fn zone_label(offset_minutes: i32) -> String {
    if offset_minutes == 9 * 60 {
        return "JST".to_owned();
    }
    let sign = if offset_minutes < 0 { '-' } else { '+' };
    let (hours, minutes) = (offset_minutes.abs() / 60, offset_minutes.abs() % 60);
    match (hours, minutes) {
        (0, 0) => "GMT".to_owned(),
        (_, 0) => format!("GMT{sign}{hours}"),
        _ => format!("GMT{sign}{hours}:{minutes:02}"),
    }
}

/// Whole seconds since an RFC 3339 timestamp by the device clock.
pub(crate) fn seconds_since(value: &str) -> Option<i64> {
    let then = DateTime::parse_from_rfc3339(value.trim()).ok()?;
    Some((Utc::now() - then.with_timezone(&Utc)).num_seconds())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zone_labels_follow_the_browser_short_names() {
        assert_eq!(zone_label(540), "JST");
        assert_eq!(zone_label(0), "GMT");
        assert_eq!(zone_label(60), "GMT+1");
        assert_eq!(zone_label(-300), "GMT-5");
        assert_eq!(zone_label(330), "GMT+5:30");
    }

    #[test]
    fn timestamps_parse_and_invalid_ones_are_absent() {
        assert!(compact_datetime("2026-10-27T07:16:00Z").is_some());
        assert!(compact_datetime("2026-10-27T07:16:00.123+09:00").is_some());
        assert!(compact_datetime("yesterday").is_none());
        assert!(seconds_since("2000-01-01T00:00:00Z").is_some_and(|s| s > 0));
    }
}
