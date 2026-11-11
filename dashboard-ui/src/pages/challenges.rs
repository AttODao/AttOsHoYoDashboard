use super::common::{LoadingLine, WarningLine, pending, refresh_problem};
use super::dashboard::use_clock;
use super::icons::{Glyph, Icon, NamedIcon, Status, StatusMark};
use crate::app::display::{compact_datetime, days_hours, error_text, seconds_since};
use crate::app::preferences::{Preferences, use_preferences};
use crate::app::state::AppState;
use crate::models::{ChallengeMode, GameChallenges};
use dioxus::prelude::*;

/// Endgame summary of the selected UID as one tile per mode: the mode's icon and name,
/// the stars or medals reached, how far, and the time until its period resets. The
/// section heading is for screen readers; when the records were fetched is in the card's
/// details. Hidden until the game has a selected UID (the status section above explains
/// how to choose one).
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
    let loading = pending(
        selection.challenges_loading,
        challenges.is_some(),
        selection.challenges_error.is_some(),
    );
    drop(snapshot);
    if selection.challenges_unavailable && challenges.is_none() {
        return rsx! {};
    }
    rsx! {
        section { class: "challenge-section", id: "challenges-{game}", "aria-labelledby": "challenge-title-{game}",
            h3 { class: "visually-hidden", id: "challenge-title-{game}", {ui.text("challenges.title")} }
            if let Some(key) = selection.challenges_error {
                WarningLine {
                    status: if challenges.is_some() { Status::Warn } else { Status::Failed },
                    text: ui.text(refresh_problem(key, challenges.is_some(), "challenges.fetch_failed")),
                }
            }
            div { "aria-busy": loading,
                if let Some(challenges) = challenges {
                    ChallengeList { challenges }
                } else if loading {
                    LoadingLine {}
                }
            }
        }
    }
}

#[component]
fn ChallengeList(challenges: GameChallenges) -> Element {
    let ui = use_preferences();
    let notice = challenge_notice(&challenges, ui);
    let failed = challenges.modes.is_none();
    rsx! {
        if let Some(notice) = notice {
            WarningLine { status: if failed { Status::Failed } else { Status::Warn }, text: notice }
        }
        if let Some(modes) = challenges.modes {
            if modes.is_empty() {
                p { class: "quiet-line", {ui.text("challenges.no_modes")} }
            }
            ul { class: "tiles challenge-list",
                for mode in modes {
                    ChallengeTile { key: "{mode.mode}", mode }
                }
            }
        }
    }
}

#[component]
fn ChallengeTile(mode: ChallengeMode) -> Element {
    let ui = use_preferences();
    let clock = use_clock();
    let _ = clock();
    let reset = seconds_since(mode.ends_at.as_deref());
    let progress = progress(&mode, ui);
    rsx! {
        li { class: "tile challenge", "data-mode": "{mode.mode}",
            p { class: "tile-h",
                Glyph { icon: mode_icon(&mode.mode) }
                span { class: "challenge-name", {mode_name(&mode.mode, ui)} }
            }
            match progress {
                Progress::Quiet(status, text) => rsx! {
                    p { class: "tile-v quiet challenge-progress", StatusMark { status, text } }
                },
                Progress::Reached { score, detail, spoken } => {
                    // Without a score the detail is the tile's value, not a footnote.
                    let detail_class = if score.is_some() { "tile-sub challenge-progress" } else { "tile-v tile-text challenge-progress" };
                    rsx! {
                        if let Some((icon, name, count)) = score {
                            p { class: "tile-v challenge-score",
                                NamedIcon { icon, name }
                                strong { "{count}" }
                            }
                        }
                        if !detail.is_empty() {
                            p { class: detail_class,
                                span { class: "visually-hidden", "{spoken}" }
                                span { "aria-hidden": "true", "{detail}" }
                            }
                        }
                    }
                }
            }
            if let Some(elapsed) = reset {
                time { class: "tile-r challenge-reset", datetime: mode.ends_at.clone(), title: compact_datetime(mode.ends_at.as_deref()),
                    if elapsed >= 0 {
                        {ui.text("challenges.period_ended")}
                    } else {
                        Glyph { icon: Icon::Hourglass }
                        span { class: "visually-hidden", {ui.format("challenges.resets_in", &[("duration", &days_hours(-elapsed, ui))])} }
                        span { "aria-hidden": "true", {days_hours(-elapsed, ui)} }
                    }
                }
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

fn mode_icon(mode: &str) -> Icon {
    match mode {
        "spiral_abyss" | "memory_of_chaos" => Icon::Spiral,
        "imaginarium_theater" => Icon::Curtain,
        "stygian_onslaught" => Icon::Swords,
        "pure_fiction" => Icon::Book,
        "apocalyptic_shadow" => Icon::Eclipse,
        "anomaly_arbitration" => Icon::Scales,
        _ => Icon::Unknown,
    }
}

/// What a mode's tile shows below its name.
#[derive(Clone, Debug, PartialEq)]
enum Progress {
    /// Nothing reached to show: a status symbol with its word, in the quiet tone.
    Quiet(Status, String),
    /// What the player reached this period, as reported; unknown values are left out.
    Reached {
        /// Stars or medals: the icon, its accessible name and the count.
        score: Option<(Icon, String, i64)>,
        /// The rest in its short visual form, e.g. "12-3" or "Act 8 · Difficulty 4".
        detail: String,
        /// The same with its words, for screen readers, e.g. "Reached 12-3".
        spoken: String,
    },
}

fn progress(mode: &ChallengeMode, ui: Preferences) -> Progress {
    let quiet = |status, key| Progress::Quiet(status, ui.text(key).to_owned());
    if mode.error_code.is_some() {
        return quiet(Status::Failed, "challenges.mode_unavailable");
    }
    if mode.unlocked == Some(false) {
        return quiet(Status::Locked, "challenges.locked");
    }
    if mode.has_data == Some(false) {
        return quiet(Status::Open, "challenges.not_attempted");
    }
    let count = |key: &str, name: &str, value: i64| ui.format(key, &[(name, &value.to_string())]);
    let score = mode.stars.map(|stars| {
        if mode.mode == "imaginarium_theater" {
            (Icon::Medal, ui.text("challenges.medals").to_owned(), stars)
        } else {
            (Icon::Star, ui.text("challenges.stars").to_owned(), stars)
        }
    });
    let (mut detail, mut spoken) = (Vec::new(), Vec::new());
    if let Some(round) = mode.max_round {
        detail.push(count("challenges.act", "round", round));
    }
    if let Some(level) = mode.difficulty {
        detail.push(count("challenges.difficulty", "level", level));
    }
    spoken.clone_from(&detail);
    if let Some(floor) = &mode.max_floor {
        detail.push(floor.clone());
        spoken.push(ui.format("challenges.reached", &[("floor", floor)]));
    }
    if let Some(seconds) = mode.best_seconds {
        let time = count("challenges.clear_time", "seconds", seconds);
        detail.push(time.clone());
        spoken.push(time);
    }
    if score.is_none() && detail.is_empty() {
        return quiet(Status::Unknown, "dashboard.unknown");
    }
    Progress::Reached {
        score,
        detail: detail.join(" · "),
        spoken: spoken.join(" · "),
    }
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
        let reached =
            |score: Option<(Icon, &str, i64)>, detail: &str, spoken: &str| Progress::Reached {
                score: score.map(|(icon, name, count)| (icon, name.to_owned(), count)),
                detail: detail.to_owned(),
                spoken: spoken.to_owned(),
            };
        assert_eq!(
            progress(
                &mode(
                    serde_json::json!({"mode": "spiral_abyss", "stars": 33, "max_floor": "12-3"})
                ),
                ui
            ),
            reached(Some((Icon::Star, "星", 33)), "12-3", "到達 12-3")
        );
        assert_eq!(
            progress(
                &mode(
                    serde_json::json!({"mode": "imaginarium_theater", "max_round": 8, "difficulty": 4, "stars": 6})
                ),
                ui
            ),
            reached(
                Some((Icon::Medal, "勲章", 6)),
                "第8幕 · 難易度4",
                "第8幕 · 難易度4"
            )
        );
        // No stars reported: no score symbol, and no zero in its place.
        assert_eq!(
            progress(
                &mode(
                    serde_json::json!({"mode": "stygian_onslaught", "difficulty": 5, "best_seconds": 112})
                ),
                ui
            ),
            reached(None, "難易度5 · 112秒", "難易度5 · 112秒")
        );
        // Nothing at all reads as unknown, not as zero.
        assert_eq!(
            progress(&mode(serde_json::json!({"mode": "pure_fiction"})), ui),
            Progress::Quiet(Status::Unknown, "不明".to_owned())
        );
        assert_eq!(
            progress(
                &mode(
                    serde_json::json!({"mode": "memory_of_chaos", "has_data": false, "stars": 0})
                ),
                ui
            ),
            Progress::Quiet(Status::Open, "未挑戦".to_owned())
        );
        assert_eq!(
            progress(
                &mode(serde_json::json!({"mode": "imaginarium_theater", "unlocked": false})),
                ui
            ),
            Progress::Quiet(Status::Locked, "未開放".to_owned())
        );
        assert_eq!(
            progress(
                &mode(
                    serde_json::json!({"mode": "apocalyptic_shadow", "error_code": "network", "stars": 3})
                ),
                ui
            ),
            Progress::Quiet(Status::Failed, "取得失敗".to_owned())
        );
        let english = Preferences {
            language: Language::English,
            ..ui
        };
        assert_eq!(
            progress(
                &mode(serde_json::json!({"mode": "anomaly_arbitration", "stars": 11})),
                english
            ),
            reached(Some((Icon::Star, "Stars", 11)), "", "")
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
    fn every_mode_has_an_icon_and_the_paired_modes_share_one() {
        assert_eq!(mode_icon("spiral_abyss"), Icon::Spiral);
        assert_eq!(mode_icon("memory_of_chaos"), Icon::Spiral);
        assert_eq!(mode_icon("imaginarium_theater"), Icon::Curtain);
        assert_eq!(mode_icon("stygian_onslaught"), Icon::Swords);
        assert_eq!(mode_icon("pure_fiction"), Icon::Book);
        assert_eq!(mode_icon("apocalyptic_shadow"), Icon::Eclipse);
        assert_eq!(mode_icon("anomaly_arbitration"), Icon::Scales);
        assert_eq!(mode_icon("future_mode"), Icon::Unknown);
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
