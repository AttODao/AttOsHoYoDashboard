use super::about::AboutPage;
use super::accounts::UidsPage;
use super::admin::AdminPage;
use super::app_nav::AppNav;
use super::common::{Icon, focus_element, use_modal};
use super::dashboard::DashboardPage;
use super::deletion::DeletePage;
use super::hoyolab::LinkPage;
use super::notifications::NotificationsPage;
use super::rewards::RewardsPage;
use super::security::SecurityPage;
use crate::app::actions::spawn_action;
use crate::app::navigation::open_web_version;
use crate::app::preferences::{Preferences, use_preferences};
use crate::app::session::logout;
use crate::app::state::{AppState, Page, clear_status, set_page};
use crate::i18n::Language;
use dioxus::prelude::*;

/// Entries of the account menu above the sign-out group, in display order.
#[derive(Clone, Copy, PartialEq, Debug)]
enum MenuItem {
    HoYoLab,
    Settings,
    OpenInBrowser,
    Administration,
    Docs,
    About,
}

/// The web menu puts About directly below the API docs. The app leaves admin and API
/// docs to the web version and starts with "Open in browser"; it has no Settings entry
/// because the bottom navigation (<=600px) and the header settings button (wider) lead there.
fn menu_items(app: bool, admin: bool) -> Vec<MenuItem> {
    if app {
        return vec![MenuItem::OpenInBrowser, MenuItem::HoYoLab, MenuItem::About];
    }
    let mut items = vec![MenuItem::HoYoLab, MenuItem::Settings];
    if admin {
        items.push(MenuItem::Administration);
    }
    items.push(MenuItem::Docs);
    items.push(MenuItem::About);
    items
}

const SETTINGS: [(Page, &str, &str, &str); 6] = [
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
        "settings.security",
    ),
    (
        Page::Notifications,
        "notifications",
        "settings.notifications",
        "settings.notifications_short",
    ),
    (Page::Delete, "delete", "settings.delete", "security.delete"),
];

/// Settings tabs in this build; the app leaves account deletion to the web version.
fn settings_tabs(app: bool) -> Vec<(Page, &'static str, &'static str, &'static str)> {
    SETTINGS
        .into_iter()
        .filter(|(page, ..)| !(app && page.web_only()))
        .collect()
}

#[component]
pub(in crate::app) fn Shell(state: Signal<AppState>) -> Element {
    let ui = use_preferences();
    let page = state.read().page;
    let is_settings = SETTINGS.iter().any(|(p, ..)| *p == page);
    rsx! {
        a { class: "skip-link", href: "#main", {ui.text("shell.skip_to_content")} }
        Header { state }
        main { class: "container", id: "main", tabindex: "-1",
            Status { state }
            if is_settings {
                div { class: "page-intro",
                    h1 { {ui.text("shell.settings")} }
                    button { r#type: "button", onclick: move |_| set_page(state, Page::Dashboard), {ui.text("shell.dashboard")} }
                }
                div { class: "tabs settings-tabs", role: "tablist", "aria-label": ui.text("shell.settings_sections"),
                    for (target, key, label, short) in settings_tabs(crate::platform::APP) {
                        button { id: "settings-tab-{key}", r#type: "button", role: "tab", "aria-controls": "settings-panel",
                            "aria-selected": (page == target).to_string(), tabindex: if page == target { "0" } else { "-1" },
                            "aria-label": ui.text(label), onclick: move |_| set_page(state, target),
                            onkeydown: move |event| {
                                if let Some(next) = next_settings(page, &event.key(), crate::platform::APP) {
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
                        Page::Notifications => rsx! { NotificationsPage { state } },
                        Page::Delete if !crate::platform::APP => rsx! { DeletePage { state } },
                        _ => rsx! {},
                    }
                }
            } else {
                if page == Page::Admin && !crate::platform::APP {
                    AdminPage { state }
                } else if page == Page::About {
                    AboutPage {}
                } else {
                    DashboardPage { state }
                }
            }
        }
    }
}

fn next_settings(current: Page, key: &Key, app: bool) -> Option<Page> {
    let tabs = settings_tabs(app);
    let index = tabs.iter().position(|(p, ..)| *p == current)?;
    let next = match key {
        Key::ArrowRight => (index + 1) % tabs.len(),
        Key::ArrowLeft => (index + tabs.len() - 1) % tabs.len(),
        Key::Home => 0,
        Key::End => tabs.len() - 1,
        _ => return None,
    };
    Some(tabs[next].0)
}

#[component]
pub(super) fn Header(mut state: Signal<AppState>) -> Element {
    let ui = use_preferences();
    let mut preferences = use_context::<Signal<Preferences>>();
    let mut open = use_signal(|| false);
    let me = state.read().me.clone();
    let ready = state.read().ready();
    use_modal(
        "ahd-menu".to_owned(),
        open() && ready,
        EventHandler::new(move |_| open.set(false)),
    );
    rsx! {
        header { class: "topbar",
            div { class: "topbar-inner",
                a { class: "wordmark", href: "/dashboard", "aria-label": ui.text("shell.dashboard"),
                    onclick: move |event| { event.prevent_default(); set_page(state, Page::Dashboard); },
                    img { src: asset!("/assets/dashboard-icon.png"), width: "36", height: "36", alt: "" }
                    span { {ui.text("app.title")} }
                }
                div { class: "account",
                    if let Some(me) = me.clone() {
                        span { class: "muted account-name", "{me.username}" }
                        if ready {
                            if crate::platform::APP {
                                button { id: "ahd-settings-trigger", class: "icon-button app-settings", r#type: "button", "aria-label": ui.text("shell.settings"),
                                    onclick: move |_| set_page(state, Page::Uids), Icon { name: "settings" }
                                }
                            }
                            button { id: "ahd-menu-trigger", class: "menu", r#type: "button", "aria-label": ui.text("shell.open_menu"),
                                "aria-controls": "ahd-menu", "aria-expanded": open().to_string(), onclick: move |_| open.set(true), Icon { name: "menu" }
                            }
                        } else {
                            button { r#type: "button", onclick: move |_| { spawn_action(state, move |generation| async move { logout(state, generation).await; }); }, {ui.text("shell.sign_out")} }
                        }
                    }
                }
            }
        }
        if ready {
            dialog { id: "ahd-menu", class: "menu-drawer", "aria-label": ui.text("shell.account_menu"),
                div { class: "drawer-heading", h2 { {ui.text("shell.menu")} }
                    button { class: "icon-button", r#type: "button", "aria-label": ui.text("shell.close_menu"), onclick: move |_| open.set(false), Icon { name: "close" } }
                }
                nav { "aria-label": ui.text("shell.account_menu"),
                    for item in menu_items(crate::platform::APP, me.as_ref().is_some_and(|me| me.is_admin)) {
                        match item {
                            MenuItem::HoYoLab => rsx! { a { href: "https://www.hoyolab.com/home", target: "_blank", rel: "noopener noreferrer", Icon { name: "link" } "HoYoLAB" } },
                            MenuItem::Settings => rsx! { button { r#type: "button", onclick: move |_| { open.set(false); set_page(state, Page::Uids); }, Icon { name: "settings" } {ui.text("shell.settings")} } },
                            MenuItem::OpenInBrowser => rsx! { button { r#type: "button", onclick: move |_| { open.set(false); let page = state.read().page; open_web_version(state, page); }, Icon { name: "external" } {ui.text("shell.open_in_browser")} } },
                            MenuItem::Administration => rsx! { button { r#type: "button", onclick: move |_| { open.set(false); set_page(state, Page::Admin); }, Icon { name: "user" } {ui.text("shell.administration")} } },
                            MenuItem::Docs => rsx! { button { r#type: "button", onclick: move |_| { open.set(false); set_page(state, Page::Docs); }, Icon { name: "docs" } {ui.text("docs.title")} } },
                            MenuItem::About => rsx! { button { r#type: "button", onclick: move |_| { open.set(false); set_page(state, Page::About); }, Icon { name: "info" } {ui.text("about.title")} } },
                        }
                    }
                    div { class: "menu-logout-group",
                        button { class: "menu-logout", r#type: "button", onclick: move |_| { open.set(false); spawn_action(state, move |generation| async move { logout(state, generation).await; }); }, Icon { name: "logout" } {ui.text("shell.sign_out")} }
                    }
                }
                div { class: "menu-preferences",
                    div { class: "language-options",
                        label { r#for: "menu-theme", {ui.text("shell.theme")} }
                        select { id: "menu-theme", value: ui.theme_code(),
                            onchange: move |event| preferences.write().theme = Preferences::theme_from_code(&event.value()),
                            option { value: "system", {ui.text("shell.theme_system")} }
                            option { value: "dark", {ui.text("shell.theme_dark")} }
                            option { value: "light", {ui.text("shell.theme_light")} }
                        }
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
            // Only the Android app renders the bottom navigation.
            if crate::platform::APP { AppNav { state, open } }
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
    // Errors stay until dismissed; success and info lines clear themselves (see set_status).
    rsx! { div { class: "live-region", role: "status", "aria-live": "polite",
        if let Some(status) = status {
            div { class: "status {status.kind}",
                p { {ui.text(status.message_key)} }
                if status.kind == "error" {
                    button { class: "icon-button status-dismiss", r#type: "button", "aria-label": ui.text("shell.dismiss_message"),
                        onclick: move |_| clear_status(state),
                        Icon { name: "close" }
                    }
                }
            }
        }
    } }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn app_menu_starts_with_open_in_browser_and_has_no_settings_while_the_web_menu_is_unchanged() {
        use MenuItem::*;
        assert_eq!(
            menu_items(false, true),
            vec![HoYoLab, Settings, Administration, Docs, About]
        );
        assert_eq!(
            menu_items(false, false),
            vec![HoYoLab, Settings, Docs, About]
        );
        assert_eq!(menu_items(true, true), vec![OpenInBrowser, HoYoLab, About]);
        assert_eq!(menu_items(true, false), vec![OpenInBrowser, HoYoLab, About]);
    }
    #[test]
    fn settings_tabs_wrap_and_do_not_handle_unrelated_keys() {
        assert!(next_settings(Page::Uids, &Key::ArrowLeft, false) == Some(Page::Delete));
        assert!(next_settings(Page::Delete, &Key::ArrowRight, false) == Some(Page::Uids));
        assert!(next_settings(Page::Rewards, &Key::Home, false) == Some(Page::Uids));
        assert!(next_settings(Page::Rewards, &Key::End, false) == Some(Page::Delete));
        assert!(next_settings(Page::Dashboard, &Key::ArrowRight, false).is_none());
        assert!(next_settings(Page::Uids, &Key::Enter, false).is_none());
    }

    #[test]
    fn app_settings_tabs_leave_out_account_deletion() {
        assert!(
            settings_tabs(false)
                .iter()
                .any(|(page, ..)| *page == Page::Delete)
        );
        assert!(
            !settings_tabs(true)
                .iter()
                .any(|(page, ..)| *page == Page::Delete)
        );
        assert!(next_settings(Page::Uids, &Key::ArrowLeft, true) == Some(Page::Notifications));
        assert!(next_settings(Page::Notifications, &Key::ArrowRight, true) == Some(Page::Uids));
        assert!(next_settings(Page::Delete, &Key::ArrowRight, true).is_none());
    }
}
