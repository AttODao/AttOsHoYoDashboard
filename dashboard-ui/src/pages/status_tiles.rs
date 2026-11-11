//! The secondary values of a game card as icon tiles: an icon with a short name, the
//! value, and a bar, pips or a status symbol below it. Screen readers get the full names
//! and sentences; a value that was not returned shows the unknown mark, never a number.
use super::dashboard::use_clock;
use super::icons::{Glyph, Icon, Status, StatusMark};
use crate::app::display::{days_hours, fill_level, hours_minutes, seconds_since};
use crate::app::preferences::use_preferences;
use crate::models::DailySummary;
use dioxus::prelude::*;

/// More pips than this would not read at a glance; the count stays as text only.
const MAX_PIPS: i64 = 10;

/// Realm currency, commissions (with the extra reward), weekly boss discounts and the
/// Parametric Transformer.
#[component]
pub(super) fn GenshinTiles(data: DailySummary) -> Element {
    let ui = use_preferences();
    // The currency and transformer countdowns read the clock.
    let clock = use_clock();
    let _ = clock();
    let coin_full = fill_level(data.current_home_coin, data.max_home_coin) == "full";
    let coin_left = seconds_since(data.home_coin_full_at.as_deref()).map(|elapsed| -elapsed);
    let extra = extra_reward(data.is_extra_task_reward_received);
    let extra_text = format!(
        "{} {}",
        ui.text("dashboard.extra_commission_reward"),
        ui.text(extra.1)
    );
    let transformer = transformer_state(&data, seconds_since(data.transformer_ready_at.as_deref()));
    rsx! {
        h3 { class: "visually-hidden", {ui.text("dashboard.other_values")} }
        ul { class: "tiles",
            Tile { id: "coin", icon: Icon::Coin, name: "dashboard.realm_currency_short", full_name: "dashboard.realm_currency", warn: coin_full,
                TileCount { current: data.current_home_coin, maximum: data.max_home_coin }
                div { class: "tile-x",
                    TileBar { current: data.current_home_coin, maximum: data.max_home_coin }
                    // At the cap (also once the estimate has passed) the badge says so;
                    // before it only the time left is shown.
                    if coin_full || coin_left.is_some_and(|left| left <= 0) {
                        StatusMark { status: Status::Warn, text: ui.text("dashboard.at_cap"), class: "cap-badge" }
                    } else if let Some(left) = coin_left {
                        TimeLeft { label: ui.text("dashboard.currency_full_at"), duration: hours_minutes(left, ui) }
                    }
                }
            }
            Tile { id: "daily", icon: Icon::Daily, name: "dashboard.commissions_short", full_name: "dashboard.commissions",
                TileCount { current: data.finished_task_num, maximum: data.total_task_num }
                div { class: "tile-x",
                    Pips { filled: data.finished_task_num, total: data.total_task_num }
                    span { class: "tile-extra", "data-status": extra.0.name(), title: extra_text.clone(),
                        Glyph { icon: Icon::Gift }
                        StatusMark { status: extra.0, text: extra_text.clone(), hide_text: true }
                    }
                }
            }
            Tile { id: "discount", icon: Icon::Discount, name: "dashboard.weekly_boss_discounts_short", full_name: "dashboard.weekly_boss_discounts",
                TileLeft { left: data.remain_resin_discount_num, maximum: data.resin_discount_num_limit }
            }
            Tile { id: "transformer", icon: Icon::Transformer, name: "dashboard.transformer_short", full_name: "dashboard.transformer",
                match transformer {
                    Transformer::NotObtained => rsx! { p { class: "tile-v quiet", StatusMark { status: Status::Locked, text: ui.text("dashboard.transformer_not_obtained") } } },
                    Transformer::Ready => rsx! { p { class: "tile-v quiet", StatusMark { status: Status::Done, text: ui.text("dashboard.transformer_ready") } } },
                    Transformer::Cooling(Some(left)) => rsx! {
                        p { class: "tile-v quiet",
                            Glyph { icon: Icon::Hourglass, class: "icon-sm" }
                            span { class: "visually-hidden", {ui.format("dashboard.in_duration", &[("duration", &days_hours(left, ui))])} }
                            strong { class: "tile-text", "aria-hidden": "true", {days_hours(left, ui)} }
                        }
                    },
                    Transformer::Cooling(None) => rsx! { p { class: "tile-v quiet", StatusMark { status: Status::Waiting, text: ui.text("dashboard.transformer_cooling") } } },
                    Transformer::Unknown => rsx! { NotFetched {} },
                }
            }
        }
    }
}

/// Reserved Trailblaze Power, Daily Training, Simulated Universe, Divergent Universe (once
/// unlocked) and Echo of War.
#[component]
pub(super) fn StarRailTiles(data: DailySummary) -> Element {
    let ui = use_preferences();
    let reserve_full = data.is_reserve_stamina_full == Some(true);
    rsx! {
        h3 { class: "visually-hidden", {ui.text("dashboard.other_values")} }
        ul { class: "tiles",
            Tile { id: "reserve", icon: Icon::Reserve, name: "dashboard.reserved_trailblaze_power_short", full_name: "dashboard.reserved_trailblaze_power", warn: reserve_full,
                TileCount { current: data.current_reserve_stamina, maximum: None }
                if reserve_full {
                    div { class: "tile-x", StatusMark { status: Status::Warn, text: ui.text("dashboard.at_cap"), class: "cap-badge" } }
                }
            }
            Tile { id: "training", icon: Icon::Daily, name: "dashboard.commissions_short", full_name: "dashboard.daily_training",
                ScoreTile { current: data.current_train_score, maximum: data.max_train_score }
            }
            Tile { id: "simulated", icon: Icon::Cube, name: "dashboard.simulated_universe_short", full_name: "dashboard.simulated_universe",
                ScoreTile { current: data.current_rogue_score, maximum: data.max_rogue_score }
            }
            if data.rogue_tourn_weekly_unlocked == Some(true) {
                Tile { id: "divergent", icon: Icon::Orbit, name: "dashboard.divergent_universe_short", full_name: "dashboard.divergent_universe",
                    ScoreTile { current: data.rogue_tourn_weekly_cur, maximum: data.rogue_tourn_weekly_max }
                }
            }
            Tile { id: "echo", icon: Icon::Echo, name: "dashboard.echo_of_war", full_name: "dashboard.echo_of_war",
                TileLeft { left: data.weekly_cocoon_cnt, maximum: data.weekly_cocoon_limit }
            }
        }
    }
}

/// One tile: the icon and the short name, then the value. `name` and `full_name` are
/// catalog keys; when they differ the full name is what a screen reader hears.
#[component]
fn Tile(
    id: &'static str,
    icon: Icon,
    name: &'static str,
    full_name: &'static str,
    #[props(default)] warn: bool,
    children: Element,
) -> Element {
    let ui = use_preferences();
    rsx! {
        li { class: "tile", "data-tile": id, "data-warn": warn,
            p { class: "tile-h", title: ui.text(full_name),
                Glyph { icon }
                if name == full_name {
                    span { {ui.text(name)} }
                } else {
                    span { class: "visually-hidden", {ui.text(full_name)} }
                    span { "aria-hidden": "true", {ui.text(name)} }
                }
            }
            {children}
        }
    }
}

/// "300/500" in the numeric face, or the unknown mark when no value was returned.
#[component]
fn TileCount(current: Option<i64>, maximum: Option<i64>) -> Element {
    rsx! {
        if let Some(value) = current {
            p { class: "tile-v",
                strong { "{value}" }
                if let Some(maximum) = maximum { span { "/{maximum}" } }
            }
        } else {
            NotFetched {}
        }
    }
}

#[component]
fn NotFetched() -> Element {
    let ui = use_preferences();
    rsx! { p { class: "tile-v quiet", StatusMark { status: Status::Unknown, text: ui.text("common.not_fetched") } } }
}

/// The share of the maximum as a bar. The number beside it is the text alternative.
#[component]
fn TileBar(current: Option<i64>, maximum: Option<i64>) -> Element {
    rsx! {
        if let (Some(value), Some(max)) = (current, maximum) {
            if max > 0 && value >= 0 && value <= max {
                progress { class: "tile-bar", value: "{value}", max: "{max}", "aria-hidden": "true" }
            }
        }
    }
}

/// A weekly score with its bar; reaching the maximum adds the done symbol.
#[component]
fn ScoreTile(current: Option<i64>, maximum: Option<i64>) -> Element {
    let ui = use_preferences();
    rsx! {
        TileCount { current, maximum }
        div { class: "tile-x",
            TileBar { current, maximum }
            if fill_level(current, maximum) == "full" {
                StatusMark { status: Status::Done, text: ui.text("dashboard.goal_reached"), hide_text: true }
            }
        }
    }
}

/// A weekly allowance that counts down: the number left, and one pip per use with the
/// remaining ones filled.
#[component]
fn TileLeft(left: Option<i64>, maximum: Option<i64>) -> Element {
    let ui = use_preferences();
    let Some(left) = left else {
        return rsx! { NotFetched {} };
    };
    rsx! {
        p { class: "tile-v",
            if let Some(maximum) = maximum {
                span { class: "visually-hidden", {ui.format("dashboard.remaining_of", &[("left", &left.to_string()), ("max", &maximum.to_string())])} }
                span { class: "tile-count", "aria-hidden": "true", strong { "{left}" } span { "/{maximum}" } }
            } else {
                strong { "{left}" }
            }
        }
        div { class: "tile-x",
            Pips { filled: Some(left), total: maximum }
            span { class: "tile-note", "aria-hidden": maximum.is_some().then_some("true"), {ui.text("dashboard.left_note")} }
        }
    }
}

/// Time left before something, as the hourglass and a duration; the label is read out.
#[component]
fn TimeLeft(label: &'static str, duration: String) -> Element {
    let ui = use_preferences();
    rsx! {
        span { class: "tile-r",
            Glyph { icon: Icon::Hourglass }
            span { class: "visually-hidden", "{label} " {ui.format("dashboard.in_duration", &[("duration", &duration)])} }
            span { "aria-hidden": "true", "{duration}" }
        }
    }
}

/// Small circles for a count out of a few; decorative, the number beside them is text.
#[component]
fn Pips(filled: Option<i64>, total: Option<i64>) -> Element {
    let Some(pips) = pips(filled, total) else {
        return rsx! {};
    };
    rsx! {
        span { class: "pips", "aria-hidden": "true",
            for (index, on) in pips.into_iter().enumerate() {
                i { key: "{index}", "data-on": on }
            }
        }
    }
}

/// One entry per unit, the first `filled` of them on. Nothing for unknown, inconsistent or
/// large counts, which stay text only.
fn pips(filled: Option<i64>, total: Option<i64>) -> Option<Vec<bool>> {
    let (filled, total) = (filled?, total?);
    ((1..=MAX_PIPS).contains(&total) && (0..=total).contains(&filled))
        .then(|| (0..total).map(|index| index < filled).collect())
}

/// The extra commission reward as a status and the catalog key of its word.
fn extra_reward(received: Option<bool>) -> (Status, &'static str) {
    match received {
        Some(true) => (Status::Done, "dashboard.received"),
        Some(false) => (Status::Open, "dashboard.not_received"),
        None => (Status::Unknown, "common.not_fetched"),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Transformer {
    NotObtained,
    Ready,
    /// Cooling down, with the seconds left when the ready time is known.
    Cooling(Option<i64>),
    Unknown,
}

/// `elapsed` is the time since `transformer_ready_at` (negative while cooling down).
fn transformer_state(data: &DailySummary, elapsed: Option<i64>) -> Transformer {
    match (data.transformer_obtained, data.transformer_ready) {
        (Some(false), _) => Transformer::NotObtained,
        (_, Some(true)) => Transformer::Ready,
        (_, Some(false)) => match elapsed {
            // The cooldown ended since the fetch.
            Some(elapsed) if elapsed >= 0 => Transformer::Ready,
            Some(elapsed) => Transformer::Cooling(Some(-elapsed)),
            None => Transformer::Cooling(None),
        },
        _ => Transformer::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::preferences::Preferences;

    fn short_names(ui: Preferences) -> Vec<&'static str> {
        [
            "dashboard.realm_currency_short",
            "dashboard.commissions_short",
            "dashboard.weekly_boss_discounts_short",
            "dashboard.transformer_short",
            "dashboard.reserved_trailblaze_power_short",
            "dashboard.simulated_universe_short",
            "dashboard.divergent_universe_short",
        ]
        .into_iter()
        .map(|key| ui.text(key))
        .collect()
    }

    fn summary(value: serde_json::Value) -> DailySummary {
        serde_json::from_value(value).unwrap()
    }

    #[test]
    fn pips_fill_from_the_start_and_only_for_small_consistent_counts() {
        assert_eq!(pips(Some(2), Some(3)).unwrap(), [true, true, false]);
        assert_eq!(pips(Some(0), Some(4)).unwrap(), [false; 4]);
        assert_eq!(pips(Some(4), Some(4)).unwrap(), [true; 4]);
        assert_eq!(pips(Some(1), Some(10)).unwrap().len(), 10);
        // Unknown, impossible or large counts draw nothing rather than a guess.
        assert_eq!(pips(None, Some(3)), None);
        assert_eq!(pips(Some(1), None), None);
        assert_eq!(pips(Some(5), Some(4)), None);
        assert_eq!(pips(Some(-1), Some(4)), None);
        assert_eq!(pips(Some(0), Some(0)), None);
        assert_eq!(pips(Some(300), Some(500)), None);
    }

    #[test]
    fn the_transformer_is_locked_ready_cooling_or_unknown() {
        let cooling =
            summary(serde_json::json!({"transformer_obtained": true, "transformer_ready": false}));
        assert_eq!(
            transformer_state(&cooling, Some(-(2 * 86_400 + 5 * 3600))),
            Transformer::Cooling(Some(2 * 86_400 + 5 * 3600))
        );
        assert_eq!(
            transformer_state(&cooling, None),
            Transformer::Cooling(None)
        );
        // The cooldown ran out since the fetch.
        assert_eq!(transformer_state(&cooling, Some(0)), Transformer::Ready);
        assert_eq!(
            transformer_state(
                &summary(
                    serde_json::json!({"transformer_obtained": true, "transformer_ready": true})
                ),
                None
            ),
            Transformer::Ready
        );
        assert_eq!(
            transformer_state(
                &summary(
                    serde_json::json!({"transformer_obtained": false, "transformer_ready": true})
                ),
                None
            ),
            Transformer::NotObtained
        );
        // Nothing returned is unknown, never "ready".
        assert_eq!(
            transformer_state(&summary(serde_json::json!({})), Some(100)),
            Transformer::Unknown
        );
    }

    #[test]
    fn the_extra_reward_keeps_unknown_apart_from_not_received() {
        assert_eq!(
            extra_reward(Some(true)),
            (Status::Done, "dashboard.received")
        );
        assert_eq!(
            extra_reward(Some(false)),
            (Status::Open, "dashboard.not_received")
        );
        assert_eq!(extra_reward(None), (Status::Unknown, "common.not_fetched"));
    }

    #[test]
    fn short_tile_names_are_as_approved_in_both_languages() {
        let ui = Preferences::default();
        assert_eq!(
            short_names(ui),
            [
                "宝銭",
                "デイリー",
                "週ボス半減",
                "変化器",
                "予備開拓力",
                "模擬宇宙",
                "差分宇宙"
            ]
        );
        let english = Preferences {
            language: crate::i18n::Language::English,
            ..ui
        };
        assert_eq!(
            short_names(english),
            [
                "Realm",
                "Dailies",
                "Boss discount",
                "Transformer",
                "Reserve",
                "Sim. Universe",
                "Div. Universe"
            ]
        );
    }
}
