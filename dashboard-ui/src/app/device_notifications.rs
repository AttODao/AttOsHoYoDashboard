//! Device-local reminder lifecycle and notification-open events.
use super::{
    preferences::Preferences,
    state::{self, AppState, Page, Phase},
};
use crate::mobile::{
    notify,
    reminders::{Outgoing, Schedule, Settings, forgets_schedule},
    storage,
};
use dioxus::prelude::*;
use std::{cell::RefCell, rc::Rc};
const SETTINGS_FILE: &str = "device-notifications.json";
const SCHEDULE_FILE: &str = "notification-schedule.json";
#[derive(Clone, Copy, PartialEq)]
pub(super) struct DeviceNotifications {
    pub(super) settings: Signal<Settings>,
    pub(super) granted: Signal<bool>,
    pub(super) upcoming: Signal<usize>,
}
impl DeviceNotifications {
    pub(super) fn save(&self) {
        let _ = storage::write(SETTINGS_FILE, &self.settings.peek().to_json());
    }
}
pub(super) fn use_device_context() -> DeviceNotifications {
    use_context()
}
pub(super) fn use_device_notifications(state: Signal<AppState>) -> DeviceNotifications {
    let settings = use_signal(|| Settings::from_json(storage::read(SETTINGS_FILE).as_deref()));
    let mut granted = use_signal(notify::permission_granted);
    let mut upcoming = use_signal(|| 0);
    let saved = use_hook(|| {
        Rc::new(RefCell::new((
            Schedule::from_json(storage::read(SCHEDULE_FILE).as_deref()),
            None::<Vec<Outgoing>>,
        )))
    });
    let preferences = use_context::<Signal<Preferences>>();
    use_effect(move || {
        let snapshot = state.read();
        let settings = settings();
        // Resolve the context hook above; only its signal is read in the effect.
        let ui = *preferences.read();
        let mut saved = saved.borrow_mut();
        let (schedule, last_pushed) = &mut *saved;
        let before = schedule.to_json();
        match snapshot.phase {
            Phase::SignedOut => {
                if forgets_schedule(
                    snapshot.phase == Phase::SignedOut,
                    snapshot.status.as_ref().map(|status| status.message_key),
                ) {
                    schedule.clear();
                }
            }
            Phase::SignedIn => {
                if let Some(me) = &snapshot.me {
                    for entry in &snapshot.account_statuses {
                        schedule.update_account(me.id, entry.id, &entry.status);
                    }
                    if let Some(accounts) = &snapshot.accounts {
                        let ids: Vec<_> = state::visible_detected_accounts(accounts)
                            .iter()
                            .map(|a| a.id)
                            .collect();
                        schedule.retain_accounts(&ids);
                    }
                }
            }
            _ => {}
        }
        let now = crate::platform::now_ms() as i64;
        schedule.drop_past(now);
        let after = schedule.to_json();
        if before != after {
            let _ = storage::write(SCHEDULE_FILE, &after);
        }
        // Forgetting sign-out already cleared the schedule; a kept offline schedule is pushed again to restore force-stop alarms.
        let outgoing =
            crate::mobile::reminders::render(&schedule.upcoming(&settings, now), ui.language);
        if last_pushed.as_ref() != Some(&outgoing) {
            notify::schedule(ui.text("notifications.device_channel"), &outgoing);
            *last_pushed = Some(outgoing.clone());
        }
        let count = outgoing.iter().filter(|entry| entry.at > now).count();
        if *upcoming.peek() != count {
            upcoming.set(count);
        }
    });
    use_hook(move || {
        let mut eval = document::eval(
            "window.__ahdNotificationEvent = (kind) => dioxus.send(String(kind)); document.addEventListener('visibilitychange', () => { if (!document.hidden) dioxus.send('visible'); }); dioxus.send('start');",
        );
        spawn(async move {
            while eval.recv::<String>().await.is_ok() {
                let value = notify::permission_granted();
                if *granted.peek() != value {
                    granted.set(value);
                }
                if let Some(target) = notify::take_open_target() {
                    let open = {
                        let read = state.read();
                        target == "dashboard" && read.phase == Phase::SignedIn && read.ready()
                    };
                    if open {
                        state::set_page(state, Page::Dashboard);
                    }
                }
            }
        });
    });
    let device = DeviceNotifications {
        settings,
        granted,
        upcoming,
    };
    use_context_provider(|| device);
    device
}
