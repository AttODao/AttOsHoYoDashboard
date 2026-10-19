use super::common::{Field, Metric};
use crate::app::actions::{handle_api_error_for, spawn_action};
use crate::app::preferences::use_preferences;
use crate::app::session::refresh_gate;
use crate::app::state::{AppState, generation_matches, set_status};
use crate::components::{Button, ButtonVariant};
use dioxus::prelude::*;
use serde_json::{Value, json};

#[component]
pub(super) fn LinkPage(state: Signal<AppState>) -> Element {
    let preferences = use_preferences();
    rsx! {
        section { class: "panel page-stack",
            h2 { {preferences.text("shell.hoyolab_link")} }
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
        Some("verified") => preferences.text("hoyolab.verified"),
        Some("pending_verification") => preferences.text("hoyolab.pending_verification"),
        Some("not_configured" | "missing") => preferences.text("hoyolab.not_configured"),
        Some("cookie_invalid" | "invalid") => preferences.text("dashboard.invalid"),
        Some("unreadable") => preferences.text("hoyolab.unreadable"),
        Some("storage_unavailable") => preferences.text("hoyolab.storage_unavailable"),
        Some(other) => other,
        None => preferences.text("hoyolab.not_loaded"),
    }
    .to_owned();
    rsx! {
        div { class: "form-stack",
            if !password_ok { p { class: "warning", {preferences.text("hoyolab.password_change_required")} } }
            if let Some(error) = &state.read().hoyolab_error { p { class: "warning", {preferences.text(error)} } }
            dl { class: "metric-list",
                Metric { label: preferences.text("hoyolab.cookie_status"), value: status }
                Metric { label: preferences.text("hoyolab.hoyolab_uid"), value: hoyolab.as_ref().and_then(|h| h.hoyolab_uid.clone()).unwrap_or_else(|| preferences.text("hoyolab.not_reported").to_owned()) }
                Metric { label: preferences.text("hoyolab.storage_verification"), value: match available {
                    Some(true) => preferences.text("hoyolab.available"),
                    Some(false) => preferences.text("hoyolab.unavailable"),
                    None => preferences.text("hoyolab.not_loaded"),
                }.to_owned() }
                Metric { label: preferences.text("hoyolab.updated"), value: hoyolab.as_ref().and_then(|h| h.updated_at.clone()).unwrap_or_else(|| preferences.text("hoyolab.not_reported").to_owned()) }
            }
            Field { id: "hoyolab-cookie", label: preferences.text("hoyolab.cookie"),
                textarea { id: "hoyolab-cookie", autocomplete: "off", disabled, value: "{cookie}", oninput: move |event| cookie.set(event.value()) }
            }
            div { class: "button-row",
                Button { r#type: "button", disabled: disabled || cookie().trim().is_empty(), onclick: move |_| {
                    if disabled || !state.read().password_ok() || cookie().trim().is_empty() { return; }
                    let secret = std::mem::take(&mut *cookie.write());
                    spawn_cookie_action(state, "PUT", "/me/hoyolab/cookie", json!({"cookie": secret}), "hoyolab.cookie_save_accepted");
                }, {preferences.text("hoyolab.save")} }
                Button { variant: ButtonVariant::Secondary, r#type: "button", disabled: disabled || !stored, onclick: move |_| {
                    if disabled || !stored || !state.read().password_ok() { return; }
                    spawn_cookie_action(state, "POST", "/me/hoyolab/verify", json!({}), "hoyolab.verification_response_received");
                }, {preferences.text("hoyolab.verify")} }
                Button { variant: ButtonVariant::Secondary, r#type: "button", disabled: disabled || !complete, onclick: move |_| {
                    if disabled || !complete || !state.read().password_ok() { return; }
                    spawn_cookie_action(state, "POST", "/me/hoyolab/sync", json!({}), "hoyolab.sync_response_received");
                }, {preferences.text("hoyolab.sync_uids")} }
            }
        }
    }
}

// Linkage mutations can move the onboarding gate either way; refresh it in place
// so the acknowledgement stays visible.
fn spawn_cookie_action(
    state: Signal<AppState>,
    method: &'static str,
    path: &'static str,
    body: Value,
    success: &'static str,
) {
    spawn_action(state, move |generation| async move {
        match crate::api::send(method, path, Some(body)).await {
            Ok(()) if generation_matches(state, generation) => {
                set_status(state, success, "success");
                refresh_gate(state, generation).await;
            }
            Ok(()) => {}
            Err(error) => handle_api_error_for(state, generation, error),
        }
    });
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
