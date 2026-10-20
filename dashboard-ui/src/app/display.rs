use super::preferences::Preferences;

pub(super) fn count_pair(current: i64, max: Option<i64>) -> String {
    match max {
        Some(max) => format!("{current}/{max}"),
        None => current.to_string(),
    }
}

/// Remaining seconds as hours and minutes, rounding up so unfinished work never shows zero.
pub(super) fn hours_minutes(seconds: i64, ui: Preferences) -> String {
    let minutes = seconds.max(0).unsigned_abs().div_ceil(60);
    ui.format(
        "dashboard.hours_minutes",
        &[
            ("hours", &(minutes / 60).to_string()),
            ("minutes", &format!("{:02}", minutes % 60)),
        ],
    )
}

/// Remaining seconds as days and hours when a day or more is left, else hours and minutes.
pub(super) fn days_hours(seconds: i64, ui: Preferences) -> String {
    let hours = seconds.max(0).unsigned_abs().div_ceil(3600);
    if hours < 24 {
        return hours_minutes(seconds, ui);
    }
    ui.format(
        "dashboard.days_hours",
        &[
            ("days", &(hours / 24).to_string()),
            ("hours", &(hours % 24).to_string()),
        ],
    )
}

/// Short relative age such as "3 min ago", for always-visible freshness labels.
pub(super) fn relative_age(seconds: i64, ui: Preferences) -> String {
    let minutes = seconds.max(0) / 60;
    let (key, count) = match minutes {
        0 => return ui.text("time.just_now").to_owned(),
        1..60 => ("time.minutes_ago", minutes),
        60..2880 => ("time.hours_ago", minutes / 60),
        _ => ("time.days_ago", minutes / 1440),
    };
    ui.format(key, &[("count", &count.to_string())])
}

/// How close a resource is to its cap: "full" at or over it, "near" from 90%.
pub(super) fn fill_level(current: Option<i64>, maximum: Option<i64>) -> &'static str {
    match (current, maximum) {
        (Some(current), Some(maximum)) if maximum > 0 && current >= maximum => "full",
        (Some(current), Some(maximum)) if maximum > 0 && current * 10 >= maximum * 9 => "near",
        _ => "",
    }
}

/// The browser's UTC offset in minutes east, e.g. 540 for JST.
pub(super) fn browser_offset_minutes() -> Option<i32> {
    #[cfg(target_arch = "wasm32")]
    {
        Some(-(js_sys::Date::new_0().get_timezone_offset() as i32))
    }
    #[cfg(not(target_arch = "wasm32"))]
    None
}

/// Minutes east of UTC for a server offset such as "+09:00".
pub(super) fn offset_minutes(offset: &str) -> Option<i32> {
    let (sign, rest) = match offset.as_bytes().first()? {
        b'+' => (1, &offset[1..]),
        b'-' => (-1, &offset[1..]),
        _ => return None,
    };
    let (hours, minutes) = rest.split_once(':')?;
    if hours.len() != 2 || minutes.len() != 2 {
        return None;
    }
    let hours: i32 = hours.parse().ok()?;
    let minutes: i32 = minutes.parse().ok()?;
    (hours <= 18 && minutes < 60).then_some(sign * (hours * 60 + minutes))
}

/// An "HH:MM" clock moved from one UTC offset to another, wrapping at midnight.
pub(super) fn shift_clock(clock: &str, from_minutes: i32, to_minutes: i32) -> Option<String> {
    let (hour, minute) = clock.split_once(':')?;
    let total = hour.parse::<i32>().ok()? * 60 + minute.parse::<i32>().ok()?;
    let shifted = (total - from_minutes + to_minutes).rem_euclid(24 * 60);
    Some(format!("{:02}:{:02}", shifted / 60, shifted % 60))
}

/// Compact month/day and 24-hour browser-local time, with an explicit timezone.
pub(super) fn compact_datetime(value: Option<&str>) -> Option<String> {
    let value = value.filter(|value| !value.trim().is_empty())?;
    #[cfg(target_arch = "wasm32")]
    {
        let date = js_sys::Date::new(&wasm_bindgen::JsValue::from_str(value));
        if !date.get_time().is_finite() {
            return None;
        }
        let options = js_sys::JSON::parse(
            r#"{"hour":"2-digit","minute":"2-digit","hourCycle":"h23","timeZoneName":"short"}"#,
        )
        .ok()?;
        // ja-JP gives the familiar JST label in Japan; other browser zones remain explicit.
        let clock = date.to_locale_string("ja-JP", &options);
        Some(format!(
            "{:02}/{:02} {}",
            date.get_month() + 1,
            date.get_date(),
            String::from(clock)
        ))
    }
    // The dashboard is browser-only; native tests must not invoke JS bindings.
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = value;
        None
    }
}

/// Compact local datetime for display, or the shared "not fetched" placeholder.
/// Absent and unparseable values never fall back to the raw ISO string.
pub(super) fn datetime_text(value: Option<&str>, ui: Preferences) -> String {
    compact_datetime(value).unwrap_or_else(|| ui.text("common.not_fetched").to_owned())
}

/// How old cached data is, in the largest whole unit (minutes, hours or days).
pub(super) fn data_age(seconds: i64, ui: Preferences) -> String {
    let minutes = (seconds.max(0) / 60).max(1);
    let (key, count) = match minutes {
        ..60 => ("dashboard.data_age_minutes", minutes),
        60..2880 => ("dashboard.data_age_hours", minutes / 60),
        _ => ("dashboard.data_age_days", minutes / 1440),
    };
    ui.format(key, &[("count", &count.to_string())])
}

/// Seconds elapsed since an ISO timestamp, by the browser clock.
pub(super) fn seconds_since(value: Option<&str>) -> Option<i64> {
    let value = value.filter(|value| !value.trim().is_empty())?;
    #[cfg(target_arch = "wasm32")]
    {
        let then = js_sys::Date::new(&wasm_bindgen::JsValue::from_str(value)).get_time();
        then.is_finite()
            .then(|| ((js_sys::Date::now() - then) / 1000.0) as i64)
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = value;
        None
    }
}

/// Catalog key for a backend `error_code` (see `hoyolab::gateway::error_code`).
/// Unknown codes get a generic message; raw codes stay in technical details only.
pub(super) fn error_message_key(code: &str) -> &'static str {
    match code {
        "network" => "errors.network",
        "upstream_data" => "errors.upstream_data",
        "action_required" => "errors.action_required",
        // Mirrors `gateway::cookie_invalid` and the private-record code.
        "upstream:-100" | "upstream:10001" | "upstream:-1071" => "errors.cookie_invalid",
        "upstream:10102" => "errors.record_private",
        "http:429" => "errors.rate_limited",
        _ if code.starts_with("http:5") => "errors.upstream_unavailable",
        _ if code.starts_with("http:") || code.starts_with("upstream:") => {
            "errors.upstream_rejected"
        }
        _ => "errors.unknown",
    }
}

/// Localized error text, or "None" when the backend reported no error.
pub(super) fn error_text(code: Option<&str>, ui: Preferences) -> &'static str {
    code.map_or(ui.text("dashboard.none"), |code| {
        ui.text(error_message_key(code))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durations_ages_and_fill_levels_read_naturally() {
        let ui = Preferences::default();
        assert_eq!(days_hours(3 * 3600, ui), "3時間00分");
        assert_eq!(days_hours(26 * 3600 + 1, ui), "1日3時間");
        assert_eq!(relative_age(30, ui), "たった今");
        assert_eq!(relative_age(5 * 60, ui), "5分前");
        assert_eq!(relative_age(3 * 3600, ui), "3時間前");
        assert_eq!(relative_age(3 * 86400, ui), "3日前");
        assert_eq!(fill_level(Some(200), Some(200)), "full");
        assert_eq!(fill_level(Some(180), Some(200)), "near");
        assert_eq!(fill_level(Some(179), Some(200)), "");
        assert_eq!(fill_level(Some(5), None), "");
        assert_eq!(fill_level(Some(0), Some(0)), "");
    }

    #[test]
    fn server_clock_converts_between_offsets() {
        assert_eq!(offset_minutes("+09:00"), Some(540));
        assert_eq!(offset_minutes("-05:30"), Some(-330));
        assert_eq!(offset_minutes("09:00"), None);
        assert_eq!(offset_minutes("+9:00"), None);
        assert_eq!(shift_clock("06:00", 540, 540).as_deref(), Some("06:00"));
        assert_eq!(shift_clock("06:00", 540, 60).as_deref(), Some("22:00"));
        assert_eq!(shift_clock("23:30", 540, 600).as_deref(), Some("00:30"));
        // The reward day starts at 00:00 UTC+8, which is 01:00 at UTC+9.
        assert_eq!(shift_clock("00:00", 480, 540).as_deref(), Some("01:00"));
    }

    #[test]
    fn backend_error_codes_map_to_messages_without_echoing_the_code() {
        for (code, key) in [
            ("network", "errors.network"),
            ("upstream_data", "errors.upstream_data"),
            ("action_required", "errors.action_required"),
            ("upstream:-100", "errors.cookie_invalid"),
            ("upstream:10001", "errors.cookie_invalid"),
            ("upstream:-1071", "errors.cookie_invalid"),
            ("upstream:10102", "errors.record_private"),
            ("http:429", "errors.rate_limited"),
            ("http:503", "errors.upstream_unavailable"),
            ("http:404", "errors.upstream_rejected"),
            ("upstream:-1", "errors.upstream_rejected"),
            ("upstream_unavailable", "errors.unknown"),
            ("", "errors.unknown"),
        ] {
            assert_eq!(error_message_key(code), key, "{code}");
        }
        let ui = Preferences::default();
        assert_eq!(error_text(None, ui), ui.text("dashboard.none"));
        let unknown = error_text(Some("synthetic_code"), ui);
        assert!(!unknown.contains("synthetic_code"));
        assert_eq!(unknown, ui.text("errors.unknown"));
    }

    #[test]
    fn data_age_uses_the_largest_whole_unit() {
        let ui = Preferences::default();
        assert_eq!(data_age(0, ui), "1分前のデータです");
        assert_eq!(data_age(-30, ui), "1分前のデータです");
        assert_eq!(data_age(59 * 60 + 59, ui), "59分前のデータです");
        assert_eq!(data_age(60 * 60, ui), "1時間前のデータです");
        assert_eq!(data_age(6 * 3600 + 1800, ui), "6時間前のデータです");
        assert_eq!(data_age(47 * 3600, ui), "47時間前のデータです");
        assert_eq!(data_age(48 * 3600, ui), "2日前のデータです");
        let english = Preferences {
            language: crate::i18n::Language::English,
            ..ui
        };
        assert_eq!(data_age(5 * 60, english), "Data from 5 min ago");
        assert_eq!(data_age(3 * 86400, english), "Data from 3 days ago");
        assert_eq!(seconds_since(None), None);
    }

    #[test]
    fn unparseable_datetimes_use_the_placeholder_not_the_raw_value() {
        let ui = Preferences::default();
        assert_eq!(datetime_text(None, ui), "未取得");
        // Native tests have no JS Date; the browser check covers real formatting.
        assert!(!datetime_text(Some("2026-10-17T12:38:00Z"), ui).contains('T'));
    }

    #[test]
    fn absent_datetimes_do_not_invent_estimates() {
        assert_eq!(compact_datetime(None), None);
        assert_eq!(compact_datetime(Some("")), None);
        assert_eq!(compact_datetime(Some("  ")), None);
    }

    #[test]
    fn remaining_seconds_render_as_hours_and_minutes() {
        let ui = Preferences::default();
        assert_eq!(hours_minutes(0, ui), "0時間00分");
        assert_eq!(hours_minutes(-5, ui), "0時間00分");
        assert_eq!(hours_minutes(1, ui), "0時間01分");
        assert_eq!(hours_minutes(509, ui), "0時間09分");
        assert_eq!(hours_minutes(72_000, ui), "20時間00分");
        assert_eq!(hours_minutes(3_723, ui), "1時間03分");
        let english = Preferences {
            language: crate::i18n::Language::English,
            ..ui
        };
        assert_eq!(hours_minutes(3_723, english), "1h 03m");
    }

    #[test]
    fn known_values_do_not_invent_a_missing_maximum() {
        assert_eq!(count_pair(2, Some(5)), "2/5");
        assert_eq!(count_pair(2, None), "2");
        assert_eq!(count_pair(0, None), "0");
    }
}
