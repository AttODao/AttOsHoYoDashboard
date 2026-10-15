use super::common::{Field, Metric, PasswordInput};
use crate::app::actions::{handle_api_error_for, handle_password_error_for, spawn_action};
use crate::app::data::load_passkeys;
use crate::app::preferences::use_preferences;
use crate::app::session::{bootstrap, refresh_me};
use crate::app::state::{AppState, Phase, generation_matches, set_status};
use crate::components::{Button, ButtonVariant};
use dioxus::prelude::*;
use serde_json::{Value, json};

#[component]
pub(super) fn SecurityPage(state: Signal<AppState>) -> Element {
    let ui = use_preferences();
    let server = state.read().server.clone();
    rsx! {
        div { class: "page-stack",
            h2 { {ui.text("認証・セキュリティ", "Security")} }
            PasswordCard { state, title: ui.text("パスワードを変更", "Change password") }
            PasskeyCard { state }
            section { class: "panel",
                h3 { {ui.text("セッション", "Session")} }
                dl { class: "metric-list",
                    Metric { label: ui.text("セッション有効期間", "Session lifetime"), value: server.as_ref().map(|s| format!("{} {}", s.session_ttl_days, ui.text("日", "days"))).unwrap_or_else(|| ui.text("未取得", "Not loaded").to_owned()) }
                }
            }
        }
    }
}

// Only the mandatory setup path may omit current_password. Never retain a login secret.
fn password_body(mandatory: bool, current: String, next: String) -> Value {
    if mandatory {
        json!({"new_password": next})
    } else {
        json!({"current_password": current, "new_password": next})
    }
}

fn valid_password_change(mandatory: bool, current: &str, next: &str, confirm: &str) -> bool {
    (mandatory || !current.is_empty()) && (12..=256).contains(&next.len()) && next == confirm
}

#[component]
pub(super) fn PasswordCard(mut state: Signal<AppState>, title: &'static str) -> Element {
    let ui = use_preferences();
    let mut current = use_signal(String::new);
    let mut new_password = use_signal(String::new);
    let mut confirmation = use_signal(String::new);
    let mandatory = state
        .read()
        .me
        .as_ref()
        .is_some_and(|me| me.onboarding.password_change_required);
    use_drop(move || {
        current.write().clear();
        new_password.write().clear();
        confirmation.write().clear();
    });
    rsx! {
        section { class: "panel",
            h3 { "{title}" }
            form { onsubmit: move |event| {
                event.prevent_default();
                if state.read().busy { return; }
                let current_password = std::mem::take(&mut *current.write());
                let next = std::mem::take(&mut *new_password.write());
                let confirm = std::mem::take(&mut *confirmation.write());
                // Read the current server gate at submission, not a captured setup reason.
                let mandatory = state.read().me.as_ref().is_some_and(|me| me.onboarding.password_change_required);
                if !valid_password_change(mandatory, &current_password, &next, &confirm) {
                    set_status(state, ui.text("新しいパスワードは12〜256バイトで確認欄と一致する必要があります。通常の変更には現在のパスワードも必要です。", "Use a matching new password of 12–256 bytes. Ordinary changes also require your current password."), "error");
                    return;
                }
                drop(confirm);
                let body = password_body(mandatory, current_password, next);
                let owner = state.read().me.as_ref().map(|me| me.id);
                spawn_action(state, move |generation| async move {
                    match crate::api::send("PUT", "/me/password", Some(body)).await {
                        Ok(()) if generation_matches(state, generation) => {
                            if refresh_me(state).await && generation_matches(state, generation) {
                                // Bootstrap loads the newly available page only after server readiness.
                                let ready = state.read().ready();
                                if ready { bootstrap(state).await; }
                                let expected = if ready { generation.saturating_add(1) } else { generation };
                                let same_owner = state.read().phase == Phase::SignedIn
                                    && state.read().me.as_ref().map(|me| me.id) == owner;
                                if same_owner && generation_matches(state, expected) {
                                    set_status(state, ui.text("パスワードを変更しました。", "Password changed."), "success");
                                }
                            }
                        }
                        Ok(()) => {}
                        Err(error) if mandatory => { handle_api_error_for(state, generation, error); }
                        Err(error) => { handle_password_error_for(state, generation, error).await; }
                    }
                });
            },
                div { class: "form-grid",
                    if !mandatory {
                        Field { id: "current-password", label: ui.text("現在のパスワード", "Current password"),
                            PasswordInput { id: "current-password", value: current, autocomplete: "current-password" }
                        }
                    }
                    Field { id: "new-password", label: ui.text("新しいパスワード", "New password"),
                        PasswordInput { id: "new-password", value: new_password, autocomplete: "new-password" }
                    }
                    Field { id: "confirm-password", label: ui.text("新しいパスワード（確認）", "Confirm new password"),
                        PasswordInput { id: "confirm-password", value: confirmation, autocomplete: "new-password" }
                    }
                }
                div { class: "auth-submit",
                    Button { r#type: "submit", disabled: state.read().busy, {ui.text("パスワードを変更", "Change password")} }
                }
            }
        }
    }
}

#[component]
pub(super) fn PasskeyCard(mut state: Signal<AppState>) -> Element {
    let ui = use_preferences();
    let mut label = use_signal(String::new);
    use_drop(move || {
        label.write().clear();
    });
    rsx! {
        section { class: "panel",
            h3 { {ui.text("パスキー", "Passkeys")} }
            if let Some(error) = &state.read().passkeys_error { p { class: "warning", "{error}" } }
            if state.read().passkeys.is_empty() && state.read().passkeys_error.is_none() {
                p { {ui.text("登録済みのパスキーはありません。", "No passkeys registered.")} }
            } else {
                ul { class: "item-list",
                    for passkey in state.read().passkeys.clone() {
                        li {
                            span { "{passkey.label} — " {ui.text("登録日時", "Created")} " {passkey.created_at}" }
                            Button { variant: ButtonVariant::Destructive, r#type: "button", onclick: move |_| {
                                let id = passkey.id;
                                spawn_action(state, move |generation| async move {
                                    match crate::api::send("DELETE", &format!("/me/passkeys/{id}"), None).await {
                                        Ok(()) if generation_matches(state, generation) => {
                                            set_status(state, ui.text("パスキーを削除しました。", "Passkey deleted."), "success");
                                            load_passkeys(state).await;
                                        }
                                        Ok(()) => {}
                                        Err(error) => { handle_api_error_for(state, generation, error); }
                                    }
                                });
                            }, {ui.text("削除", "Delete")} }
                        }
                    }
                }
            }
            form { onsubmit: move |event| {
                event.prevent_default();
                if state.read().busy { return; }
                let label_value = std::mem::take(&mut *label.write());
                spawn_action(state, move |generation| async move {
                    match crate::webauthn::register(&label_value).await {
                        Ok(()) if generation_matches(state, generation) => {
                            set_status(state, ui.text("パスキーを登録しました。", "Passkey registered."), "success");
                            load_passkeys(state).await;
                        }
                        Ok(()) => {}
                        Err(error) => { handle_api_error_for(state, generation, error); }
                    }
                });
            },
                Field { id: "passkey-label", label: ui.text("新しいパスキーの名前", "New passkey label"),
                    input { id: "passkey-label", maxlength: "100", autocomplete: "off", value: "{label}", oninput: move |event| label.set(event.value()) }
                }
                div { class: "auth-submit",
                    Button { variant: ButtonVariant::Secondary, r#type: "submit", disabled: state.read().busy, {ui.text("パスキーを登録", "Register passkey")} }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mandatory_password_payload_omits_current_but_security_keeps_it() {
        let setup = password_body(true, String::new(), "synthetic-new-password".to_owned());
        assert!(setup.get("current_password").is_none());
        assert_eq!(setup["new_password"], "synthetic-new-password");
        let ordinary = password_body(
            false,
            "synthetic-current".to_owned(),
            "synthetic-new-password".to_owned(),
        );
        assert_eq!(ordinary["current_password"], "synthetic-current");
    }

    #[test]
    fn confirmation_and_utf8_byte_limits_apply_to_both_password_flows() {
        let next = "synthetic-password";
        assert!(valid_password_change(true, "", next, next));
        assert!(!valid_password_change(false, "", next, next));
        assert!(valid_password_change(false, "current", next, next));
        assert!(!valid_password_change(true, "", next, "different"));
        for length in [11, 257] {
            let next = "a".repeat(length);
            assert!(!valid_password_change(true, "", &next, &next));
        }
        for length in [12, 256] {
            let next = "a".repeat(length);
            assert!(valid_password_change(true, "", &next, &next));
        }
        let unicode = "あ".repeat(86);
        assert!(!valid_password_change(true, "", &unicode, &unicode));
    }
}
