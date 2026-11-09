//! Endgame challenge summaries: the current period, its progress and reset time per
//! mode, read defensively from untyped upstream JSON. The field names come from
//! genshin.py and are not verified live, so a missing field stays unknown (`None`)
//! and never implies a locked, private or unsupported record. Per-character records
//! (teams, ranks, damage) are deliberately not read.
use attoshoyodashboard::challenge::Challenge;
use chrono::{DateTime, FixedOffset, NaiveDate, TimeZone, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// One mode as stored in the cache and returned by the API. Every progress field is
/// optional because each mode reports a different subset.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub(in crate::dashboard) struct ModeSummary {
    pub(in crate::dashboard) mode: String,
    pub(in crate::dashboard) unlocked: Option<bool>,
    pub(in crate::dashboard) has_data: Option<bool>,
    pub(in crate::dashboard) name: Option<String>,
    pub(in crate::dashboard) starts_at: Option<DateTime<Utc>>,
    pub(in crate::dashboard) ends_at: Option<DateTime<Utc>>,
    pub(in crate::dashboard) stars: Option<i64>,
    pub(in crate::dashboard) max_floor: Option<String>,
    pub(in crate::dashboard) max_round: Option<i64>,
    pub(in crate::dashboard) difficulty: Option<i64>,
    pub(in crate::dashboard) best_seconds: Option<i64>,
    pub(in crate::dashboard) battles: Option<i64>,
    /// A sanitized code when this mode alone could not be fetched.
    pub(in crate::dashboard) error_code: Option<String>,
}

pub(in crate::dashboard::games) fn summarize(
    challenge: Challenge,
    region: &str,
    raw: &Value,
) -> ModeSummary {
    let mut out = ModeSummary {
        mode: challenge.id().to_owned(),
        ..ModeSummary::default()
    };
    let Some(source) = raw.as_object() else {
        return out;
    };
    let zone = server_offset(region);
    match challenge {
        Challenge::SpiralAbyss => spiral_abyss(source, &mut out),
        Challenge::ImaginariumTheater => imaginarium_theater(source, zone, &mut out),
        Challenge::StygianOnslaught => stygian_onslaught(source, zone, &mut out),
        Challenge::MemoryOfChaos | Challenge::PureFiction | Challenge::ApocalypticShadow => {
            starrail_floors(source, zone, &mut out)
        }
        Challenge::AnomalyArbitration => anomaly_arbitration(source, zone, &mut out),
    }
    out
}

pub(in crate::dashboard::games) fn failed(challenge: Challenge, error_code: String) -> ModeSummary {
    ModeSummary {
        mode: challenge.id().to_owned(),
        error_code: Some(error_code),
        ..ModeSummary::default()
    }
}

/// HoYoverse server time zones. The wall-clock period times of Star Rail (and the
/// fallback date objects of Genshin) are in the account's server time; an unknown
/// server leaves those times unknown instead of guessing.
fn server_offset(region: &str) -> Option<FixedOffset> {
    let hours = match region {
        "os_usa" | "prod_official_usa" => -5,
        "os_euro" | "prod_official_eur" => 1,
        "os_asia" | "os_cht" | "prod_official_asia" | "prod_official_cht" => 8,
        _ => return None,
    };
    FixedOffset::east_opt(hours * 3600)
}

fn spiral_abyss(source: &Map<String, Value>, out: &mut ModeSummary) {
    out.unlocked = boolean(source.get("is_unlock"));
    out.starts_at = unix(source.get("start_time"));
    out.ends_at = unix(source.get("end_time"));
    out.stars = integer(source.get("total_star"));
    out.max_floor = text(source.get("max_floor"));
    out.battles = integer(source.get("total_battle_times"));
    out.has_data = out.battles.map(|battles| battles > 0);
}

fn imaginarium_theater(
    source: &Map<String, Value>,
    zone: Option<FixedOffset>,
    out: &mut ModeSummary,
) {
    out.unlocked = boolean(source.get("is_unlock"));
    let entries = objects(source.get("data"));
    // The list may hold earlier seasons; schedule_type 1 marks the current one.
    let Some(entry) = entries
        .iter()
        .find(|entry| {
            integer(object(entry.get("schedule")).and_then(|s| s.get("schedule_type"))) == Some(1)
        })
        .or(entries.first())
    else {
        return;
    };
    out.has_data = boolean(entry.get("has_data"));
    if let Some(stat) = object(entry.get("stat")) {
        out.max_round = integer(stat.get("max_round_id"));
        out.difficulty = integer(stat.get("difficulty_id"));
        out.stars = integer(stat.get("medal_num"));
    }
    if let Some(schedule) = object(entry.get("schedule")) {
        period(
            schedule,
            ("start_time", "start_date_time"),
            ("end_time", "end_date_time"),
            zone,
            out,
        );
    }
}

fn stygian_onslaught(
    source: &Map<String, Value>,
    zone: Option<FixedOffset>,
    out: &mut ModeSummary,
) {
    out.unlocked = boolean(source.get("is_unlock"));
    let entries = objects(source.get("data"));
    let Some(entry) = entries
        .iter()
        .find(|entry| {
            boolean(object(entry.get("schedule")).and_then(|s| s.get("is_valid"))) == Some(true)
        })
        .or(entries.first())
    else {
        return;
    };
    if let Some(schedule) = object(entry.get("schedule")) {
        out.name = text(schedule.get("name"));
        period(
            schedule,
            ("start_time", "start_date_time"),
            ("end_time", "end_date_time"),
            zone,
            out,
        );
    }
    let single = object(entry.get("single"));
    let multi = object(entry.get("mp"));
    let single_data = single.and_then(|record| boolean(record.get("has_data")));
    let multi_data = multi.and_then(|record| boolean(record.get("has_data")));
    out.has_data = match (single_data, multi_data) {
        (Some(true), _) | (_, Some(true)) => Some(true),
        (Some(false), Some(false)) => Some(false),
        _ => None,
    };
    // The solo record when there is one, else the co-op record.
    let best = if single_data == Some(true) {
        single
    } else if multi_data == Some(true) {
        multi
    } else {
        None
    }
    .and_then(|record| object(record.get("best")));
    if let Some(best) = best {
        out.difficulty = integer(best.get("difficulty"));
        out.best_seconds = integer(best.get("second"));
    }
}

/// Memory of Chaos, Pure Fiction and Apocalyptic Shadow share one shape.
fn starrail_floors(source: &Map<String, Value>, zone: Option<FixedOffset>, out: &mut ModeSummary) {
    out.has_data = boolean(source.get("has_data"));
    out.stars = integer(source.get("star_num"));
    out.max_floor = text(source.get("max_floor"));
    out.battles = integer(source.get("battle_num"));
    let schedule_id = integer(source.get("schedule_id"));
    let groups = objects(source.get("groups"));
    let group = groups
        .iter()
        .find(|group| schedule_id.is_some() && integer(group.get("schedule_id")) == schedule_id)
        .or(groups.first());
    if let Some(group) = group {
        out.name = text(group.get("name_mi18n"));
        out.starts_at = wall_clock(group.get("begin_time"), zone);
        out.ends_at = wall_clock(group.get("end_time"), zone);
    }
    out.starts_at = out
        .starts_at
        .or_else(|| wall_clock(source.get("begin_time"), zone));
    out.ends_at = out
        .ends_at
        .or_else(|| wall_clock(source.get("end_time"), zone));
}

fn anomaly_arbitration(
    source: &Map<String, Value>,
    zone: Option<FixedOffset>,
    out: &mut ModeSummary,
) {
    let records = objects(source.get("challenge_peak_records"));
    let Some(record) = records.first() else {
        return;
    };
    out.has_data = boolean(record.get("has_challenge_record"));
    out.battles = integer(record.get("battle_num"));
    out.stars = match (
        integer(record.get("boss_stars")),
        integer(record.get("mob_stars")),
    ) {
        (None, None) => None,
        (boss, mobs) => Some(boss.unwrap_or(0).saturating_add(mobs.unwrap_or(0))),
    };
    if let Some(group) = object(record.get("group")) {
        out.name = text(group.get("name_mi18n"));
        out.starts_at = wall_clock(group.get("begin_time"), zone);
        out.ends_at = wall_clock(group.get("end_time"), zone);
    }
}

/// A period from Unix-second fields, falling back to server-time date objects.
fn period(
    schedule: &Map<String, Value>,
    start: (&str, &str),
    end: (&str, &str),
    zone: Option<FixedOffset>,
    out: &mut ModeSummary,
) {
    out.starts_at = unix(schedule.get(start.0)).or_else(|| wall_clock(schedule.get(start.1), zone));
    out.ends_at = unix(schedule.get(end.0)).or_else(|| wall_clock(schedule.get(end.1), zone));
}

fn object(value: Option<&Value>) -> Option<&Map<String, Value>> {
    value.and_then(Value::as_object)
}

fn objects(value: Option<&Value>) -> Vec<&Map<String, Value>> {
    value
        .and_then(Value::as_array)
        .map(|items| items.iter().filter_map(Value::as_object).collect())
        .unwrap_or_default()
}

fn boolean(value: Option<&Value>) -> Option<bool> {
    value.and_then(Value::as_bool)
}

/// Integers arrive as numbers or as decimal strings.
fn integer(value: Option<&Value>) -> Option<i64> {
    match value? {
        Value::Number(number) => number.as_i64(),
        Value::String(text) => text.trim().parse().ok(),
        _ => None,
    }
}

fn text(value: Option<&Value>) -> Option<String> {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_owned)
}

/// Unix seconds; zero (an unset period) is unknown.
fn unix(value: Option<&Value>) -> Option<DateTime<Utc>> {
    integer(value)
        .filter(|seconds| *seconds > 0)
        .and_then(|seconds| Utc.timestamp_opt(seconds, 0).single())
}

/// `{year, month, day, hour, minute[, second]}` in the server's time zone.
fn wall_clock(value: Option<&Value>, zone: Option<FixedOffset>) -> Option<DateTime<Utc>> {
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
