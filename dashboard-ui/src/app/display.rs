pub(super) fn count_pair(current: i64, max: Option<i64>) -> String {
    match max {
        Some(max) => format!("{current}/{max}"),
        None => current.to_string(),
    }
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
    fn known_values_do_not_invent_a_missing_maximum() {
        assert_eq!(count_pair(2, Some(5)), "2/5");
        assert_eq!(count_pair(2, None), "2");
        assert_eq!(count_pair(0, None), "0");
    }
}
