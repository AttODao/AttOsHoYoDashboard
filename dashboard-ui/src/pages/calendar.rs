//! What the event calendar of one game says, shared by both events-page layouts
//! (`event_timeline.rs` across, `event_list.rs` down): which items run or are announced,
//! their countdowns, names, status and notices.
use super::icons::{Icon, Status, StatusMark};
use crate::app::display::{days_hours, error_text};
use crate::app::preferences::{Preferences, use_preferences};
use crate::models::{CalendarFeatured, CalendarItem, GameCalendar};
use dioxus::prelude::*;

/// The rate-up line: 5-star items (or items without a rarity) with their official images,
/// then the other names as quieter text.
#[component]
pub(super) fn FeaturedLine(item: CalendarItem) -> Element {
    let (main, minor) = featured_parts(&item);
    let has_main = !main.is_empty();
    rsx! {
        if !main.is_empty() || !minor.is_empty() {
            p { class: "calendar-featured",
                for (index, featured) in main.into_iter().enumerate() {
                    // Flex gaps separate the names on screen; readers get a separator.
                    span { key: "{index}-{featured.icon.as_deref().unwrap_or_default()}",
                        if index > 0 { span { class: "visually-hidden", " · " } }
                        FeaturedName { featured }
                    }
                }
                if !minor.is_empty() {
                    if has_main { span { class: "visually-hidden", " · " } }
                    span { class: "muted", "{minor}" }
                }
            }
        }
    }
}

/// 5-star (or unrated) rate-up items, and the other names joined for one quiet line.
fn featured_parts(item: &CalendarItem) -> (Vec<CalendarFeatured>, String) {
    let (main, minor): (Vec<_>, Vec<_>) = item
        .featured
        .iter()
        .cloned()
        .partition(|featured| featured.rarity.is_none_or(|rarity| rarity >= 5));
    let minor = minor
        .iter()
        .filter_map(|featured| featured.name.clone())
        .collect::<Vec<_>>()
        .join(" · ");
    (main, minor)
}

/// A rate-up item: the official image when it loads, and its name as text either way
/// (`dashboard.css` sizes the image: larger in the list than on a timeline bar).
#[component]
fn FeaturedName(featured: CalendarFeatured) -> Element {
    let mut failed = use_signal(|| false);
    let name = featured.name.clone().unwrap_or_default();
    rsx! {
        span { class: "featured",
            if let Some(icon) = featured.icon.clone().filter(|icon| !icon.is_empty() && !failed()) {
                img { class: "featured-icon", src: icon, alt: "", width: "40", height: "40",
                    loading: "lazy", decoding: "async", referrerpolicy: "no-referrer",
                    onerror: move |_| failed.set(true),
                }
            }
            "{name}"
        }
    }
}

/// An item with the seconds elapsed since its start and end (negative before them).
#[derive(Clone, PartialEq)]
pub(super) struct Timed {
    pub(super) item: CalendarItem,
    pub(super) since_start: Option<i64>,
    pub(super) since_end: Option<i64>,
}

impl Timed {
    /// The start as Unix seconds, for a clock reading `now`.
    pub(super) fn start(&self, now: i64) -> Option<i64> {
        self.since_start.map(|since| now - since)
    }

    pub(super) fn end(&self, now: i64) -> Option<i64> {
        self.since_end.map(|since| now - since)
    }

    pub(super) fn is_event(&self) -> bool {
        self.item.kind == "event"
    }

    /// Neither the start nor the end is known: nothing places the item in time.
    pub(super) fn is_undated(&self) -> bool {
        self.since_start.is_none() && self.since_end.is_none()
    }
}

/// Running banners and events, announced items, and items without any known period
/// (kept apart so they are never drawn as if they ran across the whole timeline);
/// ended items are dropped.
#[derive(Clone, PartialEq, Default)]
pub(super) struct Groups {
    pub(super) banners: Vec<Timed>,
    pub(super) events: Vec<Timed>,
    pub(super) upcoming: Vec<Timed>,
    /// Banners first, then events, each in the server's order.
    pub(super) undated: Vec<Timed>,
}

impl Groups {
    pub(super) fn from_items(
        items: &[CalendarItem],
        elapsed: impl Fn(Option<&str>) -> Option<i64>,
    ) -> Self {
        let mut groups = Self::default();
        for item in items {
            let timed = Timed {
                since_start: elapsed(item.starts_at.as_deref()),
                since_end: elapsed(item.ends_at.as_deref()),
                item: item.clone(),
            };
            if timed.since_end.is_some_and(|since| since >= 0) {
                continue;
            }
            if timed.is_undated() {
                groups.undated.push(timed);
            } else if timed.since_start.is_some_and(|since| since < 0) {
                groups.upcoming.push(timed);
            } else if timed.item.kind == "event" {
                groups.events.push(timed);
            } else {
                groups.banners.push(timed);
            }
        }
        // Stable: banners keep their order ahead of events.
        groups.undated.sort_by_key(Timed::is_event);
        groups
    }

    pub(super) fn is_empty(&self) -> bool {
        self.banners.is_empty()
            && self.events.is_empty()
            && self.upcoming.is_empty()
            && self.undated.is_empty()
    }

    fn running(&self) -> impl Iterator<Item = &Timed> {
        self.banners.iter().chain(&self.events)
    }

    /// Running items, banners first, each kind ending soonest first (unknown ends last).
    pub(super) fn running_by_end(&self) -> Vec<Timed> {
        let mut banners = self.banners.clone();
        let mut events = self.events.clone();
        for items in [&mut banners, &mut events] {
            items.sort_by_key(|timed| {
                (
                    timed.since_end.is_none(),
                    timed.since_end.map(|since| -since),
                )
            });
        }
        banners.extend(events);
        banners
    }

    /// Announced items, starting soonest first.
    pub(super) fn upcoming_by_start(&self) -> Vec<Timed> {
        let mut upcoming = self.upcoming.clone();
        upcoming.sort_by_key(|timed| timed.since_start.map(|since| -since));
        upcoming
    }

    /// The two lanes of the timeline: banners (running and announced) and events, each in
    /// start order (unknown starts first: they began before anything else), then end order.
    pub(super) fn lanes(&self) -> [Vec<Timed>; 2] {
        let (upcoming_events, upcoming_banners): (Vec<_>, Vec<_>) =
            self.upcoming.iter().cloned().partition(Timed::is_event);
        let mut banners = self.banners.clone();
        banners.extend(upcoming_banners);
        let mut events = self.events.clone();
        events.extend(upcoming_events);
        for lane in [&mut banners, &mut events] {
            lane.sort_by_key(|timed| {
                (
                    timed.since_start.map(|since| -since),
                    timed.since_end.is_none(),
                    timed.since_end.map(|since| -since),
                )
            });
        }
        [banners, events]
    }
}

/// One summary chip: a status symbol with a count or a duration, and the sentence it
/// stands for (readers and the tooltip get the sentence).
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Chip {
    /// The `data-chip` value.
    pub(super) name: &'static str,
    pub(super) status: Status,
    pub(super) text: String,
    pub(super) label: String,
}

/// "3 running", "1 upcoming", "2 with unknown period" and "next ends in 2d 3h" as chips;
/// unknown ends are left out of the nearest end.
pub(super) fn summary_chips(groups: &Groups, ui: Preferences) -> Vec<Chip> {
    let count = |name, status, count: usize, key| Chip {
        name,
        status,
        text: count.to_string(),
        label: ui.format(key, &[("count", &count.to_string())]),
    };
    let running = groups.running().count();
    let mut chips = vec![if running == 0 {
        Chip {
            name: "running",
            status: Status::Running,
            text: "0".to_owned(),
            label: ui.text("calendar.summary_none").to_owned(),
        }
    } else {
        count(
            "running",
            Status::Running,
            running,
            "calendar.summary_running",
        )
    }];
    if !groups.upcoming.is_empty() {
        chips.push(count(
            "upcoming",
            Status::Upcoming,
            groups.upcoming.len(),
            "calendar.summary_upcoming",
        ));
    }
    if !groups.undated.is_empty() {
        chips.push(count(
            "undated",
            Status::Unknown,
            groups.undated.len(),
            "calendar.summary_undated",
        ));
    }
    let nearest = groups.running().filter_map(|timed| timed.since_end).max();
    if let Some(since) = nearest {
        let duration = days_hours(-since, ui);
        chips.push(Chip {
            name: "next-end",
            status: Status::Waiting,
            label: ui.format("calendar.summary_next_end", &[("duration", &duration)]),
            text: duration,
        });
    }
    chips
}

/// The summary under the game heading: one chip per count, then the nearest end.
#[component]
pub(super) fn SummaryChips(groups: Groups) -> Element {
    let ui = use_preferences();
    rsx! {
        ul { class: "event-chips",
            for chip in summary_chips(&groups, ui) {
                li { key: "{chip.name}", class: "event-chip", "data-chip": chip.name,
                    StatusMark { status: chip.status, text: chip.text, label: chip.label }
                }
            }
        }
    }
}

/// A countdown as a status symbol with the time left: `sentence` says what it counts
/// down to, `at` is that moment.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Countdown {
    pub(super) status: Status,
    pub(super) duration: String,
    pub(super) sentence: String,
    pub(super) at: Option<String>,
}

/// Time until an announced item starts, else until a running item ends.
pub(super) fn countdown(timed: &Timed, ui: Preferences) -> Option<Countdown> {
    if let Some(since) = timed.since_start.filter(|since| *since < 0) {
        let duration = days_hours(-since, ui);
        return Some(Countdown {
            status: Status::Upcoming,
            sentence: ui.format("calendar.starts_in", &[("duration", &duration)]),
            duration,
            at: timed.item.starts_at.clone(),
        });
    }
    let duration = days_hours(-timed.since_end?, ui);
    Some(Countdown {
        status: Status::Waiting,
        sentence: ui.format("calendar.ends_in", &[("duration", &duration)]),
        duration,
        at: timed.item.ends_at.clone(),
    })
}

/// What the player did in an event, as a status symbol with its word (or "2/5" for a
/// progress, whose sentence is the label).
#[derive(Clone, Debug, PartialEq)]
pub(super) struct ItemStatus {
    pub(super) status: Status,
    pub(super) text: String,
    pub(super) label: Option<String>,
    /// Locked, or not completed and without a progress fraction: shown in the quiet tone.
    pub(super) quiet: bool,
}

/// The reported status of an event; banners have none.
pub(super) fn item_status(item: &CalendarItem, ui: Preferences) -> Option<ItemStatus> {
    let word = |status, key, quiet| ItemStatus {
        status,
        text: ui.text(key).to_owned(),
        label: None,
        quiet,
    };
    if item.locked == Some(true) {
        return Some(word(Status::Locked, "calendar.locked", true));
    }
    if item.finished == Some(true) {
        return Some(word(Status::Done, "calendar.finished", false));
    }
    if let (Some(current), Some(total)) = (item.progress, item.progress_total) {
        let (current, total) = (current.to_string(), total.to_string());
        return Some(ItemStatus {
            status: Status::Open,
            text: format!("{current}/{total}"),
            label: Some(ui.format(
                "calendar.progress",
                &[("current", &current), ("total", &total)],
            )),
            quiet: false,
        });
    }
    (item.finished == Some(false)).then(|| word(Status::Open, "calendar.not_finished", true))
}

/// The status of an event as its symbol and word; nothing for banners.
#[component]
pub(super) fn ItemStatusMark(item: CalendarItem, class: &'static str) -> Element {
    let ui = use_preferences();
    rsx! {
        if let Some(status) = item_status(&item, ui) {
            span { class, "data-quiet": status.quiet,
                StatusMark { status: status.status, text: status.text, label: status.label }
            }
        }
    }
}

/// The type symbol of a row: a ticket for wishes and warps, the calendar for events.
pub(super) fn kind_icon(item: &CalendarItem) -> Icon {
    if item.kind == "event" {
        Icon::Calendar
    } else {
        Icon::Ticket
    }
}

/// The shown name: upstream's, else the name of the kind.
pub(super) fn display_name(item: &CalendarItem, ui: Preferences) -> String {
    item.name
        .clone()
        .filter(|name| !name.trim().is_empty())
        .unwrap_or_else(|| kind_name(&item.kind, ui).to_owned())
}

/// The lane an item belongs to: wishes or warps for banners, events otherwise.
pub(super) fn kind_label(item: &CalendarItem, game: &str, ui: Preferences) -> &'static str {
    if item.kind == "event" {
        ui.text("calendar.events")
    } else {
        banners_title(game, ui)
    }
}

pub(super) fn banners_title(game: &str, ui: Preferences) -> &'static str {
    ui.text(if game == "starrail" {
        "calendar.banners_starrail"
    } else {
        "calendar.banners_genshin"
    })
}

/// The name shown when upstream gave none.
fn kind_name(kind: &str, ui: Preferences) -> &'static str {
    ui.text(match kind {
        "character_banner" => "calendar.character_banner",
        "weapon_banner" => "calendar.weapon_banner",
        "chronicled_banner" => "calendar.chronicled_banner",
        "light_cone_banner" => "calendar.light_cone_banner",
        "event" => "calendar.event",
        _ => "dashboard.unknown",
    })
}

/// A failed fetch without a calendar says only that; an outdated calendar says why.
pub(super) fn calendar_notice(calendar: &GameCalendar, ui: Preferences) -> Option<String> {
    let reason = calendar
        .error_code
        .as_deref()
        .map(|code| error_text(Some(code), ui));
    if calendar.items.is_none() {
        return reason
            .map(|reason| ui.format("calendar.fetch_failed_with_reason", &[("reason", reason)]));
    }
    if !calendar.stale {
        return None;
    }
    Some(match reason {
        Some(reason) => ui.format("calendar.stale_with_reason", &[("reason", reason)]),
        None => ui.text("calendar.stale").to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::Language;

    fn item(value: serde_json::Value) -> CalendarItem {
        serde_json::from_value(value).unwrap()
    }

    /// Items whose start/end strings are the seconds elapsed since them.
    fn groups(items: &[CalendarItem]) -> Groups {
        Groups::from_items(items, |at| at.and_then(|at| at.parse().ok()))
    }

    fn english() -> Preferences {
        Preferences {
            language: Language::English,
            ..Preferences::default()
        }
    }

    #[test]
    fn items_are_grouped_by_kind_and_start_and_ended_items_are_dropped() {
        let items = [
            item(
                serde_json::json!({"kind": "character_banner", "starts_at": "100", "ends_at": "-7200"}),
            ),
            item(serde_json::json!({"kind": "event", "ends_at": "-90000"})),
            item(serde_json::json!({"kind": "event", "starts_at": "-3600", "ends_at": "-99999"})),
            item(serde_json::json!({"kind": "weapon_banner", "ends_at": "0"})),
            item(serde_json::json!({"kind": "light_cone_banner"})),
        ];
        let groups = groups(&items);
        assert_eq!(groups.banners.len(), 1);
        assert_eq!(groups.events.len(), 1);
        assert_eq!(groups.upcoming.len(), 1);
        // Without a start or an end the item is not counted as running.
        assert_eq!(groups.undated.len(), 1);
        assert_eq!(groups.undated[0].item.kind, "light_cone_banner");
        assert!(!groups.is_empty());
        assert!(Groups::from_items(&items[3..4], |_| Some(1)).is_empty());
        assert!(!Groups::from_items(&items[4..], |_| None).is_empty());

        let ui = Preferences::default();
        // Each chip: its symbol, the short form on screen and the sentence for readers.
        let chips = |groups: &Groups, ui| {
            summary_chips(groups, ui)
                .into_iter()
                .map(|chip| (chip.name, chip.status, chip.text, chip.label))
                .collect::<Vec<_>>()
        };
        let chip = |name, status, text: &str, label: &str| {
            (name, status, text.to_owned(), label.to_owned())
        };
        assert_eq!(
            chips(&groups, ui),
            [
                chip("running", Status::Running, "2", "開催中2件"),
                chip("upcoming", Status::Upcoming, "1", "開催予定1件"),
                chip("undated", Status::Unknown, "1", "期間不明1件"),
                chip(
                    "next-end",
                    Status::Waiting,
                    "2時間00分",
                    "次の終了まであと2時間00分"
                ),
            ]
        );
        assert_eq!(
            chips(&groups, english())
                .into_iter()
                .map(|(_, _, text, label)| format!("{text}={label}"))
                .collect::<Vec<_>>(),
            [
                "2=2 running",
                "1=1 upcoming",
                "1=1 with unknown period",
                "2h 00m=next ends in 2h 00m"
            ]
        );
        // Running items without a known end leave the nearest end out.
        let open = Groups::from_items(
            &[item(
                serde_json::json!({"kind": "event", "starts_at": "60"}),
            )],
            |at| at.and_then(|at| at.parse().ok()),
        );
        assert_eq!(
            chips(&open, ui),
            [chip("running", Status::Running, "1", "開催中1件")]
        );
        // Nothing running still shows the running chip, as zero.
        assert_eq!(
            chips(&groups_of_unknown(), ui),
            [
                chip(
                    "running",
                    Status::Running,
                    "0",
                    "開催中のイベントはありません"
                ),
                chip("undated", Status::Unknown, "1", "期間不明1件"),
            ]
        );
        let only_upcoming = Groups {
            upcoming: groups.upcoming.clone(),
            ..Groups::default()
        };
        assert_eq!(
            chips(&only_upcoming, ui)
                .into_iter()
                .map(|(name, ..)| name)
                .collect::<Vec<_>>(),
            ["running", "upcoming"]
        );
    }

    fn groups_of_unknown() -> Groups {
        groups(&[item(serde_json::json!({"kind": "event"}))])
    }

    #[test]
    fn countdowns_target_the_start_of_announced_items_and_the_end_of_running_ones() {
        let ui = Preferences::default();
        let groups = groups(&[
            item(serde_json::json!({"kind": "event", "starts_at": "-90000", "ends_at": "-200000"})),
            item(serde_json::json!({"kind": "event", "starts_at": "10", "ends_at": "-5400"})),
        ]);
        assert_eq!(
            countdown(&groups.upcoming[0], ui),
            Some(Countdown {
                status: Status::Upcoming,
                duration: "1日1時間".to_owned(),
                sentence: "開始まで1日1時間".to_owned(),
                at: Some("-90000".to_owned()),
            })
        );
        assert_eq!(
            countdown(&groups.events[0], ui),
            Some(Countdown {
                status: Status::Waiting,
                duration: "1時間30分".to_owned(),
                sentence: "残り1時間30分".to_owned(),
                at: Some("-5400".to_owned()),
            })
        );
        assert_eq!(countdown(&groups_of_unknown().undated[0], ui), None);
        let english = countdown(&groups.events[0], english()).unwrap();
        assert_eq!(
            (english.duration.as_str(), english.sentence.as_str()),
            ("1h 30m", "1h 30m left")
        );
    }

    #[test]
    fn status_shows_what_the_event_reports_and_banners_have_none() {
        let ui = Preferences::default();
        let status = |value| {
            item_status(&item(value), ui)
                .map(|status| (status.status, status.text, status.label, status.quiet))
        };
        assert_eq!(
            status(serde_json::json!({"kind": "character_banner"})),
            None
        );
        assert_eq!(
            status(serde_json::json!({"kind": "event", "locked": true, "finished": true})),
            Some((Status::Locked, "未開放".to_owned(), None, true))
        );
        assert_eq!(
            status(
                serde_json::json!({"kind": "event", "finished": true, "progress": 1, "progress_total": 5})
            ),
            Some((Status::Done, "完了".to_owned(), None, false))
        );
        // A progress shows the fraction; its sentence is the readers' label.
        assert_eq!(
            status(
                serde_json::json!({"kind": "event", "finished": false, "progress": 2, "progress_total": 5})
            ),
            Some((
                Status::Open,
                "2/5".to_owned(),
                Some("進捗 2/5".to_owned()),
                false
            ))
        );
        assert_eq!(
            status(serde_json::json!({"kind": "event", "finished": false})),
            Some((Status::Open, "未完了".to_owned(), None, true))
        );
        // A progress without its total is not shown as a fraction.
        assert_eq!(
            status(serde_json::json!({"kind": "event", "progress": 2})),
            None
        );
    }

    #[test]
    fn rows_carry_a_ticket_for_wishes_and_warps_and_the_calendar_for_events() {
        for (kind, icon) in [
            ("character_banner", Icon::Ticket),
            ("weapon_banner", Icon::Ticket),
            ("chronicled_banner", Icon::Ticket),
            ("light_cone_banner", Icon::Ticket),
            ("future_kind", Icon::Ticket),
            ("event", Icon::Calendar),
        ] {
            assert_eq!(kind_icon(&item(serde_json::json!({"kind": kind}))), icon);
        }
    }

    #[test]
    fn titles_and_fallback_names_follow_the_game_vocabulary() {
        let ui = Preferences::default();
        assert_eq!(banners_title("genshin", ui), "祈願");
        assert_eq!(banners_title("starrail", ui), "跳躍");
        for (kind, name) in [
            ("character_banner", "キャラクターイベント祈願"),
            ("weapon_banner", "武器イベント祈願"),
            ("chronicled_banner", "集録祈願"),
            ("light_cone_banner", "光円錐イベント跳躍"),
            ("event", "イベント"),
            ("future_kind", "不明"),
        ] {
            assert_eq!(kind_name(kind, ui), name);
        }
        assert_eq!(banners_title("starrail", english()), "Warps");
        let banner = item(serde_json::json!({"kind": "light_cone_banner", "name": " "}));
        assert_eq!(display_name(&banner, ui), "光円錐イベント跳躍");
        assert_eq!(kind_label(&banner, "starrail", ui), "跳躍");
        let event = item(serde_json::json!({"kind": "event", "name": "Synthetic"}));
        assert_eq!(display_name(&event, ui), "Synthetic");
        assert_eq!(kind_label(&event, "genshin", english()), "Events");
    }

    #[test]
    fn running_items_end_soonest_first_and_lanes_follow_the_start() {
        let groups = groups(&[
            item(
                serde_json::json!({"kind": "event", "name": "late", "starts_at": "500", "ends_at": "-900"}),
            ),
            item(serde_json::json!({"kind": "event", "name": "open", "starts_at": "50"})),
            item(
                serde_json::json!({"kind": "character_banner", "name": "wish", "starts_at": "100", "ends_at": "-100"}),
            ),
            item(
                serde_json::json!({"kind": "event", "name": "soon", "starts_at": "100", "ends_at": "-200"}),
            ),
            item(
                serde_json::json!({"kind": "weapon_banner", "name": "next", "starts_at": "-50", "ends_at": "-990"}),
            ),
            item(
                serde_json::json!({"kind": "event", "name": "later", "starts_at": "-300", "ends_at": "-999"}),
            ),
            item(
                serde_json::json!({"kind": "event", "name": "next event", "starts_at": "-60", "ends_at": "-999"}),
            ),
        ]);
        let names = |items: Vec<Timed>| {
            items
                .into_iter()
                .map(|timed| timed.item.name.unwrap())
                .collect::<Vec<_>>()
        };
        assert_eq!(
            names(groups.running_by_end()),
            ["wish", "soon", "late", "open"]
        );
        assert_eq!(
            names(groups.upcoming_by_start()),
            ["next", "next event", "later"]
        );
        let [banners, events] = groups.lanes();
        assert_eq!(names(banners), ["wish", "next"]);
        assert_eq!(
            names(events),
            ["late", "soon", "open", "next event", "later"]
        );
        assert_eq!(groups.banners[0].start(1_000), Some(900));
        assert_eq!(groups.banners[0].end(1_000), Some(1_100));
    }

    #[test]
    fn undated_items_stay_out_of_the_lanes_and_list_banners_first() {
        let groups = groups(&[
            item(serde_json::json!({"kind": "event", "name": "undated event"})),
            item(serde_json::json!({"kind": "event", "name": "dated", "ends_at": "-60"})),
            item(serde_json::json!({"kind": "character_banner", "name": "undated wish"})),
            item(serde_json::json!({"kind": "event", "name": "announced", "starts_at": "-60"})),
        ]);
        let names = |items: &[Timed]| {
            items
                .iter()
                .map(|timed| timed.item.name.clone().unwrap())
                .collect::<Vec<_>>()
        };
        assert_eq!(names(&groups.undated), ["undated wish", "undated event"]);
        let [banners, events] = groups.lanes();
        assert!(banners.is_empty());
        assert_eq!(names(&events), ["dated", "announced"]);
        assert_eq!(names(&groups.running_by_end()), ["dated"]);
        assert!(groups.undated.iter().all(Timed::is_undated));
        assert!(!groups.events[0].is_undated() && !groups.upcoming[0].is_undated());
    }

    #[test]
    fn featured_items_split_into_five_stars_and_quiet_names() {
        let banner = item(serde_json::json!({"kind": "character_banner", "featured": [
            {"name": "Five", "rarity": 5, "icon": "https://example.invalid/five.png"},
            {"name": "Four", "rarity": 4},
            {"name": "Unrated"},
            {"rarity": 4},
            {"name": "Four B", "rarity": 4}
        ]}));
        let (main, minor) = featured_parts(&banner);
        assert_eq!(
            main.iter()
                .map(|f| f.name.clone().unwrap())
                .collect::<Vec<_>>(),
            ["Five", "Unrated"]
        );
        assert_eq!(minor, "Four · Four B");
    }

    #[test]
    fn notices_distinguish_a_failed_first_fetch_from_an_outdated_calendar() {
        let ui = Preferences::default();
        let mut calendar: GameCalendar = serde_json::from_value(serde_json::json!({
            "game_kind": "genshin", "game_uid": "600000000", "region": "os_asia",
            "visible": true, "detected": true, "items": null, "fetched_at": null,
            "error_code": "network", "error_at": null, "stale": true
        }))
        .unwrap();
        assert_eq!(
            calendar_notice(&calendar, ui).unwrap(),
            "カレンダーを取得できませんでした（HoYoLABに接続できませんでした）"
        );
        calendar.items = Some(Vec::new());
        assert_eq!(
            calendar_notice(&calendar, ui).unwrap(),
            "表示中のカレンダーは最新ではありません（HoYoLABに接続できませんでした）"
        );
        calendar.error_code = None;
        assert_eq!(
            calendar_notice(&calendar, ui).unwrap(),
            "表示中のカレンダーは最新ではありません"
        );
        calendar.stale = false;
        assert_eq!(calendar_notice(&calendar, ui), None);
    }
}
