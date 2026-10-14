use super::accounts::AccountTable;
use super::common::Metric;
use crate::app::actions::spawn_root;
use crate::app::data::load_selected_status;
use crate::app::display::{bool_status, count_pair, display_date, game_name};
use crate::app::state::AppState;
use crate::components::{
    Button, ButtonVariant, Card, CardContent, CardDescription, CardHeader, CardTitle,
};
use crate::models::{Expedition, GameStatus};
use dioxus::prelude::*;
use uuid::Uuid;

#[component]
pub(super) fn DashboardPage(mut state: Signal<AppState>) -> Element {
    let selected = state.read().selected_account;
    let visible = state.read().visible_accounts();
    let selected_status = selected.and_then(|id| {
        state
            .read()
            .account_statuses
            .iter()
            .find(|entry| entry.id == id)
            .map(|entry| entry.status.clone())
    });
    rsx! {
        div { class: "page-stack",
            h2 { "Dashboard" }
            div { class: "grid two",
                Card {
                    CardHeader { CardTitle { "Visible game UIDs" } CardDescription { "Only visible, detected UIDs are offered for status polling." } }
                    CardContent {
                        if let Some(error) = &state.read().accounts_error { p { class: "warning", "{error}" } }
                        AccountTable { state, controls: false }
                    }
                }
                Card {
                    CardHeader { CardTitle { "Selected game status" } CardDescription { "Fetched from the server cache/upstream path; missing fields are not invented." } }
                    CardContent {
                        if visible.is_empty() {
                            p { "No visible detected UIDs. Use UID Visibility to choose retained game accounts." }
                        } else {
                            label { r#for: "selected-account", "Selected UID" }
                            select { id: "selected-account", value: selected.map(|id| id.to_string()).unwrap_or_default(), onchange: move |event| {
                                if let Ok(id) = event.value().parse::<Uuid>() {
                                    state.write().selected_account = Some(id);
                                    let state = state;
                                    spawn_root(async move { load_selected_status(state).await; });
                                }
                            },
                                for account in visible {
                                    option { value: "{account.id}", "{game_name(&account.game_kind)} {account.game_uid}" }
                                }
                            }
                            if let Some(status) = selected_status {
                                GameStatusCard { status }
                            } else {
                                p { "Status has not been fetched yet, or the server returned no usable status." }
                                Button { variant: ButtonVariant::Secondary, r#type: "button", onclick: move |_| { let state = state; spawn_root(async move { load_selected_status(state).await; }); }, "Fetch status" }
                            }
                        }
                    }
                }
            }
            Card {
                CardHeader { CardTitle { "Login bonuses" } CardDescription { "Reward status is checked independently of the selected UID." } }
                CardContent {
                    if let Some(error) = &state.read().rewards_error { p { class: "warning", "{error}" } }
                    div { class: "reward-badges",
                        for setting in state.read().rewards.clone() {
                            div { class: "reward-badge",
                                strong { "{game_name(&setting.game_kind)}" }
                                span { "{bool_status(setting.is_sign)}" }
                                small { "Checked: {display_date(&setting.last_checked_at)}" }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn GameStatusCard(status: GameStatus) -> Element {
    let data = status.data.clone();
    rsx! {
        div { class: "status-card",
            dl { class: "metric-list",
                Metric { label: "Game", value: game_name(&status.game_kind).to_owned() }
                Metric { label: "UID", value: if status.game_uid.is_empty() { "Missing".to_owned() } else { status.game_uid.clone() } }
                Metric { label: "Region", value: status.region.clone() }
                Metric { label: "Fetched", value: display_date(&status.fetched_at) }
                Metric { label: "Stale", value: if status.stale { "Yes".to_owned() } else { "No".to_owned() } }
                Metric { label: "Error", value: status.error_code.clone().unwrap_or_else(|| "None".to_owned()) }
                Metric { label: "Error at", value: display_date(&status.error_at) }
            }
            if let Some(data) = data {
                dl { class: "metric-list wide",
                    if status.game_kind == "genshin" {
                        Metric { label: "Resin", value: count_pair(data.current_resin, data.max_resin) }
                        Metric { label: "Resin full at", value: display_date(&data.resin_full_at) }
                        Metric { label: "Home coin", value: count_pair(data.current_home_coin, data.max_home_coin) }
                        Metric { label: "Home coin full at", value: display_date(&data.home_coin_full_at) }
                        Metric { label: "Commissions", value: count_pair(data.finished_task_num, data.total_task_num) }
                        Metric { label: "Extra commission reward", value: bool_status(data.is_extra_task_reward_received).to_owned() }
                    } else {
                        Metric { label: "Stamina", value: count_pair(data.current_stamina, data.max_stamina) }
                        Metric { label: "Stamina full at", value: display_date(&data.stamina_full_at) }
                        Metric { label: "Reserve stamina", value: data.current_reserve_stamina.map(|n| n.to_string()).unwrap_or_else(|| "Not reported".to_owned()) }
                        Metric { label: "Training", value: count_pair(data.current_train_score, data.max_train_score) }
                    }
                }
                ExpeditionList { expeditions: data.expeditions.clone().unwrap_or_default() }
            } else {
                p { "No daily summary payload returned." }
            }
        }
    }
}

#[component]
fn ExpeditionList(expeditions: Vec<Expedition>) -> Element {
    let rows: Vec<(String, String, String)> = expeditions
        .into_iter()
        .map(|expedition| {
            (
                expedition.name.unwrap_or_else(|| "Unnamed".to_owned()),
                expedition.status.unwrap_or_else(|| "unknown".to_owned()),
                expedition
                    .remained_time
                    .or(expedition.remaining_time)
                    .map(|duration| duration.to_string())
                    .unwrap_or_else(|| "not reported".to_owned()),
            )
        })
        .collect();
    rsx! {
        div { class: "expeditions",
            h3 { "Expeditions" }
            if rows.is_empty() {
                p { "Not reported" }
            } else {
                ul {
                    for (name, status, remaining) in rows {
                        li { "{name} — {status} — {remaining}" }
                    }
                }
            }
        }
    }
}
