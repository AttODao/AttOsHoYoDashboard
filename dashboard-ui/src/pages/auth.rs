use super::admin::AdminCard;
use super::common::{Field, Metric};
use super::deletion::DeleteAccountCard;
use super::hoyolab::CookieCard;
use super::security::{PasskeyCard, PasswordCard};
use super::shell::{Header, ServerSettingsCard, Status};
use crate::app::actions::{handle_api_error_for, spawn_action};
use crate::app::display::humanize;
use crate::app::session::{bootstrap, logout};
use crate::app::state::AppState;
use crate::app::state::generation_matches;
use crate::components::{
    Button, ButtonVariant, Card, CardContent, CardDescription, CardFooter, CardHeader, CardTitle,
};
use dioxus::prelude::*;
use serde_json::json;

#[component]
pub(in crate::app) fn LoadingPage() -> Element {
    rsx! {
        main { class: "center-page", "aria-live": "polite",
            Card { class: "narrow-card",
                CardHeader {
                    CardTitle { "AttOs Game Dashboard" }
                    CardDescription { "Loading dashboard state…" }
                }
            }
        }
    }
}

#[component]
pub(in crate::app) fn LoginPage(mut state: Signal<AppState>) -> Element {
    let mut username = use_signal(String::new);
    let mut password = use_signal(String::new);
    let mut passkey_username = use_signal(String::new);

    rsx! {
        main { class: "center-page",
            div { class: "brand-block",
                h1 { "AttOs Game Dashboard" }
                p { "Sign in with a password, or enter your username first to use a registered passkey." }
            }
            Status { state }
            if state.read().logout_unconfirmed {
                Button { variant: ButtonVariant::Secondary, r#type: "button", onclick: move |_| { spawn_action(state, move |generation| async move { logout(state, generation).await; }); }, "Retry server logout" }
            }
            div { class: "auth-grid",
                Card {
                    CardHeader {
                        CardTitle { "Password sign in" }
                        CardDescription { "Credentials are sent only to the same-origin dashboard API." }
                    }
                    CardContent { class: "form-stack",
                        Field { id: "login-username", label: "Username",
                            input { id: "login-username", autocomplete: "username", value: "{username}", oninput: move |event| username.set(event.value()) }
                        }
                        Field { id: "login-password", label: "Password",
                            input { id: "login-password", r#type: "password", autocomplete: "current-password", value: "{password}", oninput: move |event| password.set(event.value()) }
                        }
                    }
                    CardFooter {
                        Button { r#type: "button", onclick: move |_| {
                            let user = std::mem::take(&mut *username.write());
                            let pass = std::mem::take(&mut *password.write());
                            let state = state;
                            spawn_action(state, move |generation| async move {
                                match crate::api::send("POST", "/auth/login", Some(json!({"username": user, "password": pass}))).await {
                                    Ok(()) if generation_matches(state, generation) => bootstrap(state).await,
                                    Ok(()) => {}
                                    Err(error) => { handle_api_error_for(state, generation, error); }
                                }
                            });
                        }, "Sign in" }
                    }
                }
                Card {
                    CardHeader {
                        CardTitle { "Passkey sign in" }
                        CardDescription { "Enter your username, then approve the browser passkey prompt." }
                    }
                    CardContent { class: "form-stack",
                        Field { id: "passkey-login-username", label: "Username",
                            input { id: "passkey-login-username", autocomplete: "username webauthn", value: "{passkey_username}", oninput: move |event| passkey_username.set(event.value()) }
                        }
                    }
                    CardFooter {
                        Button { variant: ButtonVariant::Secondary, r#type: "button", onclick: move |_| {
                            let user = std::mem::take(&mut *passkey_username.write());
                            let state = state;
                            spawn_action(state, move |generation| async move {
                                match crate::webauthn::login(&user).await {
                                    Ok(()) if generation_matches(state, generation) => bootstrap(state).await,
                                    Ok(()) => {}
                                    Err(error) => { handle_api_error_for(state, generation, error); }
                                }
                            });
                        }, "Use passkey" }
                    }
                }
            }
        }
    }
}

#[component]
pub(in crate::app) fn OnboardingPage(mut state: Signal<AppState>) -> Element {
    let me = state.read().me.clone();
    let onboarding = me.as_ref().map(|me| me.onboarding.clone());
    let password_title = if onboarding
        .as_ref()
        .is_some_and(|o| o.password_change_required)
    {
        "Change temporary password"
    } else {
        "Change password"
    };
    rsx! {
        main { class: "page-wrap",
            Header { state }
            Status { state }
            div { class: "setup-grid",
                Card {
                    CardHeader {
                        CardTitle { "Setup status" }
                        CardDescription { "Ordinary game status and reward screens stay locked until password and Cookie requirements are complete." }
                    }
                    CardContent {
                        dl { class: "metric-list",
                            Metric { label: "Password changed", value: if onboarding.as_ref().is_some_and(|o| o.password_change_required) { "Required".to_owned() } else { "Complete".to_owned() } }
                            Metric { label: "Cookie setup", value: onboarding.as_ref().map(|o| if o.cookie_setup_complete { "Complete".to_owned() } else { humanize(&o.cookie_status) }).unwrap_or_else(|| "Not loaded".to_owned()) }
                            Metric { label: "Server verification", value: onboarding.as_ref().map(|o| if o.verification_available { "Available".to_owned() } else { "Not available on this server".to_owned() }).unwrap_or_else(|| "Not reported".to_owned()) }
                        }
                        if let Some(error) = &state.read().hoyolab_error { p { class: "warning", "{error}" } }
                    }
                    CardFooter {
                        Button { variant: ButtonVariant::Secondary, r#type: "button", onclick: move |_| { let state = state; spawn_action(state, move |_| async move { bootstrap(state).await; }); }, "Refresh setup status" }
                    }
                }
                PasswordCard { state, title: password_title }
                if state.read().password_ok() {
                    ServerSettingsCard { state }
                    PasskeyCard { state }
                    DeleteAccountCard { state }
                }
                CookieCard { state }
                if me.as_ref().is_some_and(|me| me.is_admin) && state.read().password_ok() {
                    AdminCard { state, compact: true }
                }
            }
        }
    }
}
