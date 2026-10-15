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

#[component]
pub(super) fn AdminPage(state: Signal<AppState>) -> Element {
    let prefs = use_preferences();
    rsx! { div { class: "page-stack", h1 { {prefs.text("管理者設定", "Admin settings")} } AdminCard { state, compact: false } } }
}

#[component]
pub(super) fn AdminCard(state: Signal<AppState>, compact: bool) -> Element {
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
        return rsx! { p { role: "alert", {prefs.text("管理者権限が必要です", "Administrator access required")} } };
    }
    rsx! {
        div { class: "page-stack",
            Card {
                CardHeader { CardTitle { {if compact { prefs.text("管理者設定", "Admin settings") } else { prefs.text("ユーザー作成", "Create user") }} } }
                CardContent { class: "form-stack",
                    Field { id: "admin-create-username", label: prefs.text("ユーザー名", "Username"),
                        input { id: "admin-create-username", autocomplete: "off", value: "{username}", oninput: move |event| username.set(event.value()) }
                    }
                    Button { r#type: "button", disabled: state.read().busy || username.read().is_empty(), onclick: move |_| {
                        if !is_admin(state) || state.read().busy { return; }
                        result.set(None);
                        let user = username.read().clone();
                        spawn_action(state, move |generation| async move {
                            if !current_admin(state, generation) { return; }
                            let Ok(secret) = credentials::generate() else {
                                set_status(state, prefs.text("安全なパスワードを生成できませんでした。再試行してください。", "Could not generate a secure password. Try again."), "error");
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
                                Err(error) => admin_error(state, generation, error, prefs.text("ユーザーを作成できませんでした。", "Could not create user."), prefs.text("再ログインしてください。", "Please sign in again.")),
                            }
                        });
                    }, {prefs.text("ユーザーを作成", "Create user")} }
                }
            }
            Card {
                CardHeader { CardTitle { {prefs.text("ユーザー管理", "Manage users")} } }
                CardContent { class: "form-stack",
                    if state.read().admin_error.is_some() { p { class: "warning", role: "alert", {prefs.text("ユーザー一覧を取得できませんでした。", "Could not load users.")} } }
                    if state.read().admin_users.is_empty() && state.read().admin_error.is_none() { p { {prefs.text("ユーザーがいません", "No users")} } }
                    for user in state.read().admin_users.clone() { AdminUserRow { key: "{user.id}", state, user, result } }
                }
            }
            Card {
                CardHeader { CardTitle { {prefs.text("ログ", "Logs")} } }
                CardContent {
                    p { class: "muted", {prefs.text("API未提供", "API unavailable")} }
                    Button { r#type: "button", disabled: true, {prefs.text("ログを取得", "Load logs")} }
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
    rsx! {
        div { class: "setting-row",
            div {
                h3 { "{user.username}" }
                span { class: "status", {if user.active { prefs.text("有効", "Active") } else { prefs.text("無効", "Inactive") }} }
                if user.is_admin { span { class: "role-chip", {prefs.text("管理者", "Admin")} } }
            }
            div { class: "actions",
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
                            set_status(state, prefs.text("安全なパスワードを生成できませんでした。再試行してください。", "Could not generate a secure password. Try again."), "error");
                            return;
                        };
                        let outcome = crate::api::send("PUT", &format!("/admin/users/{id}/temporary-password"), Some(json!({"temporary_password": &secret}))).await.map(|()| name);
                        match CredentialResult::after_success(outcome, secret, generation) {
                            Ok(value) if current_admin(state, generation) => {
                                if let Ok(mut result) = result.try_write() { *result = Some(value); }
                                if refresh_me(state).await && current_admin(state, generation) { load_admin_users(state).await; }
                            }
                            Ok(_) => {}
                            Err(error) => admin_error(state, generation, error, prefs.text("パスワードを再発行できませんでした。", "Could not reissue password."), prefs.text("再ログインしてください。", "Please sign in again.")),
                        }
                    });
                }, {if own_account { prefs.text("パスワード変更", "Change password") } else { prefs.text("パスワード再発行", "Reissue password") }} }
                Button { variant: ButtonVariant::Secondary, r#type: "button", disabled: state.read().busy || last_admin || own_account, onclick: move |_| {
                    if !is_admin(state) || state.read().busy || last_admin || is_own_user(state, user.id) { return; }
                    let id = user.id;
                    let active = !user.active;
                    spawn_action(state, move |generation| async move {
                        if !current_admin(state, generation) { return; }
                        match crate::api::send("PATCH", &format!("/admin/users/{id}"), Some(json!({"active": active}))).await {
                            Ok(()) if current_admin(state, generation) => {
                                set_status(state, prefs.text("ユーザーを更新しました。", "User updated."), "success");
                                if refresh_me(state).await && current_admin(state, generation) { load_admin_users(state).await; }
                            }
                            Ok(()) => {}
                            Err(error) => admin_error(state, generation, error, prefs.text("ユーザーを更新できませんでした。", "Could not update user."), prefs.text("再ログインしてください。", "Please sign in again.")),
                        }
                    });
                }, {if user.active { prefs.text("無効化", "Disable") } else { prefs.text("再有効化", "Re-enable") }} }
                Button { variant: ButtonVariant::Destructive, r#type: "button", disabled: state.read().busy || last_admin || own_account, onclick: move |_| { if is_admin(state) && !state.read().busy && !last_admin && !is_own_user(state, user.id) { deleting.set(true); } }, {prefs.text("ユーザー削除", "Delete user")} }
                if last_admin { p { class: "warning", {prefs.text("最後の有効な管理者は無効化・削除できません。", "The last active administrator cannot be disabled or deleted.")} } }
            }
        }
        if deleting() { DeleteUserDialog { state, user, deleting } }
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
    let last_admin = protected_last_admin(&user, &state.read().admin_users);
    rsx! {
        AdminDialog { id: format!("admin-delete-{id}"), title: prefs.text("ユーザー削除", "Delete user"), onclose: move |_| { current.set(String::new()); confirmation.set(String::new()); deleting.set(false); },
            p { "{user.username}" }
            Field { id: password_id.clone(), label: prefs.text("管理者自身の現在のパスワード", "Your current password"),
                PasswordInput { id: password_id, value: current, autocomplete: "current-password" }
            }
            Field { id: confirm_id.clone(), label: prefs.text("対象名を完全一致で入力", "Exact target username"),
                input { id: confirm_id, autocomplete: "off", value: "{confirmation}", oninput: move |event| confirmation.set(event.value()) }
            }
            div { class: "actions",
                Button { variant: ButtonVariant::Secondary, r#type: "button", onclick: move |_| { current.set(String::new()); confirmation.set(String::new()); deleting.set(false); }, {prefs.text("キャンセル", "Cancel")} }
                Button { variant: ButtonVariant::Destructive, r#type: "button", disabled: state.read().busy || last_admin || current.read().is_empty() || *confirmation.read() != user.username, onclick: move |_| {
                    if !is_admin(state) || state.read().busy || last_admin || *confirmation.read() != user.username { return; }
                    let pass = std::mem::take(&mut *current.write());
                    let confirm = std::mem::take(&mut *confirmation.write());
                    spawn_action(state, move |generation| async move {
                        if !current_admin(state, generation) { return; }
                        match crate::api::send("DELETE", &format!("/admin/users/{id}"), Some(json!({"current_password": pass, "confirmation": confirm}))).await {
                            Ok(()) if current_admin(state, generation) => {
                                if let Ok(mut deleting) = deleting.try_write() { *deleting = false; }
                                set_status(state, prefs.text("ユーザー削除を受け付けました。完了は未確認です。", "User deletion accepted; completion is not confirmed."), "success");
                                if refresh_me(state).await && current_admin(state, generation) { load_admin_users(state).await; }
                            }
                            Ok(()) => {}
                            Err(error) if current_admin(state, generation) && error.is_unauthorized() => {
                                // A wrong actor password also returns 401; do not destroy a valid session.
                                if refresh_me(state).await && current_admin(state, generation) {
                                    set_status(state, prefs.text("現在のパスワードを確認してください。", "Check your current password."), "error");
                                }
                            }
                            Err(error) => admin_error(state, generation, error, prefs.text("ユーザー削除を受け付けられませんでした。", "User deletion was not accepted."), prefs.text("再ログインしてください。", "Please sign in again.")),
                        }
                    });
                }, {prefs.text("ユーザー削除", "Delete user")} }
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
        AdminDialog { id: "admin-credential-result", title: prefs.text("仮パスワードを発行しました", "Temporary password issued"), onclose: move |_| result.set(None),
            if let Some(value) = result.read().as_ref() {
                dl { class: "metric-list", dt { {prefs.text("ユーザー名", "Username")} } dd { "{value.username}" } }
                Field { id: "admin-generated-password", label: prefs.text("仮パスワード", "Temporary password"),
                    output { id: "admin-generated-password", class: "generated-password", "{value.secret}" }
                }
                p { role: "status", "aria-live": "polite",
                    {match value.copy {
                        CopyState::Ready => "",
                        CopyState::Pending => prefs.text("コピー中…", "Copying…"),
                        CopyState::Copied => prefs.text("コピーしました", "Copied"),
                        CopyState::Failed => prefs.text("コピーできませんでした。再試行してください。", "Could not copy. Try again."),
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
                    }, {prefs.text("コピー", "Copy")} }
                    Button { r#type: "button", onclick: move |_| result.set(None), {prefs.text("閉じる", "Close")} }
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
            assert!(current_admin(state, 0)); // Compact onboarding admin flow remains available.
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
