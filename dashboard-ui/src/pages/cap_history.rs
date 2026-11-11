use super::common::{WarningLine, refresh_problem};
use super::icons::{Glyph, Icon, Status};
use crate::app::display::{days_hours, hours_minutes};
use crate::app::preferences::{Preferences, use_preferences};
use crate::models::{HistorySegment, ResourceHistory};
use dioxus::prelude::*;

/// The timeline's full width in percent: segments are placed with percentage `x` and
/// `width`, so the hatch pattern keeps its pixel size at every card width.
const TIMELINE_WIDTH: f64 = 100.0;

/// Time at cap in the last 7 days below the primary resource: the history icon with a
/// short title and the total, a 7-day timeline told apart by shape (at cap: a full-height
/// block; recovering: a thin line; unknown: hatched), the unknown time when there is any,
/// and the same breakdown as a sentence for screen readers. There is no axis or legend. The timeline is SVG geometry (percentage `x`/`width`
/// attributes), so it needs no inline styles under the CSP. Hidden until the first history arrives (and on servers
/// without the endpoint).
#[component]
pub(super) fn CapHistory(
    game: &'static str,
    resource: &'static str,
    history: Option<ResourceHistory>,
    error: Option<&'static str>,
) -> Element {
    let ui = use_preferences();
    if history.is_none() && error.is_none() {
        return rsx! {};
    }
    // Drawn once any part of the week is known (an anchor snapshot just before the
    // window counts, though it is outside `sample_count`).
    let recorded = history.as_ref().filter(|history| {
        history
            .segments
            .iter()
            .any(|segment| segment.state != "unknown" && segment.seconds > 0)
    });
    let rects = recorded
        .map(|history| timeline(&history.segments))
        .unwrap_or_default();
    let totals = recorded.map(Totals::of);
    let pattern = format!("cap-unknown-{game}");
    rsx! {
        div { class: "cap-history",
            p { class: "cap-history-head",
                Glyph { icon: Icon::History }
                span { class: "cap-history-title", {ui.text("history.title")} }
                if let Some(totals) = totals {
                    strong { class: "cap-history-value", {elapsed_text(totals.at_cap, ui)} }
                }
            }
            if let Some(key) = error {
                WarningLine {
                    status: if history.is_some() { Status::Warn } else { Status::Failed },
                    text: ui.text(refresh_problem(key, history.is_some(), "history.fetch_failed")),
                }
            }
            if let Some(totals) = totals {
                svg {
                    class: "cap-timeline", "aria-hidden": "true", "focusable": "false",
                    defs {
                        pattern { id: "{pattern}", width: "6", height: "12", pattern_units: "userSpaceOnUse", pattern_transform: "rotate(45)",
                            rect { class: "cap-hatch", width: "2", height: "12" }
                        }
                    }
                    for (index, rect) in rects.into_iter().enumerate() {
                        rect {
                            key: "{index}", class: "cap-segment", "data-state": rect.state,
                            x: "{rect.x:.2}%", y: rect.y(), width: "{rect.width:.2}%", height: rect.height(),
                        }
                        if rect.state == "unknown" {
                            rect { key: "hatch-{index}", fill: "url(#{pattern})", x: "{rect.x:.2}%", y: "0", width: "{rect.width:.2}%", height: "100%" }
                        }
                    }
                }
                if totals.unknown > 0 {
                    p { class: "cap-unknown", "aria-hidden": "true",
                        span { class: "cap-swatch" }
                        {ui.format("history.unknown", &[("duration", &elapsed_text(totals.unknown, ui))])}
                    }
                }
                p { class: "visually-hidden", {description(resource, totals, ui)} }
            } else if history.is_some() {
                p { class: "cap-history-empty", {ui.text("history.no_data")} }
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Totals {
    at_cap: i64,
    below: i64,
    unknown: i64,
}

impl Totals {
    fn of(history: &ResourceHistory) -> Self {
        let sum = |state: &str| {
            history
                .segments
                .iter()
                .filter(|segment| segment.state == state)
                .map(|segment| segment.seconds.max(0))
                .sum::<i64>()
        };
        Self {
            at_cap: history.at_cap_seconds.max(0),
            below: sum("below_cap"),
            unknown: history.unknown_seconds.max(0),
        }
    }
}

fn description(resource: &str, totals: Totals, ui: Preferences) -> String {
    ui.format(
        "history.description",
        &[
            ("resource", resource),
            ("at_cap", &elapsed_text(totals.at_cap, ui)),
            ("below", &elapsed_text(totals.below, ui)),
            ("unknown", &elapsed_text(totals.unknown, ui)),
        ],
    )
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Rect {
    state: &'static str,
    x: f64,
    width: f64,
}

impl Rect {
    /// Recovering is a thin line through the middle; the other states fill the height,
    /// so time at cap stands out by shape as well as by colour.
    fn y(&self) -> &'static str {
        if self.state == "below_cap" {
            "36%"
        } else {
            "0"
        }
    }

    fn height(&self) -> &'static str {
        if self.state == "below_cap" {
            "28%"
        } else {
            "100%"
        }
    }
}

/// Segments laid end to end across the timeline in proportion to their length.
/// Unrecognized states are drawn as unknown.
fn timeline(segments: &[HistorySegment]) -> Vec<Rect> {
    let total: i64 = segments.iter().map(|segment| segment.seconds.max(0)).sum();
    if total == 0 {
        return Vec::new();
    }
    let mut x = 0.0;
    segments
        .iter()
        .filter(|segment| segment.seconds > 0)
        .map(|segment| {
            let width = TIMELINE_WIDTH * segment.seconds as f64 / total as f64;
            let rect = Rect {
                state: match segment.state.as_str() {
                    "at_cap" => "at_cap",
                    "below_cap" => "below_cap",
                    _ => "unknown",
                },
                x,
                width,
            };
            x += width;
            rect
        })
        .collect()
}

/// Elapsed time rounded down: minutes under an hour, hours and minutes under a day,
/// then days and hours.
fn elapsed_text(seconds: i64, ui: Preferences) -> String {
    let minutes = seconds.max(0) / 60;
    if minutes < 60 {
        ui.format("history.minutes", &[("minutes", &minutes.to_string())])
    } else if minutes < 24 * 60 {
        // Whole minutes, so the rounding-up remaining-time formatter keeps them as is.
        hours_minutes(minutes * 60, ui)
    } else {
        // Whole hours likewise.
        days_hours(minutes / 60 * 3600, ui)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::Language;

    fn english() -> Preferences {
        Preferences {
            language: Language::English,
            ..Preferences::default()
        }
    }

    fn segment(state: &str, seconds: i64) -> HistorySegment {
        HistorySegment {
            state: state.to_owned(),
            start: String::new(),
            end: String::new(),
            seconds,
        }
    }

    #[test]
    fn elapsed_time_rounds_down_and_grows_its_unit() {
        let ui = Preferences::default();
        assert_eq!(elapsed_text(0, ui), "0分");
        assert_eq!(elapsed_text(59, ui), "0分");
        assert_eq!(elapsed_text(59 * 60 + 59, ui), "59分");
        assert_eq!(elapsed_text(3 * 3600 + 12 * 60 + 59, ui), "3時間12分");
        assert_eq!(elapsed_text(26 * 3600 + 59 * 60, ui), "1日2時間");
        assert_eq!(elapsed_text(-5, ui), "0分");
        assert_eq!(elapsed_text(45 * 60, english()), "45m");
        assert_eq!(elapsed_text(3 * 3600 + 5 * 60, english()), "3h 05m");
        assert_eq!(elapsed_text(7 * 86_400, english()), "7d 0h");
    }

    #[test]
    fn the_timeline_is_proportional_and_ends_at_its_width() {
        let rects = timeline(&[
            segment("unknown", 300),
            segment("below_cap", 0),
            segment("below_cap", 600),
            segment("at_cap", 100),
            segment("surprise", 0),
            segment("surprise", 1000),
        ]);
        assert_eq!(
            rects.iter().map(|rect| rect.state).collect::<Vec<_>>(),
            ["unknown", "below_cap", "at_cap", "unknown"]
        );
        assert_eq!(rects[0].x, 0.0);
        assert!((rects[1].x - 15.0).abs() < 1e-9);
        assert!((rects[2].width - 5.0).abs() < 1e-9);
        let last = rects.last().unwrap();
        assert!((last.x + last.width - TIMELINE_WIDTH).abs() < 1e-9);
        // Shape, not only colour: recovering is thin, at cap and unknown are full height.
        assert_eq!((rects[1].y(), rects[1].height()), ("36%", "28%"));
        assert_eq!((rects[2].y(), rects[2].height()), ("0", "100%"));
        assert_eq!((rects[0].y(), rects[0].height()), ("0", "100%"));
        assert!(timeline(&[]).is_empty());
        assert!(timeline(&[segment("at_cap", 0)]).is_empty());
    }

    #[test]
    fn the_text_alternative_states_every_part_of_the_week() {
        let history: ResourceHistory = serde_json::from_value(serde_json::json!({
            "game_kind": "genshin", "game_uid": "800000001", "region": "os_asia",
            "window_start": "2026-11-02T00:00:00Z", "window_end": "2026-11-09T00:00:00Z",
            "sample_count": 3, "at_cap_seconds": 3600, "unknown_seconds": 7200,
            "segments": [
                {"state": "unknown", "start": "", "end": "", "seconds": 7200},
                {"state": "below_cap", "start": "", "end": "", "seconds": 594000},
                {"state": "at_cap", "start": "", "end": "", "seconds": 3600}
            ]
        }))
        .unwrap();
        let totals = Totals::of(&history);
        assert_eq!(
            totals,
            Totals {
                at_cap: 3600,
                below: 594_000,
                unknown: 7200
            }
        );
        assert_eq!(
            description("天然樹脂", totals, Preferences::default()),
            "天然樹脂は直近7日間のうち1時間00分上限に達していました（回復中6日21時間、記録がなく不明2時間00分）。"
        );
        assert_eq!(
            description("Original Resin", totals, english()),
            "Original Resin was at its cap for 1h 00m in the last 7 days, recovering for 6d 21h, and unknown (no records) for 2h 00m."
        );
    }
}
