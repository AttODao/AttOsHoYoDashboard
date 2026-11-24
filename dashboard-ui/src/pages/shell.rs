use super::about::AboutPage;
use super::accounts::UidsPage;
use super::admin::AdminPage;
use super::app_nav::AppNav;
use super::codes::CodesPage;
use super::common::{focus_element, reveal_horizontally, use_modal};
use super::dashboard::DashboardPage;
use super::deletion::DeletePage;
use super::events::EventsPage;
use super::hoyolab::LinkPage;
use super::icons::{Glyph, Icon};
use super::notifications::NotificationsPage;
use super::rewards::RewardsPage;
use super::security::SecurityPage;
use super::swipe_tabs::SwipeableTabs;
use super::transfer::TransferPage;
use crate::app::actions::{spawn_action, spawn_root};
use crate::app::data::load_page_data;
use crate::app::navigation::open_web_version;
use crate::app::preferences::{Preferences, use_preferences};
use crate::app::session::logout;
use crate::app::state::{AppState, Page, STATUS_ID, clear_status, set_page};
use crate::i18n::Language;
use dioxus::prelude::*;

/// Entries of the account menu above the sign-out group, in display order.
#[derive(Clone, Copy, PartialEq, Debug)]
enum MenuItem {
    Events,
    HoYoLab,
    Settings,
    OpenInBrowser,
    Administration,
    Docs,
    About,
}

/// Both menus start with the events page, the one page beside the dashboard (the logo leads
/// back to the dashboard, so the menu has no dashboard entry). The web menu puts About
/// directly below the API docs. The app leaves admin and API docs to the web version and
/// follows with "Open in browser"; it has no Settings entry because the bottom navigation
/// (<=600px) and the header settings button (wider) lead there. Its events entry is for
/// wider screens: at <=600px the bottom navigation has it and `mobile.css` hides the entry.
fn menu_items(app: bool, admin: bool) -> Vec<MenuItem> {
    if app {
        return vec![
            MenuItem::Events,
            MenuItem::OpenInBrowser,
            MenuItem::HoYoLab,
            MenuItem::About,
        ];
    }
    let mut items = vec![MenuItem::Events, MenuItem::HoYoLab, MenuItem::Settings];
    if admin {
        items.push(MenuItem::Administration);
    }
    items.push(MenuItem::Docs);
    items.push(MenuItem::About);
    items
}

/// Settings tabs: page, element key, label, short label (narrow screens) and icon.
const SETTINGS: [(Page, &str, &str, &str, Icon); 8] = [
    (
        Page::Uids,
        "uids",
        "settings.uids",
        "dashboard.games",
        Icon::Eye,
    ),
    (
        Page::Rewards,
        "bonus",
        "settings.bonus",
        "settings.bonus_short",
        Icon::Gift,
    ),
    (
        Page::Codes,
        "codes",
        "settings.codes",
        "settings.codes_short",
        Icon::Code,
    ),
    (
        Page::Link,
        "link",
        "settings.link",
        "settings.link_short",
        Icon::Link,
    ),
    (
        Page::Security,
        "security",
        "settings.security",
        "settings.security",
        Icon::Shield,
    ),
    (
        Page::Notifications,
        "notifications",
        "settings.notifications",
        "settings.notifications_short",
        Icon::Bell,
    ),
    (
        Page::Transfer,
        "transfer",
        "settings.transfer",
        "settings.transfer_short",
        Icon::Transfer,
    ),
    (
        Page::Delete,
        "delete",
        "settings.delete",
        "security.delete",
        Icon::Trash,
    ),
];

/// Settings tabs in this build; the app leaves the settings file and account deletion to
/// the web version.
fn settings_tabs(app: bool) -> Vec<(Page, &'static str, &'static str, &'static str, Icon)> {
    SETTINGS
        .into_iter()
        .filter(|(page, ..)| !(app && page.web_only()))
        .collect()
}

/// The settings panels rendered inside the pager: the selected tab, and while the pager
/// moves (`previews`) its neighbours as inert previews. Each entry is the tab
/// position, the page and its element key.
fn settings_panels(app: bool, page: Page, previews: bool) -> Vec<(usize, Page, &'static str)> {
    let tabs = settings_tabs(app);
    let Some(index) = tabs.iter().position(|(p, ..)| *p == page) else {
        return Vec::new();
    };
    let reach = usize::from(previews);
    tabs.iter()
        .enumerate()
        .filter(|(position, _)| position.abs_diff(index) <= reach)
        .map(|(position, (page, key, ..))| (position, *page, *key))
        .collect()
}

#[component]
pub(in crate::app) fn Shell(state: Signal<AppState>) -> Element {
    let ui = use_preferences();
    let page = state.read().page;
    let is_settings = SETTINGS.iter().any(|(p, ..)| *p == page);
    let tabs = settings_tabs(crate::platform::APP);
    let tab_index = tabs.iter().position(|(p, ..)| *p == page).unwrap_or(0);
    // A narrow tab bar scrolls sideways: keep the selected tab in view after taps, keys,
    // history and deep links. Only page changes run this, so manual scrolling is left alone.
    // On a fresh load the stylesheet and fonts can arrive after the first render and change
    // the tab widths, so the reveal is repeated briefly.
    let selected = use_memo(move || state.read().page);
    // The neighbours of the selected settings tab are rendered (as inert previews) only while
    // the pager moves, and loaded from the first drag of a settings visit on, through the
    // same reads a visit does, at most once per visit and never under the busy shell.
    let mut previews = use_signal(|| false);
    let mut loaded = use_signal(Vec::<Page>::new);
    use_effect(move || {
        let page = selected();
        if !SETTINGS.iter().any(|(p, ..)| *p == page) {
            if *previews.peek() {
                previews.set(false);
            }
            if !loaded.peek().is_empty() {
                loaded.write().clear();
            }
            return;
        }
        if !previews() {
            return;
        }
        // The selected tab loads through its visit (`set_page`).
        if !loaded.peek().contains(&page) {
            loaded.write().push(page);
        }
        for (_, neighbour, _) in settings_panels(crate::platform::APP, page, true) {
            if !loaded.peek().contains(&neighbour) {
                loaded.write().push(neighbour);
                spawn_root(async move { load_page_data(state, neighbour).await });
            }
        }
    });
    use_effect(move || {
        let page = selected();
        if let Some((_, key, ..)) = SETTINGS.iter().find(|(p, ..)| *p == page) {
            let id = format!("settings-tab-{key}");
            reveal_horizontally(&id);
            spawn(async move {
                for delay in [100, 400, 1000] {
                    crate::platform::sleep_ms(delay).await;
                    if *selected.peek() != page {
                        return;
                    }
                    reveal_horizontally(&id);
                }
            });
        }
    });
    rsx! {
        SkipLink {}
        Header { state }
        main { class: "container", id: CONTENT_ID, tabindex: "-1",
            Status { state }
            if is_settings {
                div { class: "page-intro",
                    h1 { {ui.text("shell.settings")} }
                    button { r#type: "button", onclick: move |_| set_page(state, Page::Dashboard), {ui.text("shell.dashboard")} }
                }
                div { class: "tabs settings-tabs", role: "tablist", "aria-label": ui.text("shell.settings_sections"),
                    for (target, key, label, short, icon) in tabs.clone() {
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
                            Glyph { icon }
                            span { class: "tab-full", {ui.text(label)} }
                            span { class: "tab-short", "aria-hidden": "true", {ui.text(short)} }
                        }
                    }
                }
                // Swipes switch tabs like a tap once the panels arrive: the URL, the selected
                // tab and its data load follow `set_page`; the scrolling app tab bar keeps the
                // new tab in view. Until then the neighbour is only a preview beside it.
                // While an action holds the busy shell, `set_page` refuses tab changes: no drag
                // starts then, so a completed swipe can never be left unselected.
                SwipeableTabs { id: "settings-swipe", count: tabs.len(), index: tab_index, enabled: !state.read().busy,
                    on_drag: move |active: bool| {
                        if *previews.peek() != active {
                            previews.set(active);
                        }
                    },
                    on_change: move |next: usize| {
                        if let Some((target, key, ..)) = settings_tabs(crate::platform::APP).get(next) {
                            set_page(state, *target);
                            reveal_horizontally(&format!("settings-tab-{key}"));
                        }
                    },
                    for (position, target, key) in settings_panels(crate::platform::APP, page, previews()) {
                        SettingsPanel { key: "{key}", state, page: target, tab: key, position, selected: target == page }
                    }
                }
            } else {
                if page == Page::Admin && !crate::platform::APP {
                    AdminPage { state }
                } else if page == Page::About {
                    AboutPage {}
                } else if page == Page::Events {
                    EventsPage { state }
                } else {
                    DashboardPage { state }
                }
            }
        }
    }
}

/// One settings tab's content. The selected one is the `tabpanel` the tab bar controls;
/// the others are previews the pager drags in: inert (nothing focusable), hidden from
/// assistive technology and without the panel's ID. Keyed by tab, so a completed swipe keeps
/// the previewed page (and its loaded state) as the new selected panel.
#[component]
fn SettingsPanel(
    state: Signal<AppState>,
    page: Page,
    tab: &'static str,
    position: usize,
    selected: bool,
) -> Element {
    let preview = (!selected).then_some("true");
    rsx! {
        section { class: "settings-section",
            id: selected.then_some("settings-panel"),
            role: selected.then_some("tabpanel"),
            "aria-labelledby": selected.then(|| format!("settings-tab-{tab}")),
            "data-swipe-page": position,
            "data-swipe-preview": preview,
            inert: preview,
            "aria-hidden": preview,
            match page {
                Page::Uids => rsx! { UidsPage { state } },
                Page::Rewards => rsx! { RewardsPage { state } },
                Page::Codes => rsx! { CodesPage { state } },
                Page::Link => rsx! { LinkPage { state } },
                Page::Security => rsx! { SecurityPage { state } },
                Page::Notifications => rsx! { NotificationsPage { state } },
                Page::Transfer if !crate::platform::APP => rsx! { TransferPage { state } },
                Page::Delete if !crate::platform::APP => rsx! { DeletePage { state } },
                _ => rsx! {},
            }
        }
    }
}

/// The `main` element's ID. Not "main": the Dioxus root element already uses that ID,
/// so a "#main" fragment would land on the root instead of the page content.
pub(super) const CONTENT_ID: &str = "content";

/// The first focus stop: moves keyboard focus past the header to the page content.
/// Focus moves explicitly because the fragment link alone does not move it here.
#[component]
pub(super) fn SkipLink() -> Element {
    let ui = use_preferences();
    rsx! {
        a { class: "skip-link", href: "#{CONTENT_ID}",
            onclick: move |event| { event.prevent_default(); focus_element(CONTENT_ID); },
            {ui.text("shell.skip_to_content")}
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
        true,
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
                                    onclick: move |_| set_page(state, Page::Uids), Glyph { icon: Icon::Settings }
                                }
                            }
                            button { id: "ahd-menu-trigger", class: "menu", r#type: "button", "aria-label": ui.text("shell.open_menu"),
                                "aria-controls": "ahd-menu", "aria-expanded": open().to_string(), onclick: move |_| open.set(true), Glyph { icon: Icon::Menu }
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
                    button { class: "icon-button", r#type: "button", "aria-label": ui.text("shell.close_menu"), onclick: move |_| open.set(false), Glyph { icon: Icon::Close } }
                }
                nav { "aria-label": ui.text("shell.account_menu"),
                    for item in menu_items(crate::platform::APP, me.as_ref().is_some_and(|me| me.is_admin)) {
                        match item {
                            MenuItem::Events => rsx! { button { class: "menu-events", r#type: "button", "aria-current": (state.read().page == Page::Events).then_some("page"), onclick: move |_| { open.set(false); set_page(state, Page::Events); }, Glyph { icon: Icon::Calendar } {ui.text("calendar.page_title")} } },
                            MenuItem::HoYoLab => rsx! { a { href: "https://www.hoyolab.com/home", target: "_blank", rel: "noopener noreferrer", Glyph { icon: Icon::Link } "HoYoLAB" } },
                            MenuItem::Settings => rsx! { button { r#type: "button", onclick: move |_| { open.set(false); set_page(state, Page::Uids); }, Glyph { icon: Icon::Settings } {ui.text("shell.settings")} } },
                            MenuItem::OpenInBrowser => rsx! { button { r#type: "button", onclick: move |_| { open.set(false); let page = state.read().page; open_web_version(state, page); }, Glyph { icon: Icon::External } {ui.text("shell.open_in_browser")} } },
                            MenuItem::Administration => rsx! { button { r#type: "button", onclick: move |_| { open.set(false); set_page(state, Page::Admin); }, Glyph { icon: Icon::User } {ui.text("shell.administration")} } },
                            MenuItem::Docs => rsx! { button { r#type: "button", onclick: move |_| { open.set(false); set_page(state, Page::Docs); }, Glyph { icon: Icon::Docs } {ui.text("docs.title")} } },
                            MenuItem::About => rsx! { button { r#type: "button", onclick: move |_| { open.set(false); set_page(state, Page::About); }, Glyph { icon: Icon::Info } {ui.text("about.title")} } },
                        }
                    }
                    div { class: "menu-logout-group",
                        button { class: "menu-logout", r#type: "button", onclick: move |_| { open.set(false); spawn_action(state, move |generation| async move { logout(state, generation).await; }); }, Glyph { icon: Icon::Logout } {ui.text("shell.sign_out")} }
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
                            onchange: move |event| preferences.write().choose_language(if event.value() == "en" { Language::English } else { Language::Japanese }),
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
            // Focusable so an action that fails can move focus here (see `actions::spawn_action`).
            div { class: "status {status.kind}", id: STATUS_ID, tabindex: "-1",
                p { {ui.text(status.message_key)} }
                if status.kind == "error" {
                    button { class: "icon-button status-dismiss", r#type: "button", "aria-label": ui.text("shell.dismiss_message"),
                        onclick: move |_| clear_status(state),
                        Glyph { icon: Icon::Close }
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
    fn both_menus_start_with_events_and_the_app_menu_has_no_settings() {
        use MenuItem::*;
        assert_eq!(
            menu_items(false, true),
            vec![Events, HoYoLab, Settings, Administration, Docs, About]
        );
        assert_eq!(
            menu_items(false, false),
            vec![Events, HoYoLab, Settings, Docs, About]
        );
        for admin in [true, false] {
            assert_eq!(
                menu_items(true, admin),
                vec![Events, OpenInBrowser, HoYoLab, About]
            );
        }
    }
    #[test]
    fn settings_tabs_wrap_and_do_not_handle_unrelated_keys() {
        assert!(next_settings(Page::Uids, &Key::ArrowLeft, false) == Some(Page::Delete));
        assert!(next_settings(Page::Delete, &Key::ArrowRight, false) == Some(Page::Uids));
        assert!(next_settings(Page::Rewards, &Key::Home, false) == Some(Page::Uids));
        assert!(next_settings(Page::Rewards, &Key::End, false) == Some(Page::Delete));
        // Codes sits between the login bonus and the HoYoLAB link, on the web and in the app.
        for app in [false, true] {
            assert!(next_settings(Page::Rewards, &Key::ArrowRight, app) == Some(Page::Codes));
            assert!(next_settings(Page::Codes, &Key::ArrowRight, app) == Some(Page::Link));
            assert!(next_settings(Page::Link, &Key::ArrowLeft, app) == Some(Page::Codes));
            assert!(next_settings(Page::Codes, &Key::ArrowLeft, app) == Some(Page::Rewards));
        }
        assert!(next_settings(Page::Dashboard, &Key::ArrowRight, false).is_none());
        assert!(next_settings(Page::Uids, &Key::Enter, false).is_none());
    }

    #[test]
    fn app_settings_tabs_leave_out_the_settings_file_and_account_deletion() {
        for web_only in [Page::Transfer, Page::Delete] {
            assert!(
                settings_tabs(false)
                    .iter()
                    .any(|(page, ..)| *page == web_only)
            );
            assert!(
                !settings_tabs(true)
                    .iter()
                    .any(|(page, ..)| *page == web_only)
            );
        }
        assert!(next_settings(Page::Uids, &Key::ArrowLeft, true) == Some(Page::Notifications));
        assert!(next_settings(Page::Notifications, &Key::ArrowRight, true) == Some(Page::Uids));
        assert!(next_settings(Page::Delete, &Key::ArrowRight, true).is_none());
    }

    #[test]
    fn every_settings_tab_has_its_own_icon() {
        let icons: std::collections::BTreeSet<_> =
            SETTINGS.iter().map(|(.., icon)| icon.path()).collect();
        assert_eq!(icons.len(), SETTINGS.len());
    }

    #[test]
    fn settings_previews_are_the_neighbours_while_dragging() {
        let pages = |page, previews, app| -> Vec<(usize, Page)> {
            settings_panels(app, page, previews)
                .into_iter()
                .map(|(position, page, _)| (position, page))
                .collect()
        };
        assert!(pages(Page::Rewards, false, false) == vec![(1, Page::Rewards)]);
        assert!(
            pages(Page::Rewards, true, false)
                == vec![(0, Page::Uids), (1, Page::Rewards), (2, Page::Codes)]
        );
        assert!(pages(Page::Uids, true, false) == vec![(0, Page::Uids), (1, Page::Rewards)]);
        assert!(pages(Page::Delete, true, false) == vec![(6, Page::Transfer), (7, Page::Delete)]);
        // The app ends at the notifications tab and never previews web-only pages.
        assert!(
            pages(Page::Notifications, true, true)
                == vec![(4, Page::Security), (5, Page::Notifications)]
        );
        assert!(pages(Page::Dashboard, true, false).is_empty());
        let keys: Vec<_> = settings_panels(false, Page::Link, true)
            .into_iter()
            .map(|(.., key)| key)
            .collect();
        assert_eq!(keys, ["codes", "link", "security"]);
    }

    /// Swipes walk the same tab order as the tab bar but stop at either end
    /// instead of wrapping like the arrow keys.
    #[test]
    fn settings_swipes_follow_the_tab_order_and_stop_at_the_ends() {
        use crate::app::swipe::{SwipeDirection, step};
        let swipe = |page: Page, direction, app| {
            let tabs = settings_tabs(app);
            let index = tabs.iter().position(|(p, ..)| *p == page)?;
            step(index, tabs.len(), direction).map(|next| tabs[next].0)
        };
        assert!(swipe(Page::Uids, SwipeDirection::Next, false) == Some(Page::Rewards));
        assert!(swipe(Page::Rewards, SwipeDirection::Previous, false) == Some(Page::Uids));
        assert!(swipe(Page::Rewards, SwipeDirection::Next, true) == Some(Page::Codes));
        assert!(swipe(Page::Link, SwipeDirection::Previous, true) == Some(Page::Codes));
        assert!(swipe(Page::Notifications, SwipeDirection::Next, false) == Some(Page::Transfer));
        assert!(swipe(Page::Transfer, SwipeDirection::Next, false) == Some(Page::Delete));
        assert!(swipe(Page::Uids, SwipeDirection::Previous, false).is_none());
        assert!(swipe(Page::Delete, SwipeDirection::Next, false).is_none());
        assert!(swipe(Page::Notifications, SwipeDirection::Next, true).is_none());
        assert!(swipe(Page::Security, SwipeDirection::Next, true) == Some(Page::Notifications));
    }
}
