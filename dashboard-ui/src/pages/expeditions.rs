//! Genshin expeditions: the flag with "finished/total", then one avatar per expedition
//! with only the time left below it (the done symbol once finished).
use super::dashboard::use_clock;
use super::icons::{Glyph, Icon, Status, StatusMark};
use crate::app::display::{hours_minutes, seconds_since};
use crate::app::preferences::{Preferences, use_preferences};
use crate::models::Expedition;
use dioxus::prelude::*;

/// Expeditions with their time left counted down from the fetch time.
#[component]
pub(super) fn ExpeditionList(
    expeditions: Option<Vec<Expedition>>,
    maximum: Option<i64>,
    fetched_at: Option<String>,
) -> Element {
    let ui = use_preferences();
    let clock = use_clock();
    let _ = clock();
    let elapsed = seconds_since(fetched_at.as_deref());
    let left = |row: &Expedition| expedition_left(expedition_seconds(row), elapsed);
    let summary = expeditions
        .as_ref()
        .filter(|rows| !rows.is_empty())
        .map(|rows| {
            let finished = rows
                .iter()
                .filter(|row| row_finished(row.status.as_deref(), left(row)))
                .count();
            (finished, total_slots(rows.len(), maximum))
        });
    rsx! {
        section { class: "expeditions", "aria-labelledby": "expeditions-title",
            h3 { class: "group-heading", id: "expeditions-title",
                Glyph { icon: Icon::Flag }
                span { class: "visually-hidden", {ui.text("dashboard.expeditions")} }
                if let Some((finished, total)) = summary {
                    span { class: "visually-hidden",
                        " " {ui.format("dashboard.expeditions_finished", &[("finished", &finished.to_string()), ("total", &total.to_string())])}
                    }
                    span { class: "group-count", "aria-hidden": "true", strong { "{finished}" } span { "/{total}" } }
                }
            }
            if let Some(rows) = expeditions {
                if rows.is_empty() { p { class: "quiet-line", {ui.text("dashboard.no_expeditions")} } }
                ul { class: "expedition-list",
                    for (index, expedition) in rows.into_iter().enumerate() {
                        // A finished expedition (also one whose time ran out since the fetch)
                        // has nothing left; the done symbol alone says so.
                        li { class: "expedition", "data-done": row_finished(expedition.status.as_deref(), left(&expedition)),
                            ExpeditionIcon {
                                key: "{index}-{expedition.avatar_side_icon:?}",
                                source: expedition.avatar_side_icon.clone(),
                                label: expedition_label(expedition.name.as_deref(), index, ui),
                            }
                            if row_finished(expedition.status.as_deref(), left(&expedition)) {
                                StatusMark { status: Status::Done, text: ui.text("dashboard.finished") }
                            } else {
                                span { class: "expedition-left",
                                    span { class: "visually-hidden", {ui.text("dashboard.remaining")} " " }
                                    {remaining_text(&expedition, left(&expedition), ui)}
                                }
                            }
                        }
                    }
                }
            } else {
                p { class: "quiet-line", StatusMark { status: Status::Unknown, text: ui.text("common.not_fetched") } }
            }
        }
    }
}

/// The official avatar from the URL HoYoLAB returned, or the person icon when there is
/// none or it fails to load. The name stays as the alternative text either way.
#[component]
fn ExpeditionIcon(source: Option<String>, label: String) -> Element {
    let mut failed = use_signal(|| false);
    rsx! {
        if let Some(source) = source.filter(|source| !source.is_empty() && !failed()) {
            img { class: "expedition-icon", src: source, alt: label.clone(), title: label,
                width: "48", height: "48", loading: "lazy", decoding: "async", referrerpolicy: "no-referrer",
                onerror: move |_| failed.set(true),
            }
        } else {
            span { class: "expedition-icon", role: "img", "aria-label": label.clone(), title: label,
                Glyph { icon: Icon::User }
            }
        }
    }
}

fn expedition_finished(status: Option<&str>) -> bool {
    status == Some("Finished")
}

fn expedition_seconds(row: &Expedition) -> Option<i64> {
    row.remained_time
        .as_ref()
        .or(row.remaining_time.as_ref())
        .and_then(|duration| duration.seconds())
}

/// Seconds left now: the fetched remaining time minus the time since the fetch.
fn expedition_left(remaining: Option<i64>, elapsed: Option<i64>) -> Option<i64> {
    remaining.map(|left| left - elapsed.unwrap_or(0).max(0))
}

/// Upstream says finished, or the remaining time has run out since the fetch.
fn row_finished(status: Option<&str>, left: Option<i64>) -> bool {
    expedition_finished(status) || left.is_some_and(|seconds| seconds <= 0)
}

/// The slot count beside the finished count: the reported maximum, unless more rows
/// than that came back.
fn total_slots(rows: usize, maximum: Option<i64>) -> usize {
    maximum
        .and_then(|max| usize::try_from(max).ok())
        .filter(|max| *max >= rows)
        .unwrap_or(rows)
}

fn remaining_text(row: &Expedition, left: Option<i64>, ui: Preferences) -> String {
    match (
        left,
        row.remained_time.as_ref().or(row.remaining_time.as_ref()),
    ) {
        (Some(seconds), _) => hours_minutes(seconds, ui),
        (None, Some(duration)) => duration.to_string(),
        (None, None) => ui.text("common.not_fetched").to_owned(),
    }
}

fn expedition_label(name: Option<&str>, index: usize, ui: Preferences) -> String {
    name.filter(|name| !name.trim().is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| {
            ui.format(
                "dashboard.expedition_number",
                &[("number", &(index + 1).to_string())],
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unnamed_expeditions_use_numbered_labels_without_inventing_locations() {
        let ui = Preferences::default();
        assert_eq!(expedition_label(Some("Crystal"), 0, ui), "Crystal");
        assert_eq!(expedition_label(None, 0, ui), "派遣 1");
        assert_eq!(expedition_label(Some("  "), 1, ui), "派遣 2");
    }

    #[test]
    fn the_slot_count_never_falls_below_the_rows_shown() {
        assert_eq!(total_slots(2, Some(5)), 5);
        assert_eq!(total_slots(5, Some(5)), 5);
        assert_eq!(total_slots(6, Some(5)), 6);
        assert_eq!(total_slots(3, None), 3);
        assert_eq!(total_slots(3, Some(-1)), 3);
    }

    #[test]
    fn expedition_time_counts_down_from_the_fetch() {
        assert!(expedition_finished(Some("Finished")));
        assert!(!expedition_finished(Some("Ongoing")));
        assert!(!expedition_finished(None));
        assert_eq!(expedition_left(None, Some(60)), None);
        assert_eq!(expedition_left(Some(3600), None), Some(3600));
        assert_eq!(expedition_left(Some(3600), Some(600)), Some(3000));
        // A fetch time ahead of the device clock does not add time.
        assert_eq!(expedition_left(Some(3600), Some(-30)), Some(3600));
        assert_eq!(expedition_left(Some(60), Some(90)), Some(-30));
        assert!(row_finished(Some("Ongoing"), Some(0)));
        assert!(row_finished(Some("Ongoing"), Some(-30)));
        assert!(row_finished(Some("Finished"), None));
        assert!(!row_finished(Some("Ongoing"), Some(1)));
        assert!(!row_finished(Some("Ongoing"), None));
        let row: Expedition = serde_json::from_value(
            serde_json::json!({"status": "Ongoing", "remained_time": "soon"}),
        )
        .unwrap();
        let ui = Preferences::default();
        assert_eq!(expedition_seconds(&row), None);
        assert_eq!(remaining_text(&row, None, ui), "soon");
        assert_eq!(remaining_text(&row, Some(3000), ui), "0時間50分");
    }
}
