//! App-only server choice on the login page: shows the selected origin and changes it
//! (`app::server`). The browser build never renders it.
use super::common::Field;
use crate::app::actions::spawn_action;
use crate::app::preferences::use_preferences;
use crate::app::server::{self, Outcome};
use crate::app::state::AppState;
use crate::components::{Button, ButtonVariant};
use dioxus::prelude::*;

const INPUT_ID: &str = "server-url";
const CHANGE_ID: &str = "server-change";
/// The login form's first field, focused after a change so sign-in can start right away.
const USERNAME_ID: &str = "login-username";

/// `aria-describedby` of the address field: the hint, plus the error while one is shown.
fn described_by(error: bool) -> &'static str {
    if error {
        "server-url-hint server-url-error"
    } else {
        "server-url-hint"
    }
}

/// Whether to offer going back to the build-time server: only when another one is in use.
fn offers_default(current: Option<&str>, default: Option<&str>) -> bool {
    default.is_some() && current != default
}

/// The element to focus now: the pending one, but only once the busy shell is enabled again.
fn focus_after(busy: bool, pending: Option<&'static str>) -> Option<&'static str> {
    pending.filter(|_| !busy)
}

fn focus(id: &str) {
    #[cfg(feature = "mobile")]
    crate::mobile::dom::focus(id);
    #[cfg(not(feature = "mobile"))]
    let _ = id;
}

#[component]
pub(super) fn ServerSetting(state: Signal<AppState>) -> Element {
    let ui = use_preferences();
    let mut editing = use_signal(|| false);
    let mut address = use_signal(String::new);
    let mut error = use_signal(|| None::<&'static str>);
    // Where focus goes next; applied once the busy shell is enabled again, because a disabled
    // control (the whole form while the server is checked) cannot take focus.
    let mut focus_next = use_signal(|| None::<&'static str>);
    // Re-read on every render: a change replaces the app state, which re-renders this page.
    let (current, default) = server::origins();
    let shows_default = offers_default(current.as_deref(), default.as_deref());
    let has_error = error.read().is_some();
    use_effect(move || {
        let busy = state.read().busy;
        if let Some(id) = focus_after(busy, focus_next()) {
            focus_next.set(None);
            focus(id);
        }
    });

    rsx! {
        section { class: "auth-server", "aria-labelledby": "server-title",
            h2 { id: "server-title", {ui.text("auth.server_label")} }
            p { id: "server-current", class: "auth-server-origin",
                match &current {
                    Some(origin) => rsx! { "{origin}" },
                    None => rsx! { {ui.text("auth.server_not_set")} },
                }
            }
            if !editing() {
                Button { id: CHANGE_ID, variant: ButtonVariant::Secondary, r#type: "button",
                    onclick: move |_| {
                        address.set(server::origins().0.unwrap_or_default());
                        error.set(None);
                        editing.set(true);
                        focus_next.set(Some(INPUT_ID));
                    },
                    {ui.text("auth.server_change")}
                }
            } else {
                form { id: "server-form", class: "form-stack", "novalidate": "novalidate", onsubmit: move |event| {
                    event.prevent_default();
                    if state.read().busy { return; }
                    let input = address.read().clone();
                    spawn_action(state, move |_generation| async move {
                        match server::change(state, input).await {
                            Outcome::Unchanged => {
                                error.set(None);
                                editing.set(false);
                                focus_next.set(Some(CHANGE_ID));
                            }
                            Outcome::Changed => {
                                editing.set(false);
                                error.set(None);
                                address.set(String::new());
                                focus_next.set(Some(USERNAME_ID));
                            }
                            Outcome::Rejected(key) => {
                                error.set(Some(key));
                                focus_next.set(Some(INPUT_ID));
                            }
                        }
                    });
                },
                    Field { id: INPUT_ID, label: ui.text("auth.server_address"),
                        input { id: INPUT_ID, r#type: "url", "inputmode": "url", autocomplete: "url",
                            "autocapitalize": "none", "autocorrect": "off", spellcheck: "false",
                            placeholder: "https://ahd.example.com",
                            value: "{address}",
                            "aria-describedby": described_by(has_error),
                            "aria-invalid": if has_error { "true" } else { "false" },
                            oninput: move |event| {
                                address.set(event.value());
                                error.set(None);
                            }
                        }
                        p { id: "server-url-hint", class: "field-hint", {ui.text("auth.server_hint")} }
                        if let Some(key) = *error.read() {
                            p { id: "server-url-error", class: "field-error", role: "alert", {ui.text(key)} }
                        }
                    }
                    div { class: "auth-server-actions",
                        Button { r#type: "submit", {ui.text("auth.server_save")} }
                        if shows_default {
                            Button { variant: ButtonVariant::Secondary, r#type: "button", onclick: move |_| {
                                address.set(server::origins().1.unwrap_or_default());
                                error.set(None);
                                focus_next.set(Some(INPUT_ID));
                            }, {ui.text("auth.server_use_default")} }
                        }
                        Button { variant: ButtonVariant::Secondary, r#type: "button", onclick: move |_| {
                            error.set(None);
                            editing.set(false);
                            focus_next.set(Some(CHANGE_ID));
                        }, {ui.text("auth.app_login_cancel")} }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_error_is_announced_with_the_field_only_while_shown() {
        assert_eq!(described_by(false), "server-url-hint");
        assert_eq!(described_by(true), "server-url-hint server-url-error");
    }

    #[test]
    fn focus_moves_only_after_the_busy_shell_is_enabled_again() {
        assert_eq!(focus_after(true, Some(INPUT_ID)), None);
        assert_eq!(focus_after(false, Some(INPUT_ID)), Some(INPUT_ID));
        assert_eq!(focus_after(false, None), None);
    }

    #[test]
    fn the_default_is_offered_only_when_another_server_is_in_use() {
        let default = Some("https://default.example.test");
        assert!(!offers_default(default, default));
        assert!(offers_default(Some("https://other.example.test"), default));
        assert!(offers_default(None, default));
        assert!(!offers_default(Some("https://other.example.test"), None));
    }
}
