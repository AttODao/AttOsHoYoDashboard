use super::common::{Icon, Metric, focus_element, game_name, reset_checked};
use crate::app::actions::{spawn_action, spawn_root};
use crate::app::data::{load_game_status, save_reward_setting};
use crate::app::display::{compact_datetime, count_pair, hours_minutes};
use crate::app::preferences::{Preferences, use_preferences};
use crate::app::state::{AppState, DASHBOARD_GAMES, Page, set_page};
use crate::models::{DailySummary, Expedition, GameStatus};
use dioxus::prelude::*;
use uuid::Uuid;

#[component]
pub(super) fn DashboardPage(state: Signal<AppState>) -> Element {
    let ui = use_preferences();
    let mut active = use_signal(|| "genshin");
    let mobile = use_mobile_dashboard();
    rsx! {
        div { class: "page-stack dashboard",
            h1 { {ui.text("dashboard.dashboard")} }
            if state.read().accounts_error.is_some() {
                p { class: "warning", role: "status", {ui.text("dashboard.could_not_refresh_the_uid_list")} }
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

// One media listener, removed on unmount. CSS and semantics use the same 900px boundary.
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

#[component]
fn GamePanel(
    mut state: Signal<AppState>,
    game: &'static str,
    active: bool,
    mobile: bool,
) -> Element {
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
    let status = selected.and_then(|id| {
        snapshot
            .account_statuses
            .iter()
            .find(|entry| entry.id == id)
            .map(|entry| entry.status.clone())
    });
    let setting = snapshot
        .rewards
        .iter()
        .find(|setting| setting.game_kind == game)
        .cloned();
    let rewards_error = snapshot.rewards_error.is_some();
    let posting_available = snapshot
        .me
        .as_ref()
        .is_some_and(|me| me.capabilities.reward_posting_available);
    let accounts_loaded = snapshot.accounts.is_some();
    let accounts_failed = snapshot.accounts_error.is_some();
    let busy = snapshot.busy;
    let loading = selection.loading && selection.generation == snapshot.generation;
    drop(snapshot);
    rsx! {
        section {
            class: "panel game-panel {game}", id: "game-panel-{game}",
            role: if mobile { "tabpanel" } else { "region" },
            "aria-labelledby": if mobile { format!("game-tab-{game}") } else { format!("game-title-{game}") },
            "data-selected": active, hidden: mobile && !active,
            tabindex: if mobile { "0" } else { "-1" },
            h2 { class: "game-title", id: "game-title-{game}", {game_name(game, ui)} }
            div { class: "card-automation",
                span { {ui.text("dashboard.login_bonus")} }
                if let Some(setting) = setting.clone() {
                    label { class: "switch",
                        input {
                            id: "automation-{game}", r#type: "checkbox", role: "switch", checked: setting.enabled,
                            disabled: busy || rewards_error,
                            "aria-label": ui.format("dashboard.automation_label", &[("game", game_name(game, ui))]),
                            onchange: move |event| {
                                let id = setting.id;
                                let enabled = event.checked();
                                let local_time = setting.local_time.clone();
                                spawn_action(state, move |generation| async move {
                                    save_reward_setting(state, generation, id, enabled, local_time, "dashboard.automation_setting_saved").await;
                                    let stored = state.read().rewards.iter().find(|row| row.id == id).map(|row| row.enabled);
                                    if let Some(stored) = stored { reset_checked(&format!("automation-{game}"), stored); }
                                });
                            }
                        }
                        span { class: "switch-track", "aria-hidden": "true" }
                        span { if setting.enabled { {ui.text("dashboard.on")} } else { {ui.text("dashboard.off")} } }
                    }
                } else {
                    span { {ui.text("dashboard.not_fetched")} }
                }
            }
            if rewards_error {
                p { class: "warning", {ui.text("dashboard.could_not_load_bonus_settings")} }
            }
            if !posting_available {
                p { class: "warning", {ui.text("dashboard.reward_posting_is_unavailable_on_this_server")} }
            }
            if let Some(setting) = setting {
                details { class: "bonus-status",
                    summary {
                        {ui.text("dashboard.today_s_reward")} ": " {received(setting.is_sign, ui)}
                        if let Some(run) = &setting.last_run {
                            " · " {ui.text("dashboard.last_run_state")} ": " {model_status(&run.state, ui)}
                        }
                        if setting.auth_status == "action_required" {
                            " · " {model_status(&setting.auth_status, ui)}
                        }
                        if let Some(code) = &setting.error_code {
                            " · " {ui.text("dashboard.error")} ": {code}"
                        }
                    }
                    dl { class: "metrics",
                    Metric { label: ui.text("dashboard.today_s_reward"), value: received(setting.is_sign, ui).to_owned() }
                    Metric { label: ui.text("dashboard.auth_status"), value: model_status(&setting.auth_status, ui).to_owned() }
                    Metric { label: ui.text("dashboard.checked"), value: date(&setting.last_checked_at, ui) }
                    Metric { label: ui.text("dashboard.next_run"), value: date(&setting.next_run_at, ui) }
                    Metric { label: ui.text("dashboard.last_run_state"), value: setting.last_run.as_ref().map(|run| model_status(&run.state, ui).to_owned()).unwrap_or_else(|| ui.text("dashboard.never_run").to_owned()) }
                    Metric { label: ui.text("dashboard.bonus_error"), value: setting.error_code.clone().unwrap_or_else(|| ui.text("dashboard.none").to_owned()) }
                    Metric { label: ui.text("dashboard.error_at"), value: date(&setting.error_at, ui) }
                    }
                }
            }
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
                        id: "selected-{game}", value: selected.map(|id| id.to_string()).unwrap_or_default(), disabled: busy,
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
                    p { class: "warning", role: "status", {ui.text(key)} }
                }
                div { "aria-busy": loading,
                    if let Some(status) = status {
                        GameStatusCard { status }
                    } else if loading {
                        p { role: "status", {ui.text("dashboard.loading")} }
                    } else {
                        p { {ui.text("dashboard.status_has_not_been_fetched")} }
                    }
                }
                button {
                    r#type: "button", disabled: busy || loading,
                    onclick: move |_| { spawn_root(async move { load_game_status(state, game).await; }); },
                    if loading { {ui.text("dashboard.refreshing")} } else { {ui.text("dashboard.refresh")} }
                }
            }
        }
    }
}

#[component]
fn GameStatusCard(status: GameStatus) -> Element {
    let ui = use_preferences();
    let yes_no = |value| {
        if value {
            ui.text("dashboard.yes")
        } else {
            ui.text("dashboard.no")
        }
        .to_owned()
    };
    rsx! {
        div { class: "status-card",
            if status.stale { p { class: "warning", {ui.text("dashboard.cached_status_is_stale")} } }
            if let Some(data) = status.data.as_ref() {
                if status.game_kind == "genshin" {
                    Resource { label: ui.text("dashboard.resin"), current: data.current_resin, maximum: data.max_resin, full_at: data.resin_full_at.clone() }
                    dl { class: "metrics",
                        Metric { label: ui.text("dashboard.realm_currency"), value: pair(data.current_home_coin, data.max_home_coin, ui) }
                        Estimate { label: ui.text("dashboard.currency_full_at"), value: data.home_coin_full_at.clone() }
                        Metric { label: ui.text("dashboard.commissions"), value: pair(data.finished_task_num, data.total_task_num, ui) }
                        Metric { label: ui.text("dashboard.extra_commission_reward"), value: received(data.is_extra_task_reward_received, ui).to_owned() }
                    }
                } else if status.game_kind == "starrail" {
                    Resource { label: ui.text("dashboard.trailblaze_power"), current: data.current_stamina, maximum: data.max_stamina, full_at: data.stamina_full_at.clone() }
                    dl { class: "metrics",
                        Metric { label: ui.text("dashboard.reserved_trailblaze_power"), value: number(data.current_reserve_stamina, ui) }
                        Metric { label: ui.text("dashboard.daily_training"), value: pair(data.current_train_score, data.max_train_score, ui) }
                    }
                }
                if status.game_kind == "genshin" {
                    ExpeditionList { expeditions: data.expeditions.clone() }
                }
            }
            if let Some(key) = missing_payload_notice(status.data.as_ref()) {
                p { {ui.text(key)} }
            }
            details { class: "status-details",
                summary {
                    {ui.text("dashboard.data_details")}
                    if let Some(code) = &status.error_code { " · {code}" }
                }
                dl { class: "metrics status-metadata",
                    Metric { label: ui.text("dashboard.uid"), value: if status.game_uid.is_empty() { ui.text("dashboard.missing").to_owned() } else { status.game_uid.clone() } }
                    Metric { label: ui.text("dashboard.region"), value: status.region.clone() }
                    Metric { label: ui.text("dashboard.last_fetched"), value: date(&status.fetched_at, ui) }
                    Metric { label: ui.text("dashboard.stale"), value: yes_no(status.stale) }
                    Metric { label: ui.text("dashboard.visible"), value: yes_no(status.visible) }
                    Metric { label: ui.text("dashboard.detected"), value: yes_no(status.detected) }
                    Metric { label: ui.text("dashboard.error"), value: status.error_code.clone().unwrap_or_else(|| ui.text("dashboard.none").to_owned()) }
                    Metric { label: ui.text("dashboard.error_at"), value: date(&status.error_at, ui) }
                }
            }
        }
    }
}

#[component]
fn Resource(
    label: &'static str,
    current: Option<i64>,
    maximum: Option<i64>,
    full_at: Option<String>,
) -> Element {
    let ui = use_preferences();
    rsx! {
        p { class: "resource-label", "{label}" }
        div { class: "resource",
            strong { {number(current, ui)} }
            if let Some(maximum) = maximum { span { "/ {maximum}" } }
        }
        if let (Some(value), Some(max)) = (current, maximum) {
            if max > 0 && value >= 0 && value <= max {
                progress { value: "{value}", max: "{max}", "aria-label": label }
            }
        }
        dl { class: "row", Estimate { label: ui.text("dashboard.full_recovery_at"), value: full_at, numeric: true } }
    }
}

#[component]
fn Estimate(
    label: &'static str,
    value: Option<String>,
    #[props(default)] numeric: bool,
) -> Element {
    let ui = use_preferences();
    let formatted = compact_datetime(value.as_deref());
    rsx! {
        div { class: "metric",
            dt { "{label}" }
            dd {
                if let Some(formatted) = formatted {
                    time { class: if numeric { "recovery-at" } else { "estimate-at" }, datetime: value, "{formatted}" }
                } else {
                    {ui.text("dashboard.not_reported")}
                }
            }
        }
    }
}

#[component]
fn ExpeditionList(expeditions: Option<Vec<Expedition>>) -> Element {
    let ui = use_preferences();
    rsx! {
        div { class: "expeditions",
            h3 { {ui.text("dashboard.expeditions")} }
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
                            span { {expedition.status.as_deref().map(|value| model_status(value, ui)).unwrap_or(ui.text("dashboard.unknown"))} }
                            span { {ui.text("dashboard.remaining")} ": " {expedition.remained_time.or(expedition.remaining_time).map(|duration| duration.seconds().map(|seconds| hours_minutes(seconds, ui)).unwrap_or_else(|| duration.to_string())).unwrap_or_else(|| ui.text("dashboard.not_reported").to_owned())} }
                        }
                    }
                }
            } else { p { {ui.text("dashboard.not_reported")} } }
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

fn missing_payload_notice(data: Option<&DailySummary>) -> Option<&'static str> {
    data.is_none()
        .then_some("dashboard.no_daily_summary_payload_returned")
}

fn number(value: Option<i64>, ui: Preferences) -> String {
    value
        .map(|n| n.to_string())
        .unwrap_or_else(|| ui.text("dashboard.not_reported").to_owned())
}

fn pair(current: Option<i64>, maximum: Option<i64>, ui: Preferences) -> String {
    current
        .map(|value| count_pair(value, maximum))
        .unwrap_or_else(|| ui.text("dashboard.not_reported").to_owned())
}

fn date(value: &Option<String>, ui: Preferences) -> String {
    value
        .clone()
        .unwrap_or_else(|| ui.text("dashboard.not_reported").to_owned())
}

fn received(value: Option<bool>, ui: Preferences) -> &'static str {
    match value {
        Some(true) => ui.text("dashboard.received"),
        Some(false) => ui.text("dashboard.not_received"),
        None => ui.text("dashboard.not_fetched"),
    }
}

fn model_status(value: &str, ui: Preferences) -> &str {
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
        other => other,
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
    fn unnamed_expeditions_use_numbered_labels_without_inventing_locations() {
        let ui = Preferences::default();
        assert_eq!(expedition_label(Some("Crystal"), 0, ui), "Crystal");
        assert_eq!(expedition_label(None, 0, ui), "派遣 1");
        assert_eq!(expedition_label(Some("  "), 1, ui), "派遣 2");
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
