use super::dashboard::{Freshness, use_clock};
use crate::app::display::{compact_datetime, days_hours, error_text, seconds_since};
use crate::app::preferences::{Preferences, use_preferences};
use crate::app::state::AppState;
use crate::models::{ChallengeMode, GameChallenges};
use dioxus::prelude::*;

/// Endgame summary of the selected UID: each mode's current progress and the time
/// until its period resets. Hidden until the game has a selected UID (the status
/// section above explains how to choose one).
#[component]
pub(super) fn ChallengeSection(state: Signal<AppState>, game: &'static str) -> Element {
    let ui = use_preferences();
    let snapshot = state.read();
    let Some(id) = snapshot
        .accounts
        .is_some()
        .then(|| snapshot.selected_game_account(game))
        .flatten()
    else {
        return rsx! {};
    };
    let selection = snapshot
        .selected_accounts
        .get(game)
        .cloned()
        .unwrap_or_default();
    let challenges = snapshot
        .account_challenges
        .iter()
        .find(|entry| entry.id == id)
        .map(|entry| entry.challenges.clone());
    let loading = selection.challenges_loading;
    drop(snapshot);
    if selection.challenges_unavailable && challenges.is_none() {
        return rsx! {};
    }
    rsx! {
        section { class: "challenge-section", id: "challenges-{game}", "aria-labelledby": "challenge-title-{game}",
            h3 { id: "challenge-title-{game}", {ui.text("challenges.title")} }
            if let Some(key) = selection.challenges_error {
                p { class: "warning", role: "status", {ui.text(key)} }
            }
            div { "aria-busy": loading,
                if let Some(challenges) = challenges.clone() {
                    ChallengeList { challenges }
                } else if loading {
                    p { role: "status", {ui.text("dashboard.loading")} }
                } else if selection.challenges_error.is_none() {
                    p { {ui.text("challenges.not_fetched")} }
                }
            }
            if let Some(challenges) = challenges {
                div { class: "refresh-row",
                    Freshness { fetched_at: challenges.fetched_at.clone(), stale: challenges.stale }
                }
            }
        }
    }
}

#[component]
fn ChallengeList(challenges: GameChallenges) -> Element {
    let ui = use_preferences();
    let notice = challenge_notice(&challenges, ui);
    rsx! {
        if let Some(notice) = notice {
            p { class: "warning", role: "status", {notice} }
        }
        if let Some(modes) = challenges.modes {
            if modes.is_empty() {
                p { {ui.text("challenges.no_modes")} }
            }
            ul { class: "challenge-list",
                for mode in modes {
                    ChallengeRow { key: "{mode.mode}", mode }
                }
            }
        }
    }
}

#[component]
fn ChallengeRow(mode: ChallengeMode) -> Element {
    let ui = use_preferences();
    let clock = use_clock();
    let _ = clock();
    let reset = reset_text(seconds_since(mode.ends_at.as_deref()), ui);
    let progress = progress_text(&mode, ui);
    let quiet =
        mode.error_code.is_some() || mode.unlocked == Some(false) || mode.has_data == Some(false);
    rsx! {
        li { class: "challenge", "data-mode": "{mode.mode}",
            div { class: "challenge-head",
                strong { class: "challenge-name", {mode_name(&mode.mode, ui)} }
                if let Some(reset) = reset {
                    time {
                        class: "challenge-reset", datetime: mode.ends_at.clone(),
                        title: compact_datetime(mode.ends_at.as_deref()),
                        "{reset}"
                    }
                }
            }
            p { class: if quiet { "challenge-progress quiet" } else { "challenge-progress" }, "{progress}" }
            if let Some(name) = mode.name.clone() {
                p { class: "challenge-period", "{name}" }
            }
        }
    }
}

fn mode_name(mode: &str, ui: Preferences) -> &'static str {
    ui.text(match mode {
        "spiral_abyss" => "challenges.spiral_abyss",
        "imaginarium_theater" => "challenges.imaginarium_theater",
        "stygian_onslaught" => "challenges.stygian_onslaught",
        "memory_of_chaos" => "challenges.memory_of_chaos",
        "pure_fiction" => "challenges.pure_fiction",
        "apocalyptic_shadow" => "challenges.apocalyptic_shadow",
        "anomaly_arbitration" => "challenges.anomaly_arbitration",
        _ => "dashboard.unknown",
    })
}

/// What the player reached this period, as reported; unknown values are left out.
fn progress_text(mode: &ChallengeMode, ui: Preferences) -> String {
    if mode.error_code.is_some() {
        return ui.text("challenges.mode_unavailable").to_owned();
    }
    if mode.unlocked == Some(false) {
        return ui.text("challenges.locked").to_owned();
    }
    if mode.has_data == Some(false) {
        return ui.text("challenges.not_attempted").to_owned();
    }
    let count = |key: &str, name: &str, value: i64| ui.format(key, &[(name, &value.to_string())]);
    let mut parts = Vec::new();
    if let Some(round) = mode.max_round {
        parts.push(count("challenges.act", "round", round));
    }
    if let Some(level) = mode.difficulty {
        parts.push(count("challenges.difficulty", "level", level));
    }
    if let Some(stars) = mode.stars {
        let key = if mode.mode == "imaginarium_theater" {
            "challenges.medals"
        } else {
            "challenges.stars"
        };
        parts.push(count(key, "count", stars));
    }
    if let Some(floor) = &mode.max_floor {
        parts.push(ui.format("challenges.reached", &[("floor", floor)]));
    }
    if let Some(seconds) = mode.best_seconds {
        parts.push(count("challenges.clear_time", "seconds", seconds));
    }
    if parts.is_empty() {
        ui.text("dashboard.unknown").to_owned()
    } else {
        parts.join(" · ")
    }
}

/// Time left in the period from the seconds elapsed since its end (negative while running).
fn reset_text(elapsed_since_end: Option<i64>, ui: Preferences) -> Option<String> {
    let elapsed = elapsed_since_end?;
    Some(if elapsed >= 0 {
        ui.text("challenges.period_ended").to_owned()
    } else {
        ui.format(
            "challenges.resets_in",
            &[("duration", &days_hours(-elapsed, ui))],
        )
    })
}

/// A failed fetch without a summary says only that; an outdated summary says why.
fn challenge_notice(challenges: &GameChallenges, ui: Preferences) -> Option<String> {
    let reason = challenges
        .error_code
        .as_deref()
        .map(|code| error_text(Some(code), ui));
    if challenges.modes.is_none() {
        return reason
            .map(|reason| ui.format("challenges.fetch_failed_with_reason", &[("reason", reason)]));
    }
    if !challenges.stale {
        return None;
    }
    Some(match reason {
        Some(reason) => ui.format("challenges.stale_with_reason", &[("reason", reason)]),
        None => ui.text("challenges.stale").to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::Language;

    fn mode(value: serde_json::Value) -> ChallengeMode {
        serde_json::from_value(value).unwrap()
    }

    #[test]
    fn progress_shows_what_each_mode_reports_and_never_invents_values() {
        let ui = Preferences::default();
        assert_eq!(
            progress_text(
                &mode(
                    serde_json::json!({"mode": "spiral_abyss", "stars": 33, "max_floor": "12-3"})
                ),
                ui
            ),
            "星33 · 到達 12-3"
        );
        assert_eq!(
            progress_text(
                &mode(
                    serde_json::json!({"mode": "imaginarium_theater", "max_round": 8, "difficulty": 4, "stars": 6})
                ),
                ui
            ),
            "第8幕 · 難易度4 · 勲章6"
        );
        assert_eq!(
            progress_text(
                &mode(
                    serde_json::json!({"mode": "stygian_onslaught", "difficulty": 5, "best_seconds": 112})
                ),
                ui
            ),
            "難易度5 · 112秒"
        );
        // Unknown fields stay out; nothing at all reads as unknown, not as zero.
        assert_eq!(
            progress_text(&mode(serde_json::json!({"mode": "pure_fiction"})), ui),
            "不明"
        );
        assert_eq!(
            progress_text(
                &mode(
                    serde_json::json!({"mode": "memory_of_chaos", "has_data": false, "stars": 0})
                ),
                ui
            ),
            "未挑戦"
        );
        assert_eq!(
            progress_text(
                &mode(serde_json::json!({"mode": "imaginarium_theater", "unlocked": false})),
                ui
            ),
            "未開放"
        );
        assert_eq!(
            progress_text(
                &mode(
                    serde_json::json!({"mode": "apocalyptic_shadow", "error_code": "network", "stars": 3})
                ),
                ui
            ),
            "取得できませんでした"
        );
        let english = Preferences {
            language: Language::English,
            ..ui
        };
        assert_eq!(
            progress_text(
                &mode(serde_json::json!({"mode": "anomaly_arbitration", "stars": 11})),
                english
            ),
            "11 stars"
        );
    }

    #[test]
    fn every_mode_has_a_name_and_unknown_modes_do_not_fail() {
        let ui = Preferences::default();
        for (id, name) in [
            ("spiral_abyss", "深境螺旋"),
            ("imaginarium_theater", "幻想シアター"),
            ("stygian_onslaught", "幽境の激戦"),
            ("memory_of_chaos", "混沌の記憶"),
            ("pure_fiction", "虚構叙事"),
            ("apocalyptic_shadow", "末日の幻影"),
            ("anomaly_arbitration", "異相の仲裁"),
        ] {
            assert_eq!(mode_name(id, ui), name);
        }
        assert_eq!(mode_name("future_mode", ui), "不明");
    }

    #[test]
    fn reset_counts_down_in_days_then_says_the_period_ended() {
        let ui = Preferences::default();
        assert_eq!(reset_text(None, ui), None);
        assert_eq!(
            reset_text(Some(-(5 * 86_400 + 3 * 3600)), ui).unwrap(),
            "リセットまで5日3時間"
        );
        assert_eq!(
            reset_text(Some(-(2 * 3600 + 30 * 60)), ui).unwrap(),
            "リセットまで2時間30分"
        );
        assert_eq!(reset_text(Some(0), ui).unwrap(), "期間終了");
        assert_eq!(reset_text(Some(90), ui).unwrap(), "期間終了");
        let english = Preferences {
            language: Language::English,
            ..ui
        };
        assert_eq!(
            reset_text(Some(-(5 * 86_400 + 3 * 3600)), english).unwrap(),
            "Resets in 5d 3h"
        );
    }

    #[test]
    fn notices_distinguish_a_failed_first_fetch_from_an_outdated_summary() {
        let ui = Preferences::default();
        let mut challenges: GameChallenges = serde_json::from_value(serde_json::json!({
            "game_kind": "genshin", "game_uid": "600000000", "region": "os_asia",
            "visible": true, "detected": true, "modes": null, "fetched_at": null,
            "error_code": "network", "error_at": null, "stale": true
        }))
        .unwrap();
        assert_eq!(
            challenge_notice(&challenges, ui).unwrap(),
            "記録を取得できませんでした（HoYoLABに接続できませんでした）"
        );
        challenges.modes = Some(Vec::new());
        assert_eq!(
            challenge_notice(&challenges, ui).unwrap(),
            "表示中の記録は最新ではありません（HoYoLABに接続できませんでした）"
        );
        challenges.error_code = None;
        assert_eq!(
            challenge_notice(&challenges, ui).unwrap(),
            "表示中の記録は最新ではありません"
        );
        challenges.stale = false;
        assert_eq!(challenge_notice(&challenges, ui), None);
    }
}
