use super::common::{Field, Icon, PasswordInput};
use super::hoyolab::CookieCard;
use super::security::PasswordCard;
use super::shell::{Header, Status};
use crate::app::actions::{handle_api_error_for, spawn_action};
use crate::app::navigation::open_web_version;
use crate::app::preferences::use_preferences;
use crate::app::session::{complete_login, logout, refresh_gate};
use crate::app::state::{AppState, Page, generation_matches, set_status};
use crate::components::{Button, ButtonVariant};
use dioxus::prelude::*;
use serde_json::json;

fn password_fields_complete(username: &str, password: &str) -> bool {
    !username.trim().is_empty() && !password.is_empty()
}

fn passkey_username(username: &str) -> Option<&str> {
    let username = username.trim();
    (!username.is_empty()).then_some(username)
}

#[component]
pub(in crate::app) fn LoadingPage() -> Element {
    let ui = use_preferences();
    rsx! {
        main { class: "auth-layout", "aria-live": "polite",
            section { class: "panel auth-panel",
                h1 { {ui.text("app.title")} }
                p { {ui.text("dashboard.loading")} }
            }
        }
    }
}

#[component]
pub(in crate::app) fn LoginPage(mut state: Signal<AppState>, app_login: bool) -> Element {
    let ui = use_preferences();
    let mut username = use_signal(String::new);
    let mut password = use_signal(String::new);
    use_drop(move || {
        username.write().clear();
        password.write().clear();
    });

    rsx! {
        Header { state }
        main { class: "auth-layout",
            section { class: "panel auth-panel",
                h1 { {ui.text("auth.sign_in")} }
                Status { state }
                if app_login {
                    p { {ui.text("auth.app_login_after_sign_in")} }
                }
                if state.read().logout_unconfirmed {
                    Button { variant: ButtonVariant::Secondary, r#type: "button", onclick: move |_| {
                        password.write().clear();
                        spawn_action(state, move |generation| async move { logout(state, generation).await; });
                    }, {ui.text("auth.retry_server_logout")} }
                }
                form { onsubmit: move |event| {
                    event.prevent_default();
                    if state.read().busy { return; }
                    // Keep the username for a retry; only the secret is cleared.
                    let user = username.read().clone();
                    let pass = std::mem::take(&mut *password.write());
                    if !password_fields_complete(&user, &pass) {
                        set_status(state, "auth.enter_your_username_and_credentials", "error");
                        return;
                    }
                    spawn_action(state, move |generation| async move {
                        let result = crate::api::send("POST", "/auth/login", Some(json!({"username": user, "password": pass}))).await;
                        match result {
                            Ok(()) if generation_matches(state, generation) => complete_login(state, generation).await,
                            Ok(()) => {}
                            Err(error) if error.is_unauthorized() && generation_matches(state, generation) => {
                                set_status(state, "messages.the_username_or_password_is_incorrect", "error");
                            }
                            Err(error) => { handle_api_error_for(state, generation, error); }
                        }
                    });
                },
                    Field { id: "login-username", label: ui.text("admin.username"),
                        input { id: "login-username", autocomplete: "username webauthn", value: "{username}", oninput: move |event| username.set(event.value()) }
                    }
                    Field { id: "login-password", label: ui.text("auth.password"),
                        PasswordInput { id: "login-password", value: password, autocomplete: "current-password" }
                    }
                    div { class: "auth-submit",
                        Button { r#type: "submit",
                            {ui.text("auth.sign_in")}
                        }
                    }
                }
                div { class: "auth-alternatives",
                    Button { id: "login-passkey", variant: ButtonVariant::Secondary, r#type: "button", onclick: move |_| {
                        if state.read().busy { return; }
                        password.write().clear();
                        if crate::platform::APP {
                            crate::app::browser_login::start(state);
                            return;
                        }
                        let user = username.read().clone();
                        spawn_action(state, move |generation| async move {
                            match crate::webauthn::login(passkey_username(&user)).await {
                                Ok(()) if generation_matches(state, generation) => complete_login(state, generation).await,
                                Ok(()) => {},
                                Err(error) => { handle_api_error_for(state, generation, error); }
                            }
                        });
                    }, {ui.text("auth.use_passkey")} }
                    if crate::platform::APP {
                        Button { variant: ButtonVariant::Secondary, r#type: "button", onclick: move |_| crate::app::browser_login::start(state),
                            {ui.text("auth.sign_in_in_browser")}
                        }
                        p { {ui.text("auth.passkey_in_browser_hint")} }
                    }
                }
            }
        }
    }
}

#[component]
pub(in crate::app) fn OnboardingPage(mut state: Signal<AppState>) -> Element {
    let ui = use_preferences();
    let onboarding = state.read().me.as_ref().map(|me| me.onboarding.clone());
    let password_required = onboarding
        .as_ref()
        .is_some_and(|o| o.password_change_required);
    let cookie_complete = onboarding.as_ref().is_some_and(|o| o.cookie_setup_complete);
    rsx! {
        Header { state }
        main { class: "setup-wrap page-stack",
            h1 { {ui.text("auth.setup")} }
            Status { state }
            if !cookie_complete {
                // A read-only progress list, not navigation: it must not look or act like tabs.
                ol { class: "setup-steps", "aria-label": ui.text("auth.setup_progress"),
                    SetupStepItem { number: 1, label: ui.text("auth.step_password"), step: if password_required { SetupStep::Current } else { SetupStep::Done } }
                    SetupStepItem { number: 2, label: ui.text("auth.step_hoyolab"), step: if password_required { SetupStep::Upcoming } else { SetupStep::Current } }
                }
            }
            if password_required && crate::platform::APP {
                // The app leaves password change to the web version.
                section { class: "panel page-stack",
                    p { {ui.text("auth.change_password_in_browser")} }
                    Button { variant: ButtonVariant::Secondary, r#type: "button", onclick: move |_| open_web_version(state, Page::Dashboard), {ui.text("shell.open_in_browser")} }
                }
            } else if password_required {
                PasswordCard { state, title: ui.text("auth.change_password") }
            } else if !cookie_complete {
                CookieCard { state }
            }
            if let Some(error) = &state.read().hoyolab_error { p { class: "warning", {ui.text(error)} } }
            Button { variant: ButtonVariant::Secondary, r#type: "button", onclick: move |_| {
                spawn_action(state, move |generation| async move { refresh_gate(state, generation).await; });
            }, {ui.text("auth.refresh_setup_status")} }
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SetupStep {
    Done,
    Current,
    Upcoming,
}

impl SetupStep {
    fn class(self) -> &'static str {
        match self {
            Self::Done => "done",
            Self::Current => "current",
            Self::Upcoming => "upcoming",
        }
    }
}

#[component]
fn SetupStepItem(number: u8, label: &'static str, step: SetupStep) -> Element {
    let ui = use_preferences();
    rsx! {
        li { class: "setup-step {step.class()}", "aria-current": (step == SetupStep::Current).then_some("step"),
            span { class: "step-marker", "aria-hidden": "true",
                if step == SetupStep::Done { Icon { name: "check" } } else { "{number}" }
            }
            span { class: "step-label", "{label}" }
            if step == SetupStep::Done { span { class: "visually-hidden", {ui.text("auth.step_done")} } }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn password_submit_requires_both_fields() {
        assert!(!password_fields_complete("  ", "synthetic-password"));
        assert!(!password_fields_complete("synthetic-user", ""));
        assert!(password_fields_complete(
            "synthetic-user",
            "synthetic-password"
        ));
    }

    #[test]
    fn passkey_uses_trimmed_username_only_when_filled() {
        assert_eq!(passkey_username(""), None);
        assert_eq!(passkey_username("  "), None);
        assert_eq!(
            passkey_username("  synthetic-user  "),
            Some("synthetic-user")
        );
    }
}
