//! The Codes settings tab (redemption codes), built to the approved mockup (variant A+):
//! notices, the code list status, a mode per game UID with its last result and per-game
//! summary chips, code entry with redeem now, and the redemption history.
//! `app::codes` holds the reads, the actions and the rules.
use super::common::{Field, HintFold, LoadError, Skeleton, WarningLine, game_name, reset_value};
use super::icons::{Glyph, Icon, NamedIcon, Status, StatusMark};
use crate::app::actions::spawn_action;
use crate::app::codes::{
    ChipCounts, CodeResult, CodeRow, HISTORY_PAGE, chip_counts, code_rows, enter_code, last_result,
    load_codes, manual_code, redeem_now, reward_text, save_code_mode,
};
use crate::app::display::{
    compact_datetime, counted, local_moment, relative_age, seconds_since, short_time,
};
use crate::app::preferences::{Preferences, use_preferences};
use crate::app::state::{AppState, DASHBOARD_GAMES, Page, set_page};
use crate::components::{Button, ButtonVariant};
use crate::models::{CodeCatalogue, CodeRedemption, CodeSetting};
use dioxus::prelude::*;
use uuid::Uuid;

const AUTO_HINT_ID: &str = "auto-needs-cookie";

#[component]
pub(super) fn CodesPage(state: Signal<AppState>) -> Element {
    let ui = use_preferences();
    let read = state.read();
    let loaded = (
        read.code_settings.clone(),
        read.code_catalogue.clone(),
        read.code_redemption.clone(),
    );
    let failed = read.codes_error.is_some();
    let unlinked = read.codes_unlinked;
    drop(read);
    rsx! {
        section { class: "panel page-stack codes-page",
            h2 { {ui.text("codes.tab")} }
            if failed {
                LoadError { state, message: ui.text("codes.load_failed") }
            } else if unlinked {
                NoLink { state }
            } else if let (Some(settings), Some(catalogue), Some(redemption)) = loaded {
                CodesBody { state, settings, catalogue, redemption }
            } else {
                Skeleton { label: ui.text("codes.loading"), rows: 4 }
            }
        }
    }
}

#[component]
fn NoLink(state: Signal<AppState>) -> Element {
    let ui = use_preferences();
    rsx! {
        div { class: "page-stack",
            h3 { {ui.text("codes.nolink_title")} }
            p { class: "field-hint", {ui.text("codes.nolink_body")} }
            div { class: "button-row",
                Button { r#type: "button", onclick: move |_| set_page(state, Page::Link),
                    Glyph { icon: Icon::Link }
                    span { {ui.text("codes.open_link_tab")} }
                }
            }
        }
    }
}

/// What stops code entry, most fundamental first.
fn blocked_reason(stopped: bool, credentials: bool, action: bool) -> Option<&'static str> {
    if stopped {
        Some("codes.manual_blocked_admin")
    } else if !credentials {
        Some("codes.manual_blocked_cookie")
    } else if action {
        Some("codes.manual_blocked_action")
    } else {
        None
    }
}

#[component]
fn CodesBody(
    state: Signal<AppState>,
    settings: Vec<CodeSetting>,
    catalogue: CodeCatalogue,
    redemption: CodeRedemption,
) -> Element {
    let ui = use_preferences();
    let stopped = !catalogue.enabled;
    let credentials = redemption.redemption_credentials;
    let action = redemption.action_required.clone();
    let history = state.read().code_history.clone().unwrap_or_default();
    let age = |value: &str| seconds_since(Some(value));
    let rows = code_rows(&history, Some(&catalogue), &settings, &age);
    let games: Vec<&'static str> = DASHBOARD_GAMES
        .into_iter()
        .filter(|game| settings.iter().any(|setting| setting.game_kind == *game))
        .collect();
    let action_at = short_at(redemption.action_at.as_deref(), ui).unwrap_or_default();
    let action_code = redemption
        .action_retcode
        .map(|code| ui.format("codes.action_retcode", &[("code", &code.to_string())]));
    let cookie_body = cookie_notice_body(&redemption, ui);
    rsx! {
        if stopped {
            div { class: "notice with-icon", role: "status",
                Glyph { icon: Icon::Ban }
                div { class: "notice-body",
                    strong { {ui.text("codes.admin_stopped_title")} }
                    p { {ui.text("codes.admin_stopped_body")} }
                }
            }
        } else if !credentials {
            div { class: "notice with-icon", role: "status", id: "codes-cookie-notice",
                Glyph { icon: Icon::Link }
                div { class: "notice-body",
                    strong { {ui.text("codes.cookie_reregister_title")} }
                    p { {ui.text("codes.cookie_reregister_body")} }
                }
                div { class: "button-row",
                    Button { variant: ButtonVariant::Secondary, r#type: "button", onclick: move |_| set_page(state, Page::Link),
                        Glyph { icon: Icon::Link }
                        span { {ui.text("codes.open_link_tab")} }
                    }
                }
            }
        }
        match action.as_deref() {
            Some("cookie_invalid") => rsx! {
                div { class: "warning with-icon notice error", role: "alert", id: "codes-action",
                    Glyph { icon: Icon::Failed }
                    div { class: "notice-body",
                        strong { {ui.text("codes.action_cookie_title")} }
                        p { id: "codes-action-body", "{cookie_body}" }
                        if let Some(code) = action_code.clone() { p { id: "codes-action-retcode", "{code}" } }
                    }
                    div { class: "button-row",
                        Button { r#type: "button", onclick: move |_| set_page(state, Page::Link),
                            Glyph { icon: Icon::Link }
                            span { {ui.text("codes.open_link_tab")} }
                        }
                    }
                }
            },
            Some(_) => rsx! {
                div { class: "warning with-icon notice warn", role: "alert", id: "codes-action",
                    Glyph { icon: Icon::Warning }
                    div { class: "notice-body",
                        strong { {ui.text("codes.action_captcha_title")} }
                        p { {ui.format("codes.action_captcha_body", &[("at", &action_at)])} }
                        if let Some(code) = action_code.clone() { p { id: "codes-action-retcode", "{code}" } }
                    }
                    div { class: "button-row",
                        Button { id: "codes-resume", r#type: "button", disabled: stopped, onclick: move |_| {
                            spawn_action(state, move |generation| async move { redeem_now(state, generation).await; });
                        },
                            Glyph { icon: Icon::Refresh }
                            span { {ui.text("codes.resume")} }
                        }
                    }
                }
            },
            None => rsx! {},
        }
        if !stopped { CatalogueLine { catalogue: catalogue.clone() } }
        HintFold { title: ui.text("codes.about_title"),
            p { class: "field-hint", {ui.text("codes.about_source")} }
            p { class: "field-hint", {ui.text("codes.about_modes")} }
            p { class: "field-hint", {ui.text("codes.about_rewards")} }
        }
        if !credentials {
            p { class: "field-hint", id: AUTO_HINT_ID, {ui.text("codes.auto_needs_cookie_hint")} }
        }
        div { class: "form-grid",
            for game in games.clone() {
                div { key: "{game}", class: "form-stack reward-card {game}",
                    div { class: "game-head",
                        h3 { class: "icon-title",
                            if let Some(mark) = Icon::game_mark(game) { Glyph { icon: mark } }
                            {game_name(game, ui)}
                        }
                        SummaryChips { counts: chip_counts(&rows, game) }
                    }
                    for setting in settings.iter().filter(|setting| setting.game_kind == game).cloned() {
                        ModeField {
                            key: "{setting.game_account_id}", state, last: last_result(&rows, setting.game_account_id).cloned(),
                            setting: setting.clone(), stopped, credentials, paused: action.is_some(),
                        }
                    }
                }
            }
        }
        hr { class: "divider" }
        ManualEntry { state, settings: settings.clone(), games, stopped, credentials, action: action.is_some(), pending: redemption.pending }
        hr { class: "divider" }
        History { state, rows: rows.clone() }
    }
}

/// The warning while fetching the code list fails. Without any successful fetch there are
/// no listed codes yet, so it says when they start instead of naming a last success; one
/// failure and several read differently in English.
fn source_warning_key(failures: i32, fetched_before: bool) -> Option<&'static str> {
    Some(match (failures, fetched_before) {
        (..=0, _) => return None,
        (1, true) => "codes.source_unavailable_one",
        (_, true) => "codes.source_unavailable",
        (1, false) => "codes.source_never_one",
        (_, false) => "codes.source_never",
    })
}

/// When the code list was fetched and when the next fetch is due, or that the source
/// keeps failing.
#[component]
fn CatalogueLine(catalogue: CodeCatalogue) -> Element {
    let ui = use_preferences();
    let last_success = catalogue.last_success_at.as_deref();
    if let Some(key) = source_warning_key(catalogue.consecutive_failures, last_success.is_some()) {
        let at = short_at(last_success, ui).unwrap_or_default();
        return rsx! {
            WarningLine { status: Status::Warn, text: ui.format(key, &[("count", &catalogue.consecutive_failures.to_string()), ("at", &at)]) }
        };
    }
    // Before the first fetch the line says so instead of leaving "next" alone.
    let fetched = match last_success {
        Some(_) => seconds_since(last_success).map(|seconds| {
            ui.format(
                "codes.catalogue_fetched",
                &[("ago", &relative_age(seconds, ui))],
            )
        }),
        None => Some(ui.text("codes.catalogue_not_yet").to_owned()),
    };
    let next = short_at(catalogue.next_fetch_at.as_deref(), ui);
    if fetched.is_none() && next.is_none() {
        return rsx! {};
    }
    rsx! {
        p { class: "catalogue-line", role: "status",
            Glyph { icon: Icon::Clock }
            if let Some(fetched) = &fetched { span { class: "part", "{fetched}" } }
            if fetched.is_some() && next.is_some() { span { "aria-hidden": "true", "·" } }
            if let Some(next) = &next { span { class: "part", {ui.format("codes.catalogue_next", &[("eta", next)])} } }
        }
    }
}

/// The per-game summary: symbol and count on screen, the sentence for readers and the
/// tooltip. Zero counts are left out.
#[component]
fn SummaryChips(counts: ChipCounts) -> Element {
    let ui = use_preferences();
    // The sentence for one chip and for several (English verbs agree with the count).
    let chips: Vec<(Status, usize, &'static str, &'static str)> = [
        (
            Status::Done,
            counts.done,
            "codes.chip_redeemed",
            "codes.chip_redeemed",
        ),
        (
            Status::Open,
            counts.open,
            "codes.chip_announced",
            "codes.chip_announced",
        ),
        (
            Status::Waiting,
            counts.waiting,
            "codes.chip_waiting",
            "codes.chip_waiting",
        ),
        (
            Status::Warn,
            counts.warn,
            "codes.chip_attention_one",
            "codes.chip_attention",
        ),
    ]
    .into_iter()
    .filter(|(_, count, _, _)| *count > 0)
    .collect();
    if chips.is_empty() {
        return rsx! {};
    }
    rsx! {
        ul { class: "summary-chips", "aria-label": ui.text("codes.summary_label"),
            for (status, count, one, other) in chips {
                li { key: "{other}",
                    StatusMark { status, text: count.to_string(), label: counted(ui, count as i64, one, other, &[("n", &count.to_string())]) }
                }
            }
        }
    }
}

#[component]
fn ModeField(
    state: Signal<AppState>,
    setting: CodeSetting,
    last: Option<CodeRow>,
    stopped: bool,
    credentials: bool,
    paused: bool,
) -> Element {
    let ui = use_preferences();
    let id = setting.game_account_id;
    let element = format!("code-mode-{id}");
    let written = element.clone();
    let label = format!("UID {} · {}", setting.game_uid, setting.region);
    let paused = paused && setting.mode == "auto";
    rsx! {
        div { class: "form-stack",
            Field { id: element.clone(), label,
                select { id: element.clone(), value: setting.mode.clone(), disabled: stopped,
                    "aria-describedby": (!credentials).then_some(AUTO_HINT_ID),
                    onchange: move |event| {
                        let mode = event.value();
                        let element = written.clone();
                        spawn_action(state, move |generation| async move {
                            save_code_mode(state, generation, id, mode).await;
                            let stored = state.read().code_settings.as_ref().and_then(|rows| {
                                rows.iter().find(|row| row.game_account_id == id).map(|row| row.mode.clone())
                            });
                            if let Some(stored) = stored { reset_value(&element, &stored); }
                        });
                    },
                    option { value: "off", selected: setting.mode == "off", {ui.text("codes.mode_off")} }
                    option { value: "notify", selected: setting.mode == "notify", {ui.text("codes.mode_notify")} }
                    option { value: "auto", selected: setting.mode == "auto", disabled: !credentials,
                        if credentials { {ui.text("codes.mode_auto")} } else { {ui.text("codes.mode_auto_needs_cookie")} }
                    }
                }
            }
            p { class: if paused { "run-state error" } else { "run-state" }, id: "code-last-{id}",
                {ui.text("codes.last_result")} ": "
                if paused {
                    StatusMark { status: Status::Off, text: ui.text("codes.paused").to_owned() }
                } else if let Some(row) = last {
                    StatusMark { status: result_status(row.result), text: ui.text(result_key(row.result)).to_owned() }
                    if let Some(at) = row.at.as_deref() { " · " TimeText { at: at.to_owned() } }
                } else {
                    StatusMark { status: Status::Open, text: ui.text("codes.none_yet").to_owned() }
                }
            }
        }
    }
}

#[component]
fn ManualEntry(
    state: Signal<AppState>,
    settings: Vec<CodeSetting>,
    games: Vec<&'static str>,
    stopped: bool,
    credentials: bool,
    action: bool,
    pending: u64,
) -> Element {
    let ui = use_preferences();
    let mut game = use_signal(|| games.first().copied().unwrap_or("genshin"));
    let mut uid = use_signal(String::new);
    let mut code = use_signal(String::new);
    let mut invalid = use_signal(|| false);
    // A game that disappeared (UIDs synced away) falls back to the first one shown.
    let chosen = if games.contains(&game()) {
        game()
    } else {
        games.first().copied().unwrap_or("genshin")
    };
    let uids: Vec<CodeSetting> = settings
        .iter()
        .filter(|setting| setting.game_kind == chosen)
        .cloned()
        .collect();
    let blocked = blocked_reason(stopped, credentials, action);
    let disabled = blocked.is_some();
    let described = if invalid() {
        "codes-manual-hint codes-manual-error"
    } else {
        "codes-manual-hint"
    };
    let redeem_blocked = if stopped {
        Some("codes.manual_blocked_admin")
    } else if !credentials {
        Some("codes.manual_blocked_cookie")
    } else if pending == 0 {
        Some("codes.redeem_now_nothing")
    } else {
        None
    };
    let redeem_label = if pending > 0 {
        ui.format("codes.redeem_now_count", &[("n", &pending.to_string())])
    } else {
        ui.text("codes.redeem_now").to_owned()
    };
    let listed = uids.clone();
    let mut submit = move || {
        let Some(entered) = manual_code(&code()) else {
            invalid.set(true);
            super::common::focus_element("codes-manual-code");
            return;
        };
        invalid.set(false);
        // Only a UID still listed for the chosen game; otherwise all of them.
        let target = uid()
            .parse::<Uuid>()
            .ok()
            .filter(|id| listed.iter().any(|setting| setting.game_account_id == *id));
        let game = chosen.to_owned();
        spawn_action(state, move |generation| async move {
            if enter_code(state, generation, game, entered, target).await {
                code.set(String::new());
            }
        });
    };
    rsx! {
        section { class: "page-stack", "aria-labelledby": "codes-manual-title",
            h3 { id: "codes-manual-title", {ui.text("codes.manual_title")} }
            form { class: "manual-form", novalidate: true,
                onsubmit: move |event| { event.prevent_default(); if !disabled { submit(); } },
                Field { id: "codes-manual-game", label: ui.text("codes.manual_game").to_owned(),
                    select { id: "codes-manual-game", value: chosen, disabled,
                        onchange: move |event| {
                            let next = DASHBOARD_GAMES.into_iter().find(|known| *known == event.value()).unwrap_or("genshin");
                            game.set(next);
                            uid.set(String::new());
                        },
                        for known in games.clone() {
                            option { key: "{known}", value: known, selected: known == chosen, {game_name(known, ui)} }
                        }
                    }
                }
                if uids.len() > 1 {
                    Field { id: "codes-manual-uid", label: ui.text("codes.manual_uid").to_owned(),
                        select { id: "codes-manual-uid", value: uid(), disabled,
                            onchange: move |event| uid.set(event.value()),
                            option { value: "", selected: uid().is_empty(), {ui.text("codes.manual_uid_all")} }
                            for setting in uids.clone() {
                                option { key: "{setting.game_account_id}", value: "{setting.game_account_id}",
                                    selected: uid() == setting.game_account_id.to_string(),
                                    "UID {setting.game_uid} · {setting.region}"
                                }
                            }
                        }
                    }
                }
                div { class: "field code-field",
                    label { r#for: "codes-manual-code", {ui.text("codes.manual_code")} }
                    input { id: "codes-manual-code", class: "code-input", r#type: "text", "inputmode": "text",
                        autocomplete: "off", "autocapitalize": "characters", "spellcheck": "false", maxlength: "24",
                        placeholder: ui.text("codes.manual_placeholder"), disabled,
                        "aria-describedby": described, "aria-invalid": invalid().then_some("true"),
                        value: code(), oninput: move |event| code.set(event.value()),
                    }
                }
                Button { id: "codes-manual-submit", r#type: "submit", disabled,
                    aria_describedby: blocked.map(|_| "codes-manual-blocked"),
                    {ui.text("codes.manual_submit")}
                }
                div { class: "manual-notes",
                    p { id: "codes-manual-hint", class: "field-hint", {ui.text("codes.manual_hint")} }
                    if invalid() && !disabled {
                        p { id: "codes-manual-error", class: "field-error", {ui.text("codes.manual_error_format")} }
                    }
                    if let Some(reason) = blocked {
                        p { id: "codes-manual-blocked", class: "field-hint", {ui.text(reason)} }
                    }
                }
            }
            div { class: "redeem-now",
                Button { id: "codes-redeem-now", variant: ButtonVariant::Secondary, r#type: "button",
                    disabled: redeem_blocked.is_some(), aria_describedby: redeem_blocked.map(|_| "codes-redeem-now-hint"),
                    onclick: move |_| {
                        spawn_action(state, move |generation| async move { redeem_now(state, generation).await; });
                    },
                    Glyph { icon: Icon::Refresh }
                    span { "{redeem_label}" }
                }
                if let Some(reason) = redeem_blocked {
                    p { id: "codes-redeem-now-hint", class: "field-hint", {ui.text(reason)} }
                }
            }
        }
    }
}

#[component]
fn History(state: Signal<AppState>, rows: Vec<CodeRow>) -> Element {
    let ui = use_preferences();
    let mut filter = use_signal(String::new);
    let mut limit = use_signal(|| HISTORY_PAGE);
    let shown: Vec<CodeRow> = rows
        .into_iter()
        .filter(|row| filter().is_empty() || row.game_kind == filter())
        .collect();
    let rest = shown.len().saturating_sub(limit());
    rsx! {
        section { class: "page-stack",
            div { class: "row",
                h3 { {ui.text("codes.history_title")} }
                div { class: "history-tools",
                    label { class: "visually-hidden", r#for: "codes-history-game", {ui.text("codes.filter_game")} }
                    select { id: "codes-history-game", value: filter(),
                        onchange: move |event| { filter.set(event.value()); limit.set(HISTORY_PAGE); },
                        option { value: "", {ui.text("codes.all_games")} }
                        option { value: "genshin", {game_name("genshin", ui)} }
                        option { value: "starrail", {game_name("starrail", ui)} }
                    }
                    Button { id: "codes-refresh", r#type: "button", variant: ButtonVariant::Secondary, onclick: move |_| {
                        spawn_action(state, move |_| async move { load_codes(state).await; });
                    }, {ui.text("codes.refresh")} }
                }
            }
            if shown.is_empty() {
                p { id: "codes-history-empty", {ui.text("codes.history_empty")} }
            } else {
                ul { class: "history codes-history",
                    for row in shown.iter().take(limit()).cloned() {
                        HistoryRow { key: "{row.game_account_id}-{row.code}", row }
                    }
                }
                if rest > 0 {
                    div { class: "button-row",
                        Button { id: "codes-show-more", r#type: "button", variant: ButtonVariant::Secondary,
                            onclick: move |_| limit.set(limit() + HISTORY_PAGE),
                            {ui.format("codes.show_more", &[("n", &rest.min(HISTORY_PAGE).to_string()), ("rest", &rest.to_string())])}
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn HistoryRow(row: CodeRow) -> Element {
    let ui = use_preferences();
    let game = game_name(&row.game_kind, ui);
    let reason = result_reason(&row, ui);
    let retcode = retcode_tag(&row, ui);
    let rewards = reward_text(&row.rewards);
    let mark_class = if row.game_kind == "genshin" {
        "genshin-mark"
    } else {
        "starrail-mark"
    };
    let link = row
        .redeem_url
        .clone()
        .filter(|url| row.result == CodeResult::Announced && url.starts_with("https://"));
    rsx! {
        li {
            div {
                p { class: "run-state",
                    StatusMark { status: result_status(row.result), text: ui.text(result_key(row.result)).to_owned() }
                    " · "
                    span { class: "uid-tag",
                        if let Some(mark) = Icon::game_mark(&row.game_kind) {
                            NamedIcon { icon: mark, name: game.to_owned(), class: mark_class }
                        }
                        "UID {row.game_uid}"
                    }
                    if row.manual { " · " {ui.text("codes.manual_tag")} }
                    if let Some(code) = retcode { " · " span { class: "retcode", "{code}" } }
                }
                p { class: "code-line",
                    span { class: "code-text", "{row.code}" }
                    if rewards.is_empty() {
                        span { class: "reward-text none", {ui.text("codes.no_reward_text")} }
                    } else {
                        span { class: "reward-text", "{rewards}" }
                    }
                }
                if let Some(reason) = reason { p { class: "run-reason", "{reason}" } }
                if let Some(url) = link {
                    a { class: "external-link", href: url, target: "_blank", rel: "noopener noreferrer",
                        Glyph { icon: Icon::External }
                        span { {ui.text("codes.redeem_on_official_site")} }
                        span { class: "visually-hidden", {ui.format("codes.opens_new_tab", &[("code", row.code.as_str())])} }
                    }
                }
            }
            div { class: "history-times",
                if let Some(at) = row.at.clone() { TimeText { at } }
            }
        }
    }
}

/// A moment in its short form (today the clock, otherwise month/day and clock), with the
/// full local date and zone as the tooltip.
#[component]
fn TimeText(at: String) -> Element {
    let ui = use_preferences();
    let short = short_at(Some(&at), ui).unwrap_or_default();
    let full = compact_datetime(Some(&at));
    rsx! { time { datetime: at.clone(), title: full, "{short}" } }
}

fn short_at(value: Option<&str>, ui: Preferences) -> Option<String> {
    local_moment(value).map(|moment| short_time(&moment, ui))
}

/// The status symbol of a result; colour is never alone (the word follows).
pub(super) fn result_status(result: CodeResult) -> Status {
    match result {
        CodeResult::Redeemed => Status::Done,
        CodeResult::AlreadyUsed | CodeResult::Expired => Status::Off,
        CodeResult::Invalid | CodeResult::CookieInvalid => Status::Failed,
        CodeResult::RankTooLow | CodeResult::Pending => Status::Waiting,
        CodeResult::Captcha | CodeResult::NotFound | CodeResult::Unknown => Status::Warn,
        CodeResult::Announced => Status::Open,
    }
}

pub(super) fn result_key(result: CodeResult) -> &'static str {
    match result {
        CodeResult::Redeemed => "codes.result_redeemed",
        CodeResult::AlreadyUsed => "codes.result_already_used",
        CodeResult::Expired => "codes.result_expired",
        CodeResult::Invalid => "codes.result_invalid",
        CodeResult::RankTooLow => "codes.result_rank_too_low",
        CodeResult::CookieInvalid => "codes.result_cookie_invalid",
        CodeResult::Captcha => "codes.result_captcha",
        CodeResult::NotFound => "codes.result_no_game_account",
        CodeResult::Unknown => "codes.result_unknown",
        CodeResult::Pending => "codes.result_pending",
        CodeResult::Announced => "codes.result_announced",
    }
}

/// The paused notice's text for a Cookie answer: when it came right after the Cookie was
/// registered, it says registering the same Cookie again will not help.
fn cookie_notice_body(redemption: &CodeRedemption, ui: Preferences) -> String {
    let at = short_at(redemption.action_at.as_deref(), ui).unwrap_or_default();
    match redemption
        .cookie_verified_at
        .as_deref()
        .filter(|_| redemption.action_after_cookie_update)
    {
        Some(verified) => ui.format(
            "codes.action_cookie_repeat_body",
            &[
                (
                    "registered",
                    &short_at(Some(verified), ui).unwrap_or_default(),
                ),
                ("at", &at),
            ],
        ),
        None => ui.format("codes.action_cookie_body", &[("at", &at)]),
    }
}

/// HoYoLAB's number for an answered row (never for success, waiting or notify-only rows).
fn retcode_tag(row: &CodeRow, ui: Preferences) -> Option<String> {
    match row.result {
        CodeResult::Redeemed | CodeResult::Pending | CodeResult::Announced => None,
        _ => row
            .retcode
            .map(|code| ui.format("codes.retcode_tag", &[("code", &code.to_string())])),
    }
}

/// The line under a result that says what happens next.
fn result_reason(row: &CodeRow, ui: Preferences) -> Option<String> {
    Some(match row.result {
        CodeResult::RankTooLow => {
            let at = short_at(row.next_attempt_at.as_deref(), ui)
                .unwrap_or_else(|| ui.text("common.not_fetched").to_owned());
            ui.format("codes.reason_rank_too_low", &[("at", &at)])
        }
        CodeResult::Unknown => ui.text("codes.reason_unknown").to_owned(),
        CodeResult::CookieInvalid => ui.text("codes.reason_cookie_invalid").to_owned(),
        CodeResult::Captcha => ui.text("codes.reason_captcha").to_owned(),
        CodeResult::NotFound => ui.text("codes.reason_no_game_account").to_owned(),
        CodeResult::Invalid if row.manual => ui.text("codes.reason_invalid_manual").to_owned(),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_result_has_its_own_word_and_a_symbol_that_never_reads_done_by_mistake() {
        let all = [
            CodeResult::Redeemed,
            CodeResult::AlreadyUsed,
            CodeResult::Expired,
            CodeResult::Invalid,
            CodeResult::RankTooLow,
            CodeResult::CookieInvalid,
            CodeResult::Captcha,
            CodeResult::NotFound,
            CodeResult::Unknown,
            CodeResult::Pending,
            CodeResult::Announced,
        ];
        let words: std::collections::BTreeSet<_> = all.iter().map(|r| result_key(*r)).collect();
        assert_eq!(words.len(), all.len());
        for result in all {
            assert_eq!(
                result_status(result) == Status::Done,
                result == CodeResult::Redeemed,
                "{result:?}"
            );
        }
        // Unknown outcomes look like the login bonus's unknown runs.
        assert_eq!(
            result_status(CodeResult::Unknown),
            super::super::rewards::run_status("unknown")
        );
    }

    #[test]
    fn entry_is_blocked_by_the_administrator_first_then_the_cookie_then_an_action() {
        assert_eq!(
            blocked_reason(true, false, true),
            Some("codes.manual_blocked_admin")
        );
        assert_eq!(
            blocked_reason(false, false, true),
            Some("codes.manual_blocked_cookie")
        );
        assert_eq!(
            blocked_reason(false, true, true),
            Some("codes.manual_blocked_action")
        );
        assert_eq!(blocked_reason(false, true, false), None);
    }

    #[test]
    fn the_source_warning_names_the_last_success_only_after_one_and_counts_in_english() {
        assert_eq!(source_warning_key(0, true), None);
        assert_eq!(source_warning_key(0, false), None);
        assert_eq!(
            source_warning_key(1, true),
            Some("codes.source_unavailable_one")
        );
        assert_eq!(
            source_warning_key(4, true),
            Some("codes.source_unavailable")
        );
        assert_eq!(source_warning_key(1, false), Some("codes.source_never_one"));
        assert_eq!(source_warning_key(3, false), Some("codes.source_never"));
        let english = Preferences {
            language: crate::i18n::Language::English,
            ..Preferences::default()
        };
        let text = |failures: i32, fetched_before: bool| {
            let key = source_warning_key(failures, fetched_before).unwrap();
            english.format(
                key,
                &[("count", &failures.to_string()), ("at", "11/23 13:12")],
            )
        };
        for failures in [1, 3] {
            let never = text(failures, false);
            assert!(
                never.starts_with("The code list has not been fetched yet"),
                "{never}"
            );
            assert!(
                !never.contains("last success") && !never.contains("Not fetched"),
                "{never}"
            );
            assert!(!never.contains("known codes"), "{never}");
        }
        assert!(text(1, false).contains("(1 failed attempt)"));
        assert!(text(3, false).contains("(3 failed attempts)"));
        assert!(text(1, true).contains("last success 11/23 13:12"));
        assert!(!text(1, true).contains("failures"));
        assert!(text(4, true).contains("4 failures in a row, last success 11/23 13:12"));
        let japanese = Preferences::default();
        let never = japanese.format("codes.source_never", &[("count", "3")]);
        assert!(never.contains("3") && !never.contains(japanese.text("common.not_fetched")));
    }

    #[test]
    fn reasons_follow_the_result_and_entered_invalid_codes_ask_to_check_the_input() {
        let ui = Preferences::default();
        let row = |result, manual| CodeRow {
            game_account_id: Uuid::from_u128(1),
            game_kind: "genshin".to_owned(),
            game_uid: "800000001".to_owned(),
            code: "GENSHINGIFT".to_owned(),
            rewards: String::new(),
            manual,
            result,
            retcode: None,
            stored: true,
            at: None,
            age: None,
            next_attempt_at: None,
            redeem_url: None,
        };
        assert_eq!(
            result_reason(&row(CodeResult::Invalid, true), ui).as_deref(),
            Some(ui.text("codes.reason_invalid_manual"))
        );
        assert_eq!(result_reason(&row(CodeResult::Invalid, false), ui), None);
        assert_eq!(result_reason(&row(CodeResult::Redeemed, false), ui), None);
        assert_eq!(
            result_reason(&row(CodeResult::Unknown, false), ui).as_deref(),
            Some(ui.text("codes.reason_unknown"))
        );
        assert!(
            result_reason(&row(CodeResult::RankTooLow, false), ui)
                .unwrap()
                .contains(ui.text("common.not_fetched"))
        );
    }

    #[test]
    fn the_cookie_notice_says_a_fresh_cookie_was_refused_and_names_the_number() {
        let ui = Preferences::default();
        let redemption = |after: bool, registered: Option<&str>| CodeRedemption {
            available: true,
            redemption_credentials: true,
            action_required: Some("cookie_invalid".to_owned()),
            action_at: Some("2026-11-24T06:42:50Z".to_owned()),
            action_retcode: Some(-1071),
            cookie_verified_at: registered.map(str::to_owned),
            action_after_cookie_update: after,
            pending: 1,
        };
        let repeat = ui.format(
            "codes.action_cookie_repeat_body",
            &[("registered", "R"), ("at", "A")],
        );
        let usual = ui.format("codes.action_cookie_body", &[("at", "A")]);
        let fresh = cookie_notice_body(&redemption(true, Some("2026-11-24T06:41:00Z")), ui);
        assert!(fresh.contains("cookie_token_v2"), "{fresh}");
        assert_ne!(fresh, usual);
        assert!(repeat.contains("cookie_token_v2"));
        // Without the server's judgement or the time, the usual text stays.
        let old = cookie_notice_body(&redemption(false, Some("2026-11-24T06:41:00Z")), ui);
        assert!(!old.contains("cookie_token_v2"), "{old}");
        assert!(!cookie_notice_body(&redemption(true, None), ui).contains("cookie_token_v2"));
        let english = Preferences {
            language: crate::i18n::Language::English,
            ..Preferences::default()
        };
        assert_eq!(
            english.format("codes.action_retcode", &[("code", "-1071")]),
            "HoYoLAB answered error -1071."
        );
    }

    #[test]
    fn answered_rows_show_hoyolab_numbers_and_missing_uids_wait_with_a_reason() {
        let ui = Preferences::default();
        let row = |result, retcode| CodeRow {
            game_account_id: Uuid::from_u128(1),
            game_kind: "starrail".to_owned(),
            game_uid: "800000002".to_owned(),
            code: "STARRAILGIFT".to_owned(),
            rewards: String::new(),
            manual: true,
            result,
            retcode,
            stored: true,
            at: None,
            age: None,
            next_attempt_at: None,
            redeem_url: None,
        };
        let tag = |code: &str| ui.format("codes.retcode_tag", &[("code", code)]);
        assert_eq!(
            retcode_tag(&row(CodeResult::CookieInvalid, Some(-1071)), ui),
            Some(tag("-1071"))
        );
        assert_eq!(
            retcode_tag(&row(CodeResult::NotFound, Some(-1073)), ui),
            Some(tag("-1073"))
        );
        assert_eq!(retcode_tag(&row(CodeResult::Redeemed, Some(0)), ui), None);
        // A reopened row keeps its last number but is waiting now.
        assert_eq!(
            retcode_tag(&row(CodeResult::Pending, Some(-1071)), ui),
            None
        );
        assert_eq!(retcode_tag(&row(CodeResult::Unknown, None), ui), None);
        assert_eq!(
            result_reason(&row(CodeResult::NotFound, Some(-1073)), ui).as_deref(),
            Some(ui.text("codes.reason_no_game_account"))
        );
        assert_eq!(result_status(CodeResult::NotFound), Status::Warn);
    }
}
