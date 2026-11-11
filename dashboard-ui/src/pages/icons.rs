//! The shared icon set and the only ways to draw it: `Glyph` beside text that already says
//! the same thing, `NamedIcon` when the icon alone carries the meaning, and `StatusMark`
//! for a status symbol with its word. Icons are self-drawn line art on a 24px grid (one
//! path each, stroke 1.8, round caps, no fill; `dashboard.css` styles `svg`); no game art
//! is bundled. `DESIGN.md` lists the meanings.
use dioxus::prelude::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Icon {
    // Controls and navigation.
    Home,
    Menu,
    Close,
    Check,
    Link,
    Settings,
    User,
    Logout,
    Docs,
    External,
    Eye,
    Hide,
    Info,
    Refresh,
    Calendar,
    Chevron,
    // Settings and administration: tabs, notice kinds and the per-user actions.
    Bell,
    Shield,
    ShieldOff,
    Key,
    Trash,
    Ban,
    Transfer,
    // One mark per game. Headings, tabs and chips all name these, so a mark changes by
    // replacing one path. Neither is traced from an official logo.
    MarkGenshin,
    MarkStarRail,
    // Genshin Impact resources. Resin, coin, daily and flag follow the widget's
    // `ahd_indicator_icon_*` shapes.
    Resin,
    Coin,
    Daily,
    Discount,
    Transformer,
    Flag,
    // Honkai: Star Rail resources. Bolt and cube follow the widget icons.
    Bolt,
    Reserve,
    Cube,
    Orbit,
    Echo,
    // Endgame modes and their scores.
    Spiral,
    Curtain,
    Swords,
    Book,
    Eclipse,
    Scales,
    Star,
    Medal,
    Gift,
    // Event calendar rows: a wish or warp (a ticket with a star) versus an event
    // (`Calendar`), and the running and announced phases.
    Ticket,
    Running,
    Upcoming,
    // Status symbols (see `Status`) and time.
    Pending,
    Warning,
    Lock,
    Failed,
    Unknown,
    Loading,
    Hourglass,
    Clock,
    History,
}

impl Icon {
    /// The path data on a `0 0 24 24` view box.
    pub(super) const fn path(self) -> &'static str {
        match self {
            Self::Home => "M3 10.5 12 3l9 7.5M5.5 9v11h5v-6h3v6h5V9",
            Self::Menu => "M4 6h16M4 12h16M4 18h16",
            Self::Close => "M6 6l12 12M6 18 18 6",
            Self::Check => "M5 12.5l4.5 4.5L19 7.5",
            Self::Link => {
                "M10 13a4 4 0 0 0 6 0l4-4a4 4 0 0 0-6-6l-2 2M14 11a4 4 0 0 0-6 0l-4 4a4 4 0 0 0 6 6l2-2"
            }
            Self::Settings => "M4 7h16M4 17h16M8 4v6M16 14v6",
            Self::User => "M16 7a4 4 0 1 1-8 0 4 4 0 0 1 8 0M4 21v-2a8 8 0 0 1 16 0v2",
            Self::Logout => "M10 4H4v16h6M9 12h12m-4-4 4 4-4 4",
            Self::Docs => "M6 3h9l4 4v14H6zM14 3v5h5M9 12h7M9 16h7",
            Self::External => "M14 4h6v6M20 4l-9 9M18 14v6H4V6h6",
            Self::Eye => {
                "M2 12s3.5-7 10-7 10 7 10 7-3.5 7-10 7S2 12 2 12M15 12a3 3 0 1 1-6 0 3 3 0 0 1 6 0"
            }
            Self::Hide => {
                "M3 3l18 18M9.5 5.3A12 12 0 0 1 12 5c6.5 0 10 7 10 7a18 18 0 0 1-4 4.7M6 6.5A19 19 0 0 0 2 12s3.5 7 10 7a12 12 0 0 0 5-1.1M10 10a3 3 0 0 0 4 4"
            }
            Self::Info => "M12 21a9 9 0 1 0 0-18 9 9 0 0 0 0 18M12 11v5M12 8h.01",
            Self::Refresh => "M21 12a9 9 0 1 1-2.64-6.36L21 8M21 3v5h-5",
            // A calendar page with two period bars: the events timeline.
            Self::Calendar => "M4 6h16v14H4zM4 10h16M8 3v4M16 3v4M7 14h6M11 17h6",
            Self::Chevron => "M9 6l6 6-6 6",
            Self::Bell => "M6 16.5V11a6 6 0 0 1 12 0v5.5l1.5 2h-15ZM10 20.5a2 2 0 0 0 4 0",
            Self::Shield => "M12 3 4.5 6v5.5c0 4.5 3 8 7.5 9.5 4.5-1.5 7.5-5 7.5-9.5V6Z",
            // The shield struck through: taking the administrator role away.
            Self::ShieldOff => {
                "M12 3 4.5 6v5.5c0 4.5 3 8 7.5 9.5 4.5-1.5 7.5-5 7.5-9.5V6ZM3.5 3.5l17 17"
            }
            Self::Key => {
                "M8 11a4 4 0 1 0 0 8 4 4 0 0 0 0-8ZM10.8 12.2 19 4M16 7l2.5 2.5M13.5 9.5l2 2"
            }
            Self::Trash => "M4 7h16M9.5 7V4h5v3M6.5 7l1 13h9l1-13M10 11v5.5M14 11v5.5",
            // A struck-through circle: disabled, removed, stopped.
            Self::Ban => "M12 3a9 9 0 1 0 0 18 9 9 0 0 0 0-18ZM5.7 5.7l12.6 12.6",
            // One arrow up and one down: settings saved to and read from a file.
            Self::Transfer => "M7 20V5M3.5 8.5 7 5l3.5 3.5M17 4v15M13.5 15.5 17 19l3.5-3.5",
            // A four-pointed gem.
            Self::MarkGenshin => {
                "M12 2.5C12.8 8.2 15.8 11.2 21.5 12 15.8 12.8 12.8 15.8 12 21.5 11.2 15.8 8.2 12.8 2.5 12 8.2 11.2 11.2 8.2 12 2.5ZM12 9.2 14.8 12 12 14.8 9.2 12Z"
            }
            // A train front with a star.
            Self::MarkStarRail => {
                "M7 3.5h10a2.5 2.5 0 0 1 2.5 2.5v9a2.5 2.5 0 0 1-2.5 2.5H7A2.5 2.5 0 0 1 4.5 15V6A2.5 2.5 0 0 1 7 3.5ZM4.5 11h15M12 5.2l.7 1.6 1.6.7-1.6.7-.7 1.6-.7-1.6-1.6-.7 1.6-.7ZM8 14.3h.01M16 14.3h.01M8.5 17.5 6.5 21M15.5 17.5l2 3.5"
            }
            Self::Resin => {
                "M12 2.5C12 2.5 5.5 9.6 5.5 14.2a6.5 6.8 0 0 0 13 0C18.5 9.6 12 2.5 12 2.5ZM9 14.4a3.2 3.4 0 0 0 3 3.4"
            }
            Self::Coin => "M12 3a9 9 0 1 0 0 18 9 9 0 0 0 0-18ZM12 7.5 9.5 12l2.5 4.5 2.5-4.5Z",
            Self::Daily => "M12 3a9 9 0 1 0 0 18 9 9 0 0 0 0-18ZM7.8 12.4l2.9 2.9 5.5-5.6",
            Self::Discount => {
                "M3.5 3.5h8l9 9-8 8-9-9ZM7.3 7.3h.01M13 10.2s-2 2.2-2 3.7a2 2 0 0 0 4 0c0-1.5-2-3.7-2-3.7Z"
            }
            Self::Transformer => {
                "M12 8.5 14.5 12 12 15.5 9.5 12ZM5 9.5a7.5 7.5 0 0 1 13.2-2.3M18.4 3.9v3.5h-3.5M19 14.5a7.5 7.5 0 0 1-13.2 2.3M5.6 20.1v-3.5h3.5"
            }
            Self::Flag => "M6 21V3M6 4h12.5L16 8l2.5 4H6",
            Self::Bolt => "M13.5 2 5 13.5h6L10 22l9-12h-6Z",
            Self::Reserve => "M7 6h10v15H7ZM10 3h4v3M12.8 9.5l-2.5 4.3h3.4l-2.5 4.2",
            Self::Cube => {
                "M12 2.5 20.5 7.25v9.5L12 21.5 3.5 16.75v-9.5ZM3.5 7.25 12 12l8.5-4.75M12 12v9.5"
            }
            Self::Orbit => {
                "M12 7.5a4.5 4.5 0 1 0 0 9 4.5 4.5 0 0 0 0-9ZM7.7 13.6C4.4 15.6 2.4 17.4 3 18.4c.8 1.4 5.4-.1 10.3-3M16.3 10.4c3.3-2 5.3-3.8 4.7-4.8-.5-.9-2.6-.6-5.3.4"
            }
            Self::Echo => "M4 10v4M8 7v10M12 3.5v17M16 7v10M20 10v4",
            Self::Spiral => {
                "M12 12a1.5 1.5 0 0 1 3 0 3.5 3.5 0 0 1-7 0 5.5 5.5 0 0 1 11 0 7.5 7.5 0 0 1-15 0 8 8 0 0 1 8-8"
            }
            Self::Curtain => {
                "M3 4h18M5 4v16M19 4v16M5 5c.5 5 3 7.5 7 8M19 5c-.5 5-3 7.5-7 8M3 20h18"
            }
            Self::Swords => "M4 3l16 16M20 3 4 19M14 16l3-3M7 13l3 3",
            Self::Book => {
                "M12 6.5C10 5 7 4.5 3.5 5v13c3.5-.5 6.5 0 8.5 1.5 2-1.5 5-2 8.5-1.5V5c-3.5-.5-6.5 0-8.5 1.5ZM12 6.5v13"
            }
            Self::Eclipse => "M12 4a8 8 0 1 0 0 16 8 8 0 0 0 0-16ZM12 4a5 8 0 0 0 0 16",
            Self::Scales => {
                "M12 4v16M7.5 20h9M5.5 7h13M5.5 7 3 13a2.7 2.7 0 0 0 5 0ZM18.5 7 16 13a2.7 2.7 0 0 0 5 0Z"
            }
            Self::Star => {
                "M12 3.5l2.6 5.4 5.9.8-4.3 4.1 1 5.8L12 16.9l-5.2 2.7 1-5.8-4.3-4.1 5.9-.8Z"
            }
            Self::Medal => {
                "M12 10a5 5 0 1 0 0 10 5 5 0 0 0 0-10ZM9 10.8 6.5 3.5h4l1.5 3 1.5-3h4L15 10.8"
            }
            Self::Gift => {
                "M4 11h16v9H4ZM3 7h18v4H3ZM12 7v13M12 7C10.5 3.5 7 3.2 7 5.3 7 6.6 9 7 12 7ZM12 7c1.5-3.5 5-3.8 5-1.7C17 6.6 15 7 12 7Z"
            }
            Self::Ticket => {
                "M3 6.5h18v3.5a2 2 0 0 0 0 4v3.5H3V14a2 2 0 0 0 0-4ZM12 8.3l1 2.7 2.7 1-2.7 1-1 2.7-1-2.7-2.7-1 2.7-1Z"
            }
            // A play triangle: under way now.
            Self::Running => "M8 5.5v13l10.5-6.5Z",
            // Two chevrons pointing ahead: announced, not started.
            Self::Upcoming => "M6 6l6 6-6 6M13 6l6 6-6 6",
            Self::Pending => "M12 5a7 7 0 1 0 0 14 7 7 0 0 0 0-14Z",
            Self::Warning => "M12 3.5 2.5 20h19ZM12 10v4.5M12 17.3h.01",
            Self::Lock => "M6 11h12v9H6ZM8.5 11V8a3.5 3.5 0 0 1 7 0v3",
            Self::Failed => "M12 3a9 9 0 1 0 0 18 9 9 0 0 0 0-18ZM9 9l6 6M15 9l-6 6",
            Self::Unknown => {
                "M12 3a9 9 0 1 0 0 18 9 9 0 0 0 0-18ZM9.6 9.6a2.5 2.5 0 1 1 3.7 2.2c-.9.5-1.3 1-1.3 2M12 17h.01"
            }
            Self::Loading => "M12 3a9 9 0 1 0 9 9",
            Self::Hourglass => {
                "M7 3h10M7 21h10M8 3v3.5c0 2.5 4 3.5 4 5.5s-4 3-4 5.5V21M16 3v3.5c0 2.5-4 3.5-4 5.5s4 3 4 5.5V21"
            }
            Self::Clock => "M12 3a9 9 0 1 0 0 18 9 9 0 0 0 0-18ZM12 7.5V12l3 2",
            Self::History => "M3.5 12a8.5 8.5 0 1 0 2.6-6.1L3.5 8.5M3.5 4v4.5H8M12 8v4.5l3 1.5",
        }
    }

    /// The mark of a backend `game_kind`; unknown kinds have none.
    pub(super) fn game_mark(game: &str) -> Option<Self> {
        match game {
            "genshin" => Some(Self::MarkGenshin),
            "starrail" => Some(Self::MarkStarRail),
            _ => None,
        }
    }
}

/// A decorative icon: the text beside it already names it, so it is hidden from
/// assistive technology. `class` picks a size (`icon-sm`, `icon-lg`, `icon-xl`) or a role.
#[component]
pub(super) fn Glyph(icon: Icon, #[props(default)] class: &'static str) -> Element {
    rsx! {
        svg {
            class: (!class.is_empty()).then_some(class),
            view_box: "0 0 24 24", "aria-hidden": "true", "focusable": "false",
            path { d: icon.path() }
        }
    }
}

/// An icon that carries its meaning alone: `name` is its accessible name and tooltip.
#[component]
pub(super) fn NamedIcon(
    icon: Icon,
    name: String,
    #[props(default)] class: &'static str,
) -> Element {
    rsx! {
        span { class: "named-icon {class}", role: "img", "aria-label": name.clone(), title: name,
            Glyph { icon }
        }
    }
}

/// What a status symbol says. Each has one symbol and one tone, and is always shown with
/// its word (`StatusMark`), so colour is never the only signal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Status {
    /// Finished, claimed, ready.
    Done,
    /// Still to do today or this week.
    Open,
    /// At or near a cap, or outdated data.
    Warn,
    /// Not unlocked or not obtained.
    Locked,
    /// The fetch failed.
    Failed,
    /// No value was returned.
    Unknown,
    /// Counting down.
    Waiting,
    Loading,
    /// Under way now (events and banners).
    Running,
    /// Announced, not started yet.
    Upcoming,
    /// Switched off on purpose: disabled, removed, stopped.
    Off,
}

impl Status {
    pub(super) const fn icon(self) -> Icon {
        match self {
            Self::Done => Icon::Check,
            Self::Open => Icon::Pending,
            Self::Warn => Icon::Warning,
            Self::Locked => Icon::Lock,
            Self::Failed => Icon::Failed,
            Self::Unknown => Icon::Unknown,
            Self::Waiting => Icon::Hourglass,
            Self::Loading => Icon::Loading,
            Self::Running => Icon::Running,
            Self::Upcoming => Icon::Upcoming,
            Self::Off => Icon::Ban,
        }
    }

    /// The `data-status` value `dashboard.css` colours.
    pub(super) const fn name(self) -> &'static str {
        match self {
            Self::Done => "done",
            Self::Open => "open",
            Self::Warn => "warn",
            Self::Locked => "locked",
            Self::Failed => "failed",
            Self::Unknown => "unknown",
            Self::Waiting => "waiting",
            Self::Loading => "loading",
            Self::Running => "running",
            Self::Upcoming => "upcoming",
            Self::Off => "off",
        }
    }
}

/// A status symbol with its word. `hide_text` keeps the word for screen readers (and as
/// the tooltip) where the context already shows what the symbol refers to. `label` is
/// the full sentence when `text` is only its short form (a count or a duration): readers
/// and the tooltip get the sentence, the screen shows the symbol and the short form.
#[component]
pub(super) fn StatusMark(
    status: Status,
    text: String,
    #[props(default)] hide_text: bool,
    #[props(default)] label: Option<String>,
    #[props(default)] class: &'static str,
) -> Element {
    let title = label.clone().or_else(|| hide_text.then(|| text.clone()));
    rsx! {
        span {
            class: "status-mark {class}", "data-status": status.name(), title,
            Glyph { icon: status.icon() }
            if let Some(label) = label {
                span { class: "visually-hidden", "{label}" }
                span { class: "status-short", "aria-hidden": "true", "{text}" }
            } else {
                span { class: hide_text.then_some("visually-hidden"), "{text}" }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    const ALL: [Icon; 57] = [
        Icon::Home,
        Icon::Menu,
        Icon::Close,
        Icon::Check,
        Icon::Link,
        Icon::Settings,
        Icon::User,
        Icon::Logout,
        Icon::Docs,
        Icon::External,
        Icon::Eye,
        Icon::Hide,
        Icon::Info,
        Icon::Refresh,
        Icon::Calendar,
        Icon::Chevron,
        Icon::Bell,
        Icon::Shield,
        Icon::ShieldOff,
        Icon::Key,
        Icon::Trash,
        Icon::Ban,
        Icon::Transfer,
        Icon::MarkGenshin,
        Icon::MarkStarRail,
        Icon::Resin,
        Icon::Coin,
        Icon::Daily,
        Icon::Discount,
        Icon::Transformer,
        Icon::Flag,
        Icon::Bolt,
        Icon::Reserve,
        Icon::Cube,
        Icon::Orbit,
        Icon::Echo,
        Icon::Spiral,
        Icon::Curtain,
        Icon::Swords,
        Icon::Book,
        Icon::Eclipse,
        Icon::Scales,
        Icon::Star,
        Icon::Medal,
        Icon::Gift,
        Icon::Ticket,
        Icon::Running,
        Icon::Upcoming,
        Icon::Pending,
        Icon::Warning,
        Icon::Lock,
        Icon::Failed,
        Icon::Unknown,
        Icon::Loading,
        Icon::Hourglass,
        Icon::Clock,
        Icon::History,
    ];

    #[test]
    fn every_icon_is_one_distinct_path_inside_the_24px_grid() {
        let mut seen = BTreeSet::new();
        for icon in ALL {
            let path = icon.path();
            assert!(path.starts_with('M'), "{icon:?}");
            assert!(
                path.chars()
                    .all(|ch| ch.is_ascii_alphanumeric() || " .-".contains(ch)),
                "{icon:?} has only path commands and numbers"
            );
            assert!(seen.insert(path), "{icon:?} repeats another icon");
        }
    }

    #[test]
    fn each_game_has_its_own_mark_and_unknown_kinds_have_none() {
        assert_eq!(Icon::game_mark("genshin"), Some(Icon::MarkGenshin));
        assert_eq!(Icon::game_mark("starrail"), Some(Icon::MarkStarRail));
        assert_eq!(Icon::game_mark("zzz"), None);
        assert_ne!(Icon::MarkGenshin.path(), Icon::MarkStarRail.path());
    }

    #[test]
    fn status_symbols_are_distinct_shapes_with_distinct_names() {
        let all = [
            Status::Done,
            Status::Open,
            Status::Warn,
            Status::Locked,
            Status::Failed,
            Status::Unknown,
            Status::Waiting,
            Status::Loading,
            Status::Running,
            Status::Upcoming,
            Status::Off,
        ];
        let icons: BTreeSet<_> = all.iter().map(|status| status.icon().path()).collect();
        let names: BTreeSet<_> = all.iter().map(|status| status.name()).collect();
        assert_eq!(icons.len(), all.len());
        assert_eq!(names.len(), all.len());
    }
}
