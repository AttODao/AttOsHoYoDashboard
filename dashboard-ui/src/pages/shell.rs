use super::accounts::UidsPage;
use super::admin::AdminPage;
use super::common::{Icon, Metric, focus_element, use_modal};
use super::dashboard::DashboardPage;
use super::deletion::DeletePage;
use super::docs::DocsPage;
use super::hoyolab::LinkPage;
use super::rewards::RewardsPage;
use super::security::SecurityPage;
use crate::app::actions::spawn_action;
use crate::app::preferences::{Preferences, Theme, use_preferences};
use crate::app::session::logout;
use crate::app::state::{AppState, Page, set_page};
use crate::i18n::Language;
use dioxus::prelude::*;

const SETTINGS: [(Page, &str, &str, &str); 5] = [
    (Page::Uids, "uids", "settings.uids", "dashboard.games"),
    (
        Page::Rewards,
        "bonus",
        "settings.bonus",
        "settings.bonus_short",
    ),
    (Page::Link, "link", "settings.link", "settings.link_short"),
    (
        Page::Security,
        "security",
        "settings.security",
        "settings.security_short",
    ),
    (Page::Delete, "delete", "settings.delete", "security.delete"),
];

#[component]
pub(in crate::app) fn Shell(state: Signal<AppState>) -> Element {
    let ui = use_preferences();
    let page = state.read().page;
    let is_settings = SETTINGS.iter().any(|(p, ..)| *p == page);
    rsx! {
        Header { state }
        main { class: "container",
            Status { state }
            if is_settings {
                div { class: "page-intro",
                    h1 { {ui.text("shell.settings")} }
                    button { r#type: "button", onclick: move |_| set_page(state, Page::Dashboard), {ui.text("shell.dashboard")} }
                }
                div { class: "tabs settings-tabs", role: "tablist", "aria-label": ui.text("shell.settings_sections"),
                    for (target, key, label, short) in SETTINGS {
                        button { id: "settings-tab-{key}", r#type: "button", role: "tab", "aria-controls": "settings-panel",
                            "aria-selected": (page == target).to_string(), tabindex: if page == target { "0" } else { "-1" },
                            "aria-label": ui.text(label), onclick: move |_| set_page(state, target),
                            onkeydown: move |event| {
                                if let Some(next) = next_settings(page, &event.key()) {
                                    event.prevent_default(); set_page(state, next);
                                    if let Some((_, key, ..)) = SETTINGS.iter().find(|(p, ..)| *p == next) {
                                        focus_element(&format!("settings-tab-{key}"));
                                    }
                                }
                            },
                            span { class: "tab-full", {ui.text(label)} }
                            span { class: "tab-short", "aria-hidden": "true", {ui.text(short)} }
                        }
                    }
                }
                section { class: "settings-section", id: "settings-panel", role: "tabpanel",
                    "aria-labelledby": format!("settings-tab-{}", SETTINGS.iter().find(|(p, ..)| *p == page).map(|(_, key, ..)| *key).unwrap_or("uids")),
                    match page {
                        Page::Uids => rsx! { UidsPage { state } },
                        Page::Rewards => rsx! { RewardsPage { state } },
                        Page::Link => rsx! { LinkPage { state } },
                        Page::Security => rsx! { SecurityPage { state } },
                        Page::Delete => rsx! { DeletePage { state } },
                        _ => rsx! {},
                    }
                }
            } else {
                match page {
                    Page::Admin => rsx! { AdminPage { state } },
                    Page::Docs => rsx! { DocsPage { state } },
                    _ => rsx! { DashboardPage { state } },
                }
            }
        }
    }
}

fn next_settings(current: Page, key: &Key) -> Option<Page> {
    let index = SETTINGS.iter().position(|(p, ..)| *p == current)?;
    let next = match key {
        Key::ArrowRight => (index + 1) % SETTINGS.len(),
        Key::ArrowLeft => (index + SETTINGS.len() - 1) % SETTINGS.len(),
        Key::Home => 0,
        Key::End => SETTINGS.len() - 1,
        _ => return None,
    };
    Some(SETTINGS[next].0)
}

#[component]
pub(super) fn Header(mut state: Signal<AppState>) -> Element {
    let ui = use_preferences();
    let mut preferences = use_context::<Signal<Preferences>>();
    let mut open = use_signal(|| false);
    let me = state.read().me.clone();
    let ready = state.read().ready();
    use_modal(
        "agd-menu".to_owned(),
        open() && ready,
        EventHandler::new(move |_| open.set(false)),
    );
    rsx! {
        header { class: "topbar",
            div { class: "topbar-inner",
                a { class: "wordmark", href: "#dashboard", "aria-label": ui.text("shell.dashboard"),
                    onclick: move |event| { event.prevent_default(); set_page(state, Page::Dashboard); },
                    img { src: asset!("/assets/dashboard-icon.png"), width: "36", height: "36", alt: "" }
                    span { {ui.text("app.title")} }
                }
                div { class: "account",
                    if let Some(me) = me.clone() {
                        span { class: "muted account-name", "{me.username}" }
                        if ready {
                            button { id: "agd-menu-trigger", class: "menu", r#type: "button", "aria-label": ui.text("shell.open_menu"),
                                "aria-controls": "agd-menu", "aria-expanded": open().to_string(), onclick: move |_| open.set(true), Icon { name: "menu" }
                            }
                        } else {
                            button { r#type: "button", onclick: move |_| { spawn_action(state, move |generation| async move { logout(state, generation).await; }); }, {ui.text("shell.sign_out")} }
                        }
                    }
                }
            }
        }
        if ready {
            dialog { id: "agd-menu", class: "menu-drawer", "aria-label": ui.text("shell.account_menu"),
                div { class: "drawer-heading", h2 { {ui.text("shell.menu")} }
                    button { class: "icon-button", r#type: "button", "aria-label": ui.text("shell.close_menu"), onclick: move |_| open.set(false), Icon { name: "close" } }
                }
                nav { "aria-label": ui.text("shell.account_menu"),
                    a { href: "https://www.hoyolab.com/home", target: "_blank", rel: "noopener noreferrer", Icon { name: "link" } "HoYoLAB" }
                    button { r#type: "button", onclick: move |_| { open.set(false); set_page(state, Page::Uids); }, Icon { name: "settings" } {ui.text("shell.settings")} }
                    if me.as_ref().is_some_and(|me| me.is_admin) {
                        button { r#type: "button", onclick: move |_| { open.set(false); set_page(state, Page::Admin); }, Icon { name: "user" } {ui.text("shell.administration")} }
                    }
                    button { class: "menu-logout", r#type: "button", onclick: move |_| { open.set(false); spawn_action(state, move |generation| async move { logout(state, generation).await; }); }, Icon { name: "logout" } {ui.text("shell.sign_out")} }
                }
                div { class: "menu-preferences",
                    button { class: "mode-switch", r#type: "button", role: "switch", "aria-label": ui.text("shell.dark_mode"),
                        "aria-checked": (ui.theme == Theme::Dark).to_string(), onclick: move |_| {
                            let next = if preferences.read().theme == Theme::Dark { Theme::Light } else { Theme::Dark }; preferences.write().theme = next;
                        },
                        span { {ui.text("shell.dark_mode")} } span { class: "mode-track", span { class: "mode-thumb" } }
                    }
                    div { class: "language-options",
                        label { r#for: "menu-language", {ui.text("shell.language")} }
                        select { id: "menu-language", value: if ui.language == Language::Japanese { "ja" } else { "en" },
                            onchange: move |event| preferences.write().language = if event.value() == "en" { Language::English } else { Language::Japanese },
                            option { value: "ja", lang: "ja", {ui.text("language.japanese")} }
                            option { value: "en", lang: "en", {ui.text("language.english")} }
                        }
                    }
                }
            }
        }
    }
}

#[component]
pub(super) fn Status(state: Signal<AppState>) -> Element {
    let ui = use_preferences();
    let status = state
        .read()
        .status
        .clone()
        .filter(|status| status.message_key != "messages.please_sign_in");
    rsx! { div { class: "live-region", role: "status", "aria-live": "polite",
        if let Some(status) = status { p { class: "status {status.kind}", {ui.text(status.message_key)} } }
    } }
}

#[component]
pub(super) fn ServerSettingsCard(state: Signal<AppState>) -> Element {
    let ui = use_preferences();
    let server = state.read().server.clone();
    rsx! { section { class: "panel", h2 { {ui.text("shell.server_settings")} }
        dl { class: "metric-list",
            Metric { label: ui.text("shell.time_zone"), value: server.as_ref().map(|s| s.timezone_offset.clone()).unwrap_or_else(|| ui.text("shell.not_loaded").to_owned()) }
            Metric { label: ui.text("shell.session_lifetime"), value: server.as_ref().map(|s| ui.format("session.duration_days", &[("count", &s.session_ttl_days.to_string())])).unwrap_or_else(|| ui.text("shell.not_loaded").to_owned()) }
        }
    } }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn settings_tabs_wrap_and_do_not_handle_unrelated_keys() {
        assert!(next_settings(Page::Uids, &Key::ArrowLeft) == Some(Page::Delete));
        assert!(next_settings(Page::Delete, &Key::ArrowRight) == Some(Page::Uids));
        assert!(next_settings(Page::Rewards, &Key::Home) == Some(Page::Uids));
        assert!(next_settings(Page::Rewards, &Key::End) == Some(Page::Delete));
        assert!(next_settings(Page::Dashboard, &Key::ArrowRight).is_none());
        assert!(next_settings(Page::Uids, &Key::Enter).is_none());
    }
}
