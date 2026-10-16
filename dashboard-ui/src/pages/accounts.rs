use crate::app::actions::{handle_api_error_for, spawn_action};
use crate::app::data::load_accounts;
use crate::app::preferences::use_preferences;
use crate::app::state::{AppState, generation_matches, set_status};
use dioxus::prelude::*;
use serde_json::json;

#[component]
pub(super) fn UidsPage(state: Signal<AppState>) -> Element {
    let preferences = use_preferences();
    rsx! {
        section { class: "panel page-stack",
            h2 { {preferences.text("accounts.game_visibility")} }
            if let Some(error) = &state.read().accounts_error { p { class: "warning", {preferences.text(error)} } }
            AccountTable { state, controls: true }
        }
    }
}

#[component]
pub(super) fn AccountTable(state: Signal<AppState>, controls: bool) -> Element {
    let preferences = use_preferences();
    let accounts = if controls {
        state.read().accounts.clone()
    } else {
        state.read().visible_accounts()
    };
    rsx! {
        if accounts.is_empty() && state.read().accounts_error.is_none() {
            p { {preferences.text("accounts.no_game_accounts_returned")} }
        } else {
            div { class: "account-list",
                for account in accounts {
                    div { class: "setting-row", key: "{account.id}",
                        div {
                            h3 { {match account.game_kind.as_str() {
                                "genshin" => preferences.text("dashboard.genshin_impact"),
                                "starrail" => preferences.text("dashboard.honkai_star_rail"),
                                _ => &account.game_kind,
                            }} }
                            p { {preferences.format("accounts.identity", &[("uid", if account.game_uid.is_empty() { preferences.text("accounts.missing") } else { &account.game_uid }), ("region", &account.region)])} }
                            p {
                                {preferences.text("accounts.detected")} " · "
                                {if account.detected { preferences.text("accounts.yes") } else { preferences.text("accounts.no") }}
                                " · " {preferences.text("accounts.last_detected")} " · {account.last_detected_at}"
                            }
                        }
                        if controls {
                            label { class: "checkbox-row", r#for: "account-visible-{account.id}",
                                input { id: "account-visible-{account.id}", r#type: "checkbox", checked: account.visible,
                                    disabled: state.read().busy || !state.read().ready(),
                                    "aria-label": preferences.format("accounts.visible_label", &[("game", &account.game_kind), ("uid", &account.game_uid)]),
                                    onchange: move |event| {
                                        let id = account.id;
                                        let visible = event.checked();
                                        if !state.read().ready() { return; }
                                        spawn_action(state, move |generation| async move {
                                            match crate::api::send("PATCH", &format!("/game-accounts/{id}"), Some(json!({"visible": visible}))).await {
                                                Ok(()) if generation_matches(state, generation) => {
                                                    set_status(state, "accounts.visibility_updated", "success");
                                                    load_accounts(state).await;
                                                }
                                                Ok(()) => {}
                                                Err(error) => handle_api_error_for(state, generation, error),
                                            }
                                        });
                                    }
                                }
                                {preferences.text("accounts.visible")}
                            }
                        } else {
                            span { {if account.visible { preferences.text("accounts.visible") } else { preferences.text("accounts.hidden") }} }
                        }
                    }
                }
            }
        }
    }
}
