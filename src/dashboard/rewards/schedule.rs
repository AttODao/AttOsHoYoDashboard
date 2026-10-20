//! Local-clock schedules in a fixed offset or an IANA zone, never a substitute for
//! upstream reward eligibility.
use crate::dashboard::error::{Error, Result};
use chrono::{
    DateTime, Duration, FixedOffset, LocalResult, NaiveDateTime, NaiveTime, Offset, TimeZone, Utc,
};
use chrono_tz::Tz;
use std::fmt;

/// The server's schedule clock: `TIMEZONE` (IANA, with DST) or `TIMEZONE_OFFSET`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(in crate::dashboard) enum ScheduleZone {
    Fixed(FixedOffset),
    Named(Tz),
}

impl ScheduleZone {
    pub(in crate::dashboard) fn parse(name: Option<&str>, offset: &str) -> Result<Self> {
        match name.filter(|name| !name.trim().is_empty()) {
            Some(name) => name
                .parse::<Tz>()
                .map(Self::Named)
                .map_err(|_| Error::Invalid("TIMEZONE must be an IANA time zone name")),
            None => offset
                .parse()
                .map(Self::Fixed)
                .map_err(|_| Error::Invalid("TIMEZONE_OFFSET must be a fixed UTC offset")),
        }
    }

    /// The UTC offset in effect at an instant, for display.
    pub(in crate::dashboard) fn offset_at(self, at: DateTime<Utc>) -> FixedOffset {
        match self {
            Self::Fixed(offset) => offset,
            Self::Named(zone) => zone.offset_from_utc_datetime(&at.naive_utc()).fix(),
        }
    }

    /// The zone name, when it is a named zone.
    pub(in crate::dashboard) fn name(self) -> Option<&'static str> {
        match self {
            Self::Fixed(_) => None,
            Self::Named(zone) => Some(zone.name()),
        }
    }

    fn local_date(self, at: DateTime<Utc>) -> chrono::NaiveDate {
        at.with_timezone(&self.offset_at(at)).date_naive()
    }

    /// A local wall-clock time as an instant. A time skipped by a DST gap moves
    /// to the first minute after the gap; a repeated time uses its first occurrence.
    fn instant(self, local: NaiveDateTime) -> Option<DateTime<Utc>> {
        let resolve = |local: NaiveDateTime| match self {
            Self::Fixed(offset) => offset
                .from_local_datetime(&local)
                .single()
                .map(|at| at.with_timezone(&Utc)),
            Self::Named(zone) => match zone.from_local_datetime(&local) {
                LocalResult::Single(at) => Some(at.with_timezone(&Utc)),
                LocalResult::Ambiguous(first, _) => Some(first.with_timezone(&Utc)),
                LocalResult::None => None,
            },
        };
        (0..=180).find_map(|minutes| resolve(local + Duration::minutes(minutes)))
    }
}

impl fmt::Display for ScheduleZone {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Fixed(offset) => write!(f, "{offset}"),
            Self::Named(zone) => f.write_str(zone.name()),
        }
    }
}

impl From<FixedOffset> for ScheduleZone {
    fn from(offset: FixedOffset) -> Self {
        Self::Fixed(offset)
    }
}

pub(in crate::dashboard) fn next_due(
    now: DateTime<Utc>,
    time: &str,
    zone: impl Into<ScheduleZone>,
) -> Result<DateTime<Utc>> {
    let zone = zone.into();
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
    let mut date = zone.local_date(now);
    // Two days always reach a strictly later tick, even across a DST change.
    for _ in 0..3 {
        let candidate = zone.instant(date.and_time(clock)).ok_or(Error::Internal)?;
        if candidate > now {
            return Ok(candidate);
        }
        date = date.succ_opt().ok_or(Error::Internal)?;
    }
    Err(Error::Internal)
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

    fn at(value: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(value)
            .unwrap()
            .with_timezone(&Utc)
    }

    #[test]
    fn named_zones_follow_daylight_saving_and_its_gaps() {
        let zone = ScheduleZone::parse(Some("America/New_York"), "+09:00").unwrap();
        assert_eq!(zone.to_string(), "America/New_York");
        // EST (UTC-5) before the March change, EDT (UTC-4) after it.
        assert_eq!(
            next_due(at("2026-03-07T12:00:00Z"), "06:00", zone).unwrap(),
            at("2026-03-08T10:00:00Z")
        );
        assert_eq!(
            next_due(at("2026-03-08T12:00:00Z"), "06:00", zone).unwrap(),
            at("2026-03-09T10:00:00Z")
        );
        // 02:30 does not exist on 2026-03-08; it runs at the first minute after the gap.
        assert_eq!(
            next_due(at("2026-03-08T05:00:00Z"), "02:30", zone).unwrap(),
            at("2026-03-08T07:00:00Z")
        );
        // 01:30 happens twice on 2026-11-01; the first occurrence (EDT) is used.
        assert_eq!(
            next_due(at("2026-11-01T04:00:00Z"), "01:30", zone).unwrap(),
            at("2026-11-01T05:30:00Z")
        );
        assert_eq!(
            zone.offset_at(at("2026-07-01T00:00:00Z")).to_string(),
            "-04:00"
        );
        assert_eq!(zone.name(), Some("America/New_York"));
    }

    #[test]
    fn zone_settings_prefer_a_valid_iana_name_over_the_offset() {
        let fixed = ScheduleZone::parse(None, "+09:00").unwrap();
        assert_eq!(fixed.to_string(), "+09:00");
        assert_eq!(fixed.name(), None);
        assert_eq!(ScheduleZone::parse(Some("  "), "+09:00").unwrap(), fixed);
        assert!(ScheduleZone::parse(Some("Mars/Base"), "+09:00").is_err());
        assert!(ScheduleZone::parse(None, "JST").is_err());
    }
}
