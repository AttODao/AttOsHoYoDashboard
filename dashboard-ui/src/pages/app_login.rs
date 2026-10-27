//! Browser approval for a single app sign-in request.
use super::shell::{Header, Status};
use crate::app::actions::{handle_api_error_for, spawn_action};
use crate::app::navigation;
use crate::app::preferences::use_preferences;
use crate::app::state::{AppState, generation_matches};
use crate::components::{Button, ButtonVariant};
use crate::models::AppLoginCode;
use dioxus::prelude::*;
use serde_json::json;

#[component]
pub(in crate::app) fn AppLoginPage(
    state: Signal<AppState>,
    mut request: Signal<Option<String>>,
) -> Element {
    let ui = use_preferences();
    let mut code = use_signal(|| None::<String>);
    let me = state.read().me.clone();
    rsx! {
        Header { state }
        main { class: "auth-layout",
            section { class: "panel auth-panel",
                h1 { {ui.text("auth.app_login_title")} }
                Status { state }
                if let Some(value) = code.read().clone() {
                    p { {ui.text("auth.app_login_ready")} }
                    div { class: "auth-submit",
                        Button { variant: ButtonVariant::Primary, r#type: "button", onclick: move |_| navigation::open_app_callback(&value),
                            {ui.text("auth.app_login_open_app")}
                        }
                    }
                    div { class: "auth-submit",
                        Button { variant: ButtonVariant::Secondary, r#type: "button", onclick: move |_| request.set(None),
                            {ui.text("auth.app_login_cancel")}
                        }
                    }
                } else {
                    if let Some(me) = me {
                        p { {ui.format("auth.app_login_as", &[("username", &me.username)])} }
                    }
                    p { class: "warning", {ui.text("auth.app_login_warning")} }
                    div { class: "auth-submit",
                        Button { variant: ButtonVariant::Primary, r#type: "button", onclick: move |_| {
                            let Some(challenge) = request.read().clone() else { return; };
                            spawn_action(state, move |generation| async move {
                                match crate::api::json::<AppLoginCode>("POST", "/me/app-login", Some(json!({"code_challenge": challenge}))).await {
                                    Ok(value) if generation_matches(state, generation) => code.set(Some(value.code)),
                                    Ok(_) => {},
                                    Err(error) => handle_api_error_for(state, generation, error),
                                }
                            });
                        }, {ui.text("auth.app_login_approve")} }
                    }
                    div { class: "auth-submit",
                        Button { variant: ButtonVariant::Secondary, r#type: "button", onclick: move |_| request.set(None),
                            {ui.text("auth.app_login_cancel")}
                        }
                    }
                }
            }
        }
    }
}
