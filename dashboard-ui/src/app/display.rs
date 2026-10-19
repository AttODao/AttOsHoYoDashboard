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
