//! Defensive readers for untyped upstream JSON shared by the endgame and calendar
//! summaries: a missing or differently typed value is `None` (unknown), never a guess.
use chrono::{DateTime, FixedOffset, NaiveDate, NaiveDateTime, TimeZone, Utc};
use serde_json::{Map, Value};

/// HoYoverse server time zones. The wall-clock period times of Star Rail (and the
/// fallback date objects of Genshin) are in the account's server time; an unknown
/// server leaves those times unknown instead of guessing.
pub(super) fn server_offset(region: &str) -> Option<FixedOffset> {
    let hours = match region {
        "os_usa" | "prod_official_usa" => -5,
        "os_euro" | "prod_official_eur" => 1,
        "os_asia" | "os_cht" | "prod_official_asia" | "prod_official_cht" => 8,
        _ => return None,
    };
    FixedOffset::east_opt(hours * 3600)
}

pub(super) fn object(value: Option<&Value>) -> Option<&Map<String, Value>> {
    value.and_then(Value::as_object)
}

pub(super) fn objects(value: Option<&Value>) -> Vec<&Map<String, Value>> {
    value
        .and_then(Value::as_array)
        .map(|items| items.iter().filter_map(Value::as_object).collect())
        .unwrap_or_default()
}

pub(super) fn boolean(value: Option<&Value>) -> Option<bool> {
    value.and_then(Value::as_bool)
}

/// Integers arrive as numbers or as decimal strings.
pub(super) fn integer(value: Option<&Value>) -> Option<i64> {
    match value? {
        Value::Number(number) => number.as_i64(),
        Value::String(text) => text.trim().parse().ok(),
        _ => None,
    }
}

pub(super) fn text(value: Option<&Value>) -> Option<String> {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_owned)
}

/// Unix seconds; zero (an unset period) is unknown.
pub(super) fn unix(value: Option<&Value>) -> Option<DateTime<Utc>> {
    integer(value)
        .filter(|seconds| *seconds > 0)
        .and_then(|seconds| Utc.timestamp_opt(seconds, 0).single())
}

/// `{year, month, day, hour, minute[, second]}` in the server's time zone.
pub(super) fn wall_clock(
    value: Option<&Value>,
    zone: Option<FixedOffset>,
) -> Option<DateTime<Utc>> {
    let zone = zone?;
    let parts = object(value)?;
    let part = |key: &str| integer(parts.get(key));
    let year = i32::try_from(part("year")?).ok().filter(|year| *year > 0)?;
    let at = |key: &str| part(key).and_then(|value| u32::try_from(value).ok());
    let local = NaiveDate::from_ymd_opt(year, at("month")?, at("day")?)?.and_hms_opt(
        at("hour")?,
        at("minute")?,
        at("second").unwrap_or(0),
    )?;
    zone.from_local_datetime(&local)
        .single()
        .map(|time| time.with_timezone(&Utc))
}

/// `YYYY-MM-DD HH:MM:SS` text in the server's time zone.
pub(super) fn wall_clock_text(
    value: Option<&Value>,
    zone: Option<FixedOffset>,
) -> Option<DateTime<Utc>> {
    let zone = zone?;
    let local = NaiveDateTime::parse_from_str(value?.as_str()?.trim(), "%Y-%m-%d %H:%M:%S").ok()?;
    zone.from_local_datetime(&local)
        .single()
        .map(|time| time.with_timezone(&Utc))
}
