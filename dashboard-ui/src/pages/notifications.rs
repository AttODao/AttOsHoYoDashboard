//! Personal notification settings: per-notice choices for this device (app) and the
//! Webhook, reminder times and the Webhook itself, saved together.
use super::common::{Field, LoadError};
use crate::app::actions::{handle_api_error_for, spawn_action};
use crate::app::data::{load_admin_notifications, load_notifications};
use crate::app::display::datetime_text;
use crate::app::preferences::use_preferences;
use crate::app::state::{AppState, generation_matches, set_status};
use crate::components::{Button, ButtonVariant};
use crate::models::NotificationChannelView;
use dioxus::prelude::*;
use serde_json::{Value, json};
use std::collections::BTreeMap;

/// Personal notices in the order the settings page lists them, with their label keys.
const PERSONAL_EVENTS: [(&str, &str); 11] = [
    ("reward_failed", "notifications.event_reward_failed"),
    ("reward_claimed", "notifications.event_reward_claimed"),
    ("cookie_invalid", "notifications.event_cookie_invalid"),
    ("signed_in", "notifications.event_signed_in"),
    ("sign_in_failed", "notifications.event_sign_in_failed"),
    ("resource_near_cap", "notifications.event_resource_near_cap"),
    ("resource_full", "notifications.event_resource_full"),
    ("expeditions_done", "notifications.event_expeditions_done"),
    ("transformer_ready", "notifications.event_transformer_ready"),
    ("daily_unfinished", "notifications.event_daily_unfinished"),
    ("weekly_unfinished", "notifications.event_weekly_unfinished"),
];

/// The personal settings as the form holds them.
#[derive(Clone)]
struct PersonalForm {
    format: String,
    language: String,
    events: BTreeMap<String, bool>,
    app_events: BTreeMap<String, bool>,
    warn_hours: i32,
    daily: String,
    weekly: String,
}

/// `PUT /me/notifications` body. `webhook_url`: `None` keeps the stored URL. Only the app
/// sends `app_events`; the web never shows them, so it leaves the stored choices alone.
fn personal_payload(form: &PersonalForm, webhook_url: Option<Value>, app: bool) -> Value {
    let mut payload = json!({
        "webhook_format": form.format,
        "language": form.language,
        "events": form.events,
        "resin_warn_hours": form.warn_hours,
        "daily_reminder_time": form.daily,
        "weekly_reminder_time": form.weekly,
    });
    if app {
        payload["app_events"] = json!(form.app_events);
    }
    if let Some(url) = webhook_url {
        payload["webhook_url"] = url;
    }
    payload
}

#[component]
pub(super) fn NotificationsPage(state: Signal<AppState>) -> Element {
    let ui = use_preferences();
    let device = {
        #[cfg(feature = "mobile")]
        {
            rsx! { super::device_notifications::DeviceNotificationsCard { state } }
        }
        #[cfg(not(feature = "mobile"))]
        {
            rsx! {}
        }
    };
    rsx! {
        div { class: "page-stack",
            h2 { {ui.text("notifications.personal_title")} }
            if !crate::platform::APP {
                p { class: "field-hint", {ui.text("notifications.personal_hint")} }
            }
            {device}
            if let Some(error) = state.read().notifications_error.clone() {
                LoadError { state, message: ui.text(&error) }
            }
            if let Some(view) = state.read().notifications.clone() {
                PersonalChannelCard { state, view }
            } else if state.read().notifications_error.is_none() {
                p { role: "status", {ui.text("notifications.loading")} }
            }
        }
    }
}

#[component]
fn PersonalChannelCard(
    state: Signal<AppState>,
    view: crate::models::PersonalNotificationView,
) -> Element {
    let ui = use_preferences();
    #[cfg(feature = "mobile")]
    let device = crate::app::device_notifications::use_device_context();
    let mut webhook_input = use_signal(String::new);
    let mut format = use_signal(|| view.channel.webhook_format.clone());
    let mut language = use_signal(|| view.channel.language.clone());
    let events = use_signal(|| view.channel.events.clone());
    let app_events = use_signal(|| view.app_events.clone());
    // The device-only predicted reminder without a server notice (see `EventMatrix`).
    let realm = use_signal(|| {
        #[cfg(feature = "mobile")]
        {
            device.settings.peek().realm_currency_full
        }
        #[cfg(not(feature = "mobile"))]
        {
            false
        }
    });
    let mut warn_hours = use_signal(|| view.resin_warn_hours);
    let mut daily = use_signal(|| view.daily_reminder_time.clone());
    let mut weekly = use_signal(|| view.weekly_reminder_time.clone());
    let form = move || PersonalForm {
        format: format(),
        language: language(),
        events: events(),
        app_events: app_events(),
        warn_hours: warn_hours(),
        daily: daily(),
        weekly: weekly(),
    };
    // Saved device choices also steer this device's predicted reminders.
    let follow_device = move |_chosen: &PersonalForm| {
        #[cfg(feature = "mobile")]
        {
            let mut device = device;
            device
                .settings
                .write()
                .follow_choices(&_chosen.app_events, realm());
            device.save();
        }
        let _ = realm;
    };
    let configured = view.channel.webhook_configured;
    let hint = view.channel.webhook_hint.clone();
    let last_at = view.channel.last_delivery_at.clone();
    let last_ok = view.channel.last_delivery_ok;
    rsx! {
        section { class: "panel page-stack",
            h3 { {ui.text("notifications.events")} }
            if crate::platform::APP {
                EventMatrix { events, app_events, realm }
            } else {
                div { class: "event-list",
                    for (kind, label) in PERSONAL_EVENTS {
                        EventToggle { key: "{kind}", kind, label, events }
                    }
                }
            }
            h3 { {ui.text("notifications.reminders")} }
            div { class: "form-grid",
                Field { id: "resin-warn", label: ui.text("notifications.resin_warn_hours"),
                    input { id: "resin-warn", r#type: "number", min: "1", max: "24", value: "{warn_hours}", oninput: move |event| if let Ok(value) = event.value().parse::<i32>() { warn_hours.set(value); } }
                }
                Field { id: "daily-reminder", label: ui.text("notifications.daily_reminder_time"),
                    input { id: "daily-reminder", r#type: "text", pattern: "[0-2][0-9]:[0-5][0-9]", maxlength: "5", value: "{daily}", oninput: move |event| daily.set(event.value()) }
                }
                Field { id: "weekly-reminder", label: ui.text("notifications.weekly_reminder_time"),
                    input { id: "weekly-reminder", r#type: "text", pattern: "[0-2][0-9]:[0-5][0-9]", maxlength: "5", value: "{weekly}", oninput: move |event| weekly.set(event.value()) }
                }
            }
            h3 { {ui.text("notifications.webhook")} }
            dl { class: "metric-list",
                dt { {ui.text("notifications.current_webhook")} }
                dd {
                    if configured { span { {hint.unwrap_or_default()} } }
                    else { {ui.text("notifications.not_configured")} }
                }
                if let Some(ok) = last_ok {
                    dt { {ui.text("notifications.last_delivery")} }
                    dd {
                        {datetime_text(last_at.as_deref(), ui)} " · "
                        span { class: if ok { "status" } else { "status warn" },
                            {if ok { ui.text("notifications.delivery_ok") } else { ui.text("notifications.delivery_failed") }}
                        }
                    }
                }
            }
            Field { id: "webhook-url", label: ui.text("notifications.webhook_url"),
                input { id: "webhook-url", r#type: "url", autocomplete: "off",
                    placeholder: "https://discord.com/api/webhooks/…",
                    "aria-describedby": "webhook-url-hint",
                    value: "{webhook_input}", oninput: move |event| webhook_input.set(event.value()),
                }
                p { id: "webhook-url-hint", class: "field-hint", {ui.text("notifications.webhook_url_hint")} }
            }
            div { class: "form-grid",
                Field { id: "webhook-format", label: ui.text("notifications.format"),
                    select { id: "webhook-format", value: "{format}", onchange: move |event| format.set(event.value()),
                        option { value: "discord", "Discord" }
                        option { value: "json", {ui.text("notifications.format_json")} }
                    }
                }
                Field { id: "webhook-language", label: ui.text("notifications.language"),
                    select { id: "webhook-language", value: "{language}", onchange: move |event| language.set(event.value()),
                        option { value: "ja", {ui.text("language.japanese")} }
                        option { value: "en", {ui.text("language.english")} }
                    }
                }
            }
            div { class: "button-row",
                Button { r#type: "button", onclick: {
                    move |_| {
                        let typed = webhook_input().trim().to_owned();
                        let webhook_url = (!typed.is_empty()).then_some(Value::String(typed));
                        let chosen = form();
                        let payload = personal_payload(&chosen, webhook_url, crate::platform::APP);
                        let mut webhook_input = webhook_input;
                        spawn_action(state, move |generation| async move {
                            match crate::api::send("PUT", "/me/notifications", Some(payload)).await {
                                Ok(()) if generation_matches(state, generation) => {
                                    follow_device(&chosen);
                                    set_status(state, "notifications.saved", "success");
                                    webhook_input.set(String::new());
                                    load_notifications(state).await;
                                }
                                Ok(()) => {}
                                Err(error) => handle_api_error_for(state, generation, error),
                            }
                        });
                    }
                }, {ui.text("notifications.save")} }
                if configured {
                    Button { variant: ButtonVariant::Destructive, r#type: "button", onclick: {
                        move |_| {
                            let chosen = form();
                            let payload = personal_payload(&chosen, Some(Value::Null), crate::platform::APP);
                            spawn_action(state, move |generation| async move {
                                match crate::api::send("PUT", "/me/notifications", Some(payload)).await {
                                    Ok(()) if generation_matches(state, generation) => {
                                        follow_device(&chosen);
                                        set_status(state, "notifications.webhook_removed", "success");
                                        load_notifications(state).await;
                                    }
                                    Ok(()) => {}
                                    Err(error) => handle_api_error_for(state, generation, error),
                                }
                            });
                        }
                    }, {ui.text("notifications.remove_webhook")} }
                }
                Button { variant: ButtonVariant::Secondary, r#type: "button", disabled: !configured, onclick: move |_| {
                    spawn_action(state, move |generation| async move {
                        match crate::api::json::<serde_json::Value>("POST", "/me/notifications/test", Some(json!({}))).await {
                            Ok(result) if generation_matches(state, generation) => {
                                let key = match result.get("webhook").and_then(Value::as_str) {
                                    Some("sent") => "notifications.test_sent",
                                    Some("failed") => "notifications.test_failed",
                                    _ => "notifications.test_not_configured",
                                };
                                set_status(state, key, if result.get("webhook").and_then(Value::as_str) == Some("sent") { "success" } else { "error" });
                                load_notifications(state).await;
                            }
                            Ok(_) => {}
                            Err(error) => handle_api_error_for(state, generation, error),
                        }
                    });
                }, {ui.text("notifications.test")} }
            }
        }
    }
}

/// App only: one row per notice with this device's and the Webhook's choice side by side.
/// The device column is the account's app inbox choice (`app_events`); the realm currency
/// row has no server notice, so it is only this device's predicted reminder.
#[component]
fn EventMatrix(
    events: Signal<BTreeMap<String, bool>>,
    app_events: Signal<BTreeMap<String, bool>>,
    realm: Signal<bool>,
) -> Element {
    let ui = use_preferences();
    rsx! {
        table { class: "event-matrix",
            thead { tr {
                th { scope: "col", {ui.text("notifications.column_event")} }
                th { id: "event-column-device", scope: "col", {ui.text("notifications.column_device")} }
                th { id: "event-column-webhook", scope: "col", {ui.text("notifications.column_webhook")} }
            } }
            tbody {
                for (kind, label) in PERSONAL_EVENTS {
                    tr { key: "{kind}",
                        th { id: "event-name-{kind}", scope: "row", {ui.text(label)} }
                        td { MatrixCheck { id: format!("app-event-{kind}"), kind, column: "event-column-device", choices: app_events } }
                        td { MatrixCheck { id: format!("event-{kind}"), kind, column: "event-column-webhook", choices: events } }
                    }
                    if kind == "resource_full" {
                        tr { key: "{kind}-realm",
                            th { id: "event-name-realm_currency_full", scope: "row", {ui.text("notifications.device_kind_realm_currency")} }
                            td {
                                label { class: "matrix-check",
                                    input { id: "device-kind-realm_currency_full", r#type: "checkbox", checked: realm(),
                                        "aria-labelledby": "event-name-realm_currency_full event-column-device",
                                        onchange: move |event| { let mut realm = realm; realm.set(event.checked()); },
                                    }
                                }
                            }
                            td {}
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn MatrixCheck(
    id: String,
    kind: &'static str,
    column: &'static str,
    mut choices: Signal<BTreeMap<String, bool>>,
) -> Element {
    let checked = choices().get(kind).copied().unwrap_or(false);
    rsx! {
        label { class: "matrix-check",
            input { id, r#type: "checkbox", checked, "aria-labelledby": "event-name-{kind} {column}",
                onchange: move |event| {
                    let mut next = choices();
                    next.insert(kind.to_owned(), event.checked());
                    choices.set(next);
                }
            }
        }
    }
}

#[component]
fn EventToggle(
    kind: &'static str,
    label: &'static str,
    mut events: Signal<BTreeMap<String, bool>>,
) -> Element {
    let ui = use_preferences();
    let id = format!("event-{kind}");
    let checked = events().get(kind).copied().unwrap_or(false);
    rsx! {
        label { class: "check",
            input { id: "{id}", r#type: "checkbox", checked, onchange: move |event| {
                let mut next = events();
                next.insert(kind.to_owned(), event.checked());
                events.set(next);
            } }
            span { {ui.text(label)} }
        }
    }
}

#[component]
fn AdminEventToggles(events: Signal<BTreeMap<String, bool>>) -> Element {
    rsx! {
        EventToggle { kind: "user_attention", label: "notifications.event_user_attention", events }
        EventToggle { kind: "worker_stalled", label: "notifications.event_worker_stalled", events }
        EventToggle { kind: "deletion_stuck", label: "notifications.event_deletion_stuck", events }
        EventToggle { kind: "sign_in_failed", label: "notifications.event_sign_in_failed", events }
        EventToggle { kind: "signed_in", label: "notifications.event_signed_in", events }
    }
}

/// Shown inside the administration page; shares the save/test flow.
#[component]
pub(crate) fn AdminChannelCard(state: Signal<AppState>) -> Element {
    let ui = use_preferences();
    rsx! {
        super::common::Card { title: ui.text("notifications.admin_title"),
            if let Some(error) = state.read().admin_notifications_error.clone() {
                LoadError { state, message: ui.text(&error) }
            } else if let Some(view) = state.read().admin_notifications.clone() {
                AdminEditor { state, view }
            } else {
                p { role: "status", {ui.text("notifications.loading")} }
            }
        }
    }
}

#[component]
fn AdminEditor(state: Signal<AppState>, view: NotificationChannelView) -> Element {
    let ui = use_preferences();
    let mut webhook_input = use_signal(String::new);
    let mut format = use_signal(|| view.webhook_format.clone());
    let mut language = use_signal(|| view.language.clone());
    let events = use_signal(|| view.events.clone());
    let configured = view.webhook_configured;
    let hint = view.webhook_hint.clone();
    rsx! {
        div { class: "form-stack",
            dl { class: "metric-list",
                dt { {ui.text("notifications.current_webhook")} }
                dd {
                    if configured { span { {hint.unwrap_or_default()} } }
                    else { {ui.text("notifications.not_configured")} }
                }
            }
            Field { id: "admin-webhook-url", label: ui.text("notifications.webhook_url"),
                input { id: "admin-webhook-url", r#type: "url", autocomplete: "off", value: "{webhook_input}", oninput: move |event| webhook_input.set(event.value()) }
            }
            div { class: "form-grid",
                Field { id: "admin-webhook-format", label: ui.text("notifications.format"),
                    select { id: "admin-webhook-format", value: "{format}", onchange: move |event| format.set(event.value()),
                        option { value: "discord", "Discord" }
                        option { value: "json", {ui.text("notifications.format_json")} }
                    }
                }
                Field { id: "admin-webhook-language", label: ui.text("notifications.language"),
                    select { id: "admin-webhook-language", value: "{language}", onchange: move |event| language.set(event.value()),
                        option { value: "ja", {ui.text("language.japanese")} }
                        option { value: "en", {ui.text("language.english")} }
                    }
                }
            }
            div { class: "event-list",
                AdminEventToggles { events }
            }
            div { class: "button-row",
                Button { r#type: "button", onclick: move |_| {
                    let typed = webhook_input().trim().to_owned();
                    let send_webhook = !typed.is_empty();
                    let mut payload = json!({
                        "webhook_format": format(),
                        "language": language(),
                        "events": events(),
                    });
                    if send_webhook { payload["webhook_url"] = Value::String(typed); }
                    let mut webhook_input = webhook_input;
                    spawn_action(state, move |generation| async move {
                        match crate::api::send("PUT", "/admin/notifications", Some(payload)).await {
                            Ok(()) if generation_matches(state, generation) => {
                                set_status(state, "notifications.saved", "success");
                                webhook_input.set(String::new());
                                load_admin_notifications(state).await;
                            }
                            Ok(()) => {}
                            Err(error) => handle_api_error_for(state, generation, error),
                        }
                    });
                }, {ui.text("notifications.save")} }
                if configured {
                    Button { variant: ButtonVariant::Destructive, r#type: "button", onclick: move |_| {
                        let payload = json!({
                            "webhook_url": null,
                            "webhook_format": format(),
                            "language": language(),
                            "events": events(),
                        });
                        spawn_action(state, move |generation| async move {
                            match crate::api::send("PUT", "/admin/notifications", Some(payload)).await {
                                Ok(()) if generation_matches(state, generation) => {
                                    set_status(state, "notifications.webhook_removed", "success");
                                    load_admin_notifications(state).await;
                                }
                                Ok(()) => {}
                                Err(error) => handle_api_error_for(state, generation, error),
                            }
                        });
                    }, {ui.text("notifications.remove_webhook")} }
                }
                Button { variant: ButtonVariant::Secondary, r#type: "button", disabled: !configured, onclick: move |_| {
                    spawn_action(state, move |generation| async move {
                        match crate::api::json::<serde_json::Value>("POST", "/admin/notifications/test", Some(json!({}))).await {
                            Ok(result) if generation_matches(state, generation) => {
                                let key = match result.get("webhook").and_then(Value::as_str) {
                                    Some("sent") => "notifications.test_sent",
                                    Some("failed") => "notifications.test_failed",
                                    _ => "notifications.test_not_configured",
                                };
                                set_status(state, key, if result.get("webhook").and_then(Value::as_str) == Some("sent") { "success" } else { "error" });
                                load_admin_notifications(state).await;
                            }
                            Ok(_) => {}
                            Err(error) => handle_api_error_for(state, generation, error),
                        }
                    });
                }, {ui.text("notifications.test")} }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn form() -> PersonalForm {
        PersonalForm {
            format: "discord".into(),
            language: "ja".into(),
            events: BTreeMap::from([("signed_in".into(), true)]),
            app_events: BTreeMap::from([
                ("signed_in".into(), false),
                ("resource_full".into(), true),
            ]),
            warn_hours: 2,
            daily: "21:00".into(),
            weekly: "15:00".into(),
        }
    }

    #[test]
    fn only_the_app_sends_its_device_choices() {
        let web = personal_payload(&form(), None, false);
        assert!(web.get("app_events").is_none());
        assert_eq!(web["events"], json!({"signed_in": true}));
        let app = personal_payload(&form(), None, true);
        assert_eq!(
            app["app_events"],
            json!({"resource_full": true, "signed_in": false})
        );
        assert_eq!(app["events"], json!({"signed_in": true}));
    }

    #[test]
    fn webhook_url_is_kept_replaced_or_removed() {
        assert!(
            personal_payload(&form(), None, true)
                .get("webhook_url")
                .is_none()
        );
        let typed = personal_payload(
            &form(),
            Some(Value::String("https://example.test/h".into())),
            false,
        );
        assert_eq!(typed["webhook_url"], "https://example.test/h");
        assert!(personal_payload(&form(), Some(Value::Null), false)["webhook_url"].is_null());
        assert!(
            personal_payload(&form(), Some(Value::Null), false)
                .as_object()
                .unwrap()
                .contains_key("webhook_url")
        );
    }

    #[test]
    fn every_personal_notice_is_listed_once() {
        let kinds: std::collections::BTreeSet<_> =
            PERSONAL_EVENTS.iter().map(|(kind, _)| *kind).collect();
        assert_eq!(kinds.len(), PERSONAL_EVENTS.len());
        for (kind, label) in PERSONAL_EVENTS {
            assert_eq!(label, format!("notifications.event_{kind}"));
        }
    }
}
