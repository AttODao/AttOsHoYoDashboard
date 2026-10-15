pub(super) fn count_pair(current: Option<i64>, max: Option<i64>) -> String {
    match (current, max) {
        (Some(current), Some(max)) => format!("{current}/{max}"),
        (Some(current), None) => current.to_string(),
        _ => "Not reported".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_and_known_values_keep_their_display_labels() {
        assert_eq!(count_pair(Some(2), Some(5)), "2/5");
        assert_eq!(count_pair(Some(2), None), "2");
        assert_eq!(count_pair(None, Some(5)), "Not reported");
    }
}
