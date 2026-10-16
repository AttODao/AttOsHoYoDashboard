pub(super) fn count_pair(current: i64, max: Option<i64>) -> String {
    match max {
        Some(max) => format!("{current}/{max}"),
        None => current.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_values_do_not_invent_a_missing_maximum() {
        assert_eq!(count_pair(2, Some(5)), "2/5");
        assert_eq!(count_pair(2, None), "2");
        assert_eq!(count_pair(0, None), "0");
    }
}
