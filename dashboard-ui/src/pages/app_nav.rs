//! App-only bottom navigation and head additions.

use super::icons::{Glyph, Icon};
use crate::app::preferences::use_preferences;
use crate::app::state::{AppState, Page, set_page};
use dioxus::prelude::*;

#[derive(Clone, Copy, PartialEq, Debug)]
enum NavTarget {
    Dashboard,
    Events,
    Settings,
}

fn nav_target(page: Page) -> Option<NavTarget> {
    match page {
        Page::Dashboard => Some(NavTarget::Dashboard),
        Page::Events => Some(NavTarget::Events),
        Page::Uids
        | Page::Rewards
        | Page::Link
        | Page::Security
        | Page::Notifications
        | Page::Transfer
        | Page::Delete => Some(NavTarget::Settings),
        Page::Admin | Page::Docs | Page::About => None,
    }
}

/// The page entries of the bottom bar, left to right; the menu button follows them.
const NAV: [NavTarget; 3] = [NavTarget::Dashboard, NavTarget::Events, NavTarget::Settings];

impl NavTarget {
    /// The page a tap opens (Settings opens its first tab).
    fn page(self) -> Page {
        match self {
            NavTarget::Dashboard => Page::Dashboard,
            NavTarget::Events => Page::Events,
            NavTarget::Settings => Page::Uids,
        }
    }

    fn label(self) -> &'static str {
        match self {
            NavTarget::Dashboard => "dashboard.dashboard",
            NavTarget::Events => "calendar.page_title",
            NavTarget::Settings => "shell.settings",
        }
    }
}

#[component]
pub(super) fn AppNav(state: Signal<AppState>, mut open: Signal<bool>) -> Element {
    let ui = use_preferences();
    let target = nav_target(state.read().page);
    rsx! {
        nav { class: "app-nav", "aria-label": ui.text("shell.app_navigation"),
            for entry in NAV {
                button { key: "{entry:?}", r#type: "button",
                    "aria-current": (target == Some(entry)).then_some("page"),
                    onclick: move |_| set_page(state, entry.page()),
                    span { class: "app-nav-icon",
                        match entry {
                            NavTarget::Dashboard => rsx! { Glyph { icon: Icon::Home } },
                            NavTarget::Events => rsx! { Glyph { icon: Icon::Calendar } },
                            NavTarget::Settings => rsx! { Glyph { icon: Icon::Settings } },
                        }
                    }
                    span { class: "app-nav-label", {ui.text(entry.label())} }
                }
            }
            button { id: "ahd-app-menu-trigger", r#type: "button",
                "aria-controls": "ahd-menu", "aria-haspopup": "dialog", "aria-expanded": open().to_string(),
                onclick: move |_| open.set(true),
                span { class: "app-nav-icon", Glyph { icon: Icon::Menu } }
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
    fn about_has_no_navigation_target() {
        assert_eq!(nav_target(Page::About), None);
    }

    #[test]
    fn dashboard_targets_dashboard() {
        assert_eq!(nav_target(Page::Dashboard), Some(NavTarget::Dashboard));
    }

    #[test]
    fn events_target_events_right_after_the_dashboard() {
        assert_eq!(nav_target(Page::Events), Some(NavTarget::Events));
        assert_eq!(
            NAV,
            [NavTarget::Dashboard, NavTarget::Events, NavTarget::Settings]
        );
        for entry in NAV {
            assert_eq!(nav_target(entry.page()), Some(entry));
        }
    }

    #[test]
    fn every_settings_page_targets_settings() {
        for page in [
            Page::Uids,
            Page::Rewards,
            Page::Link,
            Page::Security,
            Page::Notifications,
            Page::Transfer,
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
