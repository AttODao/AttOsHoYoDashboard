//! App-only bottom navigation and head additions.

use super::common::Icon;
use crate::app::preferences::use_preferences;
use crate::app::state::{AppState, Page, set_page};
use dioxus::prelude::*;

#[derive(Clone, Copy, PartialEq, Debug)]
enum NavTarget {
    Dashboard,
    Settings,
}

fn nav_target(page: Page) -> Option<NavTarget> {
    match page {
        Page::Dashboard => Some(NavTarget::Dashboard),
        Page::Uids
        | Page::Rewards
        | Page::Link
        | Page::Security
        | Page::Notifications
        | Page::Delete => Some(NavTarget::Settings),
        Page::Admin | Page::Docs => None,
    }
}

#[component]
pub(super) fn AppNav(state: Signal<AppState>, mut open: Signal<bool>) -> Element {
    let ui = use_preferences();
    let target = nav_target(state.read().page);
    rsx! {
        nav { class: "app-nav", "aria-label": ui.text("shell.app_navigation"),
            button { r#type: "button",
                "aria-current": (target == Some(NavTarget::Dashboard)).then_some("page"),
                onclick: move |_| set_page(state, Page::Dashboard),
                span { class: "app-nav-icon",
                    svg { view_box: "0 0 24 24", "aria-hidden": "true", "focusable": "false",
                        path { d: "M3 10.5 12 3l9 7.5M5.5 9v11h5v-6h3v6h5V9" }
                    }
                }
                span { class: "app-nav-label", {ui.text("dashboard.dashboard")} }
            }
            button { r#type: "button",
                "aria-current": (target == Some(NavTarget::Settings)).then_some("page"),
                onclick: move |_| set_page(state, Page::Uids),
                span { class: "app-nav-icon", Icon { name: "settings" } }
                span { class: "app-nav-label", {ui.text("shell.settings")} }
            }
            button { id: "ahd-app-menu-trigger", r#type: "button",
                "aria-controls": "ahd-menu", "aria-haspopup": "dialog", "aria-expanded": open().to_string(),
                onclick: move |_| open.set(true),
                span { class: "app-nav-icon", Icon { name: "menu" } }
                span { class: "app-nav-label", {ui.text("shell.menu")} }
            }
        }
    }
}

#[component]
pub(in crate::app) fn AppHead() -> Element {
    #[cfg(feature = "mobile")]
    {
        rsx! {
            document::Meta { name: "viewport", content: "width=device-width, initial-scale=1.0, maximum-scale=1.0, user-scalable=no, viewport-fit=cover" }
            document::Stylesheet { href: asset!("/assets/mobile.css") }
        }
    }
    #[cfg(not(feature = "mobile"))]
    rsx! {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dashboard_targets_dashboard() {
        assert_eq!(nav_target(Page::Dashboard), Some(NavTarget::Dashboard));
    }

    #[test]
    fn every_settings_page_targets_settings() {
        for page in [
            Page::Uids,
            Page::Rewards,
            Page::Link,
            Page::Security,
            Page::Notifications,
            Page::Delete,
        ] {
            assert_eq!(nav_target(page), Some(NavTarget::Settings));
        }
    }

    #[test]
    fn web_only_pages_have_no_navigation_target() {
        assert_eq!(nav_target(Page::Admin), None);
        assert_eq!(nav_target(Page::Docs), None);
    }
}
