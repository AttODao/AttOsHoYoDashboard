//! The administrator's redemption-code section (web only, like the whole admin page): the
//! server-wide stop switch, the fetch interval, the source status and the user counts.
use super::common::{LoadError, Skeleton, WarningLine, game_name, reset_checked, reset_value};
use super::icons::{Glyph, Icon, Status, StatusMark};
use crate::app::actions::spawn_action;
use crate::app::codes::{
    DEFAULT_INTERVAL_HOURS, INTERVAL_HOURS, interval_choice, save_admin_codes,
};
use crate::app::display::{counted, datetime_text};
use crate::app::preferences::{Preferences, use_preferences};
use crate::app::state::AppState;
use crate::components::{Button, ButtonVariant};
use crate::models::AdminCodes;
use dioxus::prelude::*;

#[component]
pub(super) fn AdminCodesCard(state: Signal<AppState>) -> Element {
    let ui = use_preferences();
    let view = state.read().admin_codes.clone();
    let failed = state.read().admin_codes_error.is_some();
    rsx! {
        section { class: "panel page-stack admin-codes", "aria-labelledby": "admin-codes-title",
            h2 { id: "admin-codes-title", class: "icon-title",
                Glyph { icon: Icon::Code }
                {ui.text("admin.codes_title")}
            }
            if failed {
                LoadError { state, message: ui.text("admin.codes_load_failed") }
            } else if let Some(view) = view {
                AdminCodesEditor { key: "{view.enabled}-{view.interval_minutes}", state, view }
            } else {
                Skeleton { label: ui.text("codes.loading"), rows: 4 }
            }
        }
    }
}

/// The text of a fetch error class; classes this build does not know stay generic.
fn error_key(kind: &str) -> &'static str {
    match kind {
        "timeout" => "admin.codes_error_timeout",
        "connect" => "admin.codes_error_connect",
        "http_status" => "admin.codes_error_http_status",
        "redirect" => "admin.codes_error_redirect",
        "too_large" => "admin.codes_error_too_large",
        "invalid_payload" => "admin.codes_error_invalid_payload",
        "storage" => "admin.codes_error_storage",
        _ => "dashboard.unknown",
    }
}

fn hours_label(hours: i32, ui: Preferences) -> String {
    if hours == DEFAULT_INTERVAL_HOURS {
        ui.text("admin.codes_hours_default").to_owned()
    } else {
        counted(
            ui,
            i64::from(hours),
            "admin.codes_hours_one",
            "admin.codes_hours",
            &[("n", &hours.to_string())],
        )
    }
}

/// The source warning: none while fetching works; without any successful fetch it says
/// that users get listed codes only after the first one; one failure and several read
/// differently in English.
fn source_warning_key(failures: i32, fetched_before: bool) -> Option<&'static str> {
    Some(match (failures, fetched_before) {
        (..=0, _) => return None,
        (1, true) => "admin.codes_source_down_one",
        (_, true) => "admin.codes_source_down",
        (1, false) => "admin.codes_source_never_one",
        (_, false) => "admin.codes_source_never",
    })
}

/// The "last success" row: the moment, or "none yet" before the first successful fetch
/// (marked as a warning while fetches fail, never as done).
fn last_success_mark(value: Option<&str>, failing: bool, ui: Preferences) -> (Status, String) {
    match value {
        Some(_) if failing => (Status::Warn, datetime_text(value, ui)),
        Some(_) => (Status::Done, datetime_text(value, ui)),
        None if failing => (
            Status::Warn,
            ui.text("admin.codes_never_succeeded").to_owned(),
        ),
        None => (
            Status::Unknown,
            ui.text("admin.codes_never_succeeded").to_owned(),
        ),
    }
}

#[component]
fn AdminCodesEditor(state: Signal<AppState>, view: AdminCodes) -> Element {
    let ui = use_preferences();
    let mut hours = use_signal(|| interval_choice(view.interval_minutes));
    let enabled = view.enabled;
    let failing = view.consecutive_failures > 0;
    let (success_status, last_success) =
        last_success_mark(view.last_success_at.as_deref(), failing, ui);
    let warning = source_warning_key(view.consecutive_failures, view.last_success_at.is_some());
    let last_attempt = datetime_text(view.last_attempt_at.as_deref(), ui);
    let attempt_failed = failing && view.last_error.is_some();
    let attempt_text = match view.last_error.as_deref().filter(|_| attempt_failed) {
        Some(kind) => format!("{last_attempt} · {}", ui.text(error_key(kind))),
        None => last_attempt,
    };
    // The switch sends the stored interval unchanged (it may be a value set through the
    // API that is not one of the choices); an unsaved choice is not part of it.
    let stored_minutes = view.interval_minutes;
    rsx! {
        div { class: "setting-row",
            div {
                h3 { {ui.text("admin.codes_enabled")} }
                p { class: "field-hint", id: "admin-codes-enabled-hint", {ui.text("admin.codes_enabled_hint")} }
            }
            label { class: "switch",
                input { id: "admin-codes-enabled", r#type: "checkbox", role: "switch", checked: enabled,
                    "aria-describedby": "admin-codes-enabled-hint", "aria-label": ui.text("admin.codes_enabled"),
                    onchange: move |event| {
                        let next = event.checked();
                        spawn_action(state, move |generation| async move {
                            save_admin_codes(state, generation, next, stored_minutes).await;
                            let stored = state.read().admin_codes.as_ref().map(|view| view.enabled);
                            if let Some(stored) = stored { reset_checked("admin-codes-enabled", stored); }
                        });
                    }
                }
                span { class: "switch-track", "aria-hidden": "true" }
                span { if enabled { {ui.text("dashboard.on")} } else { {ui.text("dashboard.off")} } }
            }
        }
        if !enabled {
            p { class: "notice with-icon", role: "status", id: "admin-codes-stopped",
                Glyph { icon: Icon::Ban }
                span { {ui.text("admin.codes_stopped_note")} }
            }
        }
        div { class: "inline-form",
            div { class: "field",
                label { r#for: "admin-codes-interval", {ui.text("admin.codes_interval")} }
                select { id: "admin-codes-interval", value: hours().to_string(), "aria-describedby": "admin-codes-interval-hint",
                    onchange: move |event| if let Ok(value) = event.value().parse::<i32>() { hours.set(value); },
                    for choice in INTERVAL_HOURS {
                        option { key: "{choice}", value: "{choice}", selected: hours() == choice, {hours_label(choice, ui)} }
                    }
                }
                p { class: "field-hint", id: "admin-codes-interval-hint", {ui.text("admin.codes_interval_hint")} }
            }
            Button { id: "admin-codes-save", r#type: "button", variant: ButtonVariant::Primary, onclick: move |_| {
                let chosen = hours();
                spawn_action(state, move |generation| async move {
                    let mut hours = hours;
                    save_admin_codes(state, generation, enabled, chosen * 60).await;
                    // A refused save keeps the stored interval: show it again.
                    let stored = state.read().admin_codes.as_ref().map(|view| interval_choice(view.interval_minutes));
                    if let Some(stored) = stored {
                        hours.set(stored);
                        reset_value("admin-codes-interval", &stored.to_string());
                    }
                });
            }, {ui.text("admin.codes_save")} }
        }
        h3 { {ui.text("admin.codes_source_title")} }
        if let Some(key) = warning {
            WarningLine { status: Status::Warn, text: ui.format(key, &[("count", &view.consecutive_failures.to_string())]) }
        }
        dl { class: "metric-list",
            div { class: "metric", dt { {ui.text("admin.codes_source")} } dd { "{view.source_host}" } }
            div { class: "metric", dt { {ui.text("admin.codes_last_success")} }
                dd { StatusMark { status: success_status, text: last_success } }
            }
            div { class: "metric", dt { {ui.text("admin.codes_last_attempt")} }
                dd { StatusMark { status: if attempt_failed { Status::Failed } else { Status::Done }, text: attempt_text } }
            }
            div { class: "metric", dt { {ui.text("admin.codes_failures")} }
                dd {
                    if failing { StatusMark { status: Status::Warn, text: view.consecutive_failures.to_string() } }
                    else { "0" }
                }
            }
            div { class: "metric", dt { {ui.text("admin.codes_catalogue")} }
                dd { class: "admin-code-counts",
                    for (game, count) in view.catalogue.iter().map(|(game, count)| (game.clone(), count.listed)) {
                        span { key: "{game}", class: "icon-text",
                            if let Some(mark) = Icon::game_mark(&game) { Glyph { icon: mark, class: "icon-sm" } }
                            {game_name(&game, ui)} " {count}"
                        }
                    }
                }
            }
            div { class: "metric", dt { {ui.text("admin.codes_next")} }
                dd {
                    if enabled { {datetime_text(view.next_fetch_at.as_deref(), ui)} }
                    else { StatusMark { status: Status::Off, text: ui.text("admin.codes_stopped").to_owned() } }
                }
            }
            div { class: "metric", dt { {ui.text("admin.codes_users_auto")} } dd { "{view.redemption.auto_users}" } }
            div { class: "metric", dt { {ui.text("admin.codes_users_attention")} }
                dd {
                    if view.redemption.action_required_users > 0 {
                        StatusMark { status: Status::Warn, text: view.redemption.action_required_users.to_string() }
                    } else { "0" }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_fetch_error_class_has_its_own_words_and_new_ones_stay_generic() {
        let classes = [
            "timeout",
            "connect",
            "http_status",
            "redirect",
            "too_large",
            "invalid_payload",
            "storage",
        ];
        let keys: std::collections::BTreeSet<_> =
            classes.iter().map(|kind| error_key(kind)).collect();
        assert_eq!(keys.len(), classes.len());
        assert!(!keys.contains("dashboard.unknown"));
        assert_eq!(error_key("synthetic"), "dashboard.unknown");
    }

    #[test]
    fn the_default_interval_is_named_as_the_default() {
        let ui = Preferences::default();
        assert_eq!(hours_label(3, ui), ui.text("admin.codes_hours_default"));
        assert_eq!(
            hours_label(6, ui),
            ui.format("admin.codes_hours", &[("n", "6")])
        );
        let english = Preferences {
            language: crate::i18n::Language::English,
            ..ui
        };
        assert_eq!(hours_label(1, english), "1 hour");
        assert_eq!(hours_label(12, english), "12 hours");
    }

    #[test]
    fn the_source_status_tells_never_fetched_apart_and_counts_in_english() {
        assert_eq!(source_warning_key(0, true), None);
        assert_eq!(source_warning_key(0, false), None);
        assert_eq!(
            source_warning_key(1, true),
            Some("admin.codes_source_down_one")
        );
        assert_eq!(source_warning_key(5, true), Some("admin.codes_source_down"));
        assert_eq!(
            source_warning_key(1, false),
            Some("admin.codes_source_never_one")
        );
        assert_eq!(
            source_warning_key(2, false),
            Some("admin.codes_source_never")
        );
        let english = Preferences {
            language: crate::i18n::Language::English,
            ..Preferences::default()
        };
        let text = |failures: i32, fetched_before: bool| {
            let key = source_warning_key(failures, fetched_before).unwrap();
            english.format(key, &[("count", &failures.to_string())])
        };
        assert!(text(1, true).starts_with("The latest fetch from the source failed."));
        assert!(text(5, true).contains("failed 5 times in a row"));
        assert!(text(1, false).contains("(1 failed attempt)"));
        assert!(text(2, false).contains("(2 failed attempts)"));
        assert!(!text(1, false).contains("Known codes"));
        assert!(!text(2, false).contains("Known codes"));

        let never = english.text("admin.codes_never_succeeded").to_owned();
        assert_eq!(
            last_success_mark(None, true, english),
            (Status::Warn, never.clone())
        );
        assert_eq!(
            last_success_mark(None, false, english),
            (Status::Unknown, never)
        );
        assert_eq!(
            last_success_mark(Some("2026-11-24T00:00:00Z"), false, english).0,
            Status::Done
        );
        assert_eq!(
            last_success_mark(Some("2026-11-24T00:00:00Z"), true, english).0,
            Status::Warn
        );
    }
}
