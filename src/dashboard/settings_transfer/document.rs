//! The `ahd-settings` file: its version 1 layout and strict, per-field validation.
//! Secrets (Cookies, the Webhook URL, passwords, passkeys, sessions, API tokens) have no
//! place in the format; only a flag says whether a Webhook was configured.
use crate::dashboard::{
    notifications::{
        LANGUAGES, PERSONAL_EVENTS, RESIN_WARN_HOURS, WEBHOOK_FORMATS, channel::valid_clock,
    },
    rewards::schedule::valid_local_time,
};
use serde::Serialize;
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, BTreeSet};

pub(in crate::dashboard) const FORMAT: &str = "ahd-settings";
pub(in crate::dashboard) const VERSION: u64 = 1;
/// Games with a reward schedule and detected UIDs (`game_kind` columns).
pub(super) const GAMES: [&str; 2] = ["genshin", "starrail"];
/// More UIDs than any HoYoLAB account holds; bounds the work of one import.
pub(super) const MAX_GAME_ACCOUNTS: usize = 64;
/// Validation stops listing problems after this many.
const MAX_ERRORS: usize = 50;

/// One problem in an imported file. `field` is a path such as `rewards[0].local_time`
/// (`$` is the whole document); `code` is one of `missing`, `type`, `unknown`, `invalid`,
/// `unsupported`, `duplicate` or `too_many`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(in crate::dashboard) struct FieldError {
    pub(in crate::dashboard) field: String,
    pub(in crate::dashboard) code: &'static str,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct RewardEntry {
    pub(super) game_kind: String,
    pub(super) enabled: bool,
    pub(super) local_time: String,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct GameEntry {
    pub(super) game_kind: String,
    pub(super) game_uid: String,
    pub(super) visible: bool,
}

/// Notification choices from a file. Event maps hold only the kinds the file lists;
/// kinds it leaves out keep their current choice.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct NotificationEntry {
    pub(super) webhook_configured: bool,
    pub(super) webhook_format: String,
    pub(super) language: String,
    pub(super) events: BTreeMap<String, bool>,
    pub(super) app_events: BTreeMap<String, bool>,
    pub(super) resin_warn_hours: i32,
    pub(super) daily_reminder_time: String,
    pub(super) weekly_reminder_time: String,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct Document {
    pub(super) rewards: Vec<RewardEntry>,
    pub(super) game_accounts: Vec<GameEntry>,
    pub(super) notifications: NotificationEntry,
}

/// Collects problems while walking the document.
#[derive(Default)]
struct Problems(Vec<FieldError>);

impl Problems {
    fn add(&mut self, field: impl Into<String>, code: &'static str) {
        if self.0.len() < MAX_ERRORS {
            self.0.push(FieldError {
                field: field.into(),
                code,
            });
        }
    }

    /// The object at `field`, reporting a missing value or a wrong type.
    fn object<'a>(
        &mut self,
        value: Option<&'a Value>,
        field: &str,
    ) -> Option<&'a Map<String, Value>> {
        match value {
            None => {
                self.add(field, "missing");
                None
            }
            Some(Value::Object(map)) => Some(map),
            Some(_) => {
                self.add(field, "type");
                None
            }
        }
    }

    fn array<'a>(&mut self, value: Option<&'a Value>, field: &str) -> Option<&'a Vec<Value>> {
        match value {
            None => {
                self.add(field, "missing");
                None
            }
            Some(Value::Array(items)) => Some(items),
            Some(_) => {
                self.add(field, "type");
                None
            }
        }
    }

    fn bool(&mut self, map: &Map<String, Value>, key: &str, path: &str) -> Option<bool> {
        match map.get(key) {
            None => {
                self.add(join(path, key), "missing");
                None
            }
            Some(Value::Bool(value)) => Some(*value),
            Some(_) => {
                self.add(join(path, key), "type");
                None
            }
        }
    }

    /// A string that must satisfy `valid`.
    fn string(
        &mut self,
        map: &Map<String, Value>,
        key: &str,
        path: &str,
        valid: impl Fn(&str) -> bool,
    ) -> Option<String> {
        match map.get(key) {
            None => {
                self.add(join(path, key), "missing");
                None
            }
            Some(Value::String(value)) if valid(value) => Some(value.clone()),
            Some(Value::String(_)) => {
                self.add(join(path, key), "invalid");
                None
            }
            Some(_) => {
                self.add(join(path, key), "type");
                None
            }
        }
    }

    /// Reports keys outside `allowed`.
    fn only(&mut self, map: &Map<String, Value>, allowed: &[&str], path: &str) {
        for key in map.keys() {
            if !allowed.contains(&key.as_str()) {
                self.add(join(path, key), "unknown");
            }
        }
    }
}

fn join(path: &str, key: &str) -> String {
    if path.is_empty() {
        key.to_owned()
    } else {
        format!("{path}.{key}")
    }
}

fn game_uid(value: &str) -> bool {
    // Same rule as the `game_accounts_uid_check` constraint.
    (1..=20).contains(&value.len())
        && value.bytes().all(|b| b.is_ascii_digit())
        && !value.starts_with('0')
}

/// Validates a whole file. Any problem rejects it; nothing is applied from a partly valid file.
pub(super) fn parse(value: &Value) -> Result<Document, Vec<FieldError>> {
    let mut problems = Problems::default();
    let Some(root) = problems.object(Some(value), "$") else {
        return Err(problems.0);
    };
    // An unknown format or version makes every other field meaningless.
    match root.get("format") {
        None => problems.add("format", "missing"),
        Some(Value::String(format)) if format == FORMAT => {}
        Some(Value::String(_)) => problems.add("format", "unsupported"),
        Some(_) => problems.add("format", "type"),
    }
    match root.get("version") {
        None => problems.add("version", "missing"),
        Some(Value::Number(version)) if version.as_u64() == Some(VERSION) => {}
        Some(Value::Number(_)) => problems.add("version", "unsupported"),
        Some(_) => problems.add("version", "type"),
    }
    if !problems.0.is_empty() {
        return Err(problems.0);
    }
    problems.only(
        root,
        &[
            "format",
            "version",
            "exported_at",
            "rewards",
            "game_accounts",
            "notifications",
        ],
        "",
    );
    match root.get("exported_at") {
        None => {}
        Some(Value::String(at)) if chrono::DateTime::parse_from_rfc3339(at).is_ok() => {}
        Some(Value::String(_)) => problems.add("exported_at", "invalid"),
        Some(_) => problems.add("exported_at", "type"),
    }
    let rewards = rewards(&mut problems, root.get("rewards"));
    let game_accounts = game_accounts(&mut problems, root.get("game_accounts"));
    let notifications = notifications(&mut problems, root.get("notifications"));
    match (problems.0.is_empty(), notifications) {
        (true, Some(notifications)) => Ok(Document {
            rewards,
            game_accounts,
            notifications,
        }),
        _ => Err(problems.0),
    }
}

fn rewards(problems: &mut Problems, value: Option<&Value>) -> Vec<RewardEntry> {
    let Some(items) = problems.array(value, "rewards") else {
        return Vec::new();
    };
    if items.len() > GAMES.len() {
        problems.add("rewards", "too_many");
        return Vec::new();
    }
    let mut seen = BTreeSet::new();
    let mut entries = Vec::new();
    for (index, item) in items.iter().enumerate() {
        let path = format!("rewards[{index}]");
        let Some(map) = problems.object(Some(item), &path) else {
            continue;
        };
        problems.only(map, &["game_kind", "enabled", "local_time"], &path);
        let kind = problems.string(map, "game_kind", &path, |kind| GAMES.contains(&kind));
        let enabled = problems.bool(map, "enabled", &path);
        let time = problems.string(map, "local_time", &path, valid_local_time);
        if let Some(kind) = &kind
            && !seen.insert(kind.clone())
        {
            problems.add(join(&path, "game_kind"), "duplicate");
            continue;
        }
        if let (Some(game_kind), Some(enabled), Some(local_time)) = (kind, enabled, time) {
            entries.push(RewardEntry {
                game_kind,
                enabled,
                local_time,
            });
        }
    }
    entries
}

fn game_accounts(problems: &mut Problems, value: Option<&Value>) -> Vec<GameEntry> {
    let Some(items) = problems.array(value, "game_accounts") else {
        return Vec::new();
    };
    if items.len() > MAX_GAME_ACCOUNTS {
        problems.add("game_accounts", "too_many");
        return Vec::new();
    }
    let mut seen = BTreeSet::new();
    let mut entries = Vec::new();
    for (index, item) in items.iter().enumerate() {
        let path = format!("game_accounts[{index}]");
        let Some(map) = problems.object(Some(item), &path) else {
            continue;
        };
        problems.only(map, &["game_kind", "game_uid", "visible"], &path);
        let kind = problems.string(map, "game_kind", &path, |kind| GAMES.contains(&kind));
        let uid = problems.string(map, "game_uid", &path, game_uid);
        let visible = problems.bool(map, "visible", &path);
        if let (Some(kind), Some(uid)) = (&kind, &uid)
            && !seen.insert((kind.clone(), uid.clone()))
        {
            problems.add(join(&path, "game_uid"), "duplicate");
            continue;
        }
        if let (Some(game_kind), Some(game_uid), Some(visible)) = (kind, uid, visible) {
            entries.push(GameEntry {
                game_kind,
                game_uid,
                visible,
            });
        }
    }
    entries
}

fn event_choices(
    problems: &mut Problems,
    map: &Map<String, Value>,
    key: &str,
) -> Option<BTreeMap<String, bool>> {
    let path = join("notifications", key);
    let choices = problems.object(map.get(key), &path)?;
    let mut parsed = BTreeMap::new();
    for (kind, choice) in choices {
        if !PERSONAL_EVENTS.iter().any(|(known, _)| known == kind) {
            problems.add(join(&path, kind), "unknown");
        } else if let Value::Bool(on) = choice {
            parsed.insert(kind.clone(), *on);
        } else {
            problems.add(join(&path, kind), "type");
        }
    }
    Some(parsed)
}

fn notifications(problems: &mut Problems, value: Option<&Value>) -> Option<NotificationEntry> {
    let path = "notifications";
    let map = problems.object(value, path)?;
    problems.only(
        map,
        &[
            "webhook_configured",
            "webhook_format",
            "language",
            "events",
            "app_events",
            "resin_warn_hours",
            "daily_reminder_time",
            "weekly_reminder_time",
        ],
        path,
    );
    let webhook_configured = problems.bool(map, "webhook_configured", path);
    let webhook_format = problems.string(map, "webhook_format", path, |v| {
        WEBHOOK_FORMATS.contains(&v)
    });
    let language = problems.string(map, "language", path, |v| LANGUAGES.contains(&v));
    let events = event_choices(problems, map, "events");
    let app_events = event_choices(problems, map, "app_events");
    let resin_warn_hours = match map.get("resin_warn_hours") {
        None => {
            problems.add(join(path, "resin_warn_hours"), "missing");
            None
        }
        Some(Value::Number(hours)) => match hours.as_i64().and_then(|h| i32::try_from(h).ok()) {
            Some(hours) if RESIN_WARN_HOURS.contains(&hours) => Some(hours),
            Some(_) => {
                problems.add(join(path, "resin_warn_hours"), "invalid");
                None
            }
            None => {
                problems.add(join(path, "resin_warn_hours"), "type");
                None
            }
        },
        Some(_) => {
            problems.add(join(path, "resin_warn_hours"), "type");
            None
        }
    };
    let daily = problems.string(map, "daily_reminder_time", path, valid_clock);
    let weekly = problems.string(map, "weekly_reminder_time", path, valid_clock);
    Some(NotificationEntry {
        webhook_configured: webhook_configured?,
        webhook_format: webhook_format?,
        language: language?,
        events: events?,
        app_events: app_events?,
        resin_warn_hours: resin_warn_hours?,
        daily_reminder_time: daily?,
        weekly_reminder_time: weekly?,
    })
}

/// The file an export writes. `exported_at` is informational only.
pub(super) fn render(document: &Document, exported_at: chrono::DateTime<chrono::Utc>) -> Value {
    let notifications = &document.notifications;
    json!({
        "format": FORMAT,
        "version": VERSION,
        "exported_at": exported_at.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        "rewards": document.rewards.iter().map(|entry| json!({
            "game_kind": entry.game_kind,
            "enabled": entry.enabled,
            "local_time": entry.local_time,
        })).collect::<Vec<_>>(),
        "game_accounts": document.game_accounts.iter().map(|entry| json!({
            "game_kind": entry.game_kind,
            "game_uid": entry.game_uid,
            "visible": entry.visible,
        })).collect::<Vec<_>>(),
        "notifications": {
            "webhook_configured": notifications.webhook_configured,
            "webhook_format": notifications.webhook_format,
            "language": notifications.language,
            "events": notifications.events,
            "app_events": notifications.app_events,
            "resin_warn_hours": notifications.resin_warn_hours,
            "daily_reminder_time": notifications.daily_reminder_time,
            "weekly_reminder_time": notifications.weekly_reminder_time,
        },
    })
}
