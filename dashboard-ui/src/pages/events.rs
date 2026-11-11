//! The events page: running and announced events and banners (wishes / warps) of each game,
//! which are the same for every UID. Wide browser windows draw a horizontal timeline
//! (`event_timeline.rs`); phones and the app read it downwards (`event_list.rs`). The data is
//! the cached calendar of each game's selected (or first visible) UID, loaded when the page
//! opens (`app::data::load_events_data`), never under the busy shell.
use super::calendar::{Groups, SummaryChips, calendar_notice};
use super::common::{LoadError, LoadingLine, WarningLine, game_name, pending, refresh_problem};
use super::dashboard::{Freshness, pull_refresh, use_clock, use_clock_provider};
use super::event_list::EventList;
use super::event_timeline::{LocalTime, WideTimeline};
use super::game_tabs::{GameTabs, game_index, select_game_index, use_narrow_games};
use super::icons::{Glyph, Icon, Status};
use super::swipe_tabs::SwipeableTabs;
use crate::app::actions::spawn_root;
use crate::app::data::load_game_calendar;
use crate::app::display::seconds_since;
use crate::app::preferences::use_preferences;
use crate::app::state::{AppState, DASHBOARD_GAMES, Page, set_page};
use crate::models::GameCalendar;
use dioxus::prelude::*;

#[component]
pub(super) fn EventsPage(state: Signal<AppState>) -> Element {
    let ui = use_preferences();
    let active = use_memo(move || state.read().dashboard_game);
    let narrow = use_narrow_games();
    use_clock_provider();
    rsx! {
        div { class: "page-stack events",
            {pull_refresh(state, Page::Events)}
            div { class: "page-intro",
                h1 { {ui.text("calendar.page_title")} }
                button { r#type: "button", onclick: move |_| set_page(state, Page::Dashboard), {ui.text("shell.dashboard")} }
            }
            if state.read().accounts_error.is_some() {
                LoadError { state, message: ui.text("dashboard.could_not_refresh_the_uid_list") }
            }
            GameTabs { state, prefix: "events" }
            // Like the dashboard: one game at a time on narrow screens, swiped like a tab tap.
            SwipeableTabs {
                id: "events-panels", class: "events-grid",
                count: DASHBOARD_GAMES.len(), index: game_index(active()), enabled: narrow(),
                on_change: move |index: usize| select_game_index(state, index),
                for game in DASHBOARD_GAMES {
                    EventsPanel { key: "{game}", state, game, active: active() == game, narrow: narrow() }
                }
            }
        }
    }
}

/// One game: its mark and title, the age of the calendar (clock and "12 min ago") and
/// (web) its refresh control, then the timeline. Without a visible UID there is nothing to
/// read the calendar through.
#[component]
fn EventsPanel(state: Signal<AppState>, game: &'static str, active: bool, narrow: bool) -> Element {
    let ui = use_preferences();
    let snapshot = state.read();
    let accounts_loaded = snapshot.accounts.is_some();
    let accounts_failed = snapshot.accounts_error.is_some();
    let id = snapshot.selected_game_account(game);
    let account = id.and_then(|id| {
        snapshot
            .account_rows()
            .iter()
            .find(|account| account.id == id)
            .cloned()
    });
    let selection = snapshot
        .selected_accounts
        .get(game)
        .cloned()
        .unwrap_or_default();
    let calendar = id.and_then(|id| {
        snapshot
            .account_calendars
            .iter()
            .find(|entry| entry.id == id)
            .map(|entry| entry.calendar.clone())
    });
    drop(snapshot);
    // A server without the endpoint (404) has nothing to show; it is not loading either.
    let unavailable = selection.calendar_unavailable && calendar.is_none();
    let loading = pending(
        selection.calendar_loading,
        calendar.is_some(),
        selection.calendar_error.is_some() || unavailable,
    );
    let (fetched_at, stale) = calendar
        .as_ref()
        .map(|calendar| (calendar.fetched_at.clone(), calendar.stale))
        .unwrap_or_default();
    let progress_shown =
        calendar.as_ref().is_some_and(|calendar| {
            calendar.items.iter().flatten().any(|item| {
                item.finished.is_some() || item.locked.is_some() || item.progress.is_some()
            })
        });
    rsx! {
        section {
            class: "panel game-panel events-panel {game}", id: "events-panel-{game}",
            role: if narrow { "tabpanel" } else { "region" },
            "aria-labelledby": if narrow { format!("events-tab-{game}") } else { format!("events-title-{game}") },
            "data-selected": active, hidden: narrow && !active,
            tabindex: if narrow { "0" } else { "-1" },
            div { class: "game-heading card-heading",
                h2 { class: "game-title", id: "events-title-{game}",
                    if let Some(mark) = Icon::game_mark(game) { Glyph { icon: mark, class: "game-mark" } }
                    span { {game_name(game, ui)} }
                }
                Freshness { fetched_at, stale, short: true }
                // The app refreshes by pulling the page down instead.
                if !crate::platform::APP && id.is_some() { CalendarRefresh { state, game } }
            }
            if !accounts_loaded {
                // A failed first load shows the page-level warning instead.
                if !accounts_failed { LoadingLine {} }
            } else if id.is_none() {
                div { class: "empty",
                    h3 { {ui.text("dashboard.no_visible_uids")} }
                    p { class: "muted", {ui.text("calendar.needs_uid")} }
                    button { r#type: "button", onclick: move |_| set_page(state, Page::Uids), {ui.text("dashboard.choose_uids")} }
                }
            } else {
                if unavailable {
                    WarningLine { status: Status::Failed, text: ui.text("calendar.fetch_failed") }
                }
                if let Some(key) = selection.calendar_error {
                    // Without a calendar there is nothing that could be outdated: the fetch
                    // failed. With one, the shown content stays and is only not renewed.
                    WarningLine {
                        status: if calendar.is_some() { Status::Warn } else { Status::Failed },
                        text: ui.text(refresh_problem(key, calendar.is_some(), "calendar.fetch_failed")),
                    }
                }
                div { class: "events-content", "aria-busy": loading,
                    if let Some(calendar) = calendar {
                        EventsBody { calendar, game }
                    } else if loading {
                        LoadingLine {}
                    }
                }
                // Event progress is the selected UID's; the banners and periods are everyone's.
                if let Some(account) = account.filter(|_| progress_shown) {
                    ProgressSource { uid: account.game_uid }
                }
            }
        }
    }
}

/// Whose progress the event rows show: the person icon and the UID; the sentence is the
/// readers' text and the tooltip.
#[component]
fn ProgressSource(uid: String) -> Element {
    let ui = use_preferences();
    let sentence = ui.format("calendar.progress_of", &[("uid", &uid)]);
    rsx! {
        p { class: "events-source", title: sentence.clone(),
            Glyph { icon: Icon::User, class: "icon-sm" }
            span { class: "visually-hidden", "{sentence}" }
            span { "aria-hidden": "true", {ui.format("dashboard.uid_only", &[("uid", &uid)])} }
        }
    }
}

/// The summary chips, then the timeline: across on wide browser windows (the list stays as
/// the readers' text), downwards on phones and in the app.
#[component]
fn EventsBody(calendar: GameCalendar, game: &'static str) -> Element {
    let ui = use_preferences();
    // Grouping and every countdown follow the shared clock.
    let clock = use_clock();
    let _ = clock();
    let local = LocalTime::current();
    let notice = calendar_notice(&calendar, ui);
    let groups = calendar
        .items
        .as_deref()
        .map(|items| Groups::from_items(items, seconds_since));
    rsx! {
        if let Some(notice) = notice {
            // No items at all: the fetch failed. Otherwise the shown calendar is outdated.
            WarningLine { status: if calendar.items.is_none() { Status::Failed } else { Status::Warn }, text: notice }
        }
        if let Some(groups) = groups {
            if groups.is_empty() {
                p { class: "calendar-empty", {ui.text("calendar.empty")} }
            } else {
                SummaryChips { groups: groups.clone() }
                div { class: "events-body", "data-wide": !crate::platform::APP,
                    if !crate::platform::APP {
                        WideTimeline { groups: groups.clone(), game, local }
                    }
                    EventList { groups, game, local }
                }
            }
        }
    }
}

/// Web refresh control for one game's calendar, like the dashboard's.
#[component]
fn CalendarRefresh(state: Signal<AppState>, game: &'static str) -> Element {
    let ui = use_preferences();
    let loading = state
        .read()
        .selected_accounts
        .get(game)
        .is_some_and(|selection| selection.calendar_loading);
    let label = ui.text(if loading {
        "dashboard.refreshing"
    } else {
        "dashboard.refresh"
    });
    rsx! {
        // Enabled while loading (aria-disabled) so keyboard focus stays; repeats are ignored.
        button {
            r#type: "button", class: "icon-button refresh-button", id: "events-refresh-{game}",
            "aria-disabled": loading, "aria-busy": loading, "aria-label": label, title: label,
            onclick: move |_| {
                // Read the flag now: a second press may come before the re-render.
                let busy = state.peek().selected_accounts.get(game).is_some_and(|selection| selection.calendar_loading);
                if !busy { spawn_root(async move { load_game_calendar(state, game).await; }); }
            },
            Glyph { icon: Icon::Refresh }
        }
    }
}
