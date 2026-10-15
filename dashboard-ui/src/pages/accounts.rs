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
            h2 { {preferences.text("ゲーム表示", "Game visibility")} }
            if let Some(error) = &state.read().accounts_error { p { class: "warning", "{error}" } }
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
            p { {preferences.text("ゲームアカウントはありません", "No game accounts returned.")} }
        } else {
            div { class: "account-list",
                for account in accounts {
                    div { class: "setting-row", key: "{account.id}",
                        div {
                            h3 { {match account.game_kind.as_str() {
                                "genshin" => preferences.text("原神", "Genshin Impact"),
                                "starrail" => preferences.text("崩壊：スターレイル", "Honkai: Star Rail"),
                                _ => &account.game_kind,
                            }} }
                            p { "UID · "
                                if account.game_uid.is_empty() { {preferences.text("欠落", "Missing")} } else { "{account.game_uid}" }
                                " · " {preferences.text("地域", "Region")} " · {account.region}"
                            }
                            p {
                                {preferences.text("検出", "Detected")} " · "
                                {if account.detected { preferences.text("あり", "Yes") } else { preferences.text("なし", "No") }}
                                " · " {preferences.text("最終検出", "Last detected")} " · {account.last_detected_at}"
                            }
                        }
                        if controls {
                            label { class: "checkbox-row", r#for: "account-visible-{account.id}",
                                input { id: "account-visible-{account.id}", r#type: "checkbox", checked: account.visible,
                                    disabled: state.read().busy || !state.read().ready(),
                                    "aria-label": format!("{} {} {}", preferences.text("表示", "Visible"), account.game_kind, account.game_uid),
                                    onchange: move |event| {
                                        let id = account.id;
                                        let visible = event.checked();
                                        if !state.read().ready() { return; }
                                        spawn_action(state, move |generation| async move {
                                            match crate::api::send("PATCH", &format!("/game-accounts/{id}"), Some(json!({"visible": visible}))).await {
                                                Ok(()) if generation_matches(state, generation) => {
                                                    set_status(state, preferences.text("表示設定を更新しました", "Visibility updated."), "success");
                                                    load_accounts(state).await;
                                                }
                                                Ok(()) => {}
                                                Err(error) => handle_api_error_for(state, generation, error),
                                            }
                                        });
                                    }
                                }
                                {preferences.text("表示", "Visible")}
                            }
                        } else {
                            span { {if account.visible { preferences.text("表示", "Visible") } else { preferences.text("非表示", "Hidden") }} }
                        }
                    }
                }
            }
        }
    }
}
