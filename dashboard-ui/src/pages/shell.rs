use super::accounts::UidsPage;
use super::admin::AdminPage;
use super::common::Metric;
use super::dashboard::DashboardPage;
use super::deletion::DeletePage;
use super::docs::DocsPage;
use super::hoyolab::LinkPage;
use super::rewards::RewardsPage;
use super::security::SecurityPage;
use crate::app::actions::spawn_action;
use crate::app::session::logout;
use crate::app::state::set_page;
use crate::app::state::{AppState, Page};
use crate::components::{
    Button, ButtonVariant, Card, CardContent, CardDescription, CardHeader, CardTitle,
};
use dioxus::prelude::*;

#[component]
pub(in crate::app) fn Shell(mut state: Signal<AppState>) -> Element {
    let page = state.read().page;
    rsx! {
        Header { state }
        div { class: "dashboard-layout",
            nav { class: "side-nav", "aria-label": "Dashboard sections",
                NavButton { state, page: Page::Dashboard, current: page, label: "Dashboard" }
                NavButton { state, page: Page::Link, current: page, label: "HoYoLAB Link" }
                NavButton { state, page: Page::Uids, current: page, label: "UID Visibility" }
                NavButton { state, page: Page::Rewards, current: page, label: "Rewards" }
                NavButton { state, page: Page::Security, current: page, label: "Security" }
                if state.read().me.as_ref().is_some_and(|me| me.is_admin) {
                    NavButton { state, page: Page::Admin, current: page, label: "Admin" }
                }
                NavButton { state, page: Page::Delete, current: page, label: "Delete Account" }
            }
            main { class: "content-panel",
                Status { state }
                ServerSettingsSummary { state }
                match page {
                    Page::Dashboard => rsx! { DashboardPage { state } },
                    Page::Link => rsx! { LinkPage { state } },
                    Page::Uids => rsx! { UidsPage { state } },
                    Page::Rewards => rsx! { RewardsPage { state } },
                    Page::Security => rsx! { SecurityPage { state } },
                    Page::Admin => rsx! { AdminPage { state } },
                    Page::Delete => rsx! { DeletePage { state } },
                    Page::Docs => rsx! { DocsPage { state } },
                }
            }
        }
    }
}

#[component]
pub(super) fn Header(mut state: Signal<AppState>) -> Element {
    let me = state.read().me.clone();
    rsx! {
        header { class: "topbar",
            div {
                p { class: "eyebrow", "Private dashboard" }
                h1 { "AttOs Game Dashboard" }
            }
            div { class: "topbar-actions",
                if let Some(me) = me {
                    span { class: "user-chip", "{me.username}" }
                    if me.is_admin { span { class: "role-chip", "Admin" } }
                    if let Some(server) = state.read().server.clone() {
                        span { class: "role-chip", "TZ {server.timezone_offset} · {server.session_ttl_days}d" }
                    }
                    Button { variant: ButtonVariant::Ghost, r#type: "button", onclick: move |_| { let state = state; spawn_action(state, move |generation| async move { logout(state, generation).await; }); }, "Logout" }
                } else {
                    Button { variant: ButtonVariant::Outline, r#type: "button", onclick: move |_| set_page(state, Page::Docs), "API docs" }
                }
            }
        }
    }
}

#[component]
pub(super) fn Status(state: Signal<AppState>) -> Element {
    let status = state.read().status.clone();
    rsx! {
        div { class: "live-region", role: "status", "aria-live": "polite",
            if let Some(status) = status {
                p { class: "status {status.kind}", "{status.text}" }
            }
        }
    }
}

#[component]
fn ServerSettingsSummary(state: Signal<AppState>) -> Element {
    let server = state.read().server.clone();
    rsx! {
        if let Some(server) = server {
            p { class: "eyebrow", "Server timezone {server.timezone_offset}; session TTL {server.session_ttl_days} days" }
        }
    }
}

#[component]
pub(super) fn ServerSettingsCard(state: Signal<AppState>) -> Element {
    let server = state.read().server.clone();
    rsx! {
        Card {
            CardHeader { CardTitle { "Server settings" } CardDescription { "Shared fixed metadata reported by the dashboard API." } }
            CardContent {
                dl { class: "metric-list",
                    Metric { label: "Fixed timezone", value: server.as_ref().map(|s| s.timezone_offset.clone()).unwrap_or_else(|| "Not loaded".to_owned()) }
                    Metric { label: "Session TTL", value: server.as_ref().map(|s| format!("{} days", s.session_ttl_days)).unwrap_or_else(|| "Not loaded".to_owned()) }
                }
            }
        }
    }
}

#[component]
fn NavButton(
    mut state: Signal<AppState>,
    page: Page,
    current: Page,
    label: &'static str,
) -> Element {
    rsx! {
        Button { variant: if page == current { ButtonVariant::Secondary } else { ButtonVariant::Ghost }, r#type: "button", "aria-current": if page == current { "page" } else { "false" }, onclick: move |_| set_page(state, page), "{label}" }
    }
}
