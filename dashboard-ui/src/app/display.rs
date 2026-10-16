pub(super) fn count_pair(current: i64, max: Option<i64>) -> String {
    match max {
        Some(max) => format!("{current}/{max}"),
        None => current.to_string(),
    }
}

/// Format API timestamps in the selected language and the browser's local timezone.
/// Include the zone so estimates cannot be mistaken for server-local times.
pub(super) fn localized_datetime(
    value: Option<&str>,
    ui: super::preferences::Preferences,
) -> Option<String> {
    let value = value.filter(|value| !value.trim().is_empty())?;
    #[cfg(target_arch = "wasm32")]
    {
        let date = js_sys::Date::new(&wasm_bindgen::JsValue::from_str(value));
        if !date.get_time().is_finite() {
            return None;
        }
        let options = js_sys::JSON::parse(
            r#"{"year":"numeric","month":"short","day":"numeric","hour":"2-digit","minute":"2-digit","timeZoneName":"short"}"#,
        ).ok()?;
        Some(date.to_locale_string(ui.language_code(), &options).into())
    }
    // The dashboard is browser-only; native tests must not invoke JS bindings.
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = (value, ui);
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn absent_datetimes_do_not_invent_estimates() {
        let ui = super::super::preferences::Preferences::default();
        assert_eq!(localized_datetime(None, ui), None);
        assert_eq!(localized_datetime(Some(""), ui), None);
        assert_eq!(localized_datetime(Some("  "), ui), None);
    }

    #[test]
    fn known_values_do_not_invent_a_missing_maximum() {
        assert_eq!(count_pair(2, Some(5)), "2/5");
        assert_eq!(count_pair(2, None), "2");
        assert_eq!(count_pair(0, None), "0");
    }
}
