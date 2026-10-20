//! Reward-day scope of the durable "POST may have happened" guard.
use crate::dashboard::db::entities::reward_guard;
use chrono::{DateTime, FixedOffset, NaiveDate, Utc};

/// HoYoLAB overseas daily check-in resets at 00:00 UTC+8, independent of the schedule offset.
const REWARD_DAY_OFFSET_SECONDS: i32 = 8 * 3600;

pub(in crate::dashboard::rewards) fn reward_day(at: DateTime<Utc>) -> NaiveDate {
    let offset = FixedOffset::east_opt(REWARD_DAY_OFFSET_SECONDS).expect("fixed valid offset");
    at.with_timezone(&offset).date_naive()
}

/// An uncertain POST blocks further POSTs only within the reward day it was recorded in.
pub(super) fn post_unresolved(guard: Option<&reward_guard::Model>, now: DateTime<Utc>) -> bool {
    guard.is_some_and(|guard| {
        guard.post_may_have_happened && reward_day(guard.updated_at) >= reward_day(now)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn guard(may_have: bool, updated_at: &str) -> reward_guard::Model {
        reward_guard::Model {
            setting_id: Uuid::nil(),
            post_may_have_happened: may_have,
            updated_at: DateTime::parse_from_rfc3339(updated_at)
                .unwrap()
                .with_timezone(&Utc),
        }
    }

    fn at(value: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(value)
            .unwrap()
            .with_timezone(&Utc)
    }

    #[test]
    fn guard_is_scoped_to_the_utc_plus_8_reward_day() {
        // 15:59:59Z is 23:59:59 UTC+8; 16:00:00Z starts the next reward day.
        let late = guard(true, "2026-01-01T15:59:59Z");
        assert!(post_unresolved(Some(&late), at("2026-01-01T15:59:59Z")));
        assert!(!post_unresolved(Some(&late), at("2026-01-01T16:00:00Z")));
        let early = guard(true, "2026-01-01T16:00:00Z");
        assert!(post_unresolved(Some(&early), at("2026-01-02T15:59:59Z")));
        assert!(!post_unresolved(
            Some(&guard(false, "2026-01-01T16:00:00Z")),
            at("2026-01-01T16:00:01Z")
        ));
        assert!(!post_unresolved(None, at("2026-01-01T16:00:00Z")));
    }
}
