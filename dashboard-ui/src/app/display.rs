pub(super) fn game_name(kind: &str) -> &str {
    match kind {
        "genshin" => "Genshin Impact",
        "starrail" => "Honkai: Star Rail",
        other => other,
    }
}

pub(super) fn humanize(value: &str) -> String {
    value.replace(['_', '-'], " ")
}

pub(super) fn display_date(value: &Option<String>) -> String {
    value.as_deref().unwrap_or("Not reported").to_owned()
}

pub(super) fn count_pair(current: Option<i64>, max: Option<i64>) -> String {
    match (current, max) {
        (Some(current), Some(max)) => format!("{current}/{max}"),
        (Some(current), None) => current.to_string(),
        _ => "Not reported".to_owned(),
    }
}

pub(super) fn bool_status(value: Option<bool>) -> &'static str {
    match value {
        Some(true) => "Received",
        Some(false) => "Not received",
        None => "Not fetched",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_and_known_values_keep_their_display_labels() {
        assert_eq!(game_name("genshin"), "Genshin Impact");
        assert_eq!(game_name("other"), "other");
        assert_eq!(humanize("not_available-yet"), "not available yet");
        assert_eq!(display_date(&None), "Not reported");
        assert_eq!(count_pair(Some(2), Some(5)), "2/5");
        assert_eq!(count_pair(Some(2), None), "2");
        assert_eq!(count_pair(None, Some(5)), "Not reported");
        assert_eq!(bool_status(None), "Not fetched");
        assert_eq!(bool_status(Some(false)), "Not received");
        assert_eq!(bool_status(Some(true)), "Received");
    }
}
