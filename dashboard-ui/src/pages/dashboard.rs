use super::common::{Metric, focus_element};
use crate::app::actions::{spawn_action, spawn_root};
use crate::app::data::{load_game_status, save_reward_setting};
use crate::app::display::count_pair;
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
            h1 { {ui.text("ダッシュボード", "Dashboard")} }
            if state.read().accounts_error.is_some() {
                p { class: "warning", role: "status", {ui.text("UID一覧を更新できませんでした。", "Could not refresh the UID list.")} }
            }
            div { class: "game-tabs tabs", role: "tablist", "aria-label": ui.text("ゲーム", "Games"),
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
                        {localized_game_name(game, ui)}
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
            h2 { class: "game-title", id: "game-title-{game}", {localized_game_name(game, ui)} }
            div { class: "card-automation",
                span { {ui.text("ログインボーナス", "Login bonus")} }
                if let Some(setting) = setting.clone() {
                    label { class: "switch",
                        input {
                            r#type: "checkbox", role: "switch", checked: setting.enabled,
                            disabled: busy || rewards_error,
                            "aria-label": format!("{} · {}", localized_game_name(game, ui), ui.text("自動受け取り", "Automatic claiming")),
                            onchange: move |event| {
                                let id = setting.id;
                                let enabled = event.checked();
                                let local_time = setting.local_time.clone();
                                spawn_action(state, move |generation| async move {
                                    save_reward_setting(state, generation, id, enabled, local_time, ui.text("自動受け取り設定を保存しました。", "Automation setting saved.")).await;
                                });
                            }
                        }
                        span { class: "switch-track", "aria-hidden": "true" }
                        span { if setting.enabled { {ui.text("ON", "ON")} } else { {ui.text("OFF", "OFF")} } }
                    }
                } else {
                    span { {ui.text("未取得", "Not fetched")} }
                }
            }
            if rewards_error {
                p { class: "warning", {ui.text("ボーナス設定を取得できませんでした。", "Could not load bonus settings.")} }
            }
            if !posting_available {
                p { class: "warning", {ui.text("このサーバーでは報酬の受け取りを利用できません。", "Reward posting is unavailable on this server.")} }
            }
            if let Some(setting) = setting {
                details { class: "bonus-status",
                    summary {
                        {ui.text("当日分", "Today's reward")} ": " {received(setting.is_sign, ui)}
                        if let Some(run) = &setting.last_run {
                            " · " {ui.text("前回の実行状態", "Last run state")} ": " {model_status(&run.state, ui)}
                        }
                        if setting.auth_status == "action_required" {
                            " · " {model_status(&setting.auth_status, ui)}
                        }
                        if let Some(code) = &setting.error_code {
                            " · " {ui.text("エラー", "Error")} ": {code}"
                        }
                    }
                    dl { class: "metrics",
                    Metric { label: ui.text("当日分", "Today's reward"), value: received(setting.is_sign, ui).to_owned() }
                    Metric { label: ui.text("認証状態", "Auth status"), value: model_status(&setting.auth_status, ui).to_owned() }
                    Metric { label: ui.text("確認日時", "Checked"), value: date(&setting.last_checked_at, ui) }
                    Metric { label: ui.text("次回実行", "Next run"), value: date(&setting.next_run_at, ui) }
                    Metric { label: ui.text("前回の実行状態", "Last run state"), value: setting.last_run.as_ref().map(|run| model_status(&run.state, ui).to_owned()).unwrap_or_else(|| ui.text("実行履歴なし", "Never run").to_owned()) }
                    Metric { label: ui.text("ボーナスエラー", "Bonus error"), value: setting.error_code.clone().unwrap_or_else(|| ui.text("なし", "None").to_owned()) }
                    Metric { label: ui.text("エラー日時", "Error at"), value: date(&setting.error_at, ui) }
                    }
                }
            }
            if visible.is_empty() {
                div { class: "empty",
                    h3 { {ui.text("表示するUIDがありません", "No visible UIDs")} }
                    button { r#type: "button", onclick: move |_| set_page(state, Page::Uids), {ui.text("UIDを選ぶ", "Choose UIDs")} }
                }
            } else {
                label { class: "uid-select", r#for: "selected-{game}",
                    span { {ui.text("表示中のUID", "Selected UID")} }
                    select {
                        id: "selected-{game}", value: selected.map(|id| id.to_string()).unwrap_or_default(), disabled: busy,
                        onchange: move |event| {
                            if let Ok(id) = event.value().parse::<Uuid>() {
                                state.write().select_game_account(game, Some(id));
                                spawn_root(async move { load_game_status(state, game).await; });
                            }
                        },
                        for account in visible {
                            option { value: "{account.id}", "{account.region} · UID {account.game_uid}" }
                        }
                    }
                }
                if selection.error.is_some() {
                    p { class: "warning", role: "status", {ui.text("状態を更新できませんでした。表示中のデータは最新ではない可能性があります。", "Could not refresh status. Displayed data may be out of date.")} }
                }
                div { "aria-busy": loading,
                    if let Some(status) = status {
                        GameStatusCard { status }
                    } else if loading {
                        p { role: "status", {ui.text("読み込み中…", "Loading…")} }
                    } else {
                        p { {ui.text("状態はまだ取得されていません。", "Status has not been fetched.")} }
                    }
                }
                button {
                    r#type: "button", disabled: busy || loading,
                    onclick: move |_| { spawn_root(async move { load_game_status(state, game).await; }); },
                    if loading { {ui.text("更新中…", "Refreshing…")} } else { {ui.text("再取得", "Refresh")} }
                }
            }
        }
    }
}

#[component]
fn GameStatusCard(status: GameStatus) -> Element {
    let ui = use_preferences();
    let yes_no = |value| {
        ui.text(
            if value { "はい" } else { "いいえ" },
            if value { "Yes" } else { "No" },
        )
        .to_owned()
    };
    rsx! {
        div { class: "status-card",
            if status.stale { p { class: "warning", {ui.text("キャッシュは最新ではありません", "Cached status is stale")} } }
            if let Some(data) = status.data.as_ref() {
                if status.game_kind == "genshin" {
                    Resource { label: ui.text("天然樹脂", "Resin"), current: data.current_resin, maximum: data.max_resin, full_at: data.resin_full_at.clone() }
                    dl { class: "metrics",
                        Metric { label: ui.text("洞天宝銭", "Realm currency"), value: pair(data.current_home_coin, data.max_home_coin, ui) }
                        Metric { label: ui.text("宝銭上限の目安", "Currency full at"), value: date(&data.home_coin_full_at, ui) }
                        Metric { label: ui.text("デイリー依頼", "Commissions"), value: pair(data.finished_task_num, data.total_task_num, ui) }
                        Metric { label: ui.text("追加依頼報酬", "Extra commission reward"), value: received(data.is_extra_task_reward_received, ui).to_owned() }
                    }
                } else if status.game_kind == "starrail" {
                    Resource { label: ui.text("開拓力", "Trailblaze Power"), current: data.current_stamina, maximum: data.max_stamina, full_at: data.stamina_full_at.clone() }
                    dl { class: "metrics",
                        Metric { label: ui.text("予備開拓力", "Reserved Trailblaze Power"), value: number(data.current_reserve_stamina, ui) }
                        Metric { label: ui.text("デイリートレーニング", "Daily training"), value: pair(data.current_train_score, data.max_train_score, ui) }
                    }
                }
                ExpeditionList { expeditions: data.expeditions.clone() }
            }
            if let Some((ja, en)) = missing_payload_notice(status.data.as_ref()) {
                p { {ui.text(ja, en)} }
            }
            details { class: "status-details",
                summary {
                    {ui.text("取得情報", "Data details")}
                    if let Some(code) = &status.error_code { " · {code}" }
                }
                dl { class: "metrics status-metadata",
                    Metric { label: ui.text("UID", "UID"), value: if status.game_uid.is_empty() { ui.text("未登録", "Missing").to_owned() } else { status.game_uid.clone() } }
                    Metric { label: ui.text("サーバー", "Region"), value: status.region.clone() }
                    Metric { label: ui.text("最終取得", "Last fetched"), value: date(&status.fetched_at, ui) }
                    Metric { label: ui.text("古いデータ", "Stale"), value: yes_no(status.stale) }
                    Metric { label: ui.text("表示対象", "Visible"), value: yes_no(status.visible) }
                    Metric { label: ui.text("検出済み", "Detected"), value: yes_no(status.detected) }
                    Metric { label: ui.text("エラー", "Error"), value: status.error_code.clone().unwrap_or_else(|| ui.text("なし", "None").to_owned()) }
                    Metric { label: ui.text("エラー日時", "Error at"), value: date(&status.error_at, ui) }
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
        dl { class: "row", Metric { label: ui.text("全回復の目安", "Full recovery at"), value: date(&full_at, ui) } }
    }
}

#[component]
fn ExpeditionList(expeditions: Option<Vec<Expedition>>) -> Element {
    let ui = use_preferences();
    rsx! {
        div { class: "expeditions",
            h3 { {ui.text("派遣", "Expeditions")} }
            if let Some(rows) = expeditions {
                if rows.is_empty() { p { {ui.text("派遣なし", "No expeditions")} } }
                ul {
                    for expedition in rows {
                        li { class: "expedition",
                            span { {expedition.name.as_deref().unwrap_or(ui.text("名称未取得", "Unnamed"))} }
                            span { {expedition.status.as_deref().map(|value| model_status(value, ui)).unwrap_or(ui.text("不明", "Unknown"))} }
                            span { {ui.text("残り", "Remaining")} ": " {expedition.remained_time.or(expedition.remaining_time).map(|duration| duration.to_string()).unwrap_or_else(|| ui.text("未取得", "Not reported").to_owned())} }
                        }
                    }
                }
            } else { p { {ui.text("未取得", "Not reported")} } }
        }
    }
}

fn missing_payload_notice(data: Option<&DailySummary>) -> Option<(&'static str, &'static str)> {
    data.is_none().then_some((
        "デイリー概要のデータがありません。",
        "No daily summary payload returned.",
    ))
}

fn localized_game_name(game: &str, ui: Preferences) -> &'static str {
    if game == "genshin" {
        ui.text("原神", "Genshin Impact")
    } else {
        ui.text("崩壊：スターレイル", "Honkai: Star Rail")
    }
}

fn number(value: Option<i64>, ui: Preferences) -> String {
    value
        .map(|n| n.to_string())
        .unwrap_or_else(|| ui.text("未取得", "Not reported").to_owned())
}

fn pair(current: Option<i64>, maximum: Option<i64>, ui: Preferences) -> String {
    if current.is_none() {
        ui.text("未取得", "Not reported").to_owned()
    } else {
        count_pair(current, maximum)
    }
}

fn date(value: &Option<String>, ui: Preferences) -> String {
    value
        .clone()
        .unwrap_or_else(|| ui.text("未取得", "Not reported").to_owned())
}

fn received(value: Option<bool>, ui: Preferences) -> &'static str {
    match value {
        Some(true) => ui.text("受取済み", "Received"),
        Some(false) => ui.text("未受取", "Not received"),
        None => ui.text("未取得", "Not fetched"),
    }
}

fn model_status(value: &str, ui: Preferences) -> &str {
    match value {
        "pending" | "queued" => ui.text("受付済み・完了未確認", "Queued; completion unconfirmed"),
        "running" => ui.text("実行中", "Running"),
        "retry_wait" => ui.text("再試行待ち", "Waiting to retry"),
        "action_required" => ui.text("対応が必要", "Action required"),
        "stopped" => ui.text("停止", "Stopped"),
        "success" => ui.text("受取確認済み", "Claim confirmed"),
        "already_claimed" => ui.text("当日分受取済み", "Already claimed today"),
        "ready" => ui.text("利用可能", "Ready"),
        "unknown" => ui.text("結果不明", "Unknown outcome"),
        "done" => ui.text("終了", "Done"),
        "failed" => ui.text("失敗", "Failed"),
        "verified" => ui.text("確認済み", "Verified"),
        "invalid" => ui.text("無効", "Invalid"),
        "Ongoing" => ui.text("進行中", "Ongoing"),
        "Finished" => ui.text("完了", "Finished"),
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
            Some((
                "デイリー概要のデータがありません。",
                "No daily summary payload returned."
            ))
        );
        let summary: DailySummary = serde_json::from_value(serde_json::json!({})).unwrap();
        assert_eq!(missing_payload_notice(Some(&summary)), None);
        assert!(summary.current_resin.is_none());
        assert!(summary.max_resin.is_none());
        assert!(summary.current_stamina.is_none());
        assert!(summary.max_stamina.is_none());
        assert!(summary.expeditions.is_none());
        assert_eq!(
            count_pair(summary.current_resin, summary.max_resin),
            "Not reported"
        );
        assert_eq!(count_pair(Some(0), None), "0");
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
