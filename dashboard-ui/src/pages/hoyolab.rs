use super::common::{Field, Metric};
use crate::app::actions::{handle_api_error_for, spawn_action};
use crate::app::preferences::use_preferences;
use crate::app::session::bootstrap;
use crate::app::state::{AppState, generation_matches, set_status};
use crate::components::{Button, ButtonVariant};
use dioxus::prelude::*;
use serde_json::json;

#[component]
pub(super) fn LinkPage(state: Signal<AppState>) -> Element {
    let preferences = use_preferences();
    rsx! {
        section { class: "panel page-stack",
            h2 { {preferences.text("HoYoLAB連携", "HoYoLAB link")} }
            CookieCard { state }
        }
    }
}

#[component]
pub(super) fn CookieCard(state: Signal<AppState>) -> Element {
    let preferences = use_preferences();
    let mut cookie = use_signal(String::new);
    let hoyolab = state.read().hoyolab.clone();
    let onboarding = state.read().me.as_ref().map(|me| me.onboarding.clone());
    let password_ok = state.read().password_ok();
    let available = hoyolab
        .as_ref()
        .map(|h| h.verification_available)
        .or_else(|| onboarding.as_ref().map(|o| o.verification_available));
    let disabled = !cookie_action_allowed(password_ok, state.read().busy, available);
    let stored = hoyolab.as_ref().is_some_and(|h| h.id.is_some());
    let complete = hoyolab.as_ref().is_some_and(|h| h.setup_complete);
    let status = hoyolab
        .as_ref()
        .map(|h| h.status.as_str())
        .or_else(|| onboarding.as_ref().map(|o| o.cookie_status.as_str()));
    let status = match status {
        Some("verified") => preferences.text("本人確認済み", "Verified"),
        Some("pending_verification") => preferences.text("本人確認待ち", "Pending verification"),
        Some("not_configured" | "missing") => preferences.text("未登録", "Not configured"),
        Some("cookie_invalid" | "invalid") => preferences.text("無効", "Invalid"),
        Some("unreadable") => preferences.text("読取不可", "Unreadable"),
        Some("storage_unavailable") => {
            preferences.text("保存機能が利用不可", "Storage unavailable")
        }
        Some(other) => other,
        None => preferences.text("未読込", "Not loaded"),
    }
    .to_owned();
    rsx! {
        div { class: "form-stack",
            if !password_ok { p { class: "warning", {preferences.text("パスワードの変更が必要です", "Password change required")} } }
            if let Some(error) = &state.read().hoyolab_error { p { class: "warning", "{error}" } }
            dl { class: "metric-list",
                Metric { label: preferences.text("Cookie状態", "Cookie status"), value: status }
                Metric { label: preferences.text("HoYoLAB UID", "HoYoLAB UID"), value: hoyolab.as_ref().and_then(|h| h.hoyolab_uid.clone()).unwrap_or_else(|| preferences.text("未報告", "Not reported").to_owned()) }
                Metric { label: preferences.text("保存・本人確認", "Storage / verification"), value: match available {
                    Some(true) => preferences.text("利用可能", "Available"),
                    Some(false) => preferences.text("利用不可", "Unavailable"),
                    None => preferences.text("未読込", "Not loaded"),
                }.to_owned() }
                Metric { label: preferences.text("更新日時", "Updated"), value: hoyolab.as_ref().and_then(|h| h.updated_at.clone()).unwrap_or_else(|| preferences.text("未報告", "Not reported").to_owned()) }
            }
            Field { id: "hoyolab-cookie", label: preferences.text("Cookie", "Cookie"),
                textarea { id: "hoyolab-cookie", autocomplete: "off", disabled, value: "{cookie}", oninput: move |event| cookie.set(event.value()) }
            }
            div { class: "button-row",
                Button { r#type: "button", disabled: disabled || cookie().trim().is_empty(), onclick: move |_| {
                    if disabled || !state.read().password_ok() || cookie().trim().is_empty() { return; }
                    let secret = std::mem::take(&mut *cookie.write());
                    spawn_action(state, move |generation| async move {
                        match crate::api::send("PUT", "/me/hoyolab/cookie", Some(json!({"cookie": secret}))).await {
                            Ok(()) if generation_matches(state, generation) => {
                                set_status(state, preferences.text("Cookieの保存を受け付けました", "Cookie save accepted."), "success");
                                bootstrap(state).await;
                            }
                            Ok(()) => {}
                            Err(error) => handle_api_error_for(state, generation, error),
                        }
                    });
                }, {preferences.text("保存", "Save")} }
                Button { variant: ButtonVariant::Secondary, r#type: "button", disabled: disabled || !stored, onclick: move |_| {
                    if disabled || !stored || !state.read().password_ok() { return; }
                    spawn_action(state, move |generation| async move {
                        match crate::api::send("POST", "/me/hoyolab/verify", Some(json!({}))).await {
                            Ok(()) if generation_matches(state, generation) => {
                                set_status(state, preferences.text("本人確認の応答を受信しました", "Verification response received."), "success");
                                bootstrap(state).await;
                            }
                            Ok(()) => {}
                            Err(error) => handle_api_error_for(state, generation, error),
                        }
                    });
                }, {preferences.text("本人確認", "Verify")} }
                Button { variant: ButtonVariant::Secondary, r#type: "button", disabled: disabled || !complete, onclick: move |_| {
                    if disabled || !complete || !state.read().password_ok() { return; }
                    spawn_action(state, move |generation| async move {
                        match crate::api::send("POST", "/me/hoyolab/sync", Some(json!({}))).await {
                            Ok(()) if generation_matches(state, generation) => {
                                set_status(state, preferences.text("同期の応答を受信しました", "Sync response received."), "success");
                                bootstrap(state).await;
                            }
                            Ok(()) => {}
                            Err(error) => handle_api_error_for(state, generation, error),
                        }
                    });
                }, {preferences.text("UIDを同期", "Sync UIDs")} }
            }
        }
    }
}

fn cookie_action_allowed(password_ok: bool, busy: bool, available: Option<bool>) -> bool {
    password_ok && !busy && available == Some(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cookie_actions_require_password_and_known_server_capability() {
        assert!(cookie_action_allowed(true, false, Some(true)));
        assert!(!cookie_action_allowed(false, false, Some(true)));
        assert!(!cookie_action_allowed(true, true, Some(true)));
        assert!(!cookie_action_allowed(true, false, Some(false)));
        assert!(!cookie_action_allowed(true, false, None));
    }
}
