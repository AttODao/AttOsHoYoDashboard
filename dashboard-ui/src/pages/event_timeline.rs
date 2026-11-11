//! The events timeline for wide screens: a date axis across the top (months, then days with
//! today marked) and one row per banner or event, with a bar from its start to its end and
//! type icon, name, rate-up images, period, countdown and status on the bar (or beside a
//! short bar). Items without any
//! known period follow in their own group as labels without a bar, so they never look
//! like something running across the whole axis.
//!
//! The grid is a fixed-layout table with one equal column per day and bars placed with
//! `colspan`, so positions need no inline styles under the CSP (`style-src 'self'`). Bars are
//! whole local days; the label keeps the exact moments. The table is `aria-hidden`: readers
//! get the same items as the list of `event_list.rs`, which is visually hidden here.
use super::calendar::{FeaturedLine, Groups, ItemStatusMark, Timed, countdown, display_name};
use super::calendar::{banners_title, kind_icon};
use super::icons::{Glyph, Icon, Status, StatusMark};
use crate::app::display::compact_datetime;
use crate::app::preferences::{Preferences, use_preferences};
use dioxus::prelude::*;

const DAY: i64 = 86_400;
/// Days shown before today at most: older starts are clipped at the left edge.
const PAST_DAYS: i64 = 7;
/// Days shown after today at most: later ends are clipped at the right edge.
const FUTURE_DAYS: i64 = 41;
/// The axis spans at least two weeks, so a short calendar does not stretch one day wide.
const MIN_DAYS: i64 = 14;
/// A bar this long (in days, or this share of the axis) carries its label inside.
const LABEL_INSIDE_DAYS: i64 = 9;
const LABEL_INSIDE_SHARE: f64 = 0.3;

/// The device clock: Unix seconds and the local UTC offset in seconds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct LocalTime {
    pub(super) now: i64,
    pub(super) offset: i64,
}

impl LocalTime {
    /// The device clock now (UTC in native tests).
    pub(super) fn current() -> Self {
        Self {
            now: (crate::platform::now_ms() / 1000.0).floor() as i64,
            offset: i64::from(crate::app::display::browser_offset_minutes().unwrap_or(0)) * 60,
        }
    }

    /// Days since 1970-01-01 in local time.
    pub(super) fn day(self, at: i64) -> i64 {
        (at + self.offset).div_euclid(DAY)
    }

    /// The local day an item that ends at `end` last runs on (an end at midnight belongs to
    /// the day before).
    fn last_day(self, end: i64) -> i64 {
        self.day(end - 1)
    }

    /// Local "MM/DD".
    pub(super) fn date(self, at: i64) -> String {
        day_text(self.day(at))
    }

    /// Local "MM/DD HH:MM".
    pub(super) fn date_time(self, at: i64) -> String {
        let seconds = (at + self.offset).rem_euclid(DAY);
        format!(
            "{} {:02}:{:02}",
            self.date(at),
            seconds / 3600,
            seconds % 3600 / 60
        )
    }
}

/// "MM/DD" of a local day number.
pub(super) fn day_text(day: i64) -> String {
    let (_, month, day) = civil(day);
    format!("{month:02}/{day:02}")
}

/// (year, month, day) of a day number counted from 1970-01-01 (proleptic Gregorian).
pub(super) fn civil(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

/// An RFC 3339 UTC timestamp of Unix seconds, for `datetime` attributes.
pub(super) fn iso(at: i64) -> String {
    let (year, month, day) = civil(at.div_euclid(DAY));
    let seconds = at.rem_euclid(DAY);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        seconds / 3600,
        seconds % 3600 / 60,
        seconds % 60
    )
}

/// The shown days: `first` (a local day number), `days` columns, and today's column.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Axis {
    first: i64,
    days: i64,
    today: i64,
}

impl Axis {
    /// From the earliest start to the latest end of the items, at most a week back and six
    /// weeks ahead, and at least two weeks; today is always on the axis.
    fn of(local: LocalTime, periods: &[(Option<i64>, Option<i64>)]) -> Self {
        let today = local.day(local.now);
        let first = periods
            .iter()
            .filter_map(|(start, _)| start.map(|start| local.day(start)))
            .min()
            .unwrap_or(today)
            .clamp(today - PAST_DAYS, today);
        let last = periods
            .iter()
            // An end at midnight belongs to the day before; a start does not.
            .filter_map(|(start, end)| {
                end.map(|end| local.last_day(end))
                    .or(start.map(|start| local.day(start)))
            })
            .max()
            .unwrap_or(today)
            .clamp(today, today + FUTURE_DAYS)
            .max(first + MIN_DAYS - 1);
        Self {
            first,
            days: last - first + 1,
            today: today - first,
        }
    }

    fn last(self) -> i64 {
        self.first + self.days - 1
    }

    /// The current moment in columns from the axis start (today's column plus the share
    /// of the local day gone).
    fn now(self, local: LocalTime) -> f64 {
        let into_day = (local.now + local.offset).rem_euclid(DAY);
        self.today as f64 + into_day as f64 / DAY as f64
    }

    /// Month labels with the number of days each covers on the axis.
    fn months(self) -> Vec<(u32, i64)> {
        let mut months: Vec<(u32, i64)> = Vec::new();
        for day in self.first..=self.last() {
            let (_, month, _) = civil(day);
            match months.last_mut() {
                Some((current, count)) if *current == month => *count += 1,
                _ => months.push((month, 1)),
            }
        }
        months
    }

    /// Day columns: the day of the month, and whether it is labelled (every seventh day
    /// counted from today, so today's label always shows).
    fn ticks(self) -> Vec<(u32, bool)> {
        (0..self.days)
            .map(|index| {
                let (_, _, day) = civil(self.first + index);
                (day, (index - self.today).rem_euclid(7) == 0)
            })
            .collect()
    }
}

/// Where an item's bar sits: empty columns before it, its own columns and empty columns
/// after it, and whether it continues past either edge (or has no known start or end).
#[derive(Clone, Copy, Debug, PartialEq)]
struct Span {
    before: i64,
    length: i64,
    after: i64,
    clip_start: bool,
    clip_end: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum LabelSide {
    Inside,
    Before,
    After,
}

impl Span {
    fn of(axis: Axis, local: LocalTime, start: Option<i64>, end: Option<i64>) -> Self {
        let first = start.map(|start| local.day(start));
        let last = end.map(|end| local.last_day(end));
        let from = first.unwrap_or(axis.first).clamp(axis.first, axis.last());
        let to = last.unwrap_or(axis.last()).clamp(from, axis.last());
        Self {
            before: from - axis.first,
            length: to - from + 1,
            after: axis.last() - to,
            clip_start: first.is_none_or(|first| first < axis.first),
            clip_end: last.is_none_or(|last| last > axis.last()),
        }
    }

    /// Long bars carry their label; a short one has it beside it, on the roomier side.
    fn label_side(self) -> LabelSide {
        let days = self.before + self.length + self.after;
        if self.length >= LABEL_INSIDE_DAYS
            || self.length as f64 >= days as f64 * LABEL_INSIDE_SHARE
            || (self.before == 0 && self.after == 0)
        {
            LabelSide::Inside
        } else if self.after >= self.before {
            LabelSide::After
        } else {
            LabelSide::Before
        }
    }

    fn clip(self) -> Option<&'static str> {
        match (self.clip_start, self.clip_end) {
            (true, true) => Some("both"),
            (true, false) => Some("start"),
            (false, true) => Some("end"),
            (false, false) => None,
        }
    }
}

/// "MM/DD – MM/DD" for a bar; an unknown side stays empty.
fn period_text(local: LocalTime, start: Option<i64>, end: Option<i64>) -> String {
    let start = start.map(|start| local.date(start)).unwrap_or_default();
    let end = end
        .map(|end| day_text(local.last_day(end)))
        .unwrap_or_default();
    format!("{start} – {end}").trim().to_owned()
}

fn month_name(month: u32, ui: Preferences) -> &'static str {
    ui.text(match month {
        1 => "calendar.month_1",
        2 => "calendar.month_2",
        3 => "calendar.month_3",
        4 => "calendar.month_4",
        5 => "calendar.month_5",
        6 => "calendar.month_6",
        7 => "calendar.month_7",
        8 => "calendar.month_8",
        9 => "calendar.month_9",
        10 => "calendar.month_10",
        11 => "calendar.month_11",
        _ => "calendar.month_12",
    })
}

/// The wide timeline of one game.
#[component]
pub(super) fn WideTimeline(groups: Groups, game: &'static str, local: LocalTime) -> Element {
    let ui = use_preferences();
    let lanes = groups.lanes();
    let undated = groups.undated.clone();
    let periods: Vec<_> = lanes
        .iter()
        .flatten()
        .map(|timed| (timed.start(local.now), timed.end(local.now)))
        .collect();
    let axis = Axis::of(local, &periods);
    let now = axis.now(local);
    let titles = [
        (Icon::Ticket, banners_title(game, ui)),
        (Icon::Calendar, ui.text("calendar.events")),
    ];
    rsx! {
        div { class: "event-timeline-wrap",
            table { class: "event-timeline", "aria-hidden": "true", "data-days": axis.days,
                colgroup {
                    for index in 0..axis.days {
                        col { key: "{index}" }
                    }
                }
                thead {
                    tr { class: "tl-months",
                        for (index, (month, span)) in axis.months().into_iter().enumerate() {
                            th { key: "{index}", colspan: span, {month_name(month, ui)} }
                        }
                    }
                    tr { class: "tl-days",
                        for (index, (day, labelled)) in axis.ticks().into_iter().enumerate() {
                            th { key: "{index}", "data-today": index as i64 == axis.today, "data-tick": labelled,
                                if index as i64 == axis.today {
                                    span { class: "tl-today-label", {ui.text("calendar.today")} }
                                    span { class: "tl-day", "{day}" }
                                } else if labelled {
                                    span { class: "tl-day", "{day}" }
                                }
                            }
                        }
                    }
                }
                tbody {
                    for (lane, items) in lanes.into_iter().enumerate() {
                        if !items.is_empty() {
                            tr { key: "lane-{lane}", class: "tl-lane",
                                th { colspan: axis.days,
                                    NowLine { now, start: 0, span: axis.days }
                                    span { class: "tl-lane-title",
                                        Glyph { icon: titles[lane].0, class: "icon-sm" }
                                        {titles[lane].1}
                                    }
                                }
                            }
                            for (index, timed) in items.into_iter().enumerate() {
                                TimelineRow { key: "{lane}-{index}", timed, axis, local, now }
                            }
                        }
                    }
                    // Not on the time axis, so the now line does not cross this group.
                    if !undated.is_empty() {
                        tr { class: "tl-lane", "data-undated": true,
                            th { colspan: axis.days,
                                span { class: "tl-lane-title",
                                    Glyph { icon: Status::Unknown.icon(), class: "icon-sm" }
                                    {ui.text("calendar.undated")}
                                }
                            }
                        }
                        for (index, timed) in undated.into_iter().enumerate() {
                            UndatedRow { key: "undated-{index}", timed, days: axis.days, local }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn TimelineRow(timed: Timed, axis: Axis, local: LocalTime, now: f64) -> Element {
    let span = Span::of(axis, local, timed.start(local.now), timed.end(local.now));
    let side = span.label_side();
    let upcoming = timed.since_start.is_some_and(|since| since < 0);
    let tone = if !timed.is_event() {
        "banner"
    } else if timed.item.finished == Some(true) {
        "done"
    } else {
        "event"
    };
    rsx! {
        tr { class: "tl-row", "data-kind": "{timed.item.kind}",
            if span.before > 0 {
                td { colspan: span.before, class: "tl-space",
                    NowLine { now, start: 0, span: span.before }
                    if side == LabelSide::Before {
                        div { class: "tl-label tl-outside", "data-side": "before", BarLabel { timed: timed.clone(), local } }
                    }
                }
            }
            td { colspan: span.length, class: "tl-cell",
                NowLine { now, start: span.before, span: span.length }
                div { class: "tl-bar", "data-tone": tone, "data-upcoming": upcoming, "data-clip": span.clip(),
                    if side == LabelSide::Inside {
                        div { class: "tl-label", BarLabel { timed: timed.clone(), local } }
                    }
                }
            }
            if span.after > 0 {
                td { colspan: span.after, class: "tl-space",
                    NowLine { now, start: span.before + span.length, span: span.after }
                    if side == LabelSide::After {
                        div { class: "tl-label tl-outside", "data-side": "after", BarLabel { timed: timed.clone(), local } }
                    }
                }
            }
        }
    }
}

/// An item without a known period: its label across the row, with no bar.
#[component]
fn UndatedRow(timed: Timed, days: i64, local: LocalTime) -> Element {
    rsx! {
        tr { class: "tl-row", "data-kind": "{timed.item.kind}", "data-undated": true,
            td { colspan: days, class: "tl-cell",
                div { class: "tl-label tl-undated", BarLabel { timed: timed.clone(), local } }
            }
        }
    }
}

/// The "now" line through the one cell of a row that holds the current moment: the
/// cell spans whole columns of equal width, so a percentage of the cell is exact.
#[component]
fn NowLine(now: f64, start: i64, span: i64) -> Element {
    let Some(x) = now_offset(now, start, span) else {
        return rsx! {};
    };
    rsx! {
        svg { class: "tl-now", "aria-hidden": "true", "focusable": "false",
            line { x1: "{x:.3}%", x2: "{x:.3}%", y1: "0", y2: "100%" }
        }
    }
}

/// Where `now` (in columns from the axis start) falls inside the cell covering `span`
/// columns from `start`, in percent of the cell.
fn now_offset(now: f64, start: i64, span: i64) -> Option<f64> {
    let (start, span) = (start as f64, span as f64);
    (span > 0.0 && now >= start && now < start + span).then(|| (now - start) / span * 100.0)
}

/// Type icon and name, rate-up items, then period, countdown and status of one bar. The
/// table is hidden from readers, so the icon is decorative here (the lane names it).
#[component]
fn BarLabel(timed: Timed, local: LocalTime) -> Element {
    let ui = use_preferences();
    let name = display_name(&timed.item, ui);
    let start = timed.start(local.now);
    let end = timed.end(local.now);
    let left = countdown(&timed, ui);
    rsx! {
        div { class: "tl-title",
            Glyph { icon: kind_icon(&timed.item), class: "icon-sm" }
            strong { class: "tl-name", "{name}" }
        }
        FeaturedLine { item: timed.item.clone() }
        p { class: "tl-meta",
            if start.is_some() || end.is_some() {
                span { class: "tl-period", title: period_title(start, end), {period_text(local, start, end)} }
            }
            if let Some(left) = left {
                StatusMark { status: left.status, text: left.duration, label: left.sentence, class: "tl-left" }
            }
            ItemStatusMark { item: timed.item.clone(), class: "tl-status" }
        }
    }
}

/// The exact moments behind a bar's dates, as a tooltip.
fn period_title(start: Option<i64>, end: Option<i64>) -> String {
    let moment = |at: Option<i64>| {
        at.and_then(|at| compact_datetime(Some(&iso(at))))
            .unwrap_or_default()
    };
    format!("{} – {}", moment(start), moment(end))
        .trim()
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 2026-11-09 12:00 UTC = 21:00 JST.
    const NOW: i64 = 1_794_225_600;
    const JST: LocalTime = LocalTime {
        now: NOW,
        offset: 9 * 3600,
    };

    #[test]
    fn civil_dates_and_iso_timestamps_follow_the_gregorian_calendar() {
        assert_eq!(civil(0), (1970, 1, 1));
        assert_eq!(civil(-1), (1969, 12, 31));
        assert_eq!(civil(20_513), (2026, 3, 1));
        assert_eq!(civil(20_512), (2026, 2, 28));
        assert_eq!(civil(11_016), (2000, 2, 29));
        assert_eq!(iso(NOW), "2026-11-09T12:00:00Z");
        assert_eq!(iso(-1), "1969-12-31T23:59:59Z");
    }

    #[test]
    fn local_days_and_times_use_the_device_offset() {
        assert_eq!(JST.date(NOW), "11/09");
        assert_eq!(JST.date_time(NOW), "11/09 21:00");
        // 15:00 UTC is already the next day in Japan, not in UTC.
        let late = NOW + 3 * 3600;
        assert_eq!(JST.date_time(late), "11/10 00:00");
        assert_eq!(LocalTime { offset: 0, ..JST }.date(late), "11/09");
        assert_eq!(
            LocalTime {
                offset: -5 * 3600,
                ..JST
            }
            .date_time(NOW),
            "11/09 07:00"
        );
        // An end at local midnight belongs to the day before.
        assert_eq!(period_text(JST, Some(NOW), Some(late)), "11/09 – 11/09");
        assert_eq!(period_text(JST, None, Some(late + 1)), "– 11/10");
        assert_eq!(period_text(JST, Some(NOW), None), "11/09 –");
    }

    #[test]
    fn the_axis_spans_the_items_within_a_week_back_and_six_weeks_ahead() {
        let today = JST.day(NOW);
        // Nothing known: two weeks from today.
        let axis = Axis::of(JST, &[]);
        assert_eq!((axis.first, axis.days, axis.today), (today, MIN_DAYS, 0));
        // A start three days back and an end ten days ahead.
        let axis = Axis::of(JST, &[(Some(NOW - 3 * DAY), Some(NOW + 10 * DAY))]);
        assert_eq!((axis.first, axis.today), (today - 3, 3));
        assert_eq!(axis.days, MIN_DAYS);
        // A long-running item is clipped a week back; a far end six weeks ahead.
        let axis = Axis::of(
            JST,
            &[
                (Some(NOW - 60 * DAY), Some(NOW + 90 * DAY)),
                (Some(NOW + 2 * DAY), None),
            ],
        );
        assert_eq!(axis.first, today - PAST_DAYS);
        assert_eq!(axis.last(), today + FUTURE_DAYS);
        assert_eq!(axis.today, PAST_DAYS);
        // An announced item without an end still widens the axis to its start.
        let axis = Axis::of(JST, &[(Some(NOW + 20 * DAY), None)]);
        assert_eq!(axis.last(), today + 20);
        // Even when it starts exactly at local midnight (21:00 JST + 3 h).
        let midnight = NOW + 3 * 3600 + 19 * DAY;
        let axis = Axis::of(JST, &[(Some(midnight), None)]);
        assert_eq!(axis.last(), today + 20);
        let span = Span::of(axis, JST, Some(midnight), None);
        assert_eq!((span.before, span.length, span.after), (20, 1, 0));
    }

    #[test]
    fn months_and_weekly_ticks_cover_every_day_of_the_axis() {
        let axis = Axis::of(JST, &[(Some(NOW - 2 * DAY), Some(NOW + 30 * DAY))]);
        // 11/07 .. 12/09: 24 November days, 9 December days.
        assert_eq!(axis.months(), [(11, 24), (12, 9)]);
        let ticks = axis.ticks();
        assert_eq!(ticks.len() as i64, axis.days);
        assert_eq!(ticks[0], (7, false));
        assert_eq!(ticks[axis.today as usize], (9, true));
        let labelled: Vec<u32> = ticks
            .iter()
            .filter(|(_, labelled)| *labelled)
            .map(|(day, _)| *day)
            .collect();
        assert_eq!(labelled, [9, 16, 23, 30, 7]);
    }

    #[test]
    fn bars_fill_their_local_days_and_are_clipped_at_the_edges() {
        let axis = Axis::of(JST, &[(Some(NOW - 2 * DAY), Some(NOW + 30 * DAY))]);
        // Started yesterday, ends at midnight in five days: yesterday through four days ahead.
        let midnight = NOW + 3 * 3600 + 4 * DAY;
        let span = Span::of(axis, JST, Some(NOW - DAY), Some(midnight));
        assert_eq!((span.before, span.length), (1, 6));
        assert_eq!(span.before + span.length + span.after, axis.days);
        assert_eq!(span.clip(), None);
        // Started before the axis and with no known end: clipped on both sides.
        let span = Span::of(axis, JST, Some(NOW - 20 * DAY), None);
        assert_eq!((span.before, span.length, span.after), (0, axis.days, 0));
        assert_eq!(span.clip(), Some("both"));
        assert_eq!(span.label_side(), LabelSide::Inside);
        // Unknown start, known end.
        let span = Span::of(axis, JST, None, Some(NOW + 3600));
        assert_eq!(span.clip(), Some("start"));
        assert_eq!(span.length, 3);
        // An item past the right edge keeps one column at the edge.
        let span = Span::of(axis, JST, Some(NOW + 80 * DAY), Some(NOW + 90 * DAY));
        assert_eq!((span.length, span.after), (1, 0));
        assert_eq!(span.clip(), Some("end"));
    }

    #[test]
    fn the_now_line_sits_in_the_one_cell_holding_the_current_moment() {
        let axis = Axis::of(JST, &[(Some(NOW - 2 * DAY), Some(NOW + 30 * DAY))]);
        // 21:00 on the third column.
        let now = axis.now(JST);
        assert!((now - 2.875).abs() < 1e-9);
        assert_eq!(now_offset(now, 0, 2), None);
        assert_eq!(now_offset(now, 2, 1), Some(87.5));
        assert_eq!(now_offset(now, 0, 4), Some(71.875));
        assert_eq!(now_offset(now, 3, 10), None);
        assert_eq!(now_offset(now, 0, 0), None);
    }

    #[test]
    fn short_bars_put_their_label_on_the_roomier_side() {
        let span = |before, length, after| Span {
            before,
            length,
            after,
            clip_start: false,
            clip_end: false,
        };
        assert_eq!(span(2, 20, 20).label_side(), LabelSide::Inside);
        assert_eq!(span(30, 10, 2).label_side(), LabelSide::Inside);
        assert_eq!(span(3, 4, 30).label_side(), LabelSide::After);
        assert_eq!(span(30, 4, 3).label_side(), LabelSide::Before);
        assert_eq!(span(10, 4, 10).label_side(), LabelSide::After);
        assert_eq!(span(0, 3, 0).label_side(), LabelSide::Inside);
        // A third of a short axis is long enough.
        assert_eq!(span(5, 5, 4).label_side(), LabelSide::Inside);
    }

    #[test]
    fn month_names_follow_the_language() {
        let ui = Preferences::default();
        assert_eq!(month_name(11, ui), "11月");
        let english = Preferences {
            language: crate::i18n::Language::English,
            ..Preferences::default()
        };
        assert_eq!(month_name(1, english), "Jan");
        assert_eq!(month_name(12, english), "Dec");
    }
}
