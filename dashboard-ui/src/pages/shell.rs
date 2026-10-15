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
use crate::app::preferences::{Language, Preferences, Theme, use_preferences};
use crate::app::session::logout;
use crate::app::state::{AppState, Page, set_page};
use dioxus::prelude::*;

const SETTINGS: [(Page, &str, &str, &str, &str); 5] = [
    (Page::Uids, "uids", "ゲーム表示", "Games", "ゲーム"),
    (
        Page::Rewards,
        "bonus",
        "ログインボーナス",
        "Check-in",
        "ボーナス",
    ),
    (Page::Link, "link", "HoYoLAB連携", "Link", "連携"),
    (
        Page::Security,
        "security",
        "セキュリティ",
        "Security",
        "認証",
    ),
    (Page::Delete, "delete", "ユーザー削除", "Delete", "削除"),
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
                    h1 { {ui.text("設定", "Settings")} }
                    button { r#type: "button", onclick: move |_| set_page(state, Page::Dashboard), {ui.text("ダッシュボードへ", "Dashboard")} }
                }
                div { class: "tabs settings-tabs", role: "tablist", "aria-label": ui.text("設定項目", "Settings sections"),
                    for (target, key, japanese, english, short) in SETTINGS {
                        button { id: "settings-tab-{key}", r#type: "button", role: "tab", "aria-controls": "settings-panel",
                            "aria-selected": (page == target).to_string(), tabindex: if page == target { "0" } else { "-1" },
                            "aria-label": ui.text(japanese, english), onclick: move |_| set_page(state, target),
                            onkeydown: move |event| {
                                if let Some(next) = next_settings(page, &event.key()) {
                                    event.prevent_default(); set_page(state, next);
                                    if let Some((_, key, ..)) = SETTINGS.iter().find(|(p, ..)| *p == next) {
                                        focus_element(&format!("settings-tab-{key}"));
                                    }
                                }
                            },
                            span { class: "tab-full", {ui.text(japanese, english)} }
                            span { class: "tab-short", "aria-hidden": "true", {ui.text(short, english)} }
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
                span { class: "wordmark", "AGD" }
                div { class: "account",
                    if let Some(me) = me.clone() {
                        span { class: "muted account-name", "{me.username}" }
                        if ready {
                            button { id: "agd-menu-trigger", class: "menu", r#type: "button", "aria-label": ui.text("メニューを開く", "Open menu"),
                                "aria-controls": "agd-menu", "aria-expanded": open().to_string(), onclick: move |_| open.set(true), Icon { name: "menu" }
                            }
                        } else {
                            button { r#type: "button", onclick: move |_| { spawn_action(state, move |generation| async move { logout(state, generation).await; }); }, {ui.text("ログアウト", "Sign out")} }
                        }
                    }
                }
            }
        }
        if ready {
            dialog { id: "agd-menu", class: "menu-drawer", "aria-label": ui.text("アカウントメニュー", "Account menu"),
                div { class: "drawer-heading", h2 { {ui.text("メニュー", "Menu")} }
                    button { class: "icon-button", r#type: "button", "aria-label": ui.text("メニューを閉じる", "Close menu"), onclick: move |_| open.set(false), Icon { name: "close" } }
                }
                nav { "aria-label": ui.text("アカウントメニュー", "Account menu"),
                    button { r#type: "button", onclick: move |_| { open.set(false); set_page(state, Page::Link); }, Icon { name: "link" } {ui.text("HoYoLAB連携", "HoYoLAB link")} }
                    button { r#type: "button", onclick: move |_| { open.set(false); set_page(state, Page::Uids); }, Icon { name: "settings" } {ui.text("設定", "Settings")} }
                    if me.as_ref().is_some_and(|me| me.is_admin) {
                        button { r#type: "button", onclick: move |_| { open.set(false); set_page(state, Page::Admin); }, Icon { name: "user" } {ui.text("管理者設定", "Administration")} }
                    }
                    button { r#type: "button", onclick: move |_| { open.set(false); spawn_action(state, move |generation| async move { logout(state, generation).await; }); }, Icon { name: "logout" } {ui.text("ログアウト", "Sign out")} }
                }
                div { class: "menu-preferences",
                    button { class: "mode-switch", r#type: "button", role: "switch", "aria-label": ui.text("ダークモード", "Dark mode"),
                        "aria-checked": (ui.theme == Theme::Dark).to_string(), onclick: move |_| {
                            let next = if preferences.read().theme == Theme::Dark { Theme::Light } else { Theme::Dark }; preferences.write().theme = next;
                        },
                        span { {ui.text("ダークモード", "Dark mode")} } span { class: "mode-track", span { class: "mode-thumb" } }
                    }
                    fieldset { class: "language-options", legend { {ui.text("言語", "Language")} }
                        div {
                            button { r#type: "button", lang: "ja", "aria-pressed": (ui.language == Language::Japanese).to_string(), onclick: move |_| preferences.write().language = Language::Japanese, "日本語" }
                            button { r#type: "button", lang: "en", "aria-pressed": (ui.language == Language::English).to_string(), onclick: move |_| preferences.write().language = Language::English, "English" }
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
        .filter(|status| status.text != "Please sign in.");
    rsx! { div { class: "live-region", role: "status", "aria-live": "polite",
        if let Some(status) = status { p { class: "status {status.kind}", "{ui.message(&status.text)}" } }
    } }
}

#[component]
pub(super) fn ServerSettingsCard(state: Signal<AppState>) -> Element {
    let ui = use_preferences();
    let server = state.read().server.clone();
    rsx! { section { class: "panel", h2 { {ui.text("サーバー設定", "Server settings")} }
        dl { class: "metric-list",
            Metric { label: ui.text("タイムゾーン", "Time zone"), value: server.as_ref().map(|s| s.timezone_offset.clone()).unwrap_or_else(|| ui.text("未取得", "Not loaded").to_owned()) }
            Metric { label: ui.text("セッション有効期間", "Session lifetime"), value: server.as_ref().map(|s| format!("{} {}", s.session_ttl_days, ui.text("日", "days"))).unwrap_or_else(|| ui.text("未取得", "Not loaded").to_owned()) }
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
