use super::common::{Field, Metric};
use crate::app::actions::{handle_api_error_for, spawn_action};
use crate::app::data::{load_rewards, save_reward_setting};
use crate::app::preferences::use_preferences;
use crate::app::state::{AppState, generation_matches, set_status};
use crate::components::{Button, ButtonVariant};
use crate::models::RewardSetting;
use dioxus::prelude::*;
use serde_json::json;

#[component]
pub(super) fn RewardsPage(state: Signal<AppState>) -> Element {
    let preferences = use_preferences();
    let available = state
        .read()
        .me
        .as_ref()
        .is_some_and(|me| me.capabilities.reward_posting_available);
    rsx! {
        section { class: "panel page-stack",
            h2 { {preferences.text("ログインボーナス", "Login bonus")} }
            if !available { p { class: "warning", {preferences.text("報酬送信は利用できません", "Reward posting unavailable")} } }
            if let Some(error) = &state.read().rewards_error { p { class: "warning", "{error}" } }
            div { class: "form-grid",
                for setting in state.read().rewards.clone() {
                    RewardSettingCard { key: "{setting.id}", state, setting }
                }
            }
            RewardHistory { state }
        }
    }
}

#[component]
fn RewardSettingCard(state: Signal<AppState>, setting: RewardSetting) -> Element {
    let preferences = use_preferences();
    let mut local_time = use_signal(|| setting.local_time.clone());
    // Reconcile refreshed server schedules without replacing edits on unrelated renders.
    use_effect(use_reactive((&setting.local_time,), move |(time,)| {
        local_time.set(time)
    }));
    let parts = schedule_parts(&local_time());
    let (hour, minute) = parts.clone().unwrap_or_default();
    let available = state
        .read()
        .me
        .as_ref()
        .is_some_and(|me| me.capabilities.reward_posting_available);
    let disabled = state.read().busy || !state.read().ready() || !available;
    let can_run = !disabled
        && run_eligible(
            &setting.auth_status,
            setting.last_run.as_ref().map(|run| run.state.as_str()),
        );
    let timezone = state
        .read()
        .server
        .as_ref()
        .map(|server| server.timezone_offset.clone());
    let game = match setting.game_kind.as_str() {
        "genshin" => preferences.text("原神", "Genshin Impact"),
        "starrail" => preferences.text("崩壊：スターレイル", "Honkai: Star Rail"),
        other => other,
    }
    .to_owned();
    let label = |value: &str| reward_label(value, |ja, en| preferences.text(ja, en));
    let missing = preferences.text("未報告", "Not reported");
    rsx! {
        div { class: "form-stack",
            h3 { "{game}" }
            fieldset { class: "time-field", disabled,
                legend { {preferences.text("自動受け取り時刻", "Automatic claim time")} }
                div { class: "time-picker",
                    Field { id: "reward-hour-{setting.id}", label: preferences.text("時", "Hour"),
                        select { id: "reward-hour-{setting.id}", value: "{hour}", onchange: move |event| {
                            if let Some((_, minute)) = schedule_parts(&local_time()) { local_time.set(format!("{}:{minute}", event.value())); }
                        },
                            if parts.is_none() { option { value: "", {preferences.text("未読込", "Not loaded")} } }
                            for value in 0..24 { option { value: format!("{value:02}"), "{value:02}" } }
                        }
                    }
                    span { class: "time-colon", ":" }
                    Field { id: "reward-minute-{setting.id}", label: preferences.text("分", "Minute"),
                        select { id: "reward-minute-{setting.id}", value: "{minute}", onchange: move |event| {
                            if let Some((hour, _)) = schedule_parts(&local_time()) { local_time.set(format!("{hour}:{}", event.value())); }
                        },
                            if parts.is_none() { option { value: "", {preferences.text("未読込", "Not loaded")} } }
                            for value in 0..60 { option { value: format!("{value:02}"), "{value:02}" } }
                        }
                    }
                }
                p { class: "timezone",
                    {preferences.text("タイムゾーン", "Timezone")} " "
                    if let Some(timezone) = timezone { "{timezone}" }
                    else { {preferences.text("+09:00（API固定値・未読込）", "+09:00 (API default; not loaded)")} }
                }
            }
            dl { class: "metric-list",
                Metric { label: preferences.text("自動受け取り", "Automatic claim"), value: preferences.text(if setting.enabled { "オン" } else { "オフ" }, if setting.enabled { "On" } else { "Off" }).to_owned() }
                Metric { label: preferences.text("認証状態", "Auth status"), value: label(&setting.auth_status) }
                Metric { label: preferences.text("受け取り状態", "Reward status"), value: match setting.is_sign { Some(true) => preferences.text("受取済み", "Received"), Some(false) => preferences.text("未受取", "Not received"), None => preferences.text("未取得", "Not fetched") }.to_owned() }
                Metric { label: preferences.text("確認日時", "Checked"), value: setting.last_checked_at.clone().unwrap_or_else(|| missing.to_owned()) }
                Metric { label: preferences.text("次回実行", "Next run"), value: setting.next_run_at.clone().unwrap_or_else(|| missing.to_owned()) }
                Metric { label: preferences.text("前回実行", "Last run"), value: setting.last_run.as_ref().map(|run| label(&run.state)).unwrap_or_else(|| preferences.text("実行なし", "Never").to_owned()) }
                Metric { label: preferences.text("エラー", "Error"), value: setting.error_code.clone().unwrap_or_else(|| preferences.text("なし", "None").to_owned()) }
                Metric { label: preferences.text("エラー日時", "Error time"), value: setting.error_at.clone().unwrap_or_else(|| missing.to_owned()) }
            }
            div { class: "button-row",
                Button { r#type: "button", disabled: disabled || parts.is_none(), onclick: move |_| {
                    let id = setting.id;
                    let time = local_time();
                    if schedule_parts(&time).is_none() { return; }
                    // Read the current server flag, not a stale card-local ON/OFF copy.
                    let Some(enabled) = state.read().rewards.iter().find(|row| row.id == id).map(|row| row.enabled) else { return; };
                    spawn_action(state, move |generation| async move {
                        save_reward_setting(state, generation, id, enabled, time, preferences.text("時刻を保存しました", "Schedule saved.")).await;
                    });
                }, {preferences.text("保存", "Save")} }
                Button { variant: ButtonVariant::Secondary, r#type: "button", disabled: !can_run, onclick: move |_| {
                    let id = setting.id;
                    let allowed = state.read().rewards.iter().find(|row| row.id == id).is_some_and(|row| run_eligible(&row.auth_status, row.last_run.as_ref().map(|run| run.state.as_str())));
                    if !allowed || !can_run { return; }
                    spawn_action(state, move |generation| async move {
                        match crate::api::send("POST", &format!("/reward-settings/{id}/run"), Some(json!({}))).await {
                            Ok(()) if generation_matches(state, generation) => {
                                set_status(state, preferences.text("実行を受け付けました（処理待ち）", "Run accepted; queued, not completed."), "success");
                                load_rewards(state).await;
                            }
                            Ok(()) => {}
                            Err(error) => {
                                handle_api_error_for(state, generation, error);
                                // A failed acknowledgement may still have enqueued work.
                                // Refresh server state; never retry the POST automatically.
                                if generation_matches(state, generation) { load_rewards(state).await; }
                            }
                        }
                    });
                }, {preferences.text("今すぐ実行", "Run now")} }
            }
        }
    }
}

#[component]
fn RewardHistory(state: Signal<AppState>) -> Element {
    let preferences = use_preferences();
    let rows = state.read().reward_runs.clone();
    rsx! {
        section { class: "page-stack",
            div { class: "row",
                h3 { {preferences.text("実行履歴 · 直近5日", "Run history · Last 5 days")} }
                Button { r#type: "button", variant: ButtonVariant::Secondary, disabled: state.read().busy, onclick: move |_| {
                    spawn_action(state, move |_| async move { load_rewards(state).await; });
                }, {preferences.text("更新", "Refresh")} }
            }
            if let Some(error) = &state.read().reward_runs_error { p { class: "warning", "{error}" } }
            else if rows.is_empty() { p { {preferences.text("直近5日の履歴はありません", "No runs returned for the last 5 days.")} } }
            ul { class: "history",
                for run in rows {
                    li { key: "{run.id}",
                        div {
                            strong { {match run.game_kind.as_str() { "genshin" => preferences.text("原神", "Genshin Impact"), "starrail" => preferences.text("崩壊：スターレイル", "Honkai: Star Rail"), _ => &run.game_kind }} }
                            " · " {reward_label(&run.state, |ja, en| preferences.text(ja, en))}
                            " · " {reward_label(&run.trigger, |ja, en| preferences.text(ja, en))}
                        }
                        time { "{run.created_at}" }
                        span { {preferences.text("終了", "Ended")} " · " {run.ended_at.as_deref().unwrap_or(preferences.text("未報告", "Not reported"))} }
                        span { {preferences.text("試行", "Attempts")} " · {run.attempt_count}" }
                        for attempt in &run.attempts {
                            div { "{attempt.number} · " {reward_label(&attempt.outcome, |ja, en| preferences.text(ja, en))}
                                if let Some(error) = &attempt.error_code { " · {error}" }
                            }
                        }
                    }
                }
            }
        }
    }
}

fn schedule_parts(value: &str) -> Option<(String, String)> {
    let bytes = value.as_bytes();
    if bytes.len() != 5
        || bytes[2] != b':'
        || !bytes
            .iter()
            .enumerate()
            .all(|(i, b)| i == 2 || b.is_ascii_digit())
    {
        return None;
    }
    let (hour, minute) = value.split_once(':')?;
    if hour.parse::<u8>().ok()? > 23 || minute.parse::<u8>().ok()? > 59 {
        return None;
    }
    Some((hour.to_owned(), minute.to_owned()))
}

// UI duplicate/uncertain-send guard only; the backend remains the policy authority.
// Manual runs are valid even when automation is OFF.
fn run_eligible(auth_status: &str, last_state: Option<&str>) -> bool {
    auth_status == "ready"
        && !matches!(
            last_state,
            Some("pending" | "queued" | "running" | "retry_wait" | "unknown")
        )
}

fn reward_label(value: &str, text: impl Fn(&'static str, &'static str) -> &'static str) -> String {
    match value {
        "ready" => text("利用可能", "Ready"),
        "pending" | "queued" => text("処理待ち", "Queued"),
        "running" => text("実行中", "Running"),
        "retry_wait" => text("再試行待ち", "Retry waiting"),
        "unknown" => text("結果不明", "Unknown outcome"),
        "action_required" => text("操作が必要", "Action required"),
        "failed" => text("失敗", "Failed"),
        "stopped" => text("停止", "Stopped"),
        "success" => text("成功", "Success"),
        "already_claimed" => text("受取済み", "Already claimed"),
        "manual" => text("手動", "Manual"),
        "scheduled" => text("定時", "Scheduled"),
        "catchup" => text("遅延実行", "Catch-up"),
        other => return other.to_owned(),
    }
    .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schedule_parser_accepts_each_minute_and_rejects_invalid_data() {
        for hour in 0..24 {
            for minute in 0..60 {
                let value = format!("{hour:02}:{minute:02}");
                let (h, m) = schedule_parts(&value).unwrap();
                assert_eq!(format!("{h}:{m}"), value);
            }
        }
        for invalid in [
            "", "6:00", "06:0", "24:00", "23:60", "-1:00", "06:00:00", "あ:00",
        ] {
            assert!(schedule_parts(invalid).is_none(), "{invalid}");
        }
    }

    #[test]
    fn manual_run_guard_blocks_inflight_and_unknown_but_not_automation_off() {
        assert!(run_eligible("ready", None));
        for state in ["pending", "queued", "running", "retry_wait", "unknown"] {
            assert!(!run_eligible("ready", Some(state)));
        }
        for state in ["success", "already_claimed", "failed", "stopped"] {
            assert!(run_eligible("ready", Some(state)));
        }
        assert!(!run_eligible("action_required", None));
        assert!(!run_eligible("", None));
    }
}
