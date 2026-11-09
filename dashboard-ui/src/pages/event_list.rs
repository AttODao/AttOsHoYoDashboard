//! The events timeline read downwards: the phone layout (<=600px and the app), and the text
//! structure readers get on every width (visually hidden beside the wide timeline). "Now"
//! heads the running items, which end soonest first; announced items follow under the date
//! they start on, and items without any known period close the list in their own group.
use super::calendar::{
    FeaturedLine, Groups, Timed, countdown, display_name, kind_label, quiet_status, status_text,
};
use super::event_timeline::{LocalTime, day_text, iso};
use crate::app::display::compact_datetime;
use crate::app::preferences::use_preferences;
use dioxus::prelude::*;

#[component]
pub(super) fn EventList(groups: Groups, game: &'static str, local: LocalTime) -> Element {
    let ui = use_preferences();
    let running = groups.running_by_end();
    let upcoming = start_days(local, groups.upcoming_by_start());
    let undated = groups.undated.clone();
    let now = compact_datetime(Some(&iso(local.now))).unwrap_or_else(|| local.date_time(local.now));
    rsx! {
        div { class: "event-list",
            section { class: "event-phase", "data-phase": "running", "aria-labelledby": "events-running-{game}",
                h3 { id: "events-running-{game}", {ui.text("calendar.running")} }
                p { class: "event-node event-now",
                    span { class: "event-node-label", {ui.text("calendar.now")} }
                    " "
                    time { datetime: iso(local.now), "{now}" }
                }
                if running.is_empty() {
                    p { class: "event-none", {ui.text("calendar.summary_none")} }
                } else {
                    ol { class: "event-cards",
                        for (index, timed) in running.into_iter().enumerate() {
                            EventCard { key: "running-{index}", timed, game, local }
                        }
                    }
                }
            }
            if !upcoming.is_empty() {
                section { class: "event-phase", "data-phase": "upcoming", "aria-labelledby": "events-upcoming-{game}",
                    h3 { id: "events-upcoming-{game}", {ui.text("calendar.upcoming")} }
                    for (day, items) in upcoming {
                        div { key: "{day}", class: "event-day",
                            h4 { class: "event-node",
                                time { datetime: iso(day * 86_400).get(..10).unwrap_or_default().to_owned(), {ui.format("calendar.starts_on", &[("date", &day_text(day))])} }
                            }
                            ol { class: "event-cards",
                                for (index, timed) in items.into_iter().enumerate() {
                                    EventCard { key: "{day}-{index}", timed, game, local }
                                }
                            }
                        }
                    }
                }
            }
            if !undated.is_empty() {
                section { class: "event-phase", "data-phase": "undated", "aria-labelledby": "events-undated-{game}",
                    h3 { id: "events-undated-{game}", {ui.text("calendar.undated")} }
                    ol { class: "event-cards",
                        for (index, timed) in undated.into_iter().enumerate() {
                            EventCard { key: "undated-{index}", timed, game, local }
                        }
                    }
                }
            }
        }
    }
}

/// Announced items grouped by the local day they start on, in order.
fn start_days(local: LocalTime, upcoming: Vec<Timed>) -> Vec<(i64, Vec<Timed>)> {
    let mut days: Vec<(i64, Vec<Timed>)> = Vec::new();
    for timed in upcoming {
        let day = timed
            .start(local.now)
            .map_or(local.day(local.now), |start| local.day(start));
        match days.last_mut() {
            Some((current, items)) if *current == day => items.push(timed),
            _ => days.push((day, vec![timed])),
        }
    }
    days
}

#[component]
fn EventCard(timed: Timed, game: &'static str, local: LocalTime) -> Element {
    let ui = use_preferences();
    let name = display_name(&timed.item, ui);
    let start = timed.start(local.now);
    let end = timed.end(local.now);
    let left = countdown(&timed, ui);
    let status = status_text(&timed.item, ui);
    let quiet = quiet_status(&timed.item);
    rsx! {
        li { class: "event-card", "data-kind": "{timed.item.kind}", "data-banner": !timed.is_event(),
            div { class: "calendar-head",
                span { class: "event-kind", {kind_label(&timed.item, game, ui)} }
                if let Some((left, at)) = left {
                    time { class: "calendar-left", datetime: at.clone(), title: compact_datetime(at.as_deref()), "{left}" }
                }
            }
            strong { class: "calendar-name", "{name}" }
            FeaturedLine { item: timed.item.clone() }
            // An undated item sits under the "period unknown" heading instead of a bare dash.
            if start.is_some() || end.is_some() {
                p { class: "event-period",
                    if let Some(start) = start {
                        time { datetime: iso(start), {local.date_time(start)} }
                    }
                    " – "
                    if let Some(end) = end {
                        time { datetime: iso(end), {local.date_time(end)} }
                    }
                }
            }
            if let Some(status) = status {
                p { class: if quiet { "calendar-status quiet" } else { "calendar-status" }, "{status}" }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::CalendarItem;

    #[test]
    fn announced_items_are_grouped_by_their_local_start_day() {
        // 2026-11-09 21:00 JST.
        let time = LocalTime {
            now: 1_794_225_600,
            offset: 9 * 3600,
        };
        let timed = |name: &str, since_start: Option<i64>| Timed {
            item: CalendarItem {
                kind: "event".to_owned(),
                name: Some(name.to_owned()),
                ..CalendarItem::default()
            },
            since_start,
            since_end: None,
        };
        // Two hours and five hours ahead: 23:00 today and 02:00 tomorrow; then 2 days ahead.
        let days = start_days(
            time,
            vec![
                timed("a", Some(-2 * 3600)),
                timed("b", Some(-5 * 3600)),
                timed("c", Some(-5 * 3600 - 60)),
                timed("d", Some(-2 * 86_400)),
            ],
        );
        let summary: Vec<(String, Vec<String>)> = days
            .into_iter()
            .map(|(day, items)| {
                (
                    day_text(day),
                    items.into_iter().map(|t| t.item.name.unwrap()).collect(),
                )
            })
            .collect();
        assert_eq!(
            summary,
            [
                ("11/09".to_owned(), vec!["a".to_owned()]),
                ("11/10".to_owned(), vec!["b".to_owned(), "c".to_owned()]),
                ("11/11".to_owned(), vec!["d".to_owned()]),
            ]
        );
    }
}
