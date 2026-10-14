use crate::app::actions::{handle_api_error_for, spawn_action};
use crate::app::data::load_accounts;
use crate::app::display::game_name;
use crate::app::state::AppState;
use crate::app::state::{generation_matches, set_status};
use crate::components::{Card, CardContent, CardDescription, CardHeader, CardTitle};
use dioxus::prelude::*;
use serde_json::json;

#[component]
pub(super) fn UidsPage(state: Signal<AppState>) -> Element {
    rsx! {
        div { class: "page-stack",
            h2 { "UID Visibility" }
            Card {
                CardHeader { CardTitle { "Retained game accounts" } CardDescription { "Hidden and missing UIDs stay listed here for management but are never fetched for home status." } }
                CardContent {
                    if let Some(error) = &state.read().accounts_error { p { class: "warning", "{error}" } }
                    AccountTable { state, controls: true }
                }
            }
        }
    }
}

#[component]
pub(super) fn AccountTable(mut state: Signal<AppState>, controls: bool) -> Element {
    let accounts = if controls {
        state.read().accounts.clone()
    } else {
        state.read().visible_accounts()
    };
    rsx! {
        if accounts.is_empty() {
            p { "No game accounts returned." }
        } else {
            div { class: "table-wrap",
                table {
                    thead { tr { th { "Game" } th { "UID" } th { "Region" } th { "Detected" } th { "Visible" } } }
                    tbody {
                        for account in accounts {
                            tr {
                                td { "{game_name(&account.game_kind)}" }
                                td { if account.game_uid.is_empty() { "Missing" } else { "{account.game_uid}" } }
                                td { "{account.region}" }
                                td { if account.detected { "Yes" } else { "No" } }
                                td {
                                    if controls {
                                        input { r#type: "checkbox", checked: account.visible, "aria-label": "Visible {account.game_uid}", onchange: move |event| {
                                            let id = account.id;
                                            let visible = event.checked();
                                            let state = state;
                                            spawn_action(state, move |generation| async move {
                                match crate::api::send("PATCH", &format!("/game-accounts/{id}"), Some(json!({"visible": visible}))).await {
                                                    Ok(()) if generation_matches(state, generation) => { set_status(state, "Visibility updated.", "success"); load_accounts(state).await; }
                                                    Ok(()) => {}
                                                    Err(error) => { handle_api_error_for(state, generation, error); }
                                                }
                                            });
                                        } }
                                    } else if account.visible { "Yes" } else { "No" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
