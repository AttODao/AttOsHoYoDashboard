//! App-only update prompt and download/install progress.
use super::common::ModalDialog;
use crate::app::{
    preferences::use_preferences,
    updates::{Flow, Updates},
};
use crate::components::{Button, ButtonVariant};
use crate::mobile::update_install::progress_text;
use dioxus::prelude::*;

#[component]
pub(crate) fn UpdateDialog(updates: Updates) -> Element {
    let ui = use_preferences();
    let flow = updates.flow.read().clone();
    let Some(check) = updates.check.read().clone() else {
        return rsx! {};
    };
    if flow == Flow::Hidden {
        return rsx! {};
    }
    let current = check.current.to_string();
    let latest = check.latest.to_string();
    let notes = if check.release_notes.trim().is_empty() {
        ui.text("update.no_release_notes").to_owned()
    } else {
        check.release_notes
    };
    let start = move |_| {
        updates.start(
            ui.format("update.installed_title", &[("version", &latest)]),
            ui.text("update.installed_body").to_owned(),
            ui.text("update.channel"),
        )
    };
    // The native dialog keeps the handler it was opened with: read the state when it fires.
    // Copied out first: a guard kept through the match would make the writes below panic
    // (AlreadyBorrowed), which poisons the WebView's request lock and aborts the app.
    let dismiss = move |_| {
        let current = updates.flow.peek().clone();
        match current {
            Flow::Prompt => updates.later(),
            Flow::Downloading(_) | Flow::Verifying => updates.cancel(),
            _ => updates.close(),
        }
    };
    rsx! {
        ModalDialog { id: "app-update-dialog", title: ui.text("update.title"), onclose: dismiss, back_closes: flow.back_closes(),
            dl { class: "metric-list",
                div { class: "metric", dt { {ui.text("update.current_version")} } dd { "{current}" } }
                div { class: "metric", dt { {ui.text("update.new_version")} } dd { "{check.latest}" } }
            }
            h3 { id: "app-update-notes-title", {ui.text("update.release_notes")} }
            div { class: "release-notes", tabindex: "0", role: "region", "aria-labelledby": "app-update-notes-title", "{notes}" }
            match flow {
                Flow::Prompt => rsx! {
                    p { id: "app-update-restart-note", class: "update-note", {ui.text("update.restart_note")} }
                    div { class: "actions update-actions",
                        Button { variant: ButtonVariant::Secondary, id: "app-update-skip", onclick: move |_| updates.skip(), {ui.text("update.skip_version")} }
                        Button { variant: ButtonVariant::Secondary, id: "app-update-later", onclick: move |_| updates.later(), {ui.text("update.later")} }
                        Button { variant: ButtonVariant::Primary, id: "app-update-now", onclick: start, {ui.text("update.update_now")} }
                    }
                },
                Flow::AwaitingNotificationAnswer => rsx! {
                    p { role: "status", "aria-live": "polite", {ui.text("update.notification_permission")} }
                },
                Flow::NeedsInstallPermission => rsx! {
                    p { role: "status", "aria-live": "polite", {ui.text("update.permission_needed")} }
                    div { class: "actions update-actions",
                        Button { variant: ButtonVariant::Secondary, id: "app-update-close", onclick: move |_| updates.close(), {ui.text("admin.cancel")} }
                        Button { variant: ButtonVariant::Primary, id: "app-update-settings", onclick: move |_| updates.open_settings(), {ui.text("update.open_settings")} }
                    }
                },
                Flow::Downloading(p) => rsx! {
                    if p.total == 0 { progress { id: "app-update-progress" } } else { progress { id: "app-update-progress", max: "{p.total}", value: "{p.received}" } }
                    p { role: "status", "aria-live": "polite", {ui.format("update.downloading", &[("progress", &progress_text(p))])} }
                    div { class: "actions update-actions", Button { variant: ButtonVariant::Secondary, id: "app-update-cancel", onclick: move |_| updates.cancel(), {ui.text("admin.cancel")} } }
                },
                Flow::Verifying => rsx! {
                    progress {}
                    p { role: "status", "aria-live": "polite", {ui.text("update.verifying")} }
                    div { class: "actions update-actions", Button { variant: ButtonVariant::Secondary, id: "app-update-cancel", onclick: move |_| updates.cancel(), {ui.text("admin.cancel")} } }
                },
                Flow::Installing => rsx! {
                    p { role: "status", "aria-live": "polite", {ui.text("update.installing")} }
                    p { id: "app-update-restart-note", class: "update-note", {ui.text("update.restart_note")} }
                    div { class: "actions update-actions", Button { id: "app-update-close", onclick: move |_| updates.close(), {ui.text("admin.close")} } }
                },
                Flow::Failed(error) => rsx! {
                    p { role: "alert", class: "update-error", {ui.text(error.message_key())} }
                    div { class: "actions update-actions",
                        Button { variant: ButtonVariant::Secondary, id: "app-update-close", onclick: move |_| updates.close(), {ui.text("admin.close")} }
                        Button { variant: ButtonVariant::Primary, id: "app-update-retry", onclick: start, {ui.text("update.retry")} }
                    }
                },
                Flow::Hidden => rsx! {},
            }
        }
    }
}
