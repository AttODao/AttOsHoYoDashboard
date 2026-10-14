//! Fixed-offset schedules, never a substitute for upstream reward eligibility.
use crate::dashboard::error::{Error, Result};
use chrono::{DateTime, FixedOffset, NaiveTime, TimeZone, Utc};
pub(in crate::dashboard) fn next_due(
    now: DateTime<Utc>,
    time: &str,
    offset: FixedOffset,
) -> Result<DateTime<Utc>> {
    if time.len() != 5
        || time.as_bytes()[2] != b':'
        || !time
            .bytes()
            .enumerate()
            .all(|(i, b)| i == 2 || b.is_ascii_digit())
    {
        return Err(Error::Invalid("time must be HH:MM"));
    }
    let clock = NaiveTime::parse_from_str(time, "%H:%M")
        .map_err(|_| Error::Invalid("time must be HH:MM"))?;
    let date = now.with_timezone(&offset).date_naive();
    let candidate = offset
        .from_local_datetime(&date.and_time(clock))
        .single()
        .ok_or(Error::Internal)?
        .with_timezone(&Utc);
    if candidate > now {
        Ok(candidate)
    } else {
        let tomorrow = date.succ_opt().ok_or(Error::Internal)?;
        Ok(offset
            .from_local_datetime(&tomorrow.and_time(clock))
            .single()
            .ok_or(Error::Internal)?
            .with_timezone(&Utc))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn strictly_next_fixed_offset_tick() {
        let offset = FixedOffset::east_opt(9 * 3600).unwrap();
        let now = DateTime::parse_from_rfc3339("2026-01-01T20:59:00Z")
            .unwrap()
            .with_timezone(&Utc);
        assert_eq!(
            next_due(now, "06:00", offset).unwrap().to_rfc3339(),
            "2026-01-01T21:00:00+00:00"
        );
        assert_eq!(
            next_due(now + chrono::Duration::minutes(1), "06:00", offset)
                .unwrap()
                .to_rfc3339(),
            "2026-01-02T21:00:00+00:00"
        );
        for bad in ["6:00", "24:00", "00:60", "06:00:00", "-1:00", "０６:００"] {
            assert!(next_due(now, bad, offset).is_err());
        }
    }
}
