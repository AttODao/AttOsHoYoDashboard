use super::common::{Field, Metric, game_name};
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
            h2 { {preferences.text("dashboard.login_bonus")} }
            if !available { p { class: "warning", {preferences.text("rewards.reward_posting_unavailable")} } }
            if let Some(error) = &state.read().rewards_error { p { class: "warning", {preferences.text(error)} } }
            div { class: "form-grid",
                for setting in state.read().rewards.clone() {
                    RewardSettingCard { key: "{setting.id}", state, setting }
                }
            }
            hr { class: "divider" }
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
    let game = game_name(&setting.game_kind, preferences).to_owned();
    let label = |value: &str| reward_label(value, preferences);
    let missing = preferences.text("hoyolab.not_reported");
    rsx! {
        div { class: "form-stack",
            h3 { "{game}" }
            fieldset { class: "time-field", disabled,
                legend { {preferences.text("rewards.automatic_claim_time")} }
                div { class: "time-picker",
                    Field { id: "reward-hour-{setting.id}", label: preferences.text("rewards.hour"),
                        select { id: "reward-hour-{setting.id}", value: "{hour}", onchange: move |event| {
                            if let Some((_, minute)) = schedule_parts(&local_time()) { local_time.set(format!("{}:{minute}", event.value())); }
                        },
                            if parts.is_none() { option { value: "", {preferences.text("hoyolab.not_loaded")} } }
                            for value in 0..24 { option { value: format!("{value:02}"), "{value:02}" } }
                        }
                    }
                    span { class: "time-colon", ":" }
                    Field { id: "reward-minute-{setting.id}", label: preferences.text("rewards.minute"),
                        select { id: "reward-minute-{setting.id}", value: "{minute}", onchange: move |event| {
                            if let Some((hour, _)) = schedule_parts(&local_time()) { local_time.set(format!("{hour}:{}", event.value())); }
                        },
                            if parts.is_none() { option { value: "", {preferences.text("hoyolab.not_loaded")} } }
                            for value in 0..60 { option { value: format!("{value:02}"), "{value:02}" } }
                        }
                    }
                }
                p { class: "timezone",
                    {preferences.text("rewards.timezone")} " "
                    if let Some(timezone) = timezone { "{timezone}" }
                    else { {preferences.text("rewards.09_00_api_default_not_loaded")} }
                }
            }
            dl { class: "metric-list",
                Metric { label: preferences.text("rewards.automatic_claim"), value: (if setting.enabled { preferences.text("rewards.on") } else { preferences.text("rewards.off") }).to_owned() }
                Metric { label: preferences.text("dashboard.auth_status"), value: label(&setting.auth_status) }
                Metric { label: preferences.text("rewards.reward_status"), value: match setting.is_sign { Some(true) => preferences.text("dashboard.received"), Some(false) => preferences.text("dashboard.not_received"), None => preferences.text("dashboard.not_fetched") }.to_owned() }
                Metric { label: preferences.text("dashboard.checked"), value: setting.last_checked_at.clone().unwrap_or_else(|| missing.to_owned()) }
                Metric { label: preferences.text("dashboard.next_run"), value: setting.next_run_at.clone().unwrap_or_else(|| missing.to_owned()) }
                Metric { label: preferences.text("rewards.last_run"), value: setting.last_run.as_ref().map(|run| label(&run.state)).unwrap_or_else(|| preferences.text("rewards.never").to_owned()) }
                Metric { label: preferences.text("dashboard.error"), value: setting.error_code.clone().unwrap_or_else(|| preferences.text("dashboard.none").to_owned()) }
                Metric { label: preferences.text("rewards.error_time"), value: setting.error_at.clone().unwrap_or_else(|| missing.to_owned()) }
            }
            div { class: "button-row",
                Button { r#type: "button", disabled: disabled || parts.is_none(), onclick: move |_| {
                    let id = setting.id;
                    let time = local_time();
                    if schedule_parts(&time).is_none() { return; }
                    // Read the current server flag, not a stale card-local ON/OFF copy.
                    let Some(enabled) = state.read().rewards.iter().find(|row| row.id == id).map(|row| row.enabled) else { return; };
                    spawn_action(state, move |generation| async move {
                        save_reward_setting(state, generation, id, enabled, time, "rewards.schedule_saved").await;
                    });
                }, {preferences.text("hoyolab.save")} }
                Button { variant: ButtonVariant::Secondary, r#type: "button", disabled: !can_run, onclick: move |_| {
                    let id = setting.id;
                    let allowed = state.read().rewards.iter().find(|row| row.id == id).is_some_and(|row| run_eligible(&row.auth_status, row.last_run.as_ref().map(|run| run.state.as_str())));
                    if !allowed || !can_run { return; }
                    spawn_action(state, move |generation| async move {
                        match crate::api::send("POST", &format!("/reward-settings/{id}/run"), Some(json!({}))).await {
                            Ok(()) if generation_matches(state, generation) => {
                                set_status(state, "rewards.run_accepted_queued_not_completed", "success");
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
                }, {preferences.text("rewards.run_now")} }
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
                h3 { {preferences.text("rewards.run_history_last_5_days")} }
                Button { r#type: "button", variant: ButtonVariant::Secondary, disabled: state.read().busy, onclick: move |_| {
                    spawn_action(state, move |_| async move { load_rewards(state).await; });
                }, {preferences.text("rewards.refresh")} }
            }
            if let Some(error) = &state.read().reward_runs_error { p { class: "warning", {preferences.text(error)} } }
            else if rows.is_none() { p { role: "status", {preferences.text("rewards.loading_run_history")} } }
            else if rows.as_ref().is_some_and(Vec::is_empty) { p { {preferences.text("rewards.no_runs_returned_for_the_last_5_days")} } }
            ul { class: "history",
                for run in rows.unwrap_or_default() {
                    li { key: "{run.id}",
                        div {
                            strong { {game_name(&run.game_kind, preferences)} }
                            " · " {reward_label(&run.state, preferences)}
                            " · " {reward_label(&run.trigger, preferences)}
                        }
                        time { "{run.created_at}" }
                        span { {preferences.text("rewards.ended")} " · " {run.ended_at.as_deref().unwrap_or(preferences.text("hoyolab.not_reported"))} }
                        span { {preferences.text("rewards.attempts")} " · {run.attempt_count}" }
                        for attempt in &run.attempts {
                            div { "{attempt.number} · " {reward_label(&attempt.outcome, preferences)}
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

fn reward_label(value: &str, preferences: crate::app::preferences::Preferences) -> String {
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
