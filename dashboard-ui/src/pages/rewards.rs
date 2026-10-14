use super::common::{Field, Metric};
use crate::app::actions::{handle_api_error_for, spawn_action};
use crate::app::data::load_rewards;
use crate::app::display::{bool_status, display_date, game_name, humanize};
use crate::app::state::AppState;
use crate::app::state::{generation_matches, set_status};
use crate::components::{
    Button, ButtonVariant, Card, CardContent, CardDescription, CardFooter, CardHeader, CardTitle,
};
use crate::models::{RewardRun, RewardSetting};
use dioxus::prelude::*;
use serde_json::json;

#[component]
pub(super) fn RewardsPage(state: Signal<AppState>) -> Element {
    rsx! {
        div { class: "page-stack",
            h2 { "Rewards" }
            if !state.read().me.as_ref().is_some_and(|me| me.capabilities.reward_posting_available) {
                p { class: "warning", "Reward posting is not available on this server. Settings and history are shown only when the API exposes them; no UI state is treated as proof of availability." }
            }
            if let Some(error) = &state.read().rewards_error { p { class: "warning", "{error}" } }
            div { class: "grid two",
                for setting in state.read().rewards.clone() {
                    RewardSettingCard { key: "{setting.id}", state, setting }
                }
            }
            RewardHistory { state }
        }
    }
}

#[component]
fn RewardSettingCard(mut state: Signal<AppState>, setting: RewardSetting) -> Element {
    let mut enabled = use_signal(|| setting.enabled);
    let mut local_time = use_signal(|| setting.local_time.clone());
    rsx! {
        Card {
            CardHeader { CardTitle { "{game_name(&setting.game_kind)}" } CardDescription { "Default is off. Manual runs enqueue server work and return accepted asynchronously." } }
            CardContent { class: "form-stack",
                label { class: "checkbox-row", input { r#type: "checkbox", checked: enabled(), onchange: move |event| enabled.set(event.checked()) } "Enabled" }
                Field { id: "reward-time-{setting.id}", label: "Local run time",
                    input { id: "reward-time-{setting.id}", r#type: "time", value: "{local_time}", oninput: move |event| local_time.set(event.value()) }
                }
                dl { class: "metric-list",
                    Metric { label: "Auth status", value: humanize(&setting.auth_status) }
                    Metric { label: "Current reward", value: bool_status(setting.is_sign).to_owned() }
                    Metric { label: "Checked", value: display_date(&setting.last_checked_at) }
                    Metric { label: "Next run", value: display_date(&setting.next_run_at) }
                    Metric { label: "Last run", value: setting.last_run.as_ref().map(|run| humanize(&run.state)).unwrap_or_else(|| "Never".to_owned()) }
                    Metric { label: "Error", value: setting.error_code.clone().unwrap_or_else(|| "None".to_owned()) }
                }
            }
            CardFooter { class: "button-row",
                Button { r#type: "button", onclick: move |_| {
                    let id = setting.id;
                    let body = json!({"enabled": enabled(), "local_time": if local_time().is_empty() { "06:00".to_owned() } else { local_time() }});
                    let state = state;
                    spawn_action(state, move |generation| async move {
                                match crate::api::send("PATCH", &format!("/reward-settings/{id}"), Some(body)).await {
                            Ok(()) if generation_matches(state, generation) => { set_status(state, "Reward setting saved.", "success"); load_rewards(state).await; }
                            Ok(()) => {}
                            Err(error) => { handle_api_error_for(state, generation, error); }
                        }
                    });
                }, "Save" }
                Button { variant: ButtonVariant::Secondary, r#type: "button", onclick: move |_| {
                    let id = setting.id;
                    let state = state;
                    spawn_action(state, move |generation| async move {
                                match crate::api::send("POST", &format!("/reward-settings/{id}/run"), Some(json!({}))).await {
                            Ok(()) if generation_matches(state, generation) => { set_status(state, "Manual reward run queued.", "success"); load_rewards(state).await; }
                            Ok(()) => {}
                            Err(error) => { handle_api_error_for(state, generation, error); }
                        }
                    });
                }, "Run now" }
            }
        }
    }
}

#[component]
fn RewardHistory(state: Signal<AppState>) -> Element {
    let rows = state.read().reward_runs.clone();
    rsx! {
        Card {
            CardHeader { CardTitle { "Reward history: last 5 days" } CardDescription { "Attempts show backend outcomes without exposing secrets." } }
            CardContent {
                if let Some(error) = &state.read().reward_runs_error { p { class: "warning", "{error}" } }
                if rows.is_empty() {
                    p { "No reward runs returned for the last 5 days." }
                } else {
                    div { class: "table-wrap",
                        table {
                            thead { tr { th { "Game" } th { "Trigger" } th { "State" } th { "Created" } th { "Attempts" } } }
                            tbody { for run in rows { tr {
                                td { "{game_name(&run.game_kind)}" }
                                td { "{humanize(&run.trigger)}" }
                                td { "{humanize(&run.state)}" }
                                td { "{run.created_at}" }
                                td { "{attempt_summary(&run)}" }
                            } } }
                        }
                    }
                }
            }
        }
    }
}

fn attempt_summary(run: &RewardRun) -> String {
    let outcomes = run
        .attempts
        .iter()
        .map(|attempt| humanize(&attempt.outcome))
        .collect::<Vec<_>>()
        .join(", ");
    if outcomes.is_empty() {
        run.attempt_count.to_string()
    } else {
        format!("{} ({outcomes})", run.attempt_count)
    }
}
