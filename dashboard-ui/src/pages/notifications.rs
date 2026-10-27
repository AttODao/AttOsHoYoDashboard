//! Personal notification channel: webhook, events, reminder times and browser push.
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
            p { class: "field-hint", {ui.text(if crate::platform::APP { "notifications.personal_hint_app" } else { "notifications.personal_hint" })} }
            {device}
            if let Some(error) = state.read().notifications_error.clone() {
                LoadError { state, message: ui.text(&error) }
            }
            if let Some(view) = state.read().notifications.clone() {
                PersonalChannelCard { state, view: view.clone() }
                // Web Push needs a browser service worker; the app does not offer it.
                if !crate::platform::APP {
                    PushCard { state, view }
                }
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
    let mut webhook_input = use_signal(String::new);
    let mut format = use_signal(|| view.channel.webhook_format.clone());
    let mut language = use_signal(|| view.channel.language.clone());
    let events = use_signal(|| view.channel.events.clone());
    let mut warn_hours = use_signal(|| view.resin_warn_hours);
    let mut daily = use_signal(|| view.daily_reminder_time.clone());
    let mut weekly = use_signal(|| view.weekly_reminder_time.clone());
    let configured = view.channel.webhook_configured;
    let hint = view.channel.webhook_hint.clone();
    let last_at = view.channel.last_delivery_at.clone();
    let last_ok = view.channel.last_delivery_ok;
    rsx! {
        section { class: "panel page-stack",
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
            h3 { {ui.text("notifications.events")} }
            div { class: "event-list",
                PersonalEventToggles { events }
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
            div { class: "button-row",
                Button { r#type: "button", onclick: {
                    move |_| {
                        let typed = webhook_input().trim().to_owned();
                        let webhook_url: Value = if typed.is_empty() { Value::Null } else { Value::String(typed) };
                        let send_webhook = !webhook_input().is_empty();
                        let payload = {
                            let mut map = json!({
                                "webhook_format": format(),
                                "language": language(),
                                "events": events(),
                                "resin_warn_hours": warn_hours(),
                                "daily_reminder_time": daily(),
                                "weekly_reminder_time": weekly(),
                            });
                            if send_webhook {
                                map["webhook_url"] = webhook_url;
                            }
                            map
                        };
                        let mut webhook_input = webhook_input;
                        spawn_action(state, move |generation| async move {
                            match crate::api::send("PUT", "/me/notifications", Some(payload)).await {
                                Ok(()) if generation_matches(state, generation) => {
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
                            let payload = json!({
                                "webhook_url": null,
                                "webhook_format": format(),
                                "language": language(),
                                "events": events(),
                                "resin_warn_hours": warn_hours(),
                                "daily_reminder_time": daily(),
                                "weekly_reminder_time": weekly(),
                            });
                            spawn_action(state, move |generation| async move {
                                match crate::api::send("PUT", "/me/notifications", Some(payload)).await {
                                    Ok(()) if generation_matches(state, generation) => {
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
fn PersonalEventToggles(events: Signal<BTreeMap<String, bool>>) -> Element {
    rsx! {
        EventToggle { kind: "reward_failed", label: "notifications.event_reward_failed", events }
        EventToggle { kind: "reward_claimed", label: "notifications.event_reward_claimed", events }
        EventToggle { kind: "cookie_invalid", label: "notifications.event_cookie_invalid", events }
        EventToggle { kind: "signed_in", label: "notifications.event_signed_in", events }
        EventToggle { kind: "sign_in_failed", label: "notifications.event_sign_in_failed", events }
        EventToggle { kind: "resource_near_cap", label: "notifications.event_resource_near_cap", events }
        EventToggle { kind: "resource_full", label: "notifications.event_resource_full", events }
        EventToggle { kind: "expeditions_done", label: "notifications.event_expeditions_done", events }
        EventToggle { kind: "transformer_ready", label: "notifications.event_transformer_ready", events }
        EventToggle { kind: "daily_unfinished", label: "notifications.event_daily_unfinished", events }
        EventToggle { kind: "weekly_unfinished", label: "notifications.event_weekly_unfinished", events }
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

#[component]
fn PushCard(state: Signal<AppState>, view: crate::models::PersonalNotificationView) -> Element {
    let ui = use_preferences();
    let available = view.push_public_key.is_some();
    let subscriptions = view.push_subscriptions;
    rsx! {
        section { class: "panel page-stack",
            h3 { {ui.text("notifications.browser_push")} }
            if !available {
                p { role: "status", {ui.text("notifications.push_unavailable")} }
            } else {
                p { class: "field-hint", {ui.format("notifications.push_subscriptions", &[("count", &subscriptions.to_string())])} }
                div { class: "button-row",
                    Button { id: "push-subscribe", r#type: "button", onclick: {
                        let key = view.push_public_key.clone().unwrap_or_default();
                        move |_| {
                            let key = key.clone();
                            spawn_action(state, move |generation| async move {
                                #[cfg(target_arch = "wasm32")]
                                match crate::webauthn::subscribe_push(&key).await {
                                    Ok(_) if generation_matches(state, generation) => {
                                        set_status(state, "notifications.push_registered", "success");
                                        load_notifications(state).await;
                                    }
                                    Ok(_) => {}
                                    Err(error) => handle_api_error_for(state, generation, error),
                                }
                                #[cfg(not(target_arch = "wasm32"))]
                                let _ = (key, generation);
                            });
                        }
                    }, {ui.text("notifications.enable_push")} }
                    Button { variant: ButtonVariant::Secondary, r#type: "button", disabled: subscriptions == 0, onclick: move |_| {
                        spawn_action(state, move |generation| async move {
                            #[cfg(target_arch = "wasm32")]
                            match crate::webauthn::unsubscribe_push().await {
                                Ok(()) if generation_matches(state, generation) => {
                                    set_status(state, "notifications.push_unregistered", "success");
                                    load_notifications(state).await;
                                }
                                Ok(()) => {}
                                Err(error) => handle_api_error_for(state, generation, error),
                            }
                            #[cfg(not(target_arch = "wasm32"))]
                            let _ = generation;
                        });
                    }, {ui.text("notifications.disable_push")} }
                }
            }
        }
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
