//! Redemption codes: the Codes tab's reads and actions, the administrator's section, the
//! dashboard hint, and the platform-free rules they share (history rows, summary chips,
//! the hint and code entry). The server stays the authority on every rule; these only
//! decide what the screens show and what they send.
use super::actions::handle_api_error_for;
use super::data::{Resource, fetch_current};
use super::state::{
    AppState, current_generation, current_ready_generation, password_generation, ready_generation,
    set_status,
};
use crate::{
    api,
    models::{
        AdminCodes, BulkCandidate, BulkSummary, CodeCatalogue, CodeHistoryEntry, CodeRedemption,
        CodeSetting,
    },
};
use dioxus::prelude::*;
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};
use uuid::Uuid;

/// The message key of `initial_setup_required`: no verified HoYoLAB link.
const SETUP_REQUIRED: &str = "messages.initial_setup_is_required_before_continuing";
/// History rows per page of the Codes tab.
pub(super) const HISTORY_PAGE: usize = 20;
const DAY: i64 = 24 * 60 * 60;
/// The window of the redeemed, announced and unknown summary chips.
const CHIP_WINDOW: i64 = 7 * DAY;
/// The window of the dashboard's redeemed hint.
const HINT_WINDOW: i64 = DAY;

/// The Codes tab: the redemption state, the code list, the modes and the history, all
/// stored reads (never HoYoLAB), applied together so the tab never mixes two loads.
pub(super) async fn load_codes(mut state: Signal<AppState>) {
    let Some(generation) = ready_generation(state) else {
        return;
    };
    let Some(redemption) = fetch_current::<CodeRedemption>(
        state,
        Resource::CodeRedemption,
        "/code-redemption",
        generation,
        current_ready_generation,
    )
    .await
    else {
        return;
    };
    let Some(catalogue) = fetch_current::<CodeCatalogue>(
        state,
        Resource::CodeCatalogue,
        "/codes",
        generation,
        current_ready_generation,
    )
    .await
    else {
        return;
    };
    let Some(settings) = fetch_current::<Vec<CodeSetting>>(
        state,
        Resource::CodeSettings,
        "/code-settings",
        generation,
        current_ready_generation,
    )
    .await
    else {
        return;
    };
    let Some(history) = fetch_current::<Vec<CodeHistoryEntry>>(
        state,
        Resource::CodeHistory,
        "/code-history",
        generation,
        current_ready_generation,
    )
    .await
    else {
        return;
    };
    let mut write = state.write();
    let unlinked = [settings.as_ref().err(), history.as_ref().err()]
        .into_iter()
        .flatten()
        .any(|key| *key == SETUP_REQUIRED);
    match (redemption, catalogue, settings, history) {
        (Ok(redemption), Ok(catalogue), Ok(settings), Ok(history)) => {
            write.code_redemption = Some(redemption);
            write.code_catalogue = Some(catalogue);
            write.code_settings = Some(settings);
            write.code_history = Some(history);
            write.codes_error = None;
            write.codes_unlinked = false;
        }
        _ if unlinked => {
            write.code_settings = None;
            write.code_history = None;
            write.codes_error = None;
            write.codes_unlinked = true;
        }
        (redemption, catalogue, settings, history) => {
            let key = [
                redemption.err(),
                catalogue.err(),
                settings.err(),
                history.err(),
            ]
            .into_iter()
            .flatten()
            .next()
            .unwrap_or("codes.load_failed");
            write.code_settings = None;
            write.code_history = None;
            write.codes_error = Some(key.to_owned());
            write.codes_unlinked = false;
        }
    }
}

/// The dashboard hint's reads (the redemption state and the history). Quiet: a failure,
/// an unlinked account or a server without the feature only leaves the hint out.
pub(super) async fn load_code_hint(mut state: Signal<AppState>) {
    let Some(generation) = ready_generation(state) else {
        return;
    };
    let Some(redemption) = fetch_current::<CodeRedemption>(
        state,
        Resource::CodeRedemption,
        "/code-redemption",
        generation,
        current_ready_generation,
    )
    .await
    else {
        return;
    };
    // A failed read drops the earlier hint instead of keeping a state the server no
    // longer reports.
    let Ok(redemption) = redemption else {
        let mut write = state.write();
        write.code_redemption = None;
        write.code_history = None;
        return;
    };
    state.write().code_redemption = Some(redemption);
    let Some(history) = fetch_current::<Vec<CodeHistoryEntry>>(
        state,
        Resource::CodeHistory,
        "/code-history",
        generation,
        current_ready_generation,
    )
    .await
    else {
        return;
    };
    state.write().code_history = history.ok();
}

/// Stores one UID's mode. Call inside `spawn_action`; the server refuses `auto` without
/// the redemption Cookie, so the tab reloads either way.
pub(super) async fn save_code_mode(
    state: Signal<AppState>,
    generation: u64,
    id: Uuid,
    mode: String,
) {
    if !current_ready_generation(state, generation) {
        return;
    }
    let result = api::send(
        "PATCH",
        &format!("/code-settings/{id}"),
        Some(json!({ "mode": mode })),
    )
    .await;
    if !current_ready_generation(state, generation) {
        return;
    }
    match result {
        Ok(()) => set_status(state, "codes.mode_saved", "success"),
        Err(error) => handle_api_error_for(state, generation, error),
    }
    load_codes(state).await;
}

/// Enters a code for one UID of a game (`uid`) or all of them. True when the server
/// accepted it, so the form may clear. Call inside `spawn_action`.
pub(super) async fn enter_code(
    state: Signal<AppState>,
    generation: u64,
    game: String,
    code: String,
    uid: Option<Uuid>,
) -> bool {
    if !current_ready_generation(state, generation) {
        return false;
    }
    let mut body = json!({ "game_kind": game, "code": code });
    if let Some(uid) = uid {
        body["game_account_id"] = json!(uid);
    }
    let result = api::json::<serde_json::Value>("POST", "/code-redemption/codes", Some(body)).await;
    if !current_ready_generation(state, generation) {
        return false;
    }
    let accepted = result.is_ok();
    match result {
        Ok(_) => set_status(state, "codes.manual_accepted", "success"),
        Err(error) => handle_api_error_for(state, generation, error),
    }
    load_codes(state).await;
    accepted
}

/// Redeem now, which is also the explicit resume after a CAPTCHA. Call inside
/// `spawn_action`.
pub(super) async fn redeem_now(state: Signal<AppState>, generation: u64) {
    if !current_ready_generation(state, generation) {
        return;
    }
    let result =
        api::json::<serde_json::Value>("POST", "/code-redemption/run", Some(json!({}))).await;
    if !current_ready_generation(state, generation) {
        return;
    }
    match result {
        Ok(_) => set_status(state, "codes.redeem_now_accepted", "success"),
        Err(error) => handle_api_error_for(state, generation, error),
    }
    load_codes(state).await;
}

/// Queues every listed code once for the chosen UIDs outside `auto`, leaving their modes
/// as they are. The summary when the server accepted it. Call inside `spawn_action`: the
/// request only writes the queue (the run goes on in the background), so the busy shell
/// never waits for HoYoLAB.
pub(super) async fn redeem_listed(
    state: Signal<AppState>,
    generation: u64,
    ids: Vec<Uuid>,
) -> Option<BulkSummary> {
    if !current_ready_generation(state, generation) {
        return None;
    }
    let body = json!({ "game_account_ids": ids });
    let result = api::json::<BulkSummary>("POST", "/code-redemption/listed", Some(body)).await;
    if !current_ready_generation(state, generation) {
        return None;
    }
    let summary = match result {
        Ok(summary) => {
            set_status(state, "codes.bulk_accepted", "success");
            Some(summary)
        }
        Err(error) => {
            handle_api_error_for(state, generation, error);
            None
        }
    };
    load_codes(state).await;
    summary
}

/// Why "redeem all listed codes" cannot start, most fundamental first (the server refuses
/// the same cases); None when it can. `codes` is what the UIDs would queue in total.
pub(super) fn bulk_blocked(
    stopped: bool,
    credentials: bool,
    action: bool,
    codes: u64,
) -> Option<&'static str> {
    if stopped {
        Some("codes.manual_blocked_admin")
    } else if !credentials {
        Some("codes.manual_blocked_cookie")
    } else if action {
        Some("codes.manual_blocked_action")
    } else if codes == 0 {
        Some("codes.bulk_nothing")
    } else {
        None
    }
}

/// The UIDs the dialog starts with: every UID outside `auto` that has something to queue.
pub(super) fn bulk_preselected(candidates: &[BulkCandidate]) -> BTreeSet<Uuid> {
    candidates
        .iter()
        .filter(|candidate| candidate.codes > 0)
        .map(|candidate| candidate.game_account_id)
        .collect()
}

/// The chosen UIDs that have something to queue (sent to the server), and their codes.
pub(super) fn bulk_choice(
    candidates: &[BulkCandidate],
    chosen: &BTreeSet<Uuid>,
) -> (Vec<Uuid>, u64) {
    let picked: Vec<&BulkCandidate> = candidates
        .iter()
        .filter(|candidate| candidate.codes > 0 && chosen.contains(&candidate.game_account_id))
        .collect();
    (
        picked
            .iter()
            .map(|candidate| candidate.game_account_id)
            .collect(),
        picked.iter().map(|candidate| candidate.codes).sum(),
    )
}

pub(super) async fn load_admin_codes(mut state: Signal<AppState>) {
    let Some(generation) = password_generation(state) else {
        return;
    };
    if !state.read().me.as_ref().is_some_and(|me| me.is_admin) {
        return;
    }
    let Some(result) = fetch_current::<AdminCodes>(
        state,
        Resource::AdminCodes,
        "/admin/codes",
        generation,
        current_generation,
    )
    .await
    else {
        return;
    };
    let mut write = state.write();
    match result {
        Ok(view) => {
            write.admin_codes = Some(view);
            write.admin_codes_error = None;
        }
        Err(key) => {
            write.admin_codes = None;
            write.admin_codes_error = Some(key.to_owned());
        }
    }
}

/// Stores the server-wide switch and the fetch interval in minutes. Call inside
/// `spawn_action`.
pub(super) async fn save_admin_codes(
    state: Signal<AppState>,
    generation: u64,
    enabled: bool,
    interval_minutes: i32,
) {
    if !current_generation(state, generation) {
        return;
    }
    let body = json!({ "enabled": enabled, "interval_minutes": interval_minutes });
    let result = api::send("PUT", "/admin/codes", Some(body)).await;
    if !current_generation(state, generation) {
        return;
    }
    match result {
        Ok(()) => set_status(state, "admin.codes_saved", "success"),
        Err(error) => handle_api_error_for(state, generation, error),
    }
    load_admin_codes(state).await;
}

/// The fetch interval choices of the administrator's section, in hours; 3 is the default.
pub(super) const INTERVAL_HOURS: [i32; 6] = [1, 2, 3, 6, 12, 24];
pub(super) const DEFAULT_INTERVAL_HOURS: i32 = 3;

/// The stored interval as one of the choices: the nearest one at or above it, so a value
/// set through the API never shows as a shorter interval than it is.
pub(super) fn interval_choice(minutes: i32) -> i32 {
    INTERVAL_HOURS
        .into_iter()
        .find(|hours| hours * 60 >= minutes)
        .unwrap_or(24)
}

/// What a history row says, shared by the history, the last result and the chips.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum CodeResult {
    Redeemed,
    AlreadyUsed,
    Expired,
    Invalid,
    RankTooLow,
    CookieInvalid,
    Captcha,
    /// HoYoLAB did not find the UID on the Cookie's account: this UID waits.
    NotFound,
    Unknown,
    Pending,
    /// Notify only: announced, or listed with nothing done yet (`available`).
    Announced,
}

impl CodeResult {
    /// A history `state`. In-flight sends wait like pending ones; states this build does
    /// not know read as unknown, never as done.
    pub(super) fn from_state(state: &str) -> Self {
        match state {
            "redeemed" => Self::Redeemed,
            "already_used" => Self::AlreadyUsed,
            "expired" => Self::Expired,
            "invalid" => Self::Invalid,
            "rank_too_low" => Self::RankTooLow,
            "cookie_invalid" => Self::CookieInvalid,
            "captcha_required" | "captcha" => Self::Captcha,
            "no_game_account" => Self::NotFound,
            "pending" | "sending" => Self::Pending,
            "announced" | "available" => Self::Announced,
            _ => Self::Unknown,
        }
    }
}

/// One code on one UID with its latest result, as the Codes tab lists it.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct CodeRow {
    pub(super) game_account_id: Uuid,
    pub(super) game_kind: String,
    pub(super) game_uid: String,
    pub(super) code: String,
    /// The source's reward text (empty when it has none, or for entered codes).
    pub(super) rewards: String,
    pub(super) manual: bool,
    pub(super) result: CodeResult,
    /// The server stored this result (false for `available` and for codes rejected for
    /// everyone before this UID sent them).
    pub(super) stored: bool,
    /// The result time (or, for a listed code without a result, when it was first seen).
    pub(super) at: Option<String>,
    /// Seconds since `at`, by the device clock.
    pub(super) age: Option<i64>,
    pub(super) next_attempt_at: Option<String>,
    pub(super) redeem_url: Option<String>,
}

/// The history as rows, newest first. A listed code without a result shows only for UIDs
/// that are not off (the server lists it for every UID); its time is when it was first seen.
pub(super) fn code_rows(
    history: &[CodeHistoryEntry],
    catalogue: Option<&CodeCatalogue>,
    settings: &[CodeSetting],
    age: &dyn Fn(&str) -> Option<i64>,
) -> Vec<CodeRow> {
    let codes: BTreeMap<Uuid, (&str, &str)> = catalogue
        .map(|catalogue| {
            catalogue
                .codes
                .iter()
                .map(|code| {
                    (
                        code.id,
                        (code.rewards.as_str(), code.first_seen_at.as_str()),
                    )
                })
                .collect()
        })
        .unwrap_or_default();
    let off = |id: Uuid| {
        settings
            .iter()
            .find(|setting| setting.game_account_id == id)
            .is_none_or(|setting| setting.mode == "off")
    };
    let mut rows: Vec<CodeRow> = history
        .iter()
        .filter(|entry| entry.state != "available" || !off(entry.game_account_id))
        .map(|entry| {
            let listed = entry.code_id.and_then(|id| codes.get(&id)).copied();
            let at = entry
                .updated_at
                .clone()
                .or_else(|| listed.map(|(_, seen)| seen.to_owned()));
            CodeRow {
                game_account_id: entry.game_account_id,
                game_kind: entry.game_kind.clone(),
                game_uid: entry.game_uid.clone(),
                code: entry.code.clone(),
                rewards: listed
                    .map(|(rewards, _)| rewards.to_owned())
                    .unwrap_or_default(),
                manual: entry.source == "manual",
                result: CodeResult::from_state(&entry.state),
                stored: entry.updated_at.is_some(),
                age: at.as_deref().and_then(age),
                at,
                next_attempt_at: entry.next_attempt_at.clone(),
                redeem_url: entry.redeem_url.clone(),
            }
        })
        .collect();
    // Newest first; rows without a time last, then by code for a stable order.
    rows.sort_by(|a, b| {
        a.age
            .unwrap_or(i64::MAX)
            .cmp(&b.age.unwrap_or(i64::MAX))
            .then_with(|| a.code.cmp(&b.code))
            .then_with(|| a.game_uid.cmp(&b.game_uid))
    });
    rows
}

/// The latest stored result of a UID for its last result line; pending codes are not
/// a result yet.
pub(super) fn last_result(rows: &[CodeRow], uid: Uuid) -> Option<&CodeRow> {
    rows.iter()
        .find(|row| row.game_account_id == uid && row.stored && row.result != CodeResult::Pending)
}

/// The summary chips of one game card, across that game's UIDs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct ChipCounts {
    /// Redeemed within 7 days.
    pub(super) done: usize,
    /// Notify only within 7 days.
    pub(super) open: usize,
    /// Pending or rank too low, any age.
    pub(super) waiting: usize,
    /// Cookie invalid or CAPTCHA (any age) and unknown within 7 days.
    pub(super) warn: usize,
}

pub(super) fn chip_counts(rows: &[CodeRow], game: &str) -> ChipCounts {
    let recent = |row: &CodeRow| row.age.is_some_and(|age| age <= CHIP_WINDOW);
    let mut counts = ChipCounts::default();
    for row in rows.iter().filter(|row| row.game_kind == game) {
        match row.result {
            CodeResult::Redeemed if recent(row) => counts.done += 1,
            CodeResult::Announced if recent(row) => counts.open += 1,
            CodeResult::Pending | CodeResult::RankTooLow => counts.waiting += 1,
            CodeResult::CookieInvalid | CodeResult::Captcha | CodeResult::NotFound => {
                counts.warn += 1
            }
            CodeResult::Unknown if recent(row) => counts.warn += 1,
            _ => {}
        }
    }
    counts
}

/// The one dashboard chip, or none on a quiet day.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum CodeHint {
    /// Redemption waits for the user (Cookie or CAPTCHA); wins over everything else.
    Action,
    /// Codes redeemed in the last 24 hours.
    Redeemed(usize),
}

pub(super) fn code_hint(
    redemption: Option<&CodeRedemption>,
    history: &[CodeHistoryEntry],
    age: &dyn Fn(&str) -> Option<i64>,
) -> Option<CodeHint> {
    if redemption.is_some_and(|state| state.action_required.is_some()) {
        return Some(CodeHint::Action);
    }
    let redeemed = history
        .iter()
        .filter(|entry| CodeResult::from_state(&entry.state) == CodeResult::Redeemed)
        .filter(|entry| {
            entry
                .updated_at
                .as_deref()
                .and_then(age)
                .is_some_and(|age| age <= HINT_WINDOW)
        })
        .count();
    (redeemed > 0).then_some(CodeHint::Redeemed(redeemed))
}

/// A code as typed into the entry form, upper-cased, or why it cannot be sent: letters
/// and digits only, 6 to 24 of them (the form's hint; the server accepts a wider range).
pub(super) fn manual_code(raw: &str) -> Option<String> {
    let code = raw.trim();
    ((6..=24).contains(&code.len()) && code.bytes().all(|byte| byte.is_ascii_alphanumeric()))
        .then(|| code.to_ascii_uppercase())
}

/// The source's reward text for display: `;` separates rewards and `*` the count.
pub(super) fn reward_text(rewards: &str) -> String {
    rewards
        .split(';')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(|part| part.replace('*', " ×"))
        .collect::<Vec<_>>()
        .join(" · ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::CatalogueCode;

    fn uuid(n: u128) -> Uuid {
        Uuid::from_u128(n)
    }

    /// Synthetic times are <seconds ago> so the age is the number itself.
    fn age(value: &str) -> Option<i64> {
        value.parse().ok()
    }

    fn entry(uid: u128, game: &str, code: &str, state: &str, at: Option<i64>) -> CodeHistoryEntry {
        CodeHistoryEntry {
            game_account_id: uuid(uid),
            game_kind: game.to_owned(),
            game_uid: format!("80000000{uid}"),
            code_id: None,
            code: code.to_owned(),
            source: "catalogue".to_owned(),
            state: state.to_owned(),
            retcode: None,
            updated_at: at.map(|at| at.to_string()),
            next_attempt_at: None,
            redeem_url: Some(format!("https://example.invalid/{code}")),
        }
    }

    fn setting(uid: u128, game: &str, mode: &str) -> CodeSetting {
        CodeSetting {
            game_account_id: uuid(uid),
            game_kind: game.to_owned(),
            game_uid: format!("80000000{uid}"),
            region: "os_asia".to_owned(),
            mode: mode.to_owned(),
        }
    }

    #[test]
    fn history_states_map_to_results_and_never_read_as_done_by_default() {
        for (state, result) in [
            ("redeemed", CodeResult::Redeemed),
            ("already_used", CodeResult::AlreadyUsed),
            ("expired", CodeResult::Expired),
            ("invalid", CodeResult::Invalid),
            ("rank_too_low", CodeResult::RankTooLow),
            ("cookie_invalid", CodeResult::CookieInvalid),
            ("captcha_required", CodeResult::Captcha),
            ("no_game_account", CodeResult::NotFound),
            ("unknown", CodeResult::Unknown),
            ("pending", CodeResult::Pending),
            ("sending", CodeResult::Pending),
            ("announced", CodeResult::Announced),
            ("available", CodeResult::Announced),
            ("synthetic_state", CodeResult::Unknown),
        ] {
            assert_eq!(CodeResult::from_state(state), result, "{state}");
        }
    }

    #[test]
    fn rows_join_rewards_sort_newest_first_and_hide_listed_codes_of_uids_that_are_off() {
        let listed = uuid(500);
        let catalogue = CodeCatalogue {
            enabled: true,
            last_success_at: None,
            consecutive_failures: 0,
            next_fetch_at: None,
            codes: vec![CatalogueCode {
                id: listed,
                game_kind: "genshin".to_owned(),
                code: "LISTEDCODE".to_owned(),
                rewards: "Primogem*60;Mora*10000".to_owned(),
                first_seen_at: "50".to_owned(),
            }],
        };
        let mut available_on = entry(1, "genshin", "LISTEDCODE", "available", None);
        available_on.code_id = Some(listed);
        let mut available_off = entry(2, "genshin", "LISTEDCODE", "available", None);
        available_off.code_id = Some(listed);
        let mut manual = entry(1, "genshin", "TYPEDCODE", "invalid", Some(10));
        manual.source = "manual".to_owned();
        let history = vec![
            entry(1, "genshin", "OLDCODE", "redeemed", Some(900)),
            available_on,
            available_off,
            manual,
            entry(1, "genshin", "DEADCODE", "expired", None),
        ];
        let settings = [
            setting(1, "genshin", "notify"),
            setting(2, "genshin", "off"),
        ];
        let rows = code_rows(&history, Some(&catalogue), &settings, &age);
        let order: Vec<_> = rows
            .iter()
            .map(|row| (row.code.as_str(), row.game_account_id))
            .collect();
        assert_eq!(
            order,
            [
                ("TYPEDCODE", uuid(1)),
                ("LISTEDCODE", uuid(1)),
                ("OLDCODE", uuid(1)),
                ("DEADCODE", uuid(1)),
            ]
        );
        assert!(rows[0].manual && rows[0].rewards.is_empty());
        assert_eq!(rows[1].rewards, "Primogem*60;Mora*10000");
        assert_eq!(rows[1].at.as_deref(), Some("50"));
        assert!(!rows[1].stored && rows[2].stored && !rows[3].stored);
        // The last result skips pending rows and rows the server never stored.
        let mut pending = entry(1, "genshin", "NEWCODE", "pending", Some(1));
        pending.code_id = None;
        let mut with_pending = history.clone();
        with_pending.push(pending);
        let rows = code_rows(&with_pending, Some(&catalogue), &settings, &age);
        assert_eq!(rows[0].code, "NEWCODE");
        assert_eq!(
            last_result(&rows, uuid(1)).map(|row| row.code.as_str()),
            Some("TYPEDCODE")
        );
        assert!(last_result(&rows, uuid(2)).is_none());
    }

    #[test]
    fn chips_count_each_game_across_its_uids_with_their_windows() {
        let week = 7 * 24 * 3600;
        let history = vec![
            entry(1, "genshin", "A", "redeemed", Some(60)),
            entry(2, "genshin", "B", "redeemed", Some(week + 1)),
            entry(1, "genshin", "C", "announced", Some(week)),
            entry(2, "genshin", "C", "announced", Some(60)),
            entry(1, "genshin", "D", "pending", Some(week * 4)),
            entry(2, "genshin", "E", "rank_too_low", Some(week * 4)),
            entry(1, "genshin", "F", "cookie_invalid", Some(week * 4)),
            entry(1, "genshin", "G", "captcha_required", Some(week * 4)),
            entry(2, "genshin", "H", "unknown", Some(60)),
            entry(2, "genshin", "I", "unknown", Some(week + 1)),
            entry(1, "genshin", "J", "expired", Some(60)),
            entry(1, "genshin", "K", "already_used", Some(60)),
            entry(1, "genshin", "L", "invalid", Some(60)),
            entry(3, "starrail", "M", "redeemed", Some(60)),
        ];
        let settings = [
            setting(1, "genshin", "auto"),
            setting(2, "genshin", "notify"),
            setting(3, "starrail", "auto"),
        ];
        let rows = code_rows(&history, None, &settings, &age);
        assert_eq!(
            chip_counts(&rows, "genshin"),
            ChipCounts {
                done: 1,
                open: 2,
                waiting: 2,
                warn: 3
            }
        );
        assert_eq!(
            chip_counts(&rows, "starrail"),
            ChipCounts {
                done: 1,
                ..ChipCounts::default()
            }
        );
        assert_eq!(chip_counts(&rows, "zzz"), ChipCounts::default());
    }

    #[test]
    fn missing_uids_count_as_warnings() {
        let mut answered = entry(1, "starrail", "NOTFOUND", "no_game_account", Some(60));
        answered.retcode = Some(-1073);
        let settings = [setting(1, "starrail", "auto")];
        let rows = code_rows(&[answered], None, &settings, &|value: &str| {
            value.parse().ok()
        });
        assert_eq!(rows[0].result, CodeResult::NotFound);
        assert_eq!(chip_counts(&rows, "starrail").warn, 1);
    }

    #[test]
    fn the_dashboard_hint_is_the_action_or_recent_redemptions_or_nothing() {
        let state = |action: Option<&str>| CodeRedemption {
            available: true,
            redemption_credentials: true,
            action_required: action.map(str::to_owned),
            action_at: None,
            action_retcode: None,
            cookie_verified_at: None,
            action_after_cookie_update: false,
            pending: 0,
            bulk: Vec::new(),
        };
        let history = vec![
            entry(1, "genshin", "A", "redeemed", Some(60)),
            entry(3, "starrail", "B", "redeemed", Some(24 * 3600)),
            entry(1, "genshin", "C", "redeemed", Some(24 * 3600 + 1)),
            entry(1, "genshin", "D", "announced", Some(60)),
        ];
        assert_eq!(
            code_hint(Some(&state(Some("captcha_required"))), &history, &age),
            Some(CodeHint::Action)
        );
        assert_eq!(
            code_hint(Some(&state(None)), &history, &age),
            Some(CodeHint::Redeemed(2))
        );
        assert_eq!(code_hint(None, &history, &age), Some(CodeHint::Redeemed(2)));
        assert_eq!(code_hint(Some(&state(None)), &history[2..], &age), None);
        assert_eq!(code_hint(None, &[], &age), None);
    }

    fn candidate(n: u128, codes: u64) -> BulkCandidate {
        BulkCandidate {
            game_account_id: uuid(n),
            game_kind: "genshin".to_owned(),
            game_uid: format!("80000000{n}"),
            region: "os_asia".to_owned(),
            mode: "notify".to_owned(),
            codes,
        }
    }

    #[test]
    fn listed_codes_wait_for_the_switch_the_cookie_the_action_and_something_to_send() {
        assert_eq!(
            bulk_blocked(true, false, true, 0),
            Some("codes.manual_blocked_admin")
        );
        assert_eq!(
            bulk_blocked(false, false, true, 3),
            Some("codes.manual_blocked_cookie")
        );
        assert_eq!(
            bulk_blocked(false, true, true, 3),
            Some("codes.manual_blocked_action")
        );
        assert_eq!(
            bulk_blocked(false, true, false, 0),
            Some("codes.bulk_nothing")
        );
        assert_eq!(bulk_blocked(false, true, false, 1), None);
    }

    #[test]
    fn the_dialog_preselects_uids_with_codes_and_sends_only_chosen_ones_with_codes() {
        let candidates = [candidate(1, 3), candidate(2, 0), candidate(3, 2)];
        let preselected = bulk_preselected(&candidates);
        assert_eq!(preselected, BTreeSet::from([uuid(1), uuid(3)]));
        assert_eq!(
            bulk_choice(&candidates, &preselected),
            (vec![uuid(1), uuid(3)], 5)
        );
        // A UID with nothing to send is never sent, even if chosen; deselecting drops one.
        let chosen = BTreeSet::from([uuid(2), uuid(3)]);
        assert_eq!(bulk_choice(&candidates, &chosen), (vec![uuid(3)], 2));
        assert_eq!(bulk_choice(&candidates, &BTreeSet::new()), (Vec::new(), 0));
    }

    #[test]
    fn servers_without_the_listed_counts_still_parse() {
        let state: CodeRedemption = serde_json::from_value(serde_json::json!({
            "available": true, "redemption_credentials": true, "action_required": null,
            "action_at": null, "pending": 0
        }))
        .unwrap();
        assert!(state.bulk.is_empty());
        let summary: BulkSummary = serde_json::from_value(serde_json::json!({
            "accounts": [], "queued": 2, "already_recorded": 1, "rejected": 0
        }))
        .unwrap();
        assert_eq!((summary.queued, summary.already_recorded), (2, 1));
    }

    #[test]
    fn entered_codes_are_letters_and_digits_upper_cased() {
        assert_eq!(manual_code(" genshingift ").as_deref(), Some("GENSHINGIFT"));
        assert_eq!(manual_code("ABC123").as_deref(), Some("ABC123"));
        assert_eq!(
            manual_code(&"A".repeat(24)).as_deref(),
            Some(&*"A".repeat(24))
        );
        for bad in [
            "",
            "ABC12",
            "GENSHIN-GIFT",
            "GENSHIN GIFT",
            "ＧＥＮＳＨＩＮ",
            &"A".repeat(25),
        ] {
            assert_eq!(manual_code(bad), None, "{bad}");
        }
    }

    #[test]
    fn reward_text_separates_rewards_and_counts() {
        assert_eq!(
            reward_text("Primogem*60;Hero's Wit*3"),
            "Primogem ×60 · Hero's Wit ×3"
        );
        assert_eq!(reward_text(" Primogem*30 ; "), "Primogem ×30");
        assert_eq!(reward_text(""), "");
    }

    #[test]
    fn stored_intervals_show_as_the_nearest_choice_not_shorter() {
        assert_eq!(interval_choice(180), 3);
        assert_eq!(interval_choice(60), 1);
        assert_eq!(interval_choice(90), 2);
        assert_eq!(interval_choice(1440), 24);
        assert_eq!(interval_choice(5000), 24);
        assert!(INTERVAL_HOURS.contains(&DEFAULT_INTERVAL_HOURS));
    }
}
