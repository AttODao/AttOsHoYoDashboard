use super::common::{Field, PasswordInput};
use crate::app::actions::{handle_password_error_for, spawn_action};
use crate::app::preferences::use_preferences;
use crate::app::session::signed_out;
use crate::app::state::{AppState, generation_matches};
use crate::components::{Button, ButtonVariant};
use dioxus::prelude::*;
use serde_json::json;

#[component]
pub(super) fn DeletePage(state: Signal<AppState>) -> Element {
    let preferences = use_preferences();
    rsx! {
        section { class: "panel page-stack danger-zone",
            h2 { {preferences.text("deletion.delete_user")} }
            DeleteAccountCard { state }
        }
    }
}

#[component]
pub(super) fn DeleteAccountCard(state: Signal<AppState>) -> Element {
    let preferences = use_preferences();
    let username = state
        .read()
        .me
        .as_ref()
        .map(|me| me.username.clone())
        .unwrap_or_default();
    let mut password = use_signal(String::new);
    let mut confirmation = use_signal(String::new);
    let disabled = !deletion_confirmed(&username, &confirmation(), &password());
    rsx! {
        div { class: "form-stack",
            div { class: "notice error danger-note", id: "delete-consequences",
                strong { {preferences.text("deletion.irreversible")} }
                p { {preferences.text("deletion.own_consequences")} }
                p { {preferences.text("deletion.own_sign_out")} }
            }
            p { {preferences.text("deletion.target_user")} " · {username}" }
            Field { id: "delete-confirmation", label: preferences.text("deletion.username_exact_match"),
                input { id: "delete-confirmation", autocomplete: "off", value: "{confirmation}", oninput: move |event| confirmation.set(event.value()) }
            }
            Field { id: "delete-password", label: preferences.text("deletion.current_password"),
                PasswordInput { id: "delete-password", value: password, autocomplete: "current-password" }
            }
            div { class: "button-row",
                Button { variant: ButtonVariant::Destructive, r#type: "button", disabled, aria_describedby: "delete-consequences", onclick: move |_| {
                    let current_username = state.read().me.as_ref().map(|me| me.username.clone()).unwrap_or_default();
                    if state.read().busy || !deletion_confirmed(&current_username, &confirmation(), &password()) { return; }
                    let pass = std::mem::take(&mut *password.write());
                    let confirm = confirmation();
                    spawn_action(state, move |generation| async move {
                        match crate::api::send("DELETE", "/me", Some(json!({"current_password": pass, "confirmation": confirm}))).await {
                            Ok(()) if generation_matches(state, generation) => signed_out(state, "deletion.user_deletion_accepted_processing_pending"),
                            Ok(()) => {}
                            Err(error) => { handle_password_error_for(state, generation, error).await; }
                        }
                    });
                }, {preferences.text("deletion.delete_user")} }
            }
        }
    }
}

fn deletion_confirmed(username: &str, confirmation: &str, password: &str) -> bool {
    !username.is_empty() && confirmation == username && !password.is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deletion_requires_exact_username_and_actor_password() {
        assert!(deletion_confirmed(
            "synthetic-user",
            "synthetic-user",
            "synthetic-password"
        ));
        for confirmation in ["", "Synthetic-user", "synthetic-user ", " synthetic-user"] {
            assert!(!deletion_confirmed(
                "synthetic-user",
                confirmation,
                "synthetic-password"
            ));
        }
        assert!(!deletion_confirmed("synthetic-user", "synthetic-user", ""));
        assert!(!deletion_confirmed("", "", "synthetic-password"));
    }
}
