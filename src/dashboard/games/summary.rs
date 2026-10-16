use attosgameapi::game::GameKind;
use chrono::{DateTime, Duration, Utc};
use serde_json::{Map, Value};

pub(in crate::dashboard::games) fn summarize(
    game: GameKind,
    fetched_at: DateTime<Utc>,
    raw: &Value,
) -> Option<Value> {
    let source = raw.as_object()?;
    let mut out = Map::new();
    match game {
        GameKind::Genshin(_) => summarize_genshin(source, fetched_at, &mut out),
        GameKind::Starrail(_) => summarize_starrail(source, fetched_at, &mut out),
    }
    (!out.is_empty()).then_some(Value::Object(out))
}

fn summarize_genshin(
    source: &Map<String, Value>,
    fetched_at: DateTime<Utc>,
    out: &mut Map<String, Value>,
) {
    copy_keys(
        source,
        out,
        &[
            "current_resin",
            "max_resin",
            "current_home_coin",
            "max_home_coin",
            "finished_task_num",
            "total_task_num",
            "is_extra_task_reward_received",
        ],
    );
    project_recovery(
        source,
        out,
        fetched_at,
        "resin_recovery_time",
        "resin_full_at",
    );
    project_recovery(
        source,
        out,
        fetched_at,
        "home_coin_recovery_time",
        "home_coin_full_at",
    );
    copy_expeditions(source, out, "remained_time", "avatar_side_icon");
}

fn summarize_starrail(
    source: &Map<String, Value>,
    fetched_at: DateTime<Utc>,
    out: &mut Map<String, Value>,
) {
    copy_keys(
        source,
        out,
        &[
            "current_stamina",
            "max_stamina",
            "current_reserve_stamina",
            "current_train_score",
            "max_train_score",
        ],
    );
    project_recovery(
        source,
        out,
        fetched_at,
        "stamina_recover_time",
        "stamina_full_at",
    );
    copy_expeditions(source, out, "remaining_time", "item_url");
}

fn copy_keys(source: &Map<String, Value>, out: &mut Map<String, Value>, keys: &[&str]) {
    for key in keys {
        if let Some(value) = source.get(*key) {
            out.insert((*key).to_owned(), value.clone());
        }
    }
}

fn project_recovery(
    source: &Map<String, Value>,
    out: &mut Map<String, Value>,
    fetched_at: DateTime<Utc>,
    source_key: &str,
    full_at_key: &str,
) {
    let Some(seconds) = source.get(source_key).and_then(nonnegative_seconds) else {
        return;
    };
    let Some(duration) = Duration::try_seconds(seconds) else {
        return;
    };
    let Some(full_at) = fetched_at.checked_add_signed(duration) else {
        return;
    };
    out.insert(full_at_key.to_owned(), Value::String(full_at.to_rfc3339()));
}

fn nonnegative_seconds(value: &Value) -> Option<i64> {
    match value {
        Value::Number(number) => number.as_i64().filter(|seconds| *seconds >= 0),
        Value::String(text) => text.parse::<i64>().ok().filter(|seconds| *seconds >= 0),
        _ => None,
    }
}

fn copy_expeditions(
    source: &Map<String, Value>,
    out: &mut Map<String, Value>,
    remaining_key: &str,
    icon_key: &str,
) {
    let Some(items) = source.get("expeditions").and_then(Value::as_array) else {
        return;
    };
    let expeditions: Vec<Value> = items
        .iter()
        .filter_map(|item| {
            let item = item.as_object()?;
            let mut projected = Map::new();
            if let Some(status) = item.get("status") {
                projected.insert("status".to_owned(), status.clone());
            }
            if let Some(remaining) = item.get(remaining_key) {
                projected.insert(remaining_key.to_owned(), remaining.clone());
            }
            if let Some(icon) = item
                .get(icon_key)
                .and_then(Value::as_str)
                .filter(|icon| !icon.trim().is_empty())
            {
                projected.insert(icon_key.to_owned(), Value::String(icon.to_owned()));
            }
            // Prefer an explicit assignment name, then an explicit destination label.
            // Do not infer names or locations from avatar/icon asset URLs.
            if let Some(name) = ["name", "location"].into_iter().find_map(|key| {
                item.get(key)
                    .and_then(Value::as_str)
                    .filter(|name| !name.trim().is_empty())
            }) {
                projected.insert("name".to_owned(), Value::String(name.to_owned()));
            }
            (!projected.is_empty()).then_some(Value::Object(projected))
        })
        .collect();
    if items.is_empty() || !expeditions.is_empty() {
        out.insert("expeditions".to_owned(), Value::Array(expeditions));
    }
}
