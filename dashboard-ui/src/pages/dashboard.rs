use super::cap_history::CapHistory;
use super::challenges::ChallengeSection;
use super::common::{
    LoadError, LoadingLine, Metric, WarningLine, game_name, pending, refresh_problem,
};
use super::expeditions::ExpeditionList;
use super::game_tabs::{GameTabs, game_index, select_game_index, use_narrow_games};
use super::icons::{Glyph, Icon, Status, StatusMark};
use super::rewards::AutomationSwitch;
use super::status_tiles::{GenshinTiles, StarRailTiles};
use super::swipe_tabs::SwipeableTabs;
use crate::app::actions::spawn_root;
use crate::app::data::{RunAll, load_game, run_all_targets, run_rewards_now};
use crate::app::display::{
    compact_datetime, data_age, datetime_text, error_text, fill_level, hours_minutes, local_moment,
    relative_age, seconds_since, short_time,
};
use crate::app::preferences::{Preferences, use_preferences};
use crate::app::state::{AppState, DASHBOARD_GAMES, Page, set_page};
use crate::models::{
    DailyAward, DailySummary, GameChallenges, GameStatus, ResourceHistory, RewardSetting,
};
use dioxus::prelude::*;
use uuid::Uuid;

#[component]
pub(super) fn DashboardPage(state: Signal<AppState>) -> Element {
    let ui = use_preferences();
    let active = use_memo(move || state.read().dashboard_game);
    let mobile = use_narrow_games();
    use_clock_provider();
    rsx! {
        div { class: "page-stack dashboard",
            {pull_refresh(state, Page::Dashboard)}
            div { class: "page-intro",
                h1 { {ui.text("dashboard.dashboard")} }
                ClaimNow { state }
            }
            if state.read().accounts_error.is_some() {
                LoadError { state, message: ui.text("dashboard.could_not_refresh_the_uid_list") }
            }
            GameTabs { state, prefix: "game" }
            // Narrow screens show one game at a time: swiping the panels switches tabs like a tap.
            SwipeableTabs {
                id: "game-panels", class: "dashboard-grid",
                count: DASHBOARD_GAMES.len(), index: game_index(active()), enabled: mobile(),
                on_change: move |index: usize| select_game_index(state, index),
                for game in DASHBOARD_GAMES {
                    GamePanel { key: "{game}", state, game, active: active() == game, mobile: mobile() }
                }
            }
        }
    }
}

/// The one manual claim control: runs every game whose bonus is still open today.
#[component]
fn ClaimNow(state: Signal<AppState>) -> Element {
    let ui = use_preferences();
    let snapshot = state.read();
    let posting = snapshot
        .me
        .as_ref()
        .is_some_and(|me| me.capabilities.reward_posting_available);
    let targets = run_all_targets(&snapshot.rewards, posting);
    let following = snapshot
        .reward_poll
        .as_ref()
        .is_some_and(|poll| poll.page == Page::Dashboard);
    let loaded = snapshot.rewards_error.is_none();
    let today = claim_summary(&snapshot.rewards);
    drop(snapshot);
    let blocked = match &targets {
        RunAll::Ready(_) => None,
        RunAll::Blocked(key) => Some(*key),
    };
    rsx! {
        div { class: "claim-now",
            // Today's claim per game beside the control that claims it; the bonus section at
            // the bottom of each card keeps the details.
            if loaded && !today.is_empty() {
                ul { class: "claim-chips", id: "claim-summary", "aria-label": ui.text("dashboard.today_s_bonus"),
                    for (game, signed) in today {
                        li { class: "claim-chip {game}", "data-game": game, "data-state": claim_tone(signed),
                            if let Some(mark) = Icon::game_mark(game) { Glyph { icon: mark, class: "game-mark" } }
                            span { class: "visually-hidden", {game_name(game, ui)} " " }
                            StatusMark { status: claim_status(signed), text: received(signed, ui) }
                        }
                    }
                }
            }
            button {
                r#type: "button", class: "primary", id: "claim-now",
                disabled: blocked.is_some() || !loaded,
                "aria-describedby": blocked.map(|_| "claim-now-hint"),
                onclick: move |_| run_rewards_now(state, Page::Dashboard),
                Glyph { icon: Icon::Gift }
                span { {ui.text("rewards.claim_now")} }
                span { class: "visually-hidden", {ui.text("rewards.claim_now_suffix")} }
            }
            if following {
                p { class: "field-hint", role: "status", {ui.text("rewards.checking_automatically")} }
            } else if let Some(key) = blocked.filter(|_| loaded) {
                // The chips already show that every game is claimed; that reason stays as
                // the button's description only.
                p { id: "claim-now-hint", class: if key == "rewards.all_claimed_today" { "visually-hidden" } else { "field-hint" }, {ui.text(key)} }
            }
        }
    }
}

/// Today's claim state per dashboard game, in tab order, for the games with a setting.
fn claim_summary(settings: &[RewardSetting]) -> Vec<(&'static str, Option<bool>)> {
    DASHBOARD_GAMES
        .iter()
        .filter_map(|game| {
            settings
                .iter()
                .find(|setting| setting.game_kind == *game)
                .map(|setting| (*game, setting.is_sign))
        })
        .collect()
}

/// The symbol of a claim state: claimed, still open, or unknown.
fn claim_status(signed: Option<bool>) -> Status {
    match signed {
        Some(true) => Status::Done,
        Some(false) => Status::Open,
        None => Status::Unknown,
    }
}

/// The status colour of a claim state: claimed, still open, or unknown.
fn claim_tone(signed: Option<bool>) -> &'static str {
    match signed {
        Some(true) => "success",
        Some(false) => "pending",
        None => "",
    }
}

/// Card order puts what players check first at the top: the game mark and name with the
/// age of the data, the UID, the primary resource with its full-recovery estimate and its
/// time at cap over 7 days, the icon tiles, expeditions, the endgame tiles, the login
/// bonus, and the one collapsed details block last. Events and banners are not tied to
/// a UID and live on the events page (`events.rs`).
#[component]
fn GamePanel(state: Signal<AppState>, game: &'static str, active: bool, mobile: bool) -> Element {
    let ui = use_preferences();
    rsx! {
        section {
            class: "panel game-panel {game}", id: "game-panel-{game}",
            role: if mobile { "tabpanel" } else { "region" },
            "aria-labelledby": if mobile { format!("game-tab-{game}") } else { format!("game-title-{game}") },
            "data-selected": active, hidden: mobile && !active,
            tabindex: if mobile { "0" } else { "-1" },
            // The age of the shown status sits beside the control that renews it.
            div { class: "game-heading card-heading",
                h2 { class: "game-title", id: "game-title-{game}",
                    if let Some(mark) = Icon::game_mark(game) { Glyph { icon: mark, class: "game-mark" } }
                    span { {game_name(game, ui)} }
                }
                StatusFreshness { state, game }
                // The app refreshes by pulling the dashboard down instead.
                if !crate::platform::APP { RefreshButton { state, game } }
            }
            GameStatusSection { state, game }
            ChallengeSection { state, game }
            BonusSection { state, game }
            CardDetails { state, game }
        }
    }
}

/// UID choice, the selected account's status and its refresh control.
#[component]
fn GameStatusSection(mut state: Signal<AppState>, game: &'static str) -> Element {
    let ui = use_preferences();
    let snapshot = state.read();
    let visible: Vec<_> = snapshot
        .visible_accounts()
        .into_iter()
        .filter(|account| account.game_kind == game)
        .collect();
    let selected = snapshot.selected_game_account(game);
    let selection = snapshot
        .selected_accounts
        .get(game)
        .cloned()
        .unwrap_or_default();
    let status: Option<GameStatus> = selected.and_then(|id| {
        snapshot
            .account_statuses
            .iter()
            .find(|entry| entry.id == id)
            .map(|entry| entry.status.clone())
    });
    let history: Option<ResourceHistory> = selected.and_then(|id| {
        snapshot
            .account_histories
            .iter()
            .find(|entry| entry.id == id)
            .map(|entry| entry.history.clone())
    });
    let accounts_loaded = snapshot.accounts.is_some();
    let accounts_failed = snapshot.accounts_error.is_some();
    let loading = pending(
        selection.loading && selection.generation == snapshot.generation,
        status.is_some(),
        selection.error.is_some(),
    );
    drop(snapshot);
    rsx! {
        if !accounts_loaded {
            // A failed first load shows the page-level warning instead.
            if !accounts_failed { LoadingLine {} }
        } else if visible.is_empty() {
            div { class: "empty",
                h3 { {ui.text("dashboard.no_visible_uids")} }
                button { r#type: "button", onclick: move |_| set_page(state, Page::Uids), {ui.text("dashboard.choose_uids")} }
            }
        } else {
            if let [only] = visible.as_slice() {
                // Nothing to choose: the UID is plain text, its server is in the details.
                p { class: "uid-one",
                    span { class: "visually-hidden", {ui.text("dashboard.selected_uid")} " " }
                    {ui.format("dashboard.uid_only", &[("uid", &only.game_uid)])}
                }
            } else {
                div { class: "uid-select",
                    label { class: "visually-hidden", r#for: "selected-{game}", {ui.text("dashboard.selected_uid")} }
                    select {
                        id: "selected-{game}", value: selected.map(|id| id.to_string()).unwrap_or_default(),
                        onchange: move |event| {
                            if let Ok(id) = event.value().parse::<Uuid>() {
                                state.write().select_game_account(game, Some(id));
                                spawn_root(async move { load_game(state, game).await; });
                            }
                        },
                        for account in visible {
                            option { value: "{account.id}", {ui.format("dashboard.uid_option", &[("region", &account.region), ("uid", &account.game_uid)])} }
                        }
                    }
                }
            }
            if let Some(key) = selection.error {
                // Without a payload there is nothing that could be outdated.
                WarningLine {
                    status: if status.is_some() { Status::Warn } else { Status::Failed },
                    text: ui.text(refresh_problem(key, status.is_some(), "dashboard.status_fetch_failed")),
                }
            }
            div { "aria-busy": loading,
                if let Some(status) = status.clone() {
                    GameStatusCard { status, history, history_error: selection.history_error }
                } else if loading {
                    LoadingLine {}
                }
            }
        }
    }
}

/// Age of the selected UID's status, beside the game title.
#[component]
fn StatusFreshness(state: Signal<AppState>, game: &'static str) -> Element {
    let snapshot = state.read();
    let status = snapshot.selected_game_account(game).and_then(|id| {
        snapshot
            .account_statuses
            .iter()
            .find(|entry| entry.id == id)
            .map(|entry| (entry.status.fetched_at.clone(), entry.status.stale))
    });
    drop(snapshot);
    let (fetched_at, stale) = status.unwrap_or_default();
    rsx! { Freshness { fetched_at, stale, short: true } }
}

/// Web refresh control: a circular-arrow icon button at the card's top right.
#[component]
fn RefreshButton(state: Signal<AppState>, game: &'static str) -> Element {
    let ui = use_preferences();
    let snapshot = state.read();
    let shown = snapshot.accounts.is_some() && snapshot.selected_game_account(game).is_some();
    // Busy until the status, the cap history and the endgame summary of this refresh are back.
    let loading = snapshot
        .selected_accounts
        .get(game)
        .is_some_and(|selection| {
            (selection.loading && selection.generation == snapshot.generation)
                || selection.challenges_loading
                || selection.history_loading
        });
    drop(snapshot);
    let label = ui.text(if loading {
        "dashboard.refreshing"
    } else {
        "dashboard.refresh"
    });
    rsx! {
        if shown {
            // While loading the button stays enabled (aria-disabled): disabling it would drop
            // keyboard focus to the page. Repeated presses are ignored instead.
            button {
                r#type: "button", class: "icon-button refresh-button", id: "refresh-{game}",
                "aria-disabled": loading, "aria-busy": loading, "aria-label": label, title: label,
                onclick: move |_| { if !loading { spawn_root(async move { load_game(state, game).await; }); } },
                Glyph { icon: Icon::Refresh }
            }
        }
    }
}

/// App only: pulling the page down at its top reloads it (the dashboard: UIDs, status and
/// bonus settings; the events page: UIDs and calendars).
#[cfg(feature = "mobile")]
pub(super) fn pull_refresh(state: Signal<AppState>, page: Page) -> Element {
    rsx! { PullRefresh { state, page } }
}

#[cfg(not(feature = "mobile"))]
pub(super) fn pull_refresh(_state: Signal<AppState>, _page: Page) -> Element {
    rsx! {}
}

#[cfg(feature = "mobile")]
#[component]
fn PullRefresh(state: Signal<AppState>, page: Page) -> Element {
    use crate::app::data::reload_page;
    let ui = use_preferences();
    let mut refreshing = use_signal(|| false);
    let _pull = use_hook(move || {
        std::rc::Rc::new(crate::mobile::dom::PullToRefresh::attach(move || {
            if *refreshing.peek() {
                return;
            }
            refreshing.set(true);
            spawn_root(async move {
                reload_page(state, page).await;
                refreshing.set(false);
            });
        }))
    });
    rsx! {
        div { id: "pull-refresh", class: "pull-refresh", "data-refreshing": refreshing(), "aria-hidden": "true",
            Glyph { icon: Icon::Refresh }
        }
        p { class: "visually-hidden", role: "status",
            if refreshing() { {ui.text("dashboard.refreshing")} }
        }
    }
}

/// Always-visible age of the shown data, kept current by the dashboard clock. The short
/// form (game cards) is the clock icon with "12 min ago"; outdated data swaps in the
/// warning symbol. The full sentence stays for screen readers.
#[component]
pub(super) fn Freshness(
    fetched_at: Option<String>,
    stale: bool,
    #[props(default)] short: bool,
) -> Element {
    let ui = use_preferences();
    let clock = use_clock();
    let _ = clock();
    let age = seconds_since(fetched_at.as_deref()).map(|seconds| relative_age(seconds, ui));
    let sentence = age.as_ref().map(|age| {
        let key = if short && stale {
            "dashboard.updated_ago_stale"
        } else {
            "dashboard.updated_ago"
        };
        ui.format(key, &[("age", age)])
    });
    rsx! {
        if let (Some(age), Some(sentence)) = (age, sentence) {
            p { class: if stale { "freshness stale" } else { "freshness" },
                if short {
                    Glyph { icon: if stale { Icon::Warning } else { Icon::Clock } }
                    span { class: "visually-hidden", "{sentence}" }
                    time { datetime: fetched_at.clone(), title: compact_datetime(fetched_at.as_deref()), "aria-hidden": "true", "{age}" }
                } else {
                    time { datetime: fetched_at, "{sentence}" }
                }
            }
        } else {
            // Keeps the heading's columns in place until the status arrives.
            span { class: "freshness" }
        }
    }
}

/// Today's claim, this month's count and reward, then the automatic claim switch with its
/// next run. The heading is the gift icon; its text is for screen readers.
#[component]
fn BonusSection(state: Signal<AppState>, game: &'static str) -> Element {
    let ui = use_preferences();
    let snapshot = state.read();
    let setting = snapshot
        .rewards
        .iter()
        .find(|setting| setting.game_kind == game)
        .cloned();
    let rewards_error = snapshot.rewards_error.is_some();
    drop(snapshot);
    rsx! {
        section { class: "bonus-section", "aria-labelledby": "bonus-title-{game}",
            h3 { class: "visually-hidden", id: "bonus-title-{game}", {ui.text("dashboard.login_bonus")} }
            if rewards_error {
                LoadError { state, message: ui.text("dashboard.could_not_load_bonus_settings") }
            }
            if let Some(setting) = setting {
                BonusToday { state, setting, disabled: rewards_error }
            } else if !rewards_error {
                div { class: "bonus-main",
                    Glyph { icon: Icon::Gift, class: "icon-lg" }
                    StatusMark { status: Status::Unknown, text: ui.text("common.not_fetched") }
                }
            }
        }
    }
}

#[component]
fn BonusToday(state: Signal<AppState>, setting: RewardSetting, disabled: bool) -> Element {
    let ui = use_preferences();
    let clock = use_clock();
    let _ = clock();
    let needs_link = matches!(
        setting.auth_status.as_str(),
        "action_required" | "cookie_invalid"
    );
    let next_in = setting
        .next_run_at
        .as_deref()
        .and_then(|next| seconds_since(Some(next)))
        .filter(|elapsed| *elapsed < 0)
        .map(|elapsed| hours_minutes(-elapsed, ui));
    let next_at = local_moment(setting.next_run_at.as_deref());
    let game = setting.game_kind.clone();
    rsx! {
        div { class: "bonus-main",
            Glyph { icon: Icon::Gift, class: "icon-lg" }
            span { class: "bonus-today", "data-state": claim_tone(setting.is_sign),
                span { class: "visually-hidden", {ui.text("dashboard.today_s_reward")} " " }
                StatusMark { status: claim_status(setting.is_sign), text: received(setting.is_sign, ui) }
            }
            if let Some(days) = setting.total_sign_day {
                span { class: "bonus-day", {ui.format("dashboard.month_sign_days", &[("count", &days.to_string())])} }
            }
            if let Some(award) = setting.today_award.clone() {
                AwardLine { award, claimed: setting.is_sign == Some(true) }
            }
        }
        div { class: "bonus-auto",
            AutomationSwitch { state, setting: setting.clone(), id: "automation-{game}", disabled, caption: "dashboard.auto" }
            // No line while it is off: the switch says so.
            if setting.enabled {
                span { class: "bonus-next",
                    Glyph { icon: Icon::Clock }
                    span { class: "visually-hidden", {ui.text("dashboard.next_run")} " " }
                    if let Some(next_at) = next_at {
                        // The run is daily at the same time, so the clock alone is enough.
                        time { datetime: setting.next_run_at.clone(), title: compact_datetime(setting.next_run_at.as_deref()), "{next_at.clock}" }
                    } else {
                        span { class: "missing", {ui.text("common.not_fetched")} }
                    }
                    // The countdown moves to the next line as a whole rather than splitting "13h 05m".
                    if let Some(next_in) = next_in { " · " span { class: "inline-unit", {ui.format("dashboard.in_duration", &[("duration", &next_in)])} } }
                }
            }
        }
        if needs_link {
            div { class: "notice warn bonus-attention", role: "status",
                {ui.text(if setting.auth_status == "action_required" { "dashboard.bonus_action_required" } else { "errors.cookie_invalid" })}
                p { button { r#type: "button", onclick: move |_| set_page(state, Page::Link), {ui.text("dashboard.open_link_settings")} } }
            }
        }
    }
}

/// The reward of the day: the official image from the URL HoYoLAB returned (dropped if
/// it fails to load), its name and count. Whether it is today's or the next one is read out.
#[component]
fn AwardLine(award: DailyAward, claimed: bool) -> Element {
    let ui = use_preferences();
    let mut failed = use_signal(|| false);
    let key = if claimed {
        "dashboard.award_claimed"
    } else {
        "dashboard.award_next"
    };
    rsx! {
        span { class: "award",
            if !award.icon.is_empty() && !failed() {
                img { class: "award-icon", src: award.icon.clone(), alt: "", width: "32", height: "32",
                    loading: "lazy", decoding: "async", referrerpolicy: "no-referrer",
                    onerror: move |_| failed.set(true),
                }
            }
            span { span { class: "visually-hidden", {ui.text(key)} " " } strong { {award.name.clone()} } " ×{award.count}" }
        }
    }
}

#[component]
fn GameStatusCard(
    status: GameStatus,
    history: Option<ResourceHistory>,
    history_error: Option<&'static str>,
) -> Element {
    let ui = use_preferences();
    rsx! {
        div { class: "status-card",
            if let Some(notice) = freshness_notice(&status, ui) {
                WarningLine { status: if status.data.is_some() { Status::Warn } else { Status::Failed }, text: notice }
            }
            if let Some(data) = status.data.as_ref() {
                if status.game_kind == "genshin" {
                    PrimaryResource { icon: Icon::Resin, label: ui.text("dashboard.resin"), current: data.current_resin, maximum: data.max_resin, full_at: data.resin_full_at.clone() }
                    CapHistory { game: "genshin", resource: ui.text("dashboard.resin"), history: history.clone(), error: history_error }
                    GenshinTiles { data: data.clone() }
                    ExpeditionList { expeditions: data.expeditions.clone(), maximum: data.max_expedition_num, fetched_at: status.fetched_at.clone() }
                } else if status.game_kind == "starrail" {
                    PrimaryResource { icon: Icon::Bolt, label: ui.text("dashboard.trailblaze_power"), current: data.current_stamina, maximum: data.max_stamina, full_at: data.stamina_full_at.clone() }
                    CapHistory { game: "starrail", resource: ui.text("dashboard.trailblaze_power"), history: history.clone(), error: history_error }
                    StarRailTiles { data: data.clone() }
                }
            }
            if let Some(key) = missing_payload_notice(status.data.as_ref()).filter(|_| status.error_code.is_none()) {
                p { class: "quiet-line", {ui.text(key)} }
            }
        }
    }
}

/// The one collapsed block at the end of a game card: the selected UID's fetch details
/// (UID, server, last fetch, the error if any), when the endgame records were fetched,
/// and the login bonus state (auth, last check, last run, its error). The summary line
/// names an error so it shows without opening.
#[component]
fn CardDetails(state: Signal<AppState>, game: &'static str) -> Element {
    let ui = use_preferences();
    let snapshot = state.read();
    let selected = snapshot.selected_game_account(game);
    let status: Option<GameStatus> = selected.and_then(|id| {
        snapshot
            .account_statuses
            .iter()
            .find(|entry| entry.id == id)
            .map(|entry| entry.status.clone())
    });
    let challenges: Option<GameChallenges> = selected.and_then(|id| {
        snapshot
            .account_challenges
            .iter()
            .find(|entry| entry.id == id)
            .map(|entry| entry.challenges.clone())
    });
    let setting = snapshot
        .rewards
        .iter()
        .find(|setting| setting.game_kind == game)
        .cloned();
    drop(snapshot);
    if status.is_none() && setting.is_none() {
        return rsx! {};
    }
    let status_error = status.as_ref().and_then(|status| status.error_code.clone());
    let bonus_error = setting
        .as_ref()
        .and_then(|setting| setting.error_code.clone());
    rsx! {
        details { class: "status-details",
            summary {
                Glyph { icon: Icon::Info }
                span { {ui.text("dashboard.data_details")} }
                if let Some(code) = &status_error {
                    span { class: "details-error", "data-source": "status", Glyph { icon: Icon::Failed } span { {error_text(Some(code), ui)} } }
                }
                if let Some(code) = &bonus_error {
                    span { class: "details-error", "data-source": "bonus", Glyph { icon: Icon::Failed } span { {error_text(Some(code), ui)} } }
                }
                Glyph { icon: Icon::Chevron, class: "chevron" }
            }
            dl { class: "metrics status-metadata",
                if let Some(status) = &status {
                    Metric { label: ui.text("dashboard.uid"), value: if status.game_uid.is_empty() { ui.text("dashboard.missing").to_owned() } else { status.game_uid.clone() } }
                    Metric { label: ui.text("dashboard.region"), value: status.region.clone() }
                    Metric { label: ui.text("dashboard.last_fetched"), value: date(&status.fetched_at, ui) }
                    if let Some(code) = &status.error_code {
                        Metric { label: ui.text("dashboard.error"), value: error_text(Some(code), ui).to_owned() }
                        Metric { label: ui.text("dashboard.error_code"), value: code.clone() }
                        Metric { label: ui.text("dashboard.error_at"), value: date(&status.error_at, ui) }
                    }
                }
                if let Some(challenges) = &challenges {
                    Metric { label: ui.text("dashboard.endgame_fetched"), value: date(&challenges.fetched_at, ui) }
                }
                if let Some(setting) = &setting {
                    Metric { label: ui.text("dashboard.bonus_auth_status"), value: model_status(&setting.auth_status, ui).to_owned() }
                    Metric { label: ui.text("dashboard.bonus_checked"), value: date(&setting.last_checked_at, ui) }
                    Metric { label: ui.text("dashboard.last_run_state"), value: setting.last_run.as_ref().map(|run| model_status(&run.state, ui).to_owned()).unwrap_or_else(|| ui.text("dashboard.never_run").to_owned()) }
                    if setting.error_code.is_some() {
                        Metric { label: ui.text("dashboard.bonus_error"), value: error_text(setting.error_code.as_deref(), ui).to_owned() }
                        Metric { label: ui.text("dashboard.bonus_error_at"), value: date(&setting.error_at, ui) }
                    }
                }
            }
        }
    }
}

/// The primary resource (resin, Trailblaze Power): its icon, the largest number on the
/// card, a bar, and the time until full recovery with a small clock time. The name is the
/// section's accessible name only; a missing value is the unknown mark, not a numeral.
#[component]
fn PrimaryResource(
    icon: Icon,
    label: &'static str,
    current: Option<i64>,
    maximum: Option<i64>,
    full_at: Option<String>,
) -> Element {
    let ui = use_preferences();
    // Re-rendered each second so the countdown changes the moment its value does.
    let clock = use_clock();
    let _ = clock();
    let fill = fill_level(current, maximum);
    let recovery = local_moment(full_at.as_deref()).zip(seconds_since(full_at.as_deref()));
    rsx! {
        section { class: "resource-block", "data-fill": fill, "aria-label": label,
            div { class: "resource-top",
                Glyph { icon, class: "icon-xl resource-icon" }
                if let Some(value) = current {
                    p { class: "resource", "data-fill": fill,
                        strong { "{value}" }
                        if let Some(maximum) = maximum { span { "/{maximum}" } }
                    }
                } else {
                    p { class: "resource", StatusMark { status: Status::Unknown, text: ui.text("common.not_fetched"), class: "missing" } }
                }
                if fill == "full" { StatusMark { status: Status::Warn, text: ui.text("dashboard.at_cap"), class: "cap-badge" } }
                else if fill == "near" { StatusMark { status: Status::Warn, text: ui.text("dashboard.near_cap"), class: "cap-badge" } }
            }
            if let (Some(value), Some(max)) = (current, maximum) {
                if max > 0 && value >= 0 && value <= max {
                    progress { value: "{value}", max: "{max}", "aria-label": label }
                }
            }
            p { class: "eta",
                Glyph { icon: Icon::Hourglass }
                span { class: "visually-hidden", {ui.text("dashboard.full_recovery_at")} " " }
                if let Some((moment, elapsed)) = recovery {
                    strong { class: "recovery-in", {countdown_text(-elapsed, "dashboard.fully_recovered", ui)} }
                    time { class: "recovery-at", datetime: full_at.clone(), title: compact_datetime(full_at.as_deref()), {short_time(&moment, ui)} }
                } else {
                    span { class: "missing", {ui.text("common.not_fetched")} }
                }
            }
        }
    }
}

/// Time left until full recovery, or "fully recovered" once the estimate has passed.
fn countdown_text(seconds_left: i64, reached: &str, ui: Preferences) -> String {
    if seconds_left <= 0 {
        ui.text(reached).to_owned()
    } else {
        ui.format(
            "dashboard.in_duration",
            &[("duration", &hours_minutes(seconds_left, ui))],
        )
    }
}

/// The dashboard's one clock: a counter that changes on every whole second, so
/// countdowns change the moment their minute does. Components subscribe with `use_clock`.
#[derive(Clone, Copy)]
struct Clock(Signal<u64>);

pub(super) fn use_clock_provider() {
    let mut tick = use_signal(|| 0u64);
    use_context_provider(|| Clock(tick));
    #[cfg(any(target_arch = "wasm32", feature = "mobile"))]
    use_future(move || async move {
        loop {
            let wait = 1000 - (crate::platform::now_ms() as u64 % 1000);
            crate::platform::sleep_ms(wait.max(1) as u32).await;
            tick += 1;
        }
    });
    #[cfg(not(any(target_arch = "wasm32", feature = "mobile")))]
    let _ = &mut tick;
}

pub(super) fn use_clock() -> Signal<u64> {
    use_context::<Clock>().0
}

/// One warning per status: a failed fetch without data says only that it
/// failed; stale data says how old it is, with the failure reason if known.
fn freshness_notice(status: &GameStatus, ui: Preferences) -> Option<String> {
    let reason = status
        .error_code
        .as_deref()
        .map(|code| error_text(Some(code), ui));
    if status.data.is_none() {
        return reason
            .map(|reason| ui.format("dashboard.fetch_failed_with_reason", &[("reason", reason)]));
    }
    if !status.stale {
        return None;
    }
    let age = seconds_since(status.fetched_at.as_deref()).map_or_else(
        || ui.text("dashboard.cached_status_is_stale").to_owned(),
        |seconds| data_age(seconds, ui),
    );
    Some(match reason {
        Some(reason) => ui.format(
            "dashboard.stale_with_reason",
            &[("age", &age), ("reason", reason)],
        ),
        None => age,
    })
}

fn missing_payload_notice(data: Option<&DailySummary>) -> Option<&'static str> {
    data.is_none()
        .then_some("dashboard.no_daily_summary_payload_returned")
}

fn date(value: &Option<String>, ui: Preferences) -> String {
    datetime_text(value.as_deref(), ui)
}

fn received(value: Option<bool>, ui: Preferences) -> &'static str {
    match value {
        Some(true) => ui.text("dashboard.received"),
        Some(false) => ui.text("dashboard.not_received"),
        None => ui.text("common.not_fetched"),
    }
}

fn model_status(value: &str, ui: Preferences) -> &'static str {
    match value {
        "pending" | "queued" => ui.text("dashboard.queued_completion_unconfirmed"),
        "running" => ui.text("dashboard.running"),
        "retry_wait" => ui.text("dashboard.waiting_to_retry"),
        "action_required" => ui.text("dashboard.action_required"),
        "stopped" => ui.text("dashboard.stopped"),
        "success" => ui.text("dashboard.claim_confirmed"),
        "already_claimed" => ui.text("dashboard.already_claimed_today"),
        "ready" => ui.text("dashboard.ready"),
        "unknown" => ui.text("dashboard.unknown_outcome"),
        "done" => ui.text("dashboard.done"),
        "failed" => ui.text("dashboard.failed"),
        "verified" => ui.text("dashboard.verified"),
        "invalid" => ui.text("dashboard.invalid"),
        _ => ui.text("dashboard.unknown"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn absent_payload_has_a_notice_but_nullable_fields_do_not_imply_zeroes() {
        assert_eq!(
            missing_payload_notice(None),
            Some("dashboard.no_daily_summary_payload_returned")
        );
        let summary: DailySummary = serde_json::from_value(serde_json::json!({})).unwrap();
        assert_eq!(missing_payload_notice(Some(&summary)), None);
        assert!(summary.current_resin.is_none());
        assert!(summary.max_resin.is_none());
        assert!(summary.current_stamina.is_none());
        assert!(summary.max_stamina.is_none());
        assert!(summary.expeditions.is_none());
    }

    #[test]
    fn failed_fetches_without_data_do_not_also_claim_outdated_data() {
        let ui = Preferences::default();
        let mut status: GameStatus = serde_json::from_value(serde_json::json!({
            "game_kind": "genshin", "game_uid": "600000000", "region": "os_asia",
            "visible": true, "detected": true, "data": null, "fetched_at": null,
            "error_code": "network", "error_at": null, "stale": true
        }))
        .unwrap();
        assert_eq!(
            freshness_notice(&status, ui).unwrap(),
            "状態を取得できませんでした（HoYoLABに接続できませんでした）"
        );
        status.data = Some(serde_json::from_value(serde_json::json!({})).unwrap());
        // Native tests cannot read fetched_at; the browser check covers the age text.
        assert_eq!(
            freshness_notice(&status, ui).unwrap(),
            "表示中のデータは最新ではありません（HoYoLABに接続できませんでした）"
        );
        status.stale = false;
        status.error_code = None;
        assert_eq!(freshness_notice(&status, ui), None);
    }

    #[test]
    fn the_claim_summary_follows_the_tab_order_and_skips_games_without_a_setting() {
        let setting = |game: &str, signed: Option<bool>| -> RewardSetting {
            serde_json::from_value(serde_json::json!({
                "id": uuid::Uuid::from_u128(1), "game_kind": game, "enabled": true,
                "local_time": "06:00", "auth_status": "ready", "is_sign": signed
            }))
            .unwrap()
        };
        assert_eq!(
            claim_summary(&[
                setting("starrail", Some(true)),
                setting("genshin", Some(false))
            ]),
            [("genshin", Some(false)), ("starrail", Some(true))]
        );
        assert_eq!(
            claim_summary(&[setting("starrail", None)]),
            [("starrail", None)]
        );
        assert!(claim_summary(&[]).is_empty());
        assert_eq!(claim_tone(Some(true)), "success");
        assert_eq!(claim_tone(Some(false)), "pending");
        assert_eq!(claim_tone(None), "");
        // Each tone has its own symbol; an unknown claim is not shown as open.
        assert_eq!(claim_status(Some(true)), Status::Done);
        assert_eq!(claim_status(Some(false)), Status::Open);
        assert_eq!(claim_status(None), Status::Unknown);
    }

    #[test]
    fn the_recovery_countdown_ends_as_fully_recovered() {
        let ui = Preferences::default();
        assert_eq!(
            countdown_text(7 * 3600 + 12 * 60, "dashboard.fully_recovered", ui),
            "あと7時間12分"
        );
        assert_eq!(
            countdown_text(30, "dashboard.fully_recovered", ui),
            "あと0時間01分"
        );
        assert_eq!(
            countdown_text(0, "dashboard.fully_recovered", ui),
            "全回復済み"
        );
        assert_eq!(
            countdown_text(-90, "dashboard.fully_recovered", ui),
            "全回復済み"
        );
        let english = Preferences {
            language: crate::i18n::Language::English,
            ..ui
        };
        assert_eq!(
            countdown_text(7 * 3600 + 12 * 60, "dashboard.fully_recovered", english),
            "in 7h 12m"
        );
    }
}
