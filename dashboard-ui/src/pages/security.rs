use super::common::Field;
use crate::app::actions::{handle_api_error_for, handle_password_error_for, spawn_action};
use crate::app::data::load_passkeys;
use crate::app::session::bootstrap;
use crate::app::state::{AppState, Phase};
use crate::app::state::{generation_matches, set_status};
use crate::components::{
    Button, ButtonVariant, Card, CardContent, CardDescription, CardFooter, CardHeader, CardTitle,
};
use dioxus::prelude::*;
use serde_json::json;

#[component]
pub(super) fn SecurityPage(state: Signal<AppState>) -> Element {
    rsx! {
        div { class: "page-stack",
            h2 { "Security" }
            div { class: "grid two",
                PasswordCard { state, title: "Change password" }
                PasskeyCard { state }
            }
        }
    }
}

#[component]
pub(super) fn PasswordCard(mut state: Signal<AppState>, title: &'static str) -> Element {
    let mut current = use_signal(String::new);
    let mut new_password = use_signal(String::new);
    rsx! {
        Card {
            CardHeader { CardTitle { "{title}" } CardDescription { "Password changes rotate the server session and refresh onboarding metadata." } }
            CardContent { class: "form-stack",
                Field { id: "current-password", label: "Current password", input { id: "current-password", r#type: "password", autocomplete: "current-password", value: "{current}", oninput: move |event| current.set(event.value()) } }
                Field { id: "new-password", label: "New password", input { id: "new-password", r#type: "password", autocomplete: "new-password", value: "{new_password}", minlength: "12", oninput: move |event| new_password.set(event.value()) } }
            }
            CardFooter { Button { r#type: "button", onclick: move |_| {
                let owner = state.read().me.as_ref().map(|me| me.id);
                let current_password = std::mem::take(&mut *current.write());
                let next = std::mem::take(&mut *new_password.write());
                let state = state;
                spawn_action(state, move |generation| async move {
                                match crate::api::send("PUT", "/me/password", Some(json!({"current_password": current_password, "new_password": next}))).await {
                        Ok(()) if generation_matches(state, generation) => {
                            bootstrap(state).await;
                            if state.read().phase == Phase::SignedIn && state.read().me.as_ref().map(|me| me.id) == owner {
                                set_status(state, "Password changed.", "success");
                            }
                        }
                        Ok(()) => {}
                        Err(error) => { handle_password_error_for(state, generation, error).await; }
                    }
                });
            }, "Change password" } }
        }
    }
}

#[component]
pub(super) fn PasskeyCard(mut state: Signal<AppState>) -> Element {
    let mut label = use_signal(String::new);
    rsx! {
        Card {
            CardHeader { CardTitle { "Passkeys" } CardDescription { "Optional browser passkeys can be added after password setup. Password sign-in remains available." } }
            CardContent { class: "form-stack",
                if let Some(error) = &state.read().passkeys_error { p { class: "warning", "{error}" } }
                Field { id: "passkey-label", label: "New passkey label", input { id: "passkey-label", maxlength: "100", autocomplete: "off", value: "{label}", oninput: move |event| label.set(event.value()) } }
                if state.read().passkeys.is_empty() {
                    p { "No passkeys registered." }
                } else {
                    ul { class: "item-list",
                        for passkey in state.read().passkeys.clone() {
                            li { span { "{passkey.label} — created {passkey.created_at}" } Button { variant: ButtonVariant::Destructive, r#type: "button", onclick: move |_| { let id = passkey.id; let state = state; spawn_action(state, move |generation| async move {
                                match crate::api::send("DELETE", &format!("/me/passkeys/{id}"), None).await {
                                    Ok(()) if generation_matches(state, generation) => { set_status(state, "Passkey deleted.", "success"); load_passkeys(state).await; }
                                    Ok(()) => {}
                                    Err(error) => { handle_api_error_for(state, generation, error); }
                                }
                            }); }, "Delete" } }
                        }
                    }
                }
            }
            CardFooter { Button { variant: ButtonVariant::Secondary, r#type: "button", onclick: move |_| {
                let label_value = std::mem::take(&mut *label.write());
                let state = state;
                spawn_action(state, move |generation| async move {
                                match crate::webauthn::register(&label_value).await {
                        Ok(()) if generation_matches(state, generation) => { set_status(state, "Passkey registered.", "success"); load_passkeys(state).await; }
                        Ok(()) => {}
                        Err(error) => { handle_api_error_for(state, generation, error); }
                    }
                });
            }, "Register passkey" } }
        }
    }
}
