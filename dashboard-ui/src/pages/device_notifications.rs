//! Device notifications use Android permission/settings, independently of Webhooks: server
//! notices (the events chosen below, like the Webhook) plus predicted local reminders.
use crate::app::{
    actions::{handle_api_error_for, spawn_action},
    device_notifications::{Server, use_device_context},
    preferences::use_preferences,
    state::{AppState, generation_matches, set_status},
};
use crate::{
    components::{Button, ButtonVariant},
    mobile::{
        notify,
        reminders::{Kind, Outgoing, TEST_NOTIFICATION_ID},
    },
};
use dioxus::prelude::*;
use serde_json::{Value, json};
#[component]
pub(super) fn DeviceNotificationsCard(state: Signal<AppState>) -> Element {
    let ui = use_preferences();
    let mut device = use_device_context();
    let enabled = device.settings.read().enabled;
    let server = (device.server)();
    rsx! {
        section { class: "panel page-stack",
            h3 { {ui.text("notifications.device_title")} }
            p { class: "field-hint", {ui.text("notifications.device_hint")} }
            label { class: "check",
                input { id: "device-notifications-enabled", r#type: "checkbox", checked: enabled, onchange: move |event| {
                    let on = event.checked();
                    device.settings.write().enabled = on;
                    device.save();
                    if on && !notify::permission_granted() { notify::request_permission(); }
                } }
                span { {ui.text("notifications.device_enabled")} }
            }
            if enabled && ! (device.granted)() {
                p { class: "warning", role: "status", {ui.text("notifications.device_permission_denied")} }
                div { class: "button-row",
                    Button { r#type: "button", onclick: move |_| notify::request_permission(), {ui.text("notifications.device_request_permission")} }
                    Button { r#type: "button", variant: ButtonVariant::Secondary, onclick: move |_| notify::open_settings(), {ui.text("notifications.device_open_settings")} }
                }
            }
            if enabled {
                match server {
                    Server::Active => rsx! { p { class: "status", role: "status", {ui.text("notifications.device_server_active")} } },
                    Server::Registering => rsx! { p { role: "status", {ui.text("notifications.device_server_pending")} } },
                    Server::Failed => rsx! { p { class: "warning", role: "status", {ui.text("notifications.device_server_failed")} } },
                    Server::Off => rsx! {},
                }
            }
            h3 { {ui.text("notifications.device_predicted_title")} }
            p { class: "field-hint", {ui.text("notifications.device_predicted_hint")} }
            div { class: "event-list",
                KindToggle { kind: Kind::ResourceFull, label: "notifications.event_resource_full" }
                KindToggle { kind: Kind::RealmCurrencyFull, label: "notifications.device_kind_realm_currency" }
                KindToggle { kind: Kind::ExpeditionsDone, label: "notifications.event_expeditions_done" }
                KindToggle { kind: Kind::TransformerReady, label: "notifications.event_transformer_ready" }
            }
            if enabled { p { class: "field-hint", {ui.format("notifications.device_scheduled", &[("count", &(device.upcoming)().to_string())])} } }
            div { class: "button-row",
                Button { r#type: "button", variant: ButtonVariant::Secondary, disabled: !enabled, onclick: move |_| {
                    if !notify::permission_granted() {
                        set_status(state, "notifications.device_permission_denied", "error");
                        notify::request_permission();
                    } else {
                        let entry = Outgoing { id: TEST_NOTIFICATION_ID, at: crate::platform::now_ms() as i64, title: ui.text("notifications.device_test_title").into(), body: ui.text("notifications.device_test_body").into(), tag: String::new() };
                        if notify::show_now(ui.text("notifications.device_channel"), &entry) { set_status(state, "notifications.device_test_sent", "success"); }
                        else { set_status(state, "notifications.device_test_failed", "error"); }
                    }
                }, {ui.text("notifications.device_test")} }
                Button { r#type: "button", variant: ButtonVariant::Secondary, disabled: server != Server::Active, onclick: move |_| {
                    spawn_action(state, move |generation| async move {
                        match crate::api::json::<Value>("POST", "/me/notifications/test", Some(json!({}))).await {
                            Ok(result) if generation_matches(state, generation) => {
                                if result.get("app").and_then(Value::as_bool) == Some(true) {
                                    notify::inbox_poll_now();
                                    set_status(state, "notifications.device_server_test_sent", "success");
                                } else {
                                    set_status(state, "notifications.device_server_test_unregistered", "error");
                                }
                            }
                            Ok(_) => {}
                            Err(error) => handle_api_error_for(state, generation, error),
                        }
                    });
                }, {ui.text("notifications.device_server_test")} }
            }
        }
    }
}
#[component]
fn KindToggle(kind: Kind, label: &'static str) -> Element {
    let ui = use_preferences();
    let mut device = use_device_context();
    let settings = device.settings.read().clone();
    let id = format!("device-kind-{}", kind.code());
    rsx! {
        label { class: "check",
            input { id, r#type: "checkbox", checked: settings.kind(kind), disabled: !settings.enabled,
                onchange: move |event| { device.settings.write().set_kind(kind, event.checked()); device.save(); }
            }
            span { {ui.text(label)} }
        }
    }
}
