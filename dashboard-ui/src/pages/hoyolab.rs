use super::common::{Field, Metric};
use crate::app::actions::{handle_api_error_for, spawn_action};
use crate::app::display::{display_date, humanize};
use crate::app::session::bootstrap;
use crate::app::state::AppState;
use crate::app::state::{generation_matches, set_status};
use crate::components::{
    Button, ButtonVariant, Card, CardContent, CardDescription, CardFooter, CardHeader, CardTitle,
};
use dioxus::prelude::*;
use serde_json::json;

#[component]
pub(super) fn LinkPage(state: Signal<AppState>) -> Element {
    rsx! {
        div { class: "page-stack",
            h2 { "HoYoLAB Link" }
            CookieCard { state }
        }
    }
}

#[component]
pub(super) fn CookieCard(mut state: Signal<AppState>) -> Element {
    let mut cookie = use_signal(String::new);
    let hoyolab = state.read().hoyolab.clone();
    let onboarding = state.read().me.as_ref().map(|me| me.onboarding.clone());
    rsx! {
        Card {
            CardHeader { CardTitle { "HoYoLAB Cookie" } CardDescription { "One Dashboard user links to one HoYoLAB account. Save a replacement Cookie for the same account and ask the server to verify/sync when available." } }
            CardContent { class: "form-stack",
                dl { class: "metric-list",
                    Metric { label: "Status", value: onboarding.as_ref().map(|o| humanize(&o.cookie_status)).unwrap_or_else(|| hoyolab.as_ref().map(|h| humanize(&h.status)).unwrap_or_else(|| "Not loaded".to_owned())) }
                    Metric { label: "HoYoLAB UID", value: hoyolab.as_ref().and_then(|h| h.hoyolab_uid.clone()).unwrap_or_else(|| "Only shown after backend verification".to_owned()) }
                    Metric { label: "Verification", value: if onboarding.as_ref().is_some_and(|o| o.verification_available) || hoyolab.as_ref().is_some_and(|h| h.verification_available) { "Available".to_owned() } else { "Unavailable; configure Cookie storage on the server".to_owned() } }
                    Metric { label: "Updated", value: hoyolab.as_ref().map(|h| display_date(&h.updated_at)).unwrap_or_else(|| "Not reported".to_owned()) }
                }
                Field { id: "hoyolab-cookie", label: "Paste one HoYoLAB Cookie",
                    textarea { id: "hoyolab-cookie", autocomplete: "off", value: "{cookie}", oninput: move |event| cookie.set(event.value()) }
                }
            }
            CardFooter { class: "button-row",
                Button { r#type: "button", onclick: move |_| {
                    let secret = std::mem::take(&mut *cookie.write());
                    let state = state;
                    spawn_action(state, move |generation| async move {
                                match crate::api::send("PUT", "/me/hoyolab/cookie", Some(json!({"cookie": secret}))).await {
                            Ok(()) if generation_matches(state, generation) => { set_status(state, "Cookie stored for server verification.", "success"); bootstrap(state).await; }
                            Ok(()) => {}
                            Err(error) => { handle_api_error_for(state, generation, error); }
                        }
                    });
                }, "Save Cookie" }
                Button { variant: ButtonVariant::Secondary, r#type: "button", onclick: move |_| { let state = state; spawn_action(state, move |generation| async move {
                                match crate::api::send("POST", "/me/hoyolab/verify", Some(json!({}))).await {
                        Ok(()) if generation_matches(state, generation) => { set_status(state, "Verification requested.", "success"); bootstrap(state).await; }
                        Ok(()) => {}
                        Err(error) => { handle_api_error_for(state, generation, error); }
                    }
                }); }, "Verify binding" }
                Button { variant: ButtonVariant::Secondary, r#type: "button", onclick: move |_| { let state = state; spawn_action(state, move |generation| async move {
                                match crate::api::send("POST", "/me/hoyolab/sync", Some(json!({}))).await {
                        Ok(()) if generation_matches(state, generation) => { set_status(state, "Sync requested.", "success"); bootstrap(state).await; }
                        Ok(()) => {}
                        Err(error) => { handle_api_error_for(state, generation, error); }
                    }
                }); }, "Sync metadata" }
            }
        }
    }
}
