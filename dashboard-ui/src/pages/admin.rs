use super::common::Field;
use crate::app::actions::{handle_api_error_for, spawn_action};
use crate::app::data::load_admin_users;
use crate::app::session::refresh_me;
use crate::app::state::AppState;
use crate::app::state::{generation_matches, set_status};
use crate::components::{
    Button, ButtonVariant, Card, CardContent, CardDescription, CardHeader, CardTitle,
};
use crate::models::UserView;
use dioxus::prelude::*;
use serde_json::json;

#[component]
pub(super) fn AdminPage(state: Signal<AppState>) -> Element {
    rsx! { div { class: "page-stack", h2 { "Admin" } AdminCard { state, compact: false } } }
}

#[component]
pub(super) fn AdminCard(mut state: Signal<AppState>, compact: bool) -> Element {
    let mut username = use_signal(String::new);
    let mut temp_password = use_signal(String::new);
    rsx! {
        Card {
            CardHeader { CardTitle { if compact { "Admin management" } else { "Users" } } CardDescription { "Create users, set temporary recovery passwords, disable/re-enable accounts, or request deletion with exact confirmation." } }
            CardContent { class: "form-stack",
                if let Some(error) = &state.read().admin_error { p { class: "warning", "{error}" } }
                div { class: "inline-form",
                    Field { id: "admin-create-username", label: "Username", input { id: "admin-create-username", autocomplete: "off", value: "{username}", oninput: move |event| username.set(event.value()) } }
                    Field { id: "admin-create-temp", label: "Temporary password", input { id: "admin-create-temp", r#type: "password", autocomplete: "new-password", value: "{temp_password}", oninput: move |event| temp_password.set(event.value()) } }
                    Button { r#type: "button", onclick: move |_| {
                        let user = std::mem::take(&mut *username.write());
                        let temp = std::mem::take(&mut *temp_password.write());
                        let state = state;
                        spawn_action(state, move |generation| async move {
                                match crate::api::send("POST", "/admin/users", Some(json!({"username": user, "temporary_password": temp}))).await {
                                Ok(()) if generation_matches(state, generation) => { set_status(state, "User created.", "success"); load_admin_users(state).await; }
                                Ok(()) => {}
                                Err(error) => { handle_api_error_for(state, generation, error); }
                            }
                        });
                    }, "Create user" }
                }
                AdminTable { state }
            }
        }
    }
}

#[component]
fn AdminTable(state: Signal<AppState>) -> Element {
    rsx! {
        div { class: "table-wrap",
            table {
                thead { tr { th { "Username" } th { "Admin" } th { "Active" } th { "Recovery" } th { "Delete" } } }
                tbody { for user in state.read().admin_users.clone() { AdminUserRow { key: "{user.id}", state, user } } }
            }
        }
    }
}

#[component]
fn AdminUserRow(mut state: Signal<AppState>, user: UserView) -> Element {
    let mut temp = use_signal(String::new);
    let mut current = use_signal(String::new);
    let mut confirmation = use_signal(String::new);
    rsx! {
        tr {
            td { "{user.username}" }
            td { if user.is_admin { "Yes" } else { "No" } }
            td { Button { variant: ButtonVariant::Secondary, r#type: "button", onclick: move |_| { let id = user.id; let active = !user.active; let state = state; spawn_action(state, move |generation| async move {
                                match crate::api::send("PATCH", &format!("/admin/users/{id}"), Some(json!({"active": active}))).await {
                    Ok(()) if generation_matches(state, generation) => { set_status(state, "User updated.", "success"); if refresh_me(state).await { load_admin_users(state).await; } }
                    Ok(()) => {}
                    Err(error) => { handle_api_error_for(state, generation, error); }
                }
            }); }, if user.active { "Disable" } else { "Re-enable" } } }
            td { class: "cell-form",
                input { r#type: "password", autocomplete: "new-password", "aria-label": "Temporary password for {user.username}", value: "{temp}", oninput: move |event| temp.set(event.value()) }
                Button { variant: ButtonVariant::Secondary, r#type: "button", onclick: move |_| { let secret = std::mem::take(&mut *temp.write()); let id = user.id; let state = state; spawn_action(state, move |generation| async move {
                                match crate::api::send("PUT", &format!("/admin/users/{id}/temporary-password"), Some(json!({"temporary_password": secret}))).await {
                        Ok(()) if generation_matches(state, generation) => { set_status(state, "Recovery password set.", "success"); if refresh_me(state).await { load_admin_users(state).await; } }
                        Ok(()) => {}
                        Err(error) => { handle_api_error_for(state, generation, error); }
                    }
                }); }, "Set recovery password" }
            }
            td { class: "cell-form danger-zone",
                input { r#type: "password", autocomplete: "current-password", "aria-label": "Your current password to delete {user.username}", value: "{current}", oninput: move |event| current.set(event.value()) }
                input { autocomplete: "off", "aria-label": "Type username to delete {user.username}", value: "{confirmation}", oninput: move |event| confirmation.set(event.value()) }
                Button { variant: ButtonVariant::Destructive, r#type: "button", onclick: move |_| { let pass = std::mem::take(&mut *current.write()); let confirm = std::mem::take(&mut *confirmation.write()); let id = user.id; let state = state; spawn_action(state, move |generation| async move {
                                match crate::api::send("DELETE", &format!("/admin/users/{id}"), Some(json!({"current_password": pass, "confirmation": confirm}))).await {
                        Ok(()) if generation_matches(state, generation) => { set_status(state, "User deletion accepted.", "success"); if refresh_me(state).await { load_admin_users(state).await; } }
                        Ok(()) => {}
                        Err(error) => { handle_api_error_for(state, generation, error); }
                    }
                }); }, "Delete user" }
            }
        }
    }
}
