use super::common::{Field, PasswordInput, use_modal};
use crate::app::actions::spawn_action;
use crate::app::credentials::{self, CopyState, CredentialResult};
use crate::app::data::load_admin_users;
use crate::app::preferences::use_preferences;
use crate::app::session::{refresh_me, signed_out};
use crate::app::state::{AppState, Page, Phase, generation_matches, set_page, set_status};
use crate::components::{Button, ButtonVariant, Card, CardContent, CardHeader, CardTitle};
use crate::models::UserView;
use dioxus::prelude::*;
use serde_json::json;

fn is_admin(state: Signal<AppState>) -> bool {
    let read = state.read();
    read.phase == Phase::SignedIn
        && read.password_ok()
        && read.me.as_ref().is_some_and(|me| me.active && me.is_admin)
}

fn current_admin(state: Signal<AppState>, generation: u64) -> bool {
    generation_matches(state, generation) && is_admin(state)
}

fn protected_last_admin(user: &UserView, users: &[UserView]) -> bool {
    user.active
        && user.is_admin
        && users
            .iter()
            .filter(|user| user.active && user.is_admin)
            .count()
            <= 1
}

fn is_own_user(state: Signal<AppState>, target: uuid::Uuid) -> bool {
    state.read().me.as_ref().is_some_and(|me| me.id == target)
}

// UI feedback only; the backend remains the authority on lifecycle policy.
fn guard_user_change(state: Signal<AppState>, target: uuid::Uuid) -> bool {
    if !is_admin(state) || state.read().busy {
        return false;
    }
    let last_admin = {
        let read = state.read();
        let Some(user) = read.admin_users.iter().find(|user| user.id == target) else {
            return false;
        };
        protected_last_admin(user, &read.admin_users)
    };
    if last_admin {
        set_status(
            state,
            "admin.the_last_active_administrator_cannot_be_disabled_or_deleted",
            "error",
        );
        return false;
    }
    !is_own_user(state, target)
}

#[component]
pub(super) fn AdminPage(state: Signal<AppState>) -> Element {
    let prefs = use_preferences();
    rsx! { div { class: "page-stack", h1 { {prefs.text("admin.admin_settings")} } AdminCard { state } } }
}

#[component]
fn AdminCard(state: Signal<AppState>) -> Element {
    let prefs = use_preferences();
    let mut username = use_signal(String::new);
    let mut result = use_signal(|| None::<CredentialResult>);
    let generation = state.read().generation;
    use_effect(move || {
        let generation = state.read().generation;
        if let Ok(mut value) = result.try_write() {
            credentials::clear_stale(&mut value, generation);
        }
    });
    // The existing owner-key boundary unmounts this signal on account changes.
    if !is_admin(state) {
        return rsx! { p { role: "alert", {prefs.text("admin.administrator_access_required")} } };
    }
    rsx! {
        div { class: "page-stack",
            Card {
                CardHeader { CardTitle { {prefs.text("admin.create_user")} } }
                CardContent { class: "form-stack",
                    Field { id: "admin-create-username", label: prefs.text("admin.username"),
                        input { id: "admin-create-username", autocomplete: "off", value: "{username}", oninput: move |event| username.set(event.value()) }
                    }
                    Button { r#type: "button", disabled: state.read().busy || username.read().is_empty(), onclick: move |_| {
                        if !is_admin(state) || state.read().busy { return; }
                        result.set(None);
                        let user = username.read().clone();
                        spawn_action(state, move |generation| async move {
                            if !current_admin(state, generation) { return; }
                            let Ok(secret) = credentials::generate() else {
                                set_status(state, "admin.could_not_generate_a_secure_password_try_again", "error");
                                return;
                            };
                            let outcome = credentials::create(&user, &secret).await.map(|user| user.username);
                            match CredentialResult::after_success(outcome, secret, generation) {
                                Ok(value) if current_admin(state, generation) => {
                                    if let Ok(mut result) = result.try_write() { *result = Some(value); }
                                    if let Ok(mut username) = username.try_write() { username.clear(); }
                                    load_admin_users(state).await;
                                }
                                Ok(_) => {}
                                Err(error) => admin_error(state, generation, error, "admin.could_not_create_user", "admin.please_sign_in_again"),
                            }
                        });
                    }, {prefs.text("admin.create_user_2")} }
                }
            }
            Card {
                CardHeader { CardTitle { {prefs.text("admin.manage_users")} } }
                CardContent { class: "form-stack",
                    if state.read().admin_error.is_some() { p { class: "warning", role: "alert", {prefs.text("admin.could_not_load_users")} } }
                    if state.read().admin_users.is_empty() && state.read().admin_error.is_none() { p { {prefs.text("admin.no_users")} } }
                    if !state.read().admin_users.is_empty() {
                        div { class: "table-wrap", role: "region", tabindex: "0", "aria-label": prefs.text("admin.manage_users"),
                            table { "aria-label": prefs.text("admin.manage_users"),
                                thead { tr {
                                    th { scope: "col", {prefs.text("admin.username")} }
                                    th { scope: "col", {prefs.text("admin.status")} }
                                    th { scope: "col", {prefs.text("admin.actions")} }
                                } }
                                tbody {
                                    for user in state.read().admin_users.clone() { AdminUserRow { key: "{user.id}", state, user, result } }
                                }
                            }
                        }
                    }
                }
            }
            Card {
                CardHeader { CardTitle { {prefs.text("admin.logs")} } }
                CardContent {
                    p { class: "muted", {prefs.text("admin.api_unavailable")} }
                    Button { r#type: "button", disabled: true, {prefs.text("admin.load_logs")} }
                }
            }
            if result.read().as_ref().is_some_and(|value| value.generation == generation) {
                CredentialDialog { state, result }
            }
        }
    }
}

fn admin_error(
    state: Signal<AppState>,
    generation: u64,
    error: crate::api::ApiError,
    message: &'static str,
    sign_in: &'static str,
) {
    if !current_admin(state, generation) {
        return;
    }
    if error.is_unauthorized() {
        signed_out(state, sign_in);
    } else {
        set_status(state, message, "error");
    }
}

#[component]
fn AdminUserRow(
    state: Signal<AppState>,
    user: UserView,
    result: Signal<Option<CredentialResult>>,
) -> Element {
    let prefs = use_preferences();
    let mut deleting = use_signal(|| false);
    let last_admin = protected_last_admin(&user, &state.read().admin_users);
    let own_account = is_own_user(state, user.id);
    let reset_name = user.username.clone();
    let status_class = if user.active { "status" } else { "status off" };
    rsx! {
        tr {
            th { scope: "row",
                "{user.username}"
                if user.is_admin { " " span { class: "role-chip", {prefs.text("admin.admin")} } }
            }
            td {
                span { class: status_class, {if user.active { prefs.text("admin.active") } else { prefs.text("admin.inactive") }} }
            }
            td {
                div { class: "actions admin-user-actions",
                Button { variant: ButtonVariant::Secondary, r#type: "button", disabled: state.read().busy || result.read().is_some(), onclick: move |_| {
                    if !is_admin(state) || state.read().busy { return; }
                    // Self-reissue revokes the session before its generated secret can be delivered.
                    if is_own_user(state, user.id) { set_page(state, Page::Security); return; }
                    let id = user.id;
                    let name = reset_name.clone();
                    let mut result = result;
                    result.set(None);
                    spawn_action(state, move |generation| async move {
                        if !current_admin(state, generation) { return; }
                        let Ok(secret) = credentials::generate() else {
                            set_status(state, "admin.could_not_generate_a_secure_password_try_again", "error");
                            return;
                        };
                        let outcome = crate::api::send("PUT", &format!("/admin/users/{id}/temporary-password"), Some(json!({"temporary_password": &secret}))).await.map(|()| name);
                        match CredentialResult::after_success(outcome, secret, generation) {
                            Ok(value) if current_admin(state, generation) => {
                                if let Ok(mut result) = result.try_write() { *result = Some(value); }
                                if refresh_me(state).await && current_admin(state, generation) { load_admin_users(state).await; }
                            }
                            Ok(_) => {}
                            Err(error) => admin_error(state, generation, error, "admin.could_not_reissue_password", "admin.please_sign_in_again"),
                        }
                    });
                }, {if own_account { prefs.text("admin.change_password") } else { prefs.text("admin.reissue_password") }} }
                Button { variant: ButtonVariant::Secondary, r#type: "button", disabled: state.read().busy || (own_account && !last_admin), onclick: move |_| {
                    if !guard_user_change(state, user.id) { return; }
                    let id = user.id;
                    let active = !user.active;
                    spawn_action(state, move |generation| async move {
                        if !current_admin(state, generation) { return; }
                        match crate::api::send("PATCH", &format!("/admin/users/{id}"), Some(json!({"active": active}))).await {
                            Ok(()) if current_admin(state, generation) => {
                                set_status(state, "admin.user_updated", "success");
                                if refresh_me(state).await && current_admin(state, generation) { load_admin_users(state).await; }
                            }
                            Ok(()) => {}
                            Err(error) => admin_error(state, generation, error, "admin.could_not_update_user", "admin.please_sign_in_again"),
                        }
                    });
                }, {if user.active { prefs.text("admin.disable") } else { prefs.text("admin.re_enable") }} }
                Button { variant: ButtonVariant::Destructive, r#type: "button", disabled: state.read().busy || (own_account && !last_admin), onclick: move |_| {
                    if guard_user_change(state, user.id) { deleting.set(true); }
                }, {prefs.text("deletion.delete_user")} }
                }
                if deleting() { DeleteUserDialog { state, user, deleting } }
            }
        }
    }
}

#[component]
fn DeleteUserDialog(
    state: Signal<AppState>,
    user: UserView,
    mut deleting: Signal<bool>,
) -> Element {
    let prefs = use_preferences();
    let mut current = use_signal(String::new);
    let mut confirmation = use_signal(String::new);
    use_effect(move || {
        let _generation = state.read().generation;
        current.set(String::new());
        confirmation.set(String::new());
    });
    let id = user.id;
    let password_id = format!("admin-delete-password-{id}");
    let confirm_id = format!("admin-delete-confirm-{id}");
    rsx! {
        AdminDialog { id: format!("admin-delete-{id}"), title: prefs.text("deletion.delete_user"), onclose: move |_| { current.set(String::new()); confirmation.set(String::new()); deleting.set(false); },
            p { "{user.username}" }
            Field { id: password_id.clone(), label: prefs.text("admin.your_current_password"),
                PasswordInput { id: password_id, value: current, autocomplete: "current-password" }
            }
            Field { id: confirm_id.clone(), label: prefs.text("admin.exact_target_username"),
                input { id: confirm_id, autocomplete: "off", value: "{confirmation}", oninput: move |event| confirmation.set(event.value()) }
            }
            div { class: "actions",
                Button { variant: ButtonVariant::Secondary, r#type: "button", onclick: move |_| { current.set(String::new()); confirmation.set(String::new()); deleting.set(false); }, {prefs.text("admin.cancel")} }
                Button { variant: ButtonVariant::Destructive, r#type: "button", disabled: state.read().busy || current.read().is_empty() || *confirmation.read() != user.username, onclick: move |_| {
                    if !guard_user_change(state, user.id) || current.read().is_empty() || *confirmation.read() != user.username { return; }
                    let pass = std::mem::take(&mut *current.write());
                    let confirm = std::mem::take(&mut *confirmation.write());
                    spawn_action(state, move |generation| async move {
                        if !current_admin(state, generation) { return; }
                        match crate::api::send("DELETE", &format!("/admin/users/{id}"), Some(json!({"current_password": pass, "confirmation": confirm}))).await {
                            Ok(()) if current_admin(state, generation) => {
                                if let Ok(mut deleting) = deleting.try_write() { *deleting = false; }
                                set_status(state, "admin.user_deletion_accepted_completion_is_not_confirmed", "success");
                                if refresh_me(state).await && current_admin(state, generation) { load_admin_users(state).await; }
                            }
                            Ok(()) => {}
                            Err(error) if current_admin(state, generation) && error.is_unauthorized() => {
                                // A wrong actor password also returns 401; do not destroy a valid session.
                                if refresh_me(state).await && current_admin(state, generation) {
                                    set_status(state, "admin.check_your_current_password", "error");
                                }
                            }
                            Err(error) => admin_error(state, generation, error, "admin.user_deletion_was_not_accepted", "admin.please_sign_in_again"),
                        }
                    });
                }, {prefs.text("deletion.delete_user")} }
            }
        }
    }
}

#[component]
fn CredentialDialog(
    state: Signal<AppState>,
    mut result: Signal<Option<CredentialResult>>,
) -> Element {
    let prefs = use_preferences();
    use_drop(move || {
        if let Ok(mut value) = result.try_write() {
            *value = None;
        }
    });
    rsx! {
        AdminDialog { id: "admin-credential-result", title: prefs.text("admin.temporary_password_issued"), onclose: move |_| result.set(None),
            if let Some(value) = result.read().as_ref() {
                dl { class: "metric-list", dt { {prefs.text("admin.username")} } dd { "{value.username}" } }
                Field { id: "admin-generated-password", label: prefs.text("admin.temporary_password"),
                    output { id: "admin-generated-password", class: "generated-password", "{value.secret}" }
                }
                p { role: "status", "aria-live": "polite",
                    {match value.copy {
                        CopyState::Ready => "",
                        CopyState::Pending => prefs.text("admin.copying"),
                        CopyState::Copied => prefs.text("admin.copied"),
                        CopyState::Failed => prefs.text("admin.could_not_copy_try_again"),
                    }}
                }
                div { class: "actions",
                    Button { variant: ButtonVariant::Secondary, r#type: "button", disabled: value.copy == CopyState::Pending, onclick: move |_| {
                        let generation = state.read().generation;
                        if !current_admin(state, generation) { result.set(None); return; }
                        let candidate = {
                            let mut write = result.write();
                            let Some(value) = write.as_mut() else { return; };
                            if value.generation != generation || value.copy == CopyState::Pending { return; }
                            value.copy = CopyState::Pending;
                            value.secret.clone()
                        };
                        // Scope-bound task is cancelled when this dialog unmounts.
                        spawn(async move {
                            let outcome = credentials::copy(&candidate).await;
                            if !current_admin(state, generation) { return; }
                            if let Some(value) = result.write().as_mut().filter(|value| value.generation == generation) {
                                value.copy = CopyState::after_write(outcome);
                            }
                        });
                    }, {prefs.text("admin.copy")} }
                    Button { r#type: "button", onclick: move |_| result.set(None), {prefs.text("admin.close")} }
                }
            }
        }
    }
}

#[component]
fn AdminDialog(
    id: String,
    title: &'static str,
    onclose: EventHandler<()>,
    children: Element,
) -> Element {
    let title_id = format!("{id}-title");
    use_modal(id.clone(), true, onclose);
    rsx! {
        dialog { id, class: "admin-dialog", "aria-labelledby": title_id.clone(),
            div { class: "form-stack", h2 { id: title_id, "{title}" } {children} }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn user(id: u128, active: bool, is_admin: bool) -> UserView {
        UserView {
            id: uuid::Uuid::from_u128(id),
            username: "mock-user".into(),
            active,
            is_admin,
            must_change_password: false,
        }
    }

    #[test]
    fn only_another_active_admin_removes_last_admin_protection() {
        let target = user(1, true, true);
        assert!(protected_last_admin(
            &target,
            &[target.clone(), user(2, false, true), user(3, true, false)]
        ));
        assert!(!protected_last_admin(
            &target,
            &[target.clone(), user(2, true, true)]
        ));
        assert!(!protected_last_admin(&user(4, false, true), &[]));
    }

    #[test]
    fn lifecycle_guard_reports_last_admin_only_on_attempt_and_rechecks_current_rows() {
        fn check() -> Element {
            let mut state = use_signal(|| AppState::new(Page::Admin));
            let target = user(1, true, true);
            state.write().phase = Phase::SignedIn;
            state.write().me = Some(
                serde_json::from_value(json!({
                    "id": target.id, "username": "mock-admin", "is_admin": true,
                    "active": true, "must_change_password": false,
                    "onboarding": {"password_change_required": false, "cookie_setup_complete": true,
                        "cookie_status": "verified", "verification_available": false},
                    "capabilities": {"reward_posting_available": false}
                }))
                .unwrap(),
            );
            state.write().admin_users = vec![target.clone()];
            assert!(state.read().status.is_none());
            assert!(!guard_user_change(state, target.id));
            {
                let read = state.read();
                let status = read.status.as_ref().unwrap();
                assert_eq!(
                    status.message_key,
                    "admin.the_last_active_administrator_cannot_be_disabled_or_deleted"
                );
                assert_eq!(status.kind, "error");
            }
            // Another active admin removes last-admin protection, not self protection.
            state.write().admin_users.push(user(2, true, true));
            state.write().status = None;
            assert!(!guard_user_change(state, target.id));
            assert!(state.read().status.is_none());
            assert!(guard_user_change(state, uuid::Uuid::from_u128(2)));
            // Recheck refreshed rows, including changes since a delete dialog opened.
            state.write().admin_users[0].is_admin = false;
            assert!(!guard_user_change(state, uuid::Uuid::from_u128(2)));
            assert!(state.read().status.is_some());
            state.write().status = None;
            state.write().busy = true;
            assert!(!guard_user_change(state, uuid::Uuid::from_u128(2)));
            assert!(state.read().status.is_none());
            state.write().busy = false;
            assert!(!guard_user_change(state, uuid::Uuid::from_u128(99)));
            state.write().phase = Phase::SignedOut;
            assert!(!guard_user_change(state, uuid::Uuid::from_u128(2)));
            assert!(state.read().status.is_none());
            rsx! {}
        }
        VirtualDom::new(check).rebuild_in_place();
    }

    #[test]
    fn administrator_actions_require_signed_in_active_password_ready_actor() {
        fn check() -> Element {
            let mut state = use_signal(|| AppState::new(crate::app::state::Page::Admin));
            assert!(!is_admin(state));
            state.write().phase = Phase::SignedIn;
            assert!(!is_admin(state));
            let actor: crate::models::MeView = serde_json::from_value(json!({
                "id": uuid::Uuid::from_u128(1), "username": "mock-admin", "is_admin": true,
                "active": true, "must_change_password": false,
                "onboarding": {"password_change_required": false, "cookie_setup_complete": false,
                    "cookie_status": "pending", "verification_available": false},
                "capabilities": {"reward_posting_available": false}
            }))
            .unwrap();
            state.write().me = Some(actor);
            assert!(current_admin(state, 0)); // Cookie setup is gated by the shell, not here.
            assert!(!current_admin(state, 1));
            state.write().me.as_mut().unwrap().is_admin = false;
            assert!(!is_admin(state));
            state.write().me.as_mut().unwrap().is_admin = true;
            state.write().me.as_mut().unwrap().active = false;
            assert!(!is_admin(state));
            state.write().me.as_mut().unwrap().active = true;
            state
                .write()
                .me
                .as_mut()
                .unwrap()
                .onboarding
                .password_change_required = true;
            assert!(!is_admin(state));
            rsx! {}
        }
        VirtualDom::new(check).rebuild_in_place();
    }
}
