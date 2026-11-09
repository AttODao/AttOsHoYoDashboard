//! Settings transfer (web only): download the user's settings as JSON, and import such a
//! file after a preview of every change. The server validates and applies; this page only
//! reads the chosen file, shows the plan and asks before applying it.
use super::common::{Field, focus_element, game_name};
use super::notifications::PERSONAL_EVENTS;
use crate::app::actions::{handle_api_error_for, spawn_action, spawn_root};
use crate::app::preferences::{Preferences, use_preferences};
use crate::app::settings_file;
use crate::app::state::{AppState, generation_matches, set_status};
use crate::components::{Button, ButtonVariant};
use crate::models::{SettingsChange, SettingsImportReport, SettingsSkipped};
use dioxus::prelude::*;
use serde_json::Value;

const FILE_INPUT: &str = "settings-file";

/// A checked file and the server's plan for it.
#[derive(Clone, PartialEq)]
struct Planned {
    file: Value,
    report: SettingsImportReport,
}

#[component]
pub(super) fn TransferPage(state: Signal<AppState>) -> Element {
    let ui = use_preferences();
    let mut planned = use_signal(|| None::<Planned>);
    let mut read_error = use_signal(|| None::<&'static str>);
    let mut checking = use_signal(|| false);
    let described = if read_error().is_some() {
        "settings-file-hint settings-file-error"
    } else {
        "settings-file-hint"
    };
    rsx! {
        div { class: "page-stack",
            h2 { {ui.text("settings.transfer")} }
            section { class: "panel page-stack",
                h3 { {ui.text("transfer.export_title")} }
                p { class: "field-hint", {ui.text("transfer.export_hint")} }
                div { class: "button-row",
                    Button { id: "transfer-download", variant: ButtonVariant::Secondary, r#type: "button", onclick: move |_| {
                        spawn_action(state, move |generation| async move {
                            match crate::api::get::<Value>("/me/settings/export").await {
                                Ok(file) if generation_matches(state, generation) => {
                                    if settings_file::download(&file) {
                                        set_status(state, "transfer.downloaded", "success");
                                    } else {
                                        set_status(state, "messages.this_action_is_available_only_in_a_browser", "error");
                                    }
                                }
                                Ok(_) => {}
                                Err(error) => handle_api_error_for(state, generation, error),
                            }
                        });
                    }, {ui.text("transfer.download")} }
                }
            }
            section { class: "panel page-stack",
                h3 { {ui.text("transfer.import_title")} }
                p { id: "settings-file-hint", class: "field-hint", {ui.text("transfer.import_hint")} }
                Field { id: FILE_INPUT, label: ui.text("transfer.file_label"),
                    input { id: FILE_INPUT, r#type: "file", accept: "application/json,.json",
                        "aria-describedby": described,
                        "aria-invalid": if read_error().is_some() { "true" } else { "false" },
                        onchange: move |_| {
                            planned.set(None);
                            read_error.set(None);
                            spawn_action(state, move |generation| async move {
                                let file = match settings_file::read_chosen(FILE_INPUT).await {
                                    None => return,
                                    Some(Err(error)) => {
                                        read_error.set(Some(error.message_key()));
                                        return;
                                    }
                                    Some(Ok(file)) => file,
                                };
                                checking.set(true);
                                let result = crate::api::json::<SettingsImportReport>("POST", "/me/settings/import/preview", Some(file.clone())).await;
                                checking.set(false);
                                match result {
                                    Ok(report) if generation_matches(state, generation) => planned.set(Some(Planned { file, report })),
                                    Ok(_) => {}
                                    Err(error) => handle_api_error_for(state, generation, error),
                                }
                            });
                        },
                    }
                    if let Some(key) = read_error() {
                        p { id: "settings-file-error", class: "field-error", {ui.text(key)} }
                    }
                }
                if checking() {
                    p { class: "field-hint", role: "status", {ui.text("transfer.checking")} }
                }
                if let Some(plan) = planned() {
                    ImportPreview { state, planned, plan }
                }
            }
        }
    }
}

/// Clears the chosen file and moves focus back to the file input once the page is usable.
fn start_over(state: Signal<AppState>, mut planned: Signal<Option<Planned>>) {
    planned.set(None);
    settings_file::clear_chosen(FILE_INPUT);
    spawn_root(async move {
        // The busy shell disables the input until the current action has finished.
        for _ in 0..20 {
            crate::platform::sleep_ms(30).await;
            if !state.peek().busy {
                break;
            }
        }
        crate::platform::sleep_ms(30).await;
        focus_element(FILE_INPUT);
    });
}

#[component]
fn ImportPreview(
    state: Signal<AppState>,
    planned: Signal<Option<Planned>>,
    plan: Planned,
) -> Element {
    let ui = use_preferences();
    let report = plan.report.clone();
    let cancel = rsx! {
        Button { id: "transfer-cancel", variant: ButtonVariant::Secondary, r#type: "button",
            onclick: move |_| start_over(state, planned), {ui.text("transfer.cancel")} }
    };
    if !report.valid {
        return rsx! {
            div { class: "transfer-preview",
                div { class: "notice error", id: "transfer-problems", role: "alert",
                    strong { {ui.text("transfer.problems_title")} }
                    p { {ui.text("transfer.problems_hint")} }
                    ul { class: "problem-list",
                        for (index, problem) in report.errors.iter().enumerate() {
                            li { key: "{index}",
                                code { "{problem.field}" } " · " {ui.text(problem_key(&problem.code))}
                            }
                        }
                    }
                }
                div { class: "button-row", {cancel} }
            }
        };
    }
    let changes = report.changes.len().to_string();
    let skipped = report.skipped.len().to_string();
    rsx! {
        div { class: "transfer-preview", "aria-labelledby": "transfer-changes-title", role: "region",
            h4 { id: "transfer-changes-title", {ui.format("transfer.changes_title", &[("count", &changes)])} }
            if report.changes.is_empty() {
                p { {ui.text("transfer.no_changes")} }
            } else {
                ul { class: "item-list transfer-list", id: "transfer-changes",
                    for (index, change) in report.changes.iter().enumerate() {
                        li { key: "{index}",
                            span { {change_label(change, ui)} }
                            span { class: "transfer-value",
                                {ui.format("transfer.value_change", &[("from", &value_text(&change.setting, &change.from, ui)), ("to", &value_text(&change.setting, &change.to, ui))])}
                            }
                        }
                    }
                }
            }
            if !report.skipped.is_empty() {
                h4 { {ui.format("transfer.skipped_title", &[("count", &skipped)])} }
                ul { class: "item-list transfer-list", id: "transfer-skipped",
                    for (index, entry) in report.skipped.iter().enumerate() {
                        li { key: "{index}",
                            span { {skipped_label(entry, ui)} }
                            span { class: "field-hint", {skip_reason(&entry.reason, ui)} }
                        }
                    }
                }
            }
            if report.webhook.in_file && !report.webhook.configured {
                div { class: "notice warn", id: "transfer-webhook-note",
                    p { {ui.text("transfer.webhook_missing")} }
                }
            }
            div { class: "button-row",
                Button { id: "transfer-apply", r#type: "button", disabled: report.changes.is_empty(), onclick: move |_| {
                    let file = plan.file.clone();
                    let previewed = plan.report.clone();
                    spawn_action(state, move |generation| async move {
                        match crate::api::json::<SettingsImportReport>("POST", "/me/settings/import", Some(file)).await {
                            Ok(applied) if generation_matches(state, generation) => {
                                set_status(state, applied_message(&previewed, &applied), "success");
                                start_over(state, planned);
                            }
                            Ok(_) => {}
                            Err(error) => handle_api_error_for(state, generation, error),
                        }
                    });
                }, {ui.text("transfer.apply")} }
                {cancel}
            }
        }
    }
}

/// The apply plans again on the server; say so when that differs from the preview shown.
fn applied_message(
    previewed: &SettingsImportReport,
    applied: &SettingsImportReport,
) -> &'static str {
    if previewed.changes == applied.changes && previewed.skipped == applied.skipped {
        "transfer.applied"
    } else {
        "transfer.applied_differs"
    }
}

fn problem_key(code: &str) -> &'static str {
    match code {
        "missing" => "transfer.problem_missing",
        "type" => "transfer.problem_type",
        "unknown" => "transfer.problem_unknown",
        "unsupported" => "transfer.problem_unsupported",
        "duplicate" => "transfer.problem_duplicate",
        "too_many" => "transfer.problem_too_many",
        _ => "transfer.problem_invalid",
    }
}

fn skip_reason(reason: &str, ui: Preferences) -> String {
    match reason {
        "not_linked" => ui.text("transfer.skip_not_linked").to_owned(),
        "game_not_found" => ui.text("transfer.skip_game_not_found").to_owned(),
        "uid_not_found" => ui.text("transfer.skip_uid_not_found").to_owned(),
        "reward_not_ready" => ui.text("transfer.skip_reward_not_ready").to_owned(),
        // A newer server's reason is still shown rather than hidden.
        other => other.to_owned(),
    }
}

fn skipped_label(entry: &SettingsSkipped, ui: Preferences) -> String {
    let game = game_name(&entry.game_kind, ui);
    match &entry.game_uid {
        Some(uid) => ui.format("transfer.skipped_uid", &[("game", game), ("uid", uid)]),
        None => ui.format("transfer.skipped_reward", &[("game", game)]),
    }
}

fn event_label(kind: &str, ui: Preferences) -> String {
    PERSONAL_EVENTS
        .iter()
        .find(|(known, _)| *known == kind)
        .map_or_else(|| kind.to_owned(), |(_, label)| ui.text(label).to_owned())
}

/// What a change is about, in the words of the settings pages.
fn change_label(change: &SettingsChange, ui: Preferences) -> String {
    let game = change
        .game_kind
        .as_deref()
        .map_or("", |game| game_name(game, ui));
    let setting = change.setting.as_str();
    match (change.section.as_str(), setting) {
        ("rewards", "enabled") => ui.format("transfer.change_reward_enabled", &[("game", game)]),
        ("rewards", "local_time") => ui.format("transfer.change_reward_time", &[("game", game)]),
        ("game_accounts", "visible") => ui.format(
            "transfer.change_visible",
            &[
                ("game", game),
                ("uid", change.game_uid.as_deref().unwrap_or("")),
            ],
        ),
        ("notifications", "webhook_format") => ui.text("transfer.change_webhook_format").to_owned(),
        ("notifications", "language") => ui.text("transfer.change_language").to_owned(),
        ("notifications", "resin_warn_hours") => {
            ui.text("notifications.resin_warn_hours").to_owned()
        }
        ("notifications", "daily_reminder_time") => {
            ui.text("notifications.daily_reminder_time").to_owned()
        }
        ("notifications", "weekly_reminder_time") => {
            ui.text("notifications.weekly_reminder_time").to_owned()
        }
        ("notifications", _) => {
            if let Some(kind) = setting.strip_prefix("app_events.") {
                ui.format(
                    "transfer.change_app_event",
                    &[("event", &event_label(kind, ui))],
                )
            } else if let Some(kind) = setting.strip_prefix("events.") {
                ui.format(
                    "transfer.change_webhook_event",
                    &[("event", &event_label(kind, ui))],
                )
            } else {
                setting.to_owned()
            }
        }
        (section, _) => format!("{section} · {setting}"),
    }
}

/// A changed value as the settings pages show it.
fn value_text(setting: &str, value: &Value, ui: Preferences) -> String {
    match (setting, value) {
        ("visible", Value::Bool(true)) => ui.text("accounts.visible").to_owned(),
        ("visible", Value::Bool(false)) => ui.text("transfer.hidden").to_owned(),
        (_, Value::Bool(true)) => ui.text("dashboard.on").to_owned(),
        (_, Value::Bool(false)) => ui.text("dashboard.off").to_owned(),
        ("webhook_format", Value::String(format)) if format == "json" => {
            ui.text("notifications.format_json").to_owned()
        }
        ("webhook_format", Value::String(format)) if format == "discord" => "Discord".to_owned(),
        ("language", Value::String(language)) if language == "ja" => {
            ui.text("language.japanese").to_owned()
        }
        ("language", Value::String(language)) if language == "en" => {
            ui.text("language.english").to_owned()
        }
        ("resin_warn_hours", Value::Number(hours)) => {
            ui.format("transfer.hours", &[("count", &hours.to_string())])
        }
        (_, Value::String(text)) => text.clone(),
        (_, other) => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::preferences::Theme;
    use crate::i18n::Language;
    use serde_json::json;

    fn english() -> Preferences {
        Preferences {
            language: Language::English,
            theme: Theme::System,
        }
    }

    fn change(
        section: &str,
        game: Option<&str>,
        uid: Option<&str>,
        setting: &str,
    ) -> SettingsChange {
        SettingsChange {
            section: section.into(),
            game_kind: game.map(Into::into),
            game_uid: uid.map(Into::into),
            setting: setting.into(),
            from: json!(false),
            to: json!(true),
        }
    }

    #[test]
    fn changes_are_named_like_the_settings_pages() {
        let ui = english();
        assert_eq!(
            change_label(&change("rewards", Some("genshin"), None, "local_time"), ui),
            "Genshin Impact · automatic claim time"
        );
        assert_eq!(
            change_label(&change("rewards", Some("starrail"), None, "enabled"), ui),
            "Honkai: Star Rail · automatic claim"
        );
        assert_eq!(
            change_label(
                &change(
                    "game_accounts",
                    Some("genshin"),
                    Some("800000001"),
                    "visible"
                ),
                ui
            ),
            "Genshin Impact UID 800000001 · shown on the dashboard"
        );
        assert_eq!(
            change_label(
                &change("notifications", None, None, "events.reward_claimed"),
                ui
            ),
            format!(
                "Webhook · {}",
                ui.text("notifications.event_reward_claimed")
            )
        );
        assert_eq!(
            change_label(
                &change("notifications", None, None, "app_events.resource_full"),
                ui
            ),
            format!("App · {}", ui.text("notifications.event_resource_full"))
        );
        assert_eq!(
            change_label(&change("notifications", None, None, "language"), ui),
            "Notification language"
        );
        assert_eq!(
            change_label(
                &change("notifications", None, None, "daily_reminder_time"),
                ui
            ),
            ui.text("notifications.daily_reminder_time")
        );
        // Unknown settings from a newer server stay readable instead of disappearing.
        assert_eq!(
            change_label(&change("notifications", None, None, "events.new_kind"), ui),
            "Webhook · new_kind"
        );
        assert_eq!(
            change_label(&change("other", None, None, "thing"), ui),
            "other · thing"
        );
    }

    #[test]
    fn values_use_the_words_of_their_controls() {
        let ui = english();
        assert_eq!(value_text("visible", &json!(false), ui), "Hidden");
        assert_eq!(
            value_text("visible", &json!(true), ui),
            ui.text("accounts.visible")
        );
        assert_eq!(
            value_text("enabled", &json!(true), ui),
            ui.text("dashboard.on")
        );
        assert_eq!(
            value_text("events.signed_in", &json!(false), ui),
            ui.text("dashboard.off")
        );
        assert_eq!(
            value_text("webhook_format", &json!("json"), ui),
            "Generic JSON"
        );
        assert_eq!(
            value_text("webhook_format", &json!("discord"), ui),
            "Discord"
        );
        assert_eq!(value_text("language", &json!("ja"), ui), "日本語");
        assert_eq!(value_text("resin_warn_hours", &json!(6), ui), "6 h");
        assert_eq!(value_text("local_time", &json!("06:30"), ui), "06:30");
    }

    #[test]
    fn problems_and_skips_have_messages() {
        let ui = english();
        for (code, key) in [
            ("missing", "transfer.problem_missing"),
            ("type", "transfer.problem_type"),
            ("unknown", "transfer.problem_unknown"),
            ("invalid", "transfer.problem_invalid"),
            ("unsupported", "transfer.problem_unsupported"),
            ("duplicate", "transfer.problem_duplicate"),
            ("too_many", "transfer.problem_too_many"),
            ("something_new", "transfer.problem_invalid"),
        ] {
            assert_eq!(problem_key(code), key);
        }
        assert_eq!(
            skip_reason("uid_not_found", ui),
            "This account has no such UID (it is not added)"
        );
        assert_eq!(skip_reason("something_new", ui), "something_new");
        let skipped = SettingsSkipped {
            section: "game_accounts".into(),
            game_kind: "starrail".into(),
            game_uid: Some("900000001".into()),
            reason: "uid_not_found".into(),
        };
        assert_eq!(
            skipped_label(&skipped, ui),
            "Honkai: Star Rail UID 900000001"
        );
        let reward = SettingsSkipped {
            game_uid: None,
            ..skipped
        };
        assert_eq!(
            skipped_label(&reward, ui),
            "Honkai: Star Rail · login bonus"
        );
    }

    #[test]
    fn an_apply_that_differs_from_its_preview_says_so() {
        let report = |to: &str| SettingsImportReport {
            valid: true,
            changes: vec![SettingsChange {
                to: json!(to),
                ..change("notifications", None, None, "language")
            }],
            skipped: Vec::new(),
            webhook: Default::default(),
            errors: Vec::new(),
        };
        assert_eq!(
            applied_message(&report("en"), &report("en")),
            "transfer.applied"
        );
        assert_eq!(
            applied_message(&report("en"), &report("ja")),
            "transfer.applied_differs"
        );
    }

    #[test]
    fn reports_parse_with_and_without_problems() {
        let valid: SettingsImportReport = serde_json::from_value(json!({
            "valid": true,
            "changes": [{"section": "notifications", "setting": "language", "from": "ja", "to": "en"}],
            "skipped": [],
            "webhook": {"in_file": true, "configured": false},
        }))
        .unwrap();
        assert!(valid.valid && valid.errors.is_empty());
        assert!(valid.webhook.in_file && !valid.webhook.configured);
        let invalid: SettingsImportReport = serde_json::from_value(json!({
            "valid": false,
            "errors": [{"field": "version", "code": "unsupported"}],
        }))
        .unwrap();
        assert!(!invalid.valid && invalid.changes.is_empty());
        assert_eq!(invalid.errors[0].field, "version");
    }
}
