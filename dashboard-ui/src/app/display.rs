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

#[cfg(test)]
mod tests {
    use super::*;

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
