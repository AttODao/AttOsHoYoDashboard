use super::common::{Field, Metric};
use crate::app::actions::{handle_api_error_for, spawn_action};
use crate::app::display::datetime_text;
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
    let raw_status = hoyolab
        .as_ref()
        .map(|h| h.status.clone())
        .or_else(|| onboarding.as_ref().map(|o| o.cookie_status.clone()));
    let step = link_step(available, stored, complete, raw_status.as_deref());
    let status = match raw_status.as_deref() {
        Some("verified") => preferences.text("hoyolab.verified"),
        Some("pending_verification") => preferences.text("hoyolab.pending_verification"),
        Some("not_configured" | "missing") => preferences.text("hoyolab.not_configured"),
        Some("cookie_invalid" | "invalid") => preferences.text("dashboard.invalid"),
        Some("unreadable") => preferences.text("hoyolab.unreadable"),
        Some("storage_unavailable") => preferences.text("hoyolab.storage_unavailable"),
        Some(_) => preferences.text("dashboard.unknown"),
        None => preferences.text("hoyolab.not_loaded"),
    }
    .to_owned();
    // The next useful action carries the primary style: verify a saved cookie, else save.
    let verify_next = step == LinkStep::NeedsVerification;
    let (save_variant, verify_variant) = if verify_next {
        (ButtonVariant::Secondary, ButtonVariant::Primary)
    } else {
        (ButtonVariant::Primary, ButtonVariant::Secondary)
    };
    let (step_class, step_key) = step.notice();
    rsx! {
        div { class: "form-stack",
            if !password_ok { p { class: "warning", {preferences.text("hoyolab.password_change_required")} } }
            if let Some(error) = &state.read().hoyolab_error { p { class: "warning", {preferences.text(error)} } }
            if password_ok { p { id: "hoyolab-step", class: "notice {step_class}", {preferences.text(step_key)} } }
            dl { class: "metric-list",
                Metric { label: preferences.text("hoyolab.cookie_status"), value: status }
                Metric { label: preferences.text("hoyolab.hoyolab_uid"), value: hoyolab.as_ref().and_then(|h| h.hoyolab_uid.clone()).unwrap_or_else(|| preferences.text("hoyolab.not_reported").to_owned()) }
                Metric { label: preferences.text("hoyolab.updated"), value: datetime_text(hoyolab.as_ref().and_then(|h| h.updated_at.as_deref()), preferences) }
            }
            Field { id: "hoyolab-cookie", label: preferences.text("hoyolab.cookie"),
                textarea { id: "hoyolab-cookie", autocomplete: "off", spellcheck: "false", disabled, "aria-describedby": "hoyolab-cookie-help", value: "{cookie}", oninput: move |event| cookie.set(event.value()) }
                p { id: "hoyolab-cookie-help", class: "field-hint", {preferences.text("hoyolab.cookie_help")} }
            }
            div { class: "button-row",
                Button { variant: save_variant, r#type: "button", disabled: disabled || cookie().trim().is_empty(), onclick: move |_| {
                    if disabled || !state.read().password_ok() || cookie().trim().is_empty() { return; }
                    let secret = std::mem::take(&mut *cookie.write());
                    spawn_cookie_action(state, "PUT", "/me/hoyolab/cookie", json!({"cookie": secret}), "hoyolab.cookie_save_accepted");
                }, {preferences.text("hoyolab.save")} }
                Button { variant: verify_variant, r#type: "button", disabled: disabled || !stored, aria_describedby: verify_next.then_some("hoyolab-step"), onclick: move |_| {
                    if disabled || !stored || !state.read().password_ok() { return; }
                    spawn_cookie_action(state, "POST", "/me/hoyolab/verify", json!({}), "hoyolab.verification_response_received");
                }, {preferences.text("hoyolab.verify")} }
                Button { variant: ButtonVariant::Secondary, r#type: "button", disabled: disabled || !complete, aria_describedby: (!complete).then_some("hoyolab-sync-hint"), onclick: move |_| {
                    if disabled || !complete || !state.read().password_ok() { return; }
                    spawn_cookie_action(state, "POST", "/me/hoyolab/sync", json!({}), "hoyolab.sync_response_received");
                }, {preferences.text("hoyolab.sync_uids")} }
            }
            if !complete {
                p { id: "hoyolab-sync-hint", class: "field-hint", {preferences.text("hoyolab.sync_requires_verification")} }
            }
        }
    }
}

/// Where the linkage stands, from the user's point of view.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LinkStep {
    Loading,
    Unavailable,
    NeedsCookie,
    NeedsVerification,
    Unusable,
    Complete,
}

impl LinkStep {
    /// Notice style and message key.
    fn notice(self) -> (&'static str, &'static str) {
        match self {
            Self::Loading => ("", "hoyolab.step_loading"),
            Self::Unavailable => ("error", "hoyolab.step_unavailable"),
            Self::NeedsCookie => ("", "hoyolab.step_needs_cookie"),
            Self::NeedsVerification => ("warn", "hoyolab.step_needs_verification"),
            Self::Unusable => ("error", "hoyolab.step_unusable"),
            Self::Complete => ("success", "hoyolab.step_complete"),
        }
    }
}

fn link_step(
    available: Option<bool>,
    stored: bool,
    complete: bool,
    status: Option<&str>,
) -> LinkStep {
    match (available, status) {
        (Some(false), _) => LinkStep::Unavailable,
        _ if complete => LinkStep::Complete,
        (None, None) => LinkStep::Loading,
        (_, Some("cookie_invalid" | "invalid" | "unreadable")) => LinkStep::Unusable,
        (_, Some("storage_unavailable")) => LinkStep::Unavailable,
        // Saving only stores the cookie; verification is a separate step.
        _ if stored => LinkStep::NeedsVerification,
        _ => LinkStep::NeedsCookie,
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

    #[test]
    fn saved_but_unverified_cookies_ask_for_verification_next() {
        use LinkStep::*;
        assert_eq!(link_step(None, false, false, None), Loading);
        assert_eq!(
            link_step(Some(false), true, false, Some("pending_verification")),
            Unavailable
        );
        assert_eq!(
            link_step(Some(true), false, false, Some("storage_unavailable")),
            Unavailable
        );
        assert_eq!(
            link_step(Some(true), false, false, Some("not_configured")),
            NeedsCookie
        );
        assert_eq!(
            link_step(Some(true), true, false, Some("pending_verification")),
            NeedsVerification
        );
        assert_eq!(
            link_step(Some(true), true, false, Some("cookie_invalid")),
            Unusable
        );
        assert_eq!(
            link_step(Some(true), true, false, Some("unreadable")),
            Unusable
        );
        assert_eq!(
            link_step(Some(true), true, true, Some("verified")),
            Complete
        );
        assert_eq!(
            link_step(Some(true), true, false, Some("pending_verification"))
                .notice()
                .0,
            "warn"
        );
    }
}
