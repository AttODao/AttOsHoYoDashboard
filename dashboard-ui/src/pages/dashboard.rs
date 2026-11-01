use super::common::{Icon, LoadError, Metric, focus_element, game_name};
use super::rewards::AutomationSwitch;
use crate::app::actions::spawn_root;
use crate::app::data::{RunAll, load_game_status, run_all_targets, run_rewards_now};
use crate::app::display::{
    compact_datetime, count_pair, data_age, datetime_text, days_hours, error_text, fill_level,
    hours_minutes, relative_age, seconds_since,
};
use crate::app::preferences::{Preferences, use_preferences};
use crate::app::state::{AppState, DASHBOARD_GAMES, Page, set_page};
use crate::models::{DailyAward, DailySummary, Expedition, GameStatus, RewardSetting};
use dioxus::prelude::*;
use uuid::Uuid;

#[component]
pub(super) fn DashboardPage(state: Signal<AppState>) -> Element {
    let ui = use_preferences();
    let mut active = use_signal(|| "genshin");
    let mobile = use_mobile_dashboard();
    use_clock_provider();
    rsx! {
        div { class: "page-stack dashboard",
            {pull_refresh(state)}
            div { class: "page-intro",
                h1 { {ui.text("dashboard.dashboard")} }
                ClaimNow { state }
            }
            if state.read().accounts_error.is_some() {
                LoadError { state, message: ui.text("dashboard.could_not_refresh_the_uid_list") }
            }
            div { class: "game-tabs tabs", role: "tablist", "aria-label": ui.text("dashboard.games"),
                for game in DASHBOARD_GAMES {
                    button {
                        r#type: "button", role: "tab", id: "game-tab-{game}",
                        "aria-controls": "game-panel-{game}", "aria-selected": active() == game,
                        tabindex: if active() == game { "0" } else { "-1" },
                        onclick: move |_| active.set(game),
                        onkeydown: move |event: KeyboardEvent| {
                            if let Some(next) = next_game_tab(game, &event.key()) {
                                event.prevent_default();
                                active.set(next);
                                focus_game_tab(next);
                            }
                        },
                        {game_name(game, ui)}
                    }
                }
            }
            div { class: "dashboard-grid",
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
    drop(snapshot);
    let blocked = match &targets {
        RunAll::Ready(_) => None,
        RunAll::Blocked(key) => Some(*key),
    };
    rsx! {
        div { class: "claim-now",
            button {
                r#type: "button", class: "primary", id: "claim-now",
                disabled: blocked.is_some() || !loaded,
                "aria-describedby": blocked.map(|_| "claim-now-hint"),
                onclick: move |_| run_rewards_now(state, Page::Dashboard),
                {ui.text("rewards.claim_now")}
            }
            if following {
                p { class: "field-hint", role: "status", {ui.text("rewards.checking_automatically")} }
            } else if let Some(key) = blocked.filter(|_| loaded) {
                p { id: "claim-now-hint", class: "field-hint", {ui.text(key)} }
            }
        }
    }
}

// One media listener, removed on unmount. CSS and semantics use the same 900px boundary.
#[cfg(feature = "mobile")]
fn use_mobile_dashboard() -> Signal<bool> {
    crate::mobile::dom::use_media_query("(max-width:900px)")
}

#[cfg(not(feature = "mobile"))]
fn use_mobile_dashboard() -> Signal<bool> {
    let mut mobile = use_signal(|| false);
    #[cfg(target_arch = "wasm32")]
    {
        let _listener = use_hook(move || {
            use wasm_bindgen::JsCast;
            let media = web_sys::window()?.match_media("(max-width:900px)").ok()??;
            mobile.set(media.matches());
            let observed = media.clone();
            let callback =
                wasm_bindgen::closure::Closure::<dyn FnMut(web_sys::Event)>::new(move |_| {
                    mobile.set(observed.matches())
                });
            media
                .add_event_listener_with_callback("change", callback.as_ref().unchecked_ref())
                .ok()?;
            Some(std::rc::Rc::new(MobileDashboardListener {
                media,
                callback,
            }))
        });
    }
    #[cfg(not(target_arch = "wasm32"))]
    let _ = &mut mobile;
    mobile
}

#[cfg(target_arch = "wasm32")]
struct MobileDashboardListener {
    media: web_sys::MediaQueryList,
    callback: wasm_bindgen::closure::Closure<dyn FnMut(web_sys::Event)>,
}

#[cfg(target_arch = "wasm32")]
impl Drop for MobileDashboardListener {
    fn drop(&mut self) {
        use wasm_bindgen::JsCast;
        let _ = self
            .media
            .remove_event_listener_with_callback("change", self.callback.as_ref().unchecked_ref());
    }
}

fn next_game_tab(game: &str, key: &Key) -> Option<&'static str> {
    match key {
        Key::Home => Some("genshin"),
        Key::End => Some("starrail"),
        Key::ArrowLeft | Key::ArrowRight => Some(if game == "genshin" {
            "starrail"
        } else {
            "genshin"
        }),
        _ => None,
    }
}

fn focus_game_tab(game: &'static str) {
    focus_element(&format!("game-tab-{game}"));
}

/// Card order puts what players check first at the top: UID, then the primary
/// resource with its full-recovery estimate, other stats, and the login bonus last.
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
            div { class: "game-heading",
                h2 { class: "game-title", id: "game-title-{game}", {game_name(game, ui)} }
                // The app refreshes by pulling the dashboard down instead.
                if !crate::platform::APP { RefreshButton { state, game } }
            }
            GameStatusSection { state, game }
            BonusSection { state, game }
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
    let accounts_loaded = snapshot.accounts.is_some();
    let accounts_failed = snapshot.accounts_error.is_some();
    let loading = selection.loading && selection.generation == snapshot.generation;
    drop(snapshot);
    rsx! {
        if !accounts_loaded {
            // A failed first load shows the page-level warning instead.
            if !accounts_failed { p { role: "status", {ui.text("dashboard.loading")} } }
        } else if visible.is_empty() {
            div { class: "empty",
                h3 { {ui.text("dashboard.no_visible_uids")} }
                button { r#type: "button", onclick: move |_| set_page(state, Page::Uids), {ui.text("dashboard.choose_uids")} }
            }
        } else {
            label { class: "uid-select", r#for: "selected-{game}",
                span { {ui.text("dashboard.selected_uid")} }
                select {
                    id: "selected-{game}", value: selected.map(|id| id.to_string()).unwrap_or_default(),
                    onchange: move |event| {
                        if let Ok(id) = event.value().parse::<Uuid>() {
                            state.write().select_game_account(game, Some(id));
                            spawn_root(async move { load_game_status(state, game).await; });
                        }
                    },
                    for account in visible {
                        option { value: "{account.id}", {ui.format("dashboard.uid_option", &[("region", &account.region), ("uid", &account.game_uid)])} }
                    }
                }
            }
            if let Some(key) = selection.error {
                // Without a payload there is nothing that could be outdated.
                p { class: "warning", role: "status", {ui.text(refresh_error_key(key, status.is_some()))} }
            }
            div { "aria-busy": loading,
                if let Some(status) = status.clone() {
                    GameStatusCard { status }
                } else if loading {
                    p { role: "status", {ui.text("dashboard.loading")} }
                } else if selection.error.is_none() {
                    p { {ui.text("dashboard.status_has_not_been_fetched")} }
                }
            }
            div { class: "refresh-row",
                Freshness { fetched_at: status.as_ref().and_then(|status| status.fetched_at.clone()), stale: status.as_ref().is_some_and(|status| status.stale) }
            }
        }
    }
}

/// Web refresh control: a circular-arrow icon button at the card's top right.
#[component]
fn RefreshButton(state: Signal<AppState>, game: &'static str) -> Element {
    let ui = use_preferences();
    let snapshot = state.read();
    let shown = snapshot.accounts.is_some() && snapshot.selected_game_account(game).is_some();
    let loading = snapshot
        .selected_accounts
        .get(game)
        .is_some_and(|selection| selection.loading && selection.generation == snapshot.generation);
    drop(snapshot);
    let label = ui.text(if loading {
        "dashboard.refreshing"
    } else {
        "dashboard.refresh"
    });
    rsx! {
        if shown {
            button {
                r#type: "button", class: "icon-button refresh-button", id: "refresh-{game}",
                disabled: loading, "aria-busy": loading, "aria-label": label, title: label,
                onclick: move |_| { spawn_root(async move { load_game_status(state, game).await; }); },
                Icon { name: "refresh" }
            }
        }
    }
}

/// App only: pulling the dashboard down at its top reloads UIDs, status and bonus settings.
#[cfg(feature = "mobile")]
fn pull_refresh(state: Signal<AppState>) -> Element {
    rsx! { PullRefresh { state } }
}

#[cfg(not(feature = "mobile"))]
fn pull_refresh(_state: Signal<AppState>) -> Element {
    rsx! {}
}

#[cfg(feature = "mobile")]
#[component]
fn PullRefresh(state: Signal<AppState>) -> Element {
    use crate::app::data::load_dashboard_data;
    let ui = use_preferences();
    let mut refreshing = use_signal(|| false);
    let _pull = use_hook(move || {
        std::rc::Rc::new(crate::mobile::dom::PullToRefresh::attach(move || {
            if *refreshing.peek() {
                return;
            }
            refreshing.set(true);
            spawn_root(async move {
                load_dashboard_data(state, true).await;
                refreshing.set(false);
            });
        }))
    });
    rsx! {
        div { id: "pull-refresh", class: "pull-refresh", "data-refreshing": refreshing(), "aria-hidden": "true",
            Icon { name: "refresh" }
        }
        p { class: "visually-hidden", role: "status",
            if refreshing() { {ui.text("dashboard.refreshing")} }
        }
    }
}

/// Always-visible age of the shown data, kept current by the dashboard clock.
#[component]
fn Freshness(fetched_at: Option<String>, stale: bool) -> Element {
    let ui = use_preferences();
    let clock = use_clock();
    let _ = clock();
    let age = seconds_since(fetched_at.as_deref()).map(|seconds| relative_age(seconds, ui));
    rsx! {
        if let Some(age) = age {
            p { class: if stale { "freshness stale" } else { "freshness" },
                time { datetime: fetched_at, {ui.format("dashboard.updated_ago", &[("age", &age)])} }
            }
        } else {
            span {}
        }
    }
}

/// Automatic login bonus toggle and today's claim summary for one game.
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
            div { class: "card-automation",
                h3 { id: "bonus-title-{game}", {ui.text("dashboard.login_bonus")} }
                if let Some(setting) = setting.clone() {
                    AutomationSwitch { state, setting, id: "automation-{game}", disabled: rewards_error }
                } else {
                    span { {ui.text("common.not_fetched")} }
                }
            }
            if rewards_error {
                LoadError { state, message: ui.text("dashboard.could_not_load_bonus_settings") }
            }
            if let Some(setting) = setting {
                BonusToday { state, setting }
            }
        }
    }
}

/// Today's claim, this month's count and reward, the next automatic run, then details.
#[component]
fn BonusToday(state: Signal<AppState>, setting: RewardSetting) -> Element {
    let ui = use_preferences();
    let clock = use_clock();
    let _ = clock();
    let tone = match setting.is_sign {
        Some(true) => "success",
        Some(false) => "pending",
        None => "",
    };
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
    rsx! {
        p { class: "run-state bonus-today {tone}",
            {ui.text("dashboard.today_s_reward")} ": "
            strong { {received(setting.is_sign, ui)} }
            if let Some(days) = setting.total_sign_day {
                " · " {ui.format("dashboard.month_sign_days", &[("count", &days.to_string())])}
            }
        }
        if let Some(award) = setting.today_award.clone() {
            AwardLine { award, claimed: setting.is_sign == Some(true) }
        }
        p { class: "run-state",
            if setting.enabled {
                {ui.text("dashboard.next_run")} ": "
                strong { {date(&setting.next_run_at, ui)} }
                if let Some(next_in) = next_in { " · " {ui.format("dashboard.in_duration", &[("duration", &next_in)])} }
            } else {
                {ui.text("dashboard.automation_off")}
            }
        }
        if needs_link {
            div { class: "notice warn bonus-attention", role: "status",
                {ui.text(if setting.auth_status == "action_required" { "dashboard.bonus_action_required" } else { "errors.cookie_invalid" })}
                p { button { r#type: "button", onclick: move |_| set_page(state, Page::Link), {ui.text("dashboard.open_link_settings")} } }
            }
        }
        details { class: "bonus-status",
            summary {
                {ui.text("dashboard.bonus_details")}
                if let Some(run) = &setting.last_run {
                    " · " {ui.text("dashboard.last_run_state")} ": " {model_status(&run.state, ui)}
                }
                if setting.error_code.is_some() {
                    " · " {error_text(setting.error_code.as_deref(), ui)}
                }
            }
            dl { class: "metrics",
                Metric { label: ui.text("dashboard.auth_status"), value: model_status(&setting.auth_status, ui).to_owned() }
                Metric { label: ui.text("dashboard.checked"), value: date(&setting.last_checked_at, ui) }
                Metric { label: ui.text("dashboard.last_run_state"), value: setting.last_run.as_ref().map(|run| model_status(&run.state, ui).to_owned()).unwrap_or_else(|| ui.text("dashboard.never_run").to_owned()) }
                if setting.error_code.is_some() {
                    Metric { label: ui.text("dashboard.bonus_error"), value: error_text(setting.error_code.as_deref(), ui).to_owned() }
                    Metric { label: ui.text("dashboard.error_at"), value: date(&setting.error_at, ui) }
                }
            }
        }
    }
}

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
        p { class: "award",
            if !award.icon.is_empty() && !failed() {
                img { class: "award-icon", src: award.icon.clone(), alt: "", width: "32", height: "32",
                    loading: "lazy", decoding: "async", referrerpolicy: "no-referrer",
                    onerror: move |_| failed.set(true),
                }
            }
            span { {ui.text(key)} ": " strong { {award.name.clone()} } " ×{award.count}" }
        }
    }
}

#[component]
fn GameStatusCard(status: GameStatus) -> Element {
    let ui = use_preferences();
    // The transformer countdown reads the clock.
    let clock = use_clock();
    let _ = clock();
    rsx! {
        div { class: "status-card",
            if let Some(notice) = freshness_notice(&status, ui) { p { class: "warning", role: "status", {notice} } }
            if let Some(data) = status.data.as_ref() {
                if status.game_kind == "genshin" {
                    Resource { label: ui.text("dashboard.resin"), current: data.current_resin, maximum: data.max_resin, full_at: data.resin_full_at.clone() }
                    Resource {
                        label: ui.text("dashboard.realm_currency"), current: data.current_home_coin, maximum: data.max_home_coin,
                        full_at: data.home_coin_full_at.clone(), estimate: "dashboard.currency_full_at", reached: "dashboard.at_cap", secondary: true,
                    }
                    dl { class: "metrics",
                        Metric { label: ui.text("dashboard.commissions"), value: pair(data.finished_task_num, data.total_task_num, ui) }
                        Metric { label: ui.text("dashboard.extra_commission_reward"), value: received(data.is_extra_task_reward_received, ui).to_owned() }
                        Metric { label: ui.text("dashboard.weekly_boss_discounts"), value: remaining(data.remain_resin_discount_num, data.resin_discount_num_limit, ui) }
                        Metric { label: ui.text("dashboard.transformer"), value: transformer_text(data, ui) }
                    }
                } else if status.game_kind == "starrail" {
                    Resource { label: ui.text("dashboard.trailblaze_power"), current: data.current_stamina, maximum: data.max_stamina, full_at: data.stamina_full_at.clone() }
                    dl { class: "metrics",
                        Metric { label: ui.text("dashboard.reserved_trailblaze_power"), value: reserve_text(data, ui) }
                        Metric { label: ui.text("dashboard.daily_training"), value: pair(data.current_train_score, data.max_train_score, ui) }
                        Metric { label: ui.text("dashboard.simulated_universe"), value: pair(data.current_rogue_score, data.max_rogue_score, ui) }
                        Metric { label: ui.text("dashboard.echo_of_war"), value: remaining(data.weekly_cocoon_cnt, data.weekly_cocoon_limit, ui) }
                        if data.rogue_tourn_weekly_unlocked == Some(true) {
                            Metric { label: ui.text("dashboard.divergent_universe"), value: pair(data.rogue_tourn_weekly_cur, data.rogue_tourn_weekly_max, ui) }
                        }
                    }
                }
                if status.game_kind == "genshin" {
                    ExpeditionList { expeditions: data.expeditions.clone(), maximum: data.max_expedition_num, fetched_at: status.fetched_at.clone() }
                }
            }
            if let Some(key) = missing_payload_notice(status.data.as_ref()).filter(|_| status.error_code.is_none()) {
                p { {ui.text(key)} }
            }
            details { class: "status-details",
                summary {
                    {ui.text("dashboard.data_details")}
                    if status.error_code.is_some() { " · " {error_text(status.error_code.as_deref(), ui)} }
                }
                dl { class: "metrics status-metadata",
                    Metric { label: ui.text("dashboard.uid"), value: if status.game_uid.is_empty() { ui.text("dashboard.missing").to_owned() } else { status.game_uid.clone() } }
                    Metric { label: ui.text("dashboard.region"), value: status.region.clone() }
                    Metric { label: ui.text("dashboard.last_fetched"), value: date(&status.fetched_at, ui) }
                    if let Some(code) = &status.error_code {
                        Metric { label: ui.text("dashboard.error"), value: error_text(Some(code), ui).to_owned() }
                        Metric { label: ui.text("dashboard.error_code"), value: code.clone() }
                        Metric { label: ui.text("dashboard.error_at"), value: date(&status.error_at, ui) }
                    }
                }
            }
        }
    }
}

/// A capped resource as a number and a bar with its estimate. The primary one
/// (resin, Trailblaze Power) uses the numeric face; a secondary one (realm
/// currency) is a step smaller and keeps its date in the body font.
#[component]
fn Resource(
    label: &'static str,
    current: Option<i64>,
    maximum: Option<i64>,
    full_at: Option<String>,
    #[props(default = "dashboard.full_recovery_at")] estimate: &'static str,
    #[props(default = "dashboard.fully_recovered")] reached: &'static str,
    #[props(default)] secondary: bool,
) -> Element {
    let ui = use_preferences();
    let fill = fill_level(current, maximum);
    rsx! {
        // One wrapper per resource keeps the fill-state sibling selectors to its own bar.
        div { class: if secondary { "resource-block secondary" } else { "resource-block" },
            p { class: "resource-label", "{label}"
                if fill == "full" { span { class: "status warn cap-badge", {ui.text("dashboard.at_cap")} } }
                else if fill == "near" { span { class: "status warn cap-badge", {ui.text("dashboard.near_cap")} } }
            }
            div { class: "resource", "data-fill": fill,
                strong { {number(current, ui)} }
                if let Some(maximum) = maximum { span { "/ {maximum}" } }
            }
            if let (Some(value), Some(max)) = (current, maximum) {
                if max > 0 && value >= 0 && value <= max {
                    progress { value: "{value}", max: "{max}", "aria-label": label }
                }
            }
            dl { class: "row",
                Estimate { label: ui.text(estimate), value: full_at, numeric: !secondary, countdown: secondary, reached }
            }
        }
    }
}

/// An estimated datetime. The numeric (full recovery) form also says how long
/// is left, re-rendered each second so the countdown changes the moment its value does.
#[component]
fn Estimate(
    label: &'static str,
    value: Option<String>,
    #[props(default)] numeric: bool,
    /// Also state the time left, without the numeric display face.
    #[props(default)]
    countdown: bool,
    /// Shown once the estimate has passed.
    #[props(default = "dashboard.fully_recovered")]
    reached: &'static str,
) -> Element {
    let ui = use_preferences();
    let clock = use_clock();
    let formatted = compact_datetime(value.as_deref());
    let countdown = (numeric || countdown)
        .then(|| {
            let _ = clock();
            seconds_since(value.as_deref())
        })
        .flatten()
        .map(|elapsed| countdown_text(-elapsed, reached, ui));
    rsx! {
        div { class: "metric",
            dt { "{label}" }
            dd {
                if let Some(formatted) = formatted {
                    time { class: if numeric { "recovery-at" } else { "estimate-at" }, datetime: value, "{formatted}" }
                    if let Some(countdown) = countdown { span { class: "recovery-in", "{countdown}" } }
                } else {
                    {ui.text("common.not_fetched")}
                }
            }
        }
    }
}

/// Expeditions with their time left counted down from the fetch time.
#[component]
fn ExpeditionList(
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
            let total = maximum
                .and_then(|max| usize::try_from(max).ok())
                .filter(|max| *max >= rows.len())
                .unwrap_or(rows.len());
            ui.format(
                "dashboard.expeditions_finished",
                &[
                    ("finished", &finished.to_string()),
                    ("total", &total.to_string()),
                ],
            )
        });
    rsx! {
        div { class: "expeditions",
            div { class: "expeditions-heading",
                h3 { {ui.text("dashboard.expeditions")} }
                if let Some(summary) = summary { span { class: "muted", "{summary}" } }
            }
            if let Some(rows) = expeditions {
                if rows.is_empty() { p { {ui.text("dashboard.no_expeditions")} } }
                ul {
                    for (index, expedition) in rows.into_iter().enumerate() {
                        li { class: "expedition",
                            ExpeditionIcon {
                                key: "{index}-{expedition.avatar_side_icon:?}",
                                source: expedition.avatar_side_icon.clone(),
                                label: expedition_label(expedition.name.as_deref(), index, ui),
                            }
                            // A finished expedition (also one whose time ran out since the fetch)
                            // has nothing left; "Finished" alone says so.
                            if row_finished(expedition.status.as_deref(), left(&expedition)) {
                                span { {model_status("Finished", ui)} }
                            } else {
                                span { {expedition.status.as_deref().map(|value| model_status(value, ui)).unwrap_or(ui.text("dashboard.unknown"))} }
                                span { {ui.text("dashboard.remaining")} ": " {remaining_text(&expedition, left(&expedition), ui)} }
                            }
                        }
                    }
                }
            } else { p { {ui.text("common.not_fetched")} } }
        }
    }
}

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
                Icon { name: "user" }
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

fn use_clock_provider() {
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

fn use_clock() -> Signal<u64> {
    use_context::<Clock>().0
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

/// A failed refresh only warns about outdated data when a payload is shown.
fn refresh_error_key(key: &'static str, has_status: bool) -> &'static str {
    if key == "dashboard.refresh_failed" && !has_status {
        "dashboard.status_fetch_failed"
    } else {
        key
    }
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

fn number(value: Option<i64>, ui: Preferences) -> String {
    value
        .map(|n| n.to_string())
        .unwrap_or_else(|| ui.text("common.not_fetched").to_owned())
}

fn pair(current: Option<i64>, maximum: Option<i64>, ui: Preferences) -> String {
    current
        .map(|value| count_pair(value, maximum))
        .unwrap_or_else(|| ui.text("common.not_fetched").to_owned())
}

/// Weekly allowances count down: "2/3 left".
fn remaining(left: Option<i64>, maximum: Option<i64>, ui: Preferences) -> String {
    match (left, maximum) {
        (Some(left), Some(maximum)) => ui.format(
            "dashboard.remaining_of",
            &[("left", &left.to_string()), ("max", &maximum.to_string())],
        ),
        (Some(left), None) => left.to_string(),
        _ => ui.text("common.not_fetched").to_owned(),
    }
}

fn transformer_text(data: &DailySummary, ui: Preferences) -> String {
    match (data.transformer_obtained, data.transformer_ready) {
        (Some(false), _) => ui.text("dashboard.transformer_not_obtained").to_owned(),
        (_, Some(true)) => ui.text("dashboard.transformer_ready").to_owned(),
        (_, Some(false)) => match seconds_since(data.transformer_ready_at.as_deref()) {
            // The cooldown ended since the fetch.
            Some(elapsed) if elapsed >= 0 => ui.text("dashboard.transformer_ready").to_owned(),
            Some(elapsed) => ui.format(
                "dashboard.in_duration",
                &[("duration", &days_hours(-elapsed, ui))],
            ),
            None => ui.text("dashboard.transformer_cooling").to_owned(),
        },
        _ => ui.text("common.not_fetched").to_owned(),
    }
}

fn reserve_text(data: &DailySummary, ui: Preferences) -> String {
    let value = number(data.current_reserve_stamina, ui);
    if data.is_reserve_stamina_full == Some(true) {
        format!("{value} · {}", ui.text("dashboard.at_cap"))
    } else {
        value
    }
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
        "Ongoing" => ui.text("dashboard.ongoing"),
        "Finished" => ui.text("dashboard.finished"),
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
        assert_eq!(
            pair(
                summary.current_resin,
                summary.max_resin,
                Preferences::default()
            ),
            "未取得"
        );
        assert_eq!(count_pair(0, None), "0");
    }

    #[test]
    fn failed_fetches_without_data_do_not_also_claim_outdated_data() {
        assert_eq!(
            refresh_error_key("dashboard.refresh_failed", false),
            "dashboard.status_fetch_failed"
        );
        assert_eq!(
            refresh_error_key("dashboard.refresh_failed", true),
            "dashboard.refresh_failed"
        );
        assert_eq!(
            refresh_error_key("dashboard.status_identity_mismatch", false),
            "dashboard.status_identity_mismatch"
        );
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
    fn unnamed_expeditions_use_numbered_labels_without_inventing_locations() {
        let ui = Preferences::default();
        assert_eq!(expedition_label(Some("Crystal"), 0, ui), "Crystal");
        assert_eq!(expedition_label(None, 0, ui), "派遣 1");
        assert_eq!(expedition_label(Some("  "), 1, ui), "派遣 2");
    }

    #[test]
    fn recovery_countdown_and_finished_expeditions_show_only_what_is_left() {
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
        assert!(expedition_finished(Some("Finished")));
        assert!(!expedition_finished(Some("Ongoing")));
        assert!(!expedition_finished(None));
    }

    #[test]
    fn expedition_time_counts_down_from_the_fetch() {
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

    #[test]
    fn game_tab_keyboard_navigation_wraps_and_reaches_edges() {
        assert_eq!(next_game_tab("genshin", &Key::ArrowLeft), Some("starrail"));
        assert_eq!(next_game_tab("starrail", &Key::ArrowRight), Some("genshin"));
        assert_eq!(next_game_tab("starrail", &Key::Home), Some("genshin"));
        assert_eq!(next_game_tab("genshin", &Key::End), Some("starrail"));
        assert_eq!(next_game_tab("genshin", &Key::Enter), None);
    }
}
