//! Event calendar summaries: the running and announced banners and events of one game
//! account with their periods and the player's progress, read defensively from
//! untyped upstream JSON. The field names come from genshin.py
//! (`GenshinEventCalendar`, `HSREventCalendar`) and are not verified live, so a
//! missing field stays unknown (`None`). The endgame lists of the calendar
//! (`fixed_act_list`, `challenge_list`) are left out: the endgame summary already
//! shows those modes. Descriptions, reward lists and strategy links are not read.
use super::upstream_json::{
    boolean, integer, object, objects, server_offset, text, unix, wall_clock_text,
};
use attoshoyodashboard::game::GameKind;
use chrono::{DateTime, Duration, FixedOffset, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// Upper bounds that keep a surprising upstream payload from growing the cache.
const MAX_ITEMS: usize = 60;
const MAX_FEATURED: usize = 8;
const MAX_TEXT: usize = 120;

/// The image hosts the dashboard's CSP `img-src` allows (`http/ui.rs`). Other URLs
/// are dropped, so the UI falls back to the text it always shows.
const IMAGE_HOSTS: [&str; 4] = [
    "upload-os-bbs.hoyolab.com",
    "act-upload.hoyolab.com",
    "act-webstatic.hoyoverse.com",
    "fastcdn.hoyoverse.com",
];

/// One banner or event as stored in the cache and returned by the API.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub(in crate::dashboard) struct CalendarItem {
    /// `character_banner`, `weapon_banner`, `chronicled_banner`, `light_cone_banner`
    /// or `event`.
    pub(in crate::dashboard) kind: String,
    pub(in crate::dashboard) id: Option<i64>,
    pub(in crate::dashboard) name: Option<String>,
    pub(in crate::dashboard) version: Option<String>,
    pub(in crate::dashboard) starts_at: Option<DateTime<Utc>>,
    pub(in crate::dashboard) ends_at: Option<DateTime<Utc>>,
    /// The player finished the event (all of its rewards), when reported.
    pub(in crate::dashboard) finished: Option<bool>,
    /// The event is not open to the player yet, when reported.
    pub(in crate::dashboard) locked: Option<bool>,
    pub(in crate::dashboard) progress: Option<i64>,
    pub(in crate::dashboard) progress_total: Option<i64>,
    /// Rate-up characters, weapons or light cones of a banner.
    pub(in crate::dashboard) featured: Vec<FeaturedItem>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub(in crate::dashboard) struct FeaturedItem {
    pub(in crate::dashboard) name: Option<String>,
    pub(in crate::dashboard) rarity: Option<i64>,
    /// An official image CDN URL, or `None`.
    pub(in crate::dashboard) icon: Option<String>,
}

/// Banners and events that have not ended at `now` (the fetch time), ordered by
/// their end (unknown ends last).
pub(in crate::dashboard::games) fn summarize(
    game: GameKind,
    region: &str,
    raw: &Value,
    now: DateTime<Utc>,
) -> Vec<CalendarItem> {
    let Some(source) = raw.as_object() else {
        return Vec::new();
    };
    let zone = server_offset(region);
    let mut items = match game {
        GameKind::Genshin(_) => genshin(source, zone, now),
        GameKind::Starrail(_) => starrail(source),
    };
    items.retain(|item| item.ends_at.is_none_or(|end| end > now));
    items.sort_by_key(|item| (item.ends_at.is_none(), item.ends_at));
    items.truncate(MAX_ITEMS);
    items
}

fn genshin(
    source: &Map<String, Value>,
    zone: Option<FixedOffset>,
    now: DateTime<Utc>,
) -> Vec<CalendarItem> {
    let mut items = Vec::new();
    for (list, kind) in [
        ("avatar_card_pool_list", "character_banner"),
        ("weapon_card_pool_list", "weapon_banner"),
        ("mixed_card_pool_list", "chronicled_banner"),
    ] {
        for pool in objects(source.get(list)) {
            let mut item = genshin_period(pool, kind, zone, now);
            item.id = integer(pool.get("pool_id"));
            item.name = short_text(pool.get("pool_name"));
            item.version = short_text(pool.get("version_name"));
            item.featured = featured(
                objects(pool.get("avatars"))
                    .into_iter()
                    .chain(objects(pool.get("weapon"))),
                "name",
                "icon",
            );
            items.push(item);
        }
    }
    for event in objects(source.get("act_list")) {
        let mut item = genshin_period(event, "event", zone, now);
        item.id = integer(event.get("id"));
        item.name = short_text(event.get("name"));
        item.finished = boolean(event.get("is_finished"));
        // Double-reward events count the boosted runs left.
        if let Some(double) = object(event.get("double_detail")) {
            let total = integer(double.get("total")).filter(|total| *total > 0);
            let left = integer(double.get("left"));
            if let (Some(total), Some(left)) = (total, left) {
                item.progress = Some(total.saturating_sub(left).clamp(0, total));
                item.progress_total = Some(total);
            }
        }
        items.push(item);
    }
    items
}

/// Unix-second timestamps, else `YYYY-MM-DD HH:MM:SS` server time, and for the end
/// the countdown from the fetch.
fn genshin_period(
    source: &Map<String, Value>,
    kind: &str,
    zone: Option<FixedOffset>,
    now: DateTime<Utc>,
) -> CalendarItem {
    let starts_at = unix(source.get("start_timestamp"))
        .or_else(|| wall_clock_text(source.get("start_time"), zone));
    // Before the start the countdown may count to the start, so it only stands in for
    // the end of an item that is known to have begun (or has no known start).
    let countdown = integer(source.get("countdown_seconds"))
        .filter(|seconds| *seconds > 0 && starts_at.is_none_or(|start| start <= now))
        .and_then(Duration::try_seconds)
        .and_then(|left| now.checked_add_signed(left));
    CalendarItem {
        kind: kind.to_owned(),
        starts_at,
        ends_at: unix(source.get("end_timestamp"))
            .or_else(|| wall_clock_text(source.get("end_time"), zone))
            .or(countdown),
        ..CalendarItem::default()
    }
}

fn starrail(source: &Map<String, Value>) -> Vec<CalendarItem> {
    let mut items = Vec::new();
    for (pools, kind, entries, icon) in [
        (
            "avatar_card_pool_list",
            "character_banner",
            "avatar_list",
            "icon_url",
        ),
        (
            "equip_card_pool_list",
            "light_cone_banner",
            "equip_list",
            "item_url",
        ),
    ] {
        for pool in objects(source.get(pools)) {
            let mut item = starrail_period(pool, kind);
            item.id = integer(pool.get("id"));
            item.name = short_text(pool.get("name"));
            item.version = short_text(pool.get("version"));
            item.featured = featured(objects(pool.get(entries)), "item_name", icon);
            items.push(item);
        }
    }
    for event in objects(source.get("act_list")) {
        // Long-running modes (Simulated Universe) are not time-limited events.
        if text(event.get("act_time_type")).as_deref() == Some("ActTimeTypeLong") {
            continue;
        }
        let mut item = starrail_period(event, "event");
        item.id = integer(event.get("id"));
        item.name = short_text(event.get("name"));
        item.version = short_text(event.get("version"));
        item.finished = boolean(event.get("all_finished"));
        item.locked = text(event.get("act_status")).map(|status| status.ends_with("Unopened"));
        let total = integer(event.get("total_progress")).filter(|total| *total > 0);
        if let Some(total) = total {
            item.progress = integer(event.get("current_progress"))
                .filter(|current| *current >= 0)
                .map(|current| current.min(total));
            item.progress_total = item.progress.map(|_| total);
        }
        items.push(item);
    }
    items
}

/// `time_info.start_ts`/`end_ts` in Unix seconds; zero or absent is unknown.
fn starrail_period(source: &Map<String, Value>, kind: &str) -> CalendarItem {
    let time = object(source.get("time_info"));
    CalendarItem {
        kind: kind.to_owned(),
        starts_at: time.and_then(|time| unix(time.get("start_ts"))),
        ends_at: time.and_then(|time| unix(time.get("end_ts"))),
        ..CalendarItem::default()
    }
}

fn featured<'a>(
    entries: impl IntoIterator<Item = &'a Map<String, Value>>,
    name: &str,
    icon: &str,
) -> Vec<FeaturedItem> {
    entries
        .into_iter()
        .map(|entry| FeaturedItem {
            name: short_text(entry.get(name)),
            rarity: integer(entry.get("rarity")),
            icon: entry
                .get(icon)
                .and_then(Value::as_str)
                .and_then(official_image),
        })
        .filter(|item| item.name.is_some() || item.icon.is_some())
        .take(MAX_FEATURED)
        .collect()
}

/// Trimmed, single-line text of at most `MAX_TEXT` characters (Star Rail names carry
/// literal `\n`).
fn short_text(value: Option<&Value>) -> Option<String> {
    let text = text(value)?
        .replace("\\n", " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    (!text.is_empty()).then(|| text.chars().take(MAX_TEXT).collect())
}

/// An `https` URL on one of the official image hosts, without credentials or port.
pub(in crate::dashboard::games) fn official_image(url: &str) -> Option<String> {
    let url = url.trim();
    let rest = url.strip_prefix("https://")?;
    let host = rest.split(['/', '?', '#']).next()?.to_ascii_lowercase();
    (url.len() <= 512
        && IMAGE_HOSTS.contains(&host.as_str())
        && !url.chars().any(|c| c.is_whitespace() || c.is_control()))
    .then(|| url.to_owned())
}
