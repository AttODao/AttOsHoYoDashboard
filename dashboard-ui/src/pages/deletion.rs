use super::common::Field;
use crate::app::actions::{handle_password_error_for, spawn_action};
use crate::app::session::signed_out;
use crate::app::state::AppState;
use crate::app::state::generation_matches;
use crate::components::{
    Button, ButtonVariant, Card, CardContent, CardDescription, CardFooter, CardHeader, CardTitle,
};
use dioxus::prelude::*;
use serde_json::json;

#[component]
pub(super) fn DeletePage(state: Signal<AppState>) -> Element {
    rsx! {
        div { class: "page-stack",
            h2 { "Delete your account" }
            DeleteAccountCard { state }
        }
    }
}

#[component]
pub(super) fn DeleteAccountCard(mut state: Signal<AppState>) -> Element {
    let username = state
        .read()
        .me
        .as_ref()
        .map(|me| me.username.clone())
        .unwrap_or_default();
    let mut password = use_signal(String::new);
    let mut confirmation = use_signal(String::new);
    rsx! {
        Card { class: "danger-zone",
            CardHeader { CardTitle { "Irreversible account deletion" } CardDescription { "Type your username exactly and enter your current password. The backend protects the last active admin." } }
            CardContent { class: "form-stack",
                Field { id: "delete-confirmation", label: "Type username: {username}", input { id: "delete-confirmation", autocomplete: "off", value: "{confirmation}", oninput: move |event| confirmation.set(event.value()) } }
                Field { id: "delete-password", label: "Current password", input { id: "delete-password", r#type: "password", autocomplete: "current-password", value: "{password}", oninput: move |event| password.set(event.value()) } }
            }
            CardFooter { Button { variant: ButtonVariant::Destructive, r#type: "button", onclick: move |_| { let pass = std::mem::take(&mut *password.write()); let confirm = std::mem::take(&mut *confirmation.write()); let state = state; spawn_action(state, move |generation| async move {
                                match crate::api::send("DELETE", "/me", Some(json!({"current_password": pass, "confirmation": confirm}))).await {
                    Ok(()) if generation_matches(state, generation) => signed_out(state, "Account deletion accepted."),
                    Ok(()) => {}
                    Err(error) => { handle_password_error_for(state, generation, error).await; }
                }
            }); }, "Delete my account" } }
        }
    }
}
