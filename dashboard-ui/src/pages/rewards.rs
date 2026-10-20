use super::common::{Field, LoadError, Metric, game_name, reset_checked};
use crate::app::actions::spawn_action;
use crate::app::data::{load_rewards, reward_run_in_flight, save_reward_setting};
use crate::app::display::{
    browser_offset_minutes, compact_datetime, datetime_text, error_text, offset_minutes,
    shift_clock,
};
use crate::app::preferences::{Preferences, use_preferences};
use crate::app::state::AppState;
use crate::components::{Button, ButtonVariant};
use crate::models::{RewardRun, RewardSetting};
use dioxus::prelude::*;

/// HoYoLAB's overseas check-in day starts at 00:00 UTC+8.
const REWARD_DAY_OFFSET_MINUTES: i32 = 8 * 60;

#[component]
pub(super) fn RewardsPage(state: Signal<AppState>) -> Element {
    let preferences = use_preferences();
    let available = state
        .read()
        .me
        .as_ref()
        .is_some_and(|me| me.capabilities.reward_posting_available);
    let offset = state
        .read()
        .server
        .as_ref()
        .map(|server| server.timezone_offset.clone());
    let reset = offset
        .as_deref()
        .and_then(offset_minutes)
        .and_then(|minutes| shift_clock("00:00", REWARD_DAY_OFFSET_MINUTES, minutes));
    rsx! {
        section { class: "panel page-stack",
            h2 { {preferences.text("dashboard.login_bonus")} }
            if !available { p { class: "warning", role: "status", {preferences.text("rewards.reward_posting_unavailable")} } }
            if let Some(error) = state.read().rewards_error.clone() { LoadError { state, message: preferences.text(&error) } }
            p { class: "field-hint", {preferences.text("rewards.claim_now_on_dashboard")} }
            if let Some(reset) = reset {
                p { class: "field-hint", id: "reward-day-note", {preferences.format("rewards.reward_day_note", &[("reset", &reset)])} }
            }
            div { class: "form-grid",
                for setting in state.read().rewards.clone() {
                    RewardSettingCard { key: "{setting.id}", state, setting, offset: offset.clone() }
                }
            }
            hr { class: "divider" }
            RewardHistory { state }
        }
    }
}

/// Automatic claiming ON/OFF, saved immediately, wherever it appears.
#[component]
pub(super) fn AutomationSwitch(
    state: Signal<AppState>,
    setting: RewardSetting,
    id: String,
    disabled: bool,
) -> Element {
    let ui = use_preferences();
    rsx! {
        label { class: "switch",
            input {
                id: id.clone(), r#type: "checkbox", role: "switch", checked: setting.enabled, disabled,
                "aria-label": ui.format("dashboard.automation_label", &[("game", game_name(&setting.game_kind, ui))]),
                onchange: move |event| {
                    let setting_id = setting.id;
                    let enabled = event.checked();
                    // Keep the stored schedule; an unsaved time edit is not part of this toggle.
                    let local_time = setting.local_time.clone();
                    let element = id.clone();
                    spawn_action(state, move |generation| async move {
                        save_reward_setting(state, generation, setting_id, enabled, local_time, "dashboard.automation_setting_saved").await;
                        let stored = state.read().rewards.iter().find(|row| row.id == setting_id).map(|row| row.enabled);
                        if let Some(stored) = stored { reset_checked(&element, stored); }
                    });
                }
            }
            span { class: "switch-track", "aria-hidden": "true" }
            span { if setting.enabled { {ui.text("dashboard.on")} } else { {ui.text("dashboard.off")} } }
        }
    }
}

#[component]
fn RewardSettingCard(
    state: Signal<AppState>,
    setting: RewardSetting,
    offset: Option<String>,
) -> Element {
    let preferences = use_preferences();
    let mut local_time = use_signal(|| setting.local_time.clone());
    // Reconcile refreshed server schedules without replacing edits on unrelated renders.
    use_effect(use_reactive((&setting.local_time,), move |(time,)| {
        local_time.set(time)
    }));
    let parts = schedule_parts(&local_time());
    let (hour, minute) = parts.clone().unwrap_or_default();
    let changed = local_time() != setting.local_time;
    let available = state
        .read()
        .me
        .as_ref()
        .is_some_and(|me| me.capabilities.reward_posting_available);
    let disabled = !state.read().ready() || !available;
    let game = game_name(&setting.game_kind, preferences).to_owned();
    let label = |value: &str| reward_label(value, preferences);
    let polling = state
        .read()
        .reward_poll
        .as_ref()
        .is_some_and(|poll| poll.settings.contains(&setting.id));
    let browser_time = offset
        .as_deref()
        .and_then(offset_minutes)
        .zip(browser_offset_minutes())
        .filter(|(server, browser)| server != browser)
        .and_then(|(server, browser)| shift_clock(&local_time(), server, browser));
    let run = setting.last_run.clone();
    let tone = run.as_ref().map_or("", |run| run_tone(&run.state));
    let last_run = run.as_ref().map_or_else(
        || preferences.text("rewards.never").to_owned(),
        |run| label(&run.state),
    );
    let ended = run.as_ref().and_then(|run| {
        let ended = run.ended_at.clone()?;
        let text = compact_datetime(Some(&ended))?;
        Some((ended, text))
    });
    let id = setting.id;
    rsx! {
        div { class: "form-stack reward-card",
            div { class: "card-automation",
                h3 { "{game}" }
                AutomationSwitch { state, setting: setting.clone(), id: "reward-enabled-{id}", disabled }
            }
            fieldset { class: "time-field", disabled,
                legend { {preferences.text("rewards.automatic_claim_time")} }
                div { class: "time-picker",
                    Field { id: "reward-hour-{id}", label: preferences.text("rewards.hour"),
                        select { id: "reward-hour-{id}", value: "{hour}", "aria-describedby": "reward-timezone-{id}", onchange: move |event| {
                            if let Some((_, minute)) = schedule_parts(&local_time()) { local_time.set(format!("{}:{minute}", event.value())); }
                        },
                            if parts.is_none() { option { value: "", {preferences.text("common.not_fetched")} } }
                            for value in 0..24 { option { value: format!("{value:02}"), "{value:02}" } }
                        }
                    }
                    span { class: "time-colon", ":" }
                    Field { id: "reward-minute-{id}", label: preferences.text("rewards.minute"),
                        select { id: "reward-minute-{id}", value: "{minute}", "aria-describedby": "reward-timezone-{id}", onchange: move |event| {
                            if let Some((hour, _)) = schedule_parts(&local_time()) { local_time.set(format!("{hour}:{}", event.value())); }
                        },
                            if parts.is_none() { option { value: "", {preferences.text("common.not_fetched")} } }
                            for value in 0..60 { option { value: format!("{value:02}"), "{value:02}" } }
                        }
                    }
                }
                p { class: "timezone", id: "reward-timezone-{id}",
                    if let Some(offset) = &offset { {preferences.format("rewards.server_time", &[("offset", offset)])} }
                    else { {preferences.text("rewards.server_time_unknown")} }
                    if let Some(browser_time) = browser_time { " " {preferences.format("rewards.browser_time", &[("time", &browser_time)])} }
                }
            }
            div { class: "button-row",
                Button { r#type: "button", disabled: disabled || parts.is_none() || !changed, aria_describedby: changed.then_some(format!("reward-unsaved-{id}")), onclick: move |_| {
                    let time = local_time();
                    if schedule_parts(&time).is_none() { return; }
                    // Read the current server flag, not a stale card-local ON/OFF copy.
                    let Some(enabled) = state.read().rewards.iter().find(|row| row.id == id).map(|row| row.enabled) else { return; };
                    spawn_action(state, move |generation| async move {
                        save_reward_setting(state, generation, id, enabled, time, "rewards.schedule_saved").await;
                    });
                }, {preferences.text("rewards.save_time")} }
                if changed {
                    Button { variant: ButtonVariant::Ghost, r#type: "button", onclick: move |_| {
                        let stored = state.read().rewards.iter().find(|row| row.id == id).map(|row| row.local_time.clone());
                        if let Some(stored) = stored { local_time.set(stored); }
                    }, {preferences.text("rewards.discard_change")} }
                }
            }
            if changed { p { id: "reward-unsaved-{id}", class: "field-hint unsaved", role: "status", {preferences.text("rewards.unsaved_change")} } }
            dl { class: "metric-list",
                Metric { label: preferences.text("dashboard.auth_status"), value: label(&setting.auth_status) }
                Metric { label: preferences.text("rewards.reward_status"), value: match setting.is_sign { Some(true) => preferences.text("dashboard.received"), Some(false) => preferences.text("dashboard.not_received"), None => preferences.text("common.not_fetched") }.to_owned() }
                Metric { label: preferences.text("dashboard.checked"), value: datetime_text(setting.last_checked_at.as_deref(), preferences) }
                Metric { label: preferences.text("dashboard.next_run"), value: if setting.enabled { datetime_text(setting.next_run_at.as_deref(), preferences) } else { preferences.text("dashboard.automation_off").to_owned() } }
                if setting.error_code.is_some() {
                    Metric { label: preferences.text("dashboard.error"), value: error_text(setting.error_code.as_deref(), preferences).to_owned() }
                    Metric { label: preferences.text("dashboard.error_at"), value: datetime_text(setting.error_at.as_deref(), preferences) }
                }
            }
            p { id: "reward-run-state-{id}", class: "run-state {tone}",
                {preferences.text("rewards.last_run")} ": "
                strong { {last_run} }
                if let Some(ended) = ended { " · " time { datetime: "{ended.0}", "{ended.1}" } }
                if polling { " · " {preferences.text("rewards.checking_automatically")} }
            }
        }
    }
}

#[component]
fn RewardHistory(state: Signal<AppState>) -> Element {
    let preferences = use_preferences();
    let mut filter = use_signal(String::new);
    let rows = state.read().reward_runs.clone();
    let shown: Vec<RewardRun> = rows
        .clone()
        .unwrap_or_default()
        .into_iter()
        .filter(|run| filter().is_empty() || run.game_kind == filter())
        .collect();
    rsx! {
        section { class: "page-stack",
            div { class: "row",
                h3 { {preferences.text("rewards.run_history_last_5_days")} }
                div { class: "history-tools",
                    label { class: "visually-hidden", r#for: "history-game", {preferences.text("rewards.filter_game")} }
                    select { id: "history-game", value: filter(), onchange: move |event| filter.set(event.value()),
                        option { value: "", {preferences.text("rewards.all_games")} }
                        option { value: "genshin", {game_name("genshin", preferences)} }
                        option { value: "starrail", {game_name("starrail", preferences)} }
                    }
                    Button { r#type: "button", variant: ButtonVariant::Secondary, onclick: move |_| {
                        spawn_action(state, move |_| async move { load_rewards(state).await; });
                    }, {preferences.text("rewards.refresh")} }
                }
            }
            if let Some(error) = state.read().reward_runs_error.clone() { LoadError { state, message: preferences.text(&error) } }
            else if rows.is_none() { p { role: "status", {preferences.text("rewards.loading_run_history")} } }
            else if shown.is_empty() { p { {preferences.text("rewards.no_runs_returned_for_the_last_5_days")} } }
            ul { class: "history",
                for run in shown {
                    HistoryRow { key: "{run.id}", run }
                }
            }
        }
    }
}

#[component]
fn HistoryRow(run: RewardRun) -> Element {
    let preferences = use_preferences();
    let tone = run_tone(&run.state);
    // The latest reason a run went wrong, readable without opening the attempts.
    let reason = run
        .attempts
        .iter()
        .rev()
        .find_map(|attempt| attempt.error_code.clone());
    rsx! {
        li {
            div {
                p { class: "run-state {tone}",
                    strong { {reward_label(&run.state, preferences)} }
                    " · " {game_name(&run.game_kind, preferences)}
                    " · " {reward_label(&run.trigger, preferences)}
                }
                if let Some(reason) = reason { p { class: "run-reason", {error_text(Some(&reason), preferences)} } }
                if run.attempts.is_empty() {
                    p { {preferences.text("rewards.attempts")} " · {run.attempt_count}" }
                } else {
                    details { class: "attempts",
                        summary { {preferences.format("rewards.attempt_count", &[("count", &run.attempt_count.to_string())])} }
                        ol {
                            for attempt in &run.attempts {
                                li { {reward_label(&attempt.outcome, preferences)}
                                    if attempt.error_code.is_some() { " · " {error_text(attempt.error_code.as_deref(), preferences)} }
                                    " · " {datetime_text(Some(&attempt.started_at), preferences)}
                                }
                            }
                        }
                    }
                }
            }
            div { class: "history-times",
                time { datetime: "{run.created_at}", {datetime_text(Some(&run.created_at), preferences)} }
                if let Some(ended) = &run.ended_at {
                    span { {preferences.text("rewards.ended")} " " {datetime_text(Some(ended), preferences)} }
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

/// Inline style of the last-run line: in flight, settled well, or needs attention.
fn run_tone(state: &str) -> &'static str {
    match state {
        _ if reward_run_in_flight(state) => "pending",
        "success" | "already_claimed" => "success",
        _ => "error",
    }
}

fn reward_label(value: &str, preferences: Preferences) -> String {
    match value {
        "ready" => preferences.text("dashboard.ready"),
        "pending" | "queued" => preferences.text("rewards.queued"),
        "running" => preferences.text("dashboard.running"),
        "retry_wait" => preferences.text("rewards.retry_waiting"),
        "unknown" => preferences.text("dashboard.unknown_outcome"),
        "action_required" => preferences.text("rewards.action_required"),
        "failed" => preferences.text("dashboard.failed"),
        "stopped" => preferences.text("dashboard.stopped"),
        "success" => preferences.text("rewards.success"),
        "already_claimed" => preferences.text("rewards.already_claimed"),
        "manual" => preferences.text("rewards.manual"),
        "scheduled" => preferences.text("rewards.scheduled"),
        "catchup" => preferences.text("rewards.catch_up"),
        _ => preferences.text("dashboard.unknown"),
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
    fn last_run_tone_separates_in_flight_success_and_attention() {
        assert_eq!(run_tone("queued"), "pending");
        assert_eq!(run_tone("retry_wait"), "pending");
        assert_eq!(run_tone("already_claimed"), "success");
        assert_eq!(run_tone("unknown"), "error");
        assert_eq!(run_tone("failed"), "error");
    }

    #[test]
    fn unknown_run_states_use_a_localized_fallback_not_the_raw_value() {
        let ui = Preferences::default();
        assert_eq!(reward_label("success", ui), ui.text("rewards.success"));
        assert_eq!(reward_label("manual", ui), ui.text("rewards.manual"));
        assert_eq!(
            reward_label("synthetic_state", ui),
            ui.text("dashboard.unknown")
        );
    }
}
