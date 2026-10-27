//! Device notification lifecycle: predicted local reminders, the server inbox registration
//! (the same notices as the web's Webhook and browser push), and notification-open events.
use super::{
    navigation,
    preferences::Preferences,
    state::{self, AppState, Phase},
};
use crate::mobile::{
    inbox::{self, Config, Plan, Registered},
    notify,
    reminders::{Outgoing, Schedule, Settings, forgets_schedule},
    storage,
};
use dioxus::prelude::*;
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
const SETTINGS_FILE: &str = "device-notifications.json";
const SCHEDULE_FILE: &str = "notification-schedule.json";
/// While the app is visible the inbox is read at most this often (plus on every return).
const FOREGROUND_POLL_MS: i64 = 60_000;

/// The server registration as the settings card shows it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Server {
    Off,
    Registering,
    Active,
    Failed,
}

#[derive(Clone, Copy, PartialEq)]
pub(super) struct DeviceNotifications {
    pub(super) settings: Signal<Settings>,
    pub(super) granted: Signal<bool>,
    pub(super) upcoming: Signal<usize>,
    pub(super) server: Signal<Server>,
}
impl DeviceNotifications {
    pub(super) fn save(&self) {
        let _ = storage::write(SETTINGS_FILE, &self.settings.peek().to_json());
    }
}
pub(super) fn use_device_context() -> DeviceNotifications {
    use_context()
}

fn registered() -> Option<Registered> {
    Registered::from_json(notify::inbox_state().as_deref())
}

/// Reads the inbox now unless it was read within `FOREGROUND_POLL_MS` (or `force`).
fn poll(last: &Cell<i64>, force: bool) {
    let now = crate::platform::now_ms() as i64;
    if force || now - last.get() >= FOREGROUND_POLL_MS {
        last.set(now);
        notify::inbox_poll_now();
    }
}

fn set_server(mut server: Signal<Server>, value: Server) {
    if *server.peek() != value {
        server.set(value);
    }
}

pub(super) fn use_device_notifications(state: Signal<AppState>) -> DeviceNotifications {
    let settings = use_signal(|| Settings::from_json(storage::read(SETTINGS_FILE).as_deref()));
    let mut granted = use_signal(notify::permission_granted);
    let mut upcoming = use_signal(|| 0);
    let server = use_signal(|| {
        if registered().is_some() {
            Server::Active
        } else {
            Server::Off
        }
    });
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

    // Server notices: register this installation while notifications are on and a user
    // who may receive them is signed in; Kotlin polls the inbox from then on.
    let registering = use_hook(|| Rc::new(Cell::new(false)));
    let last_poll = use_hook(|| Rc::new(Cell::new(0_i64)));
    let visible = use_hook(|| Rc::new(Cell::new(true)));
    {
        let (registering, last_poll, visible) =
            (registering.clone(), last_poll.clone(), visible.clone());
        use_effect(move || {
            let snapshot = state.read();
            let enabled = settings.read().enabled;
            let ui = *preferences.read();
            let user = (snapshot.phase == Phase::SignedIn)
                .then_some(snapshot.me.as_ref())
                .flatten()
                .filter(|me| !me.onboarding.password_change_required)
                .map(|me| me.id);
            let forgets = forgets_schedule(
                snapshot.phase == Phase::SignedOut,
                snapshot.status.as_ref().map(|status| status.message_key),
            );
            drop(snapshot);
            let current = registered();
            match inbox::plan(enabled, user, forgets, current.as_ref()) {
                Plan::Register(owner) => {
                    if registering.get() || *server.peek() == Server::Failed {
                        return;
                    }
                    if current.is_some() {
                        notify::inbox_stop();
                    }
                    registering.set(true);
                    set_server(server, Server::Registering);
                    let registering = registering.clone();
                    let last_poll = last_poll.clone();
                    let channel = ui.text("notifications.device_channel");
                    spawn(async move {
                        let result = crate::api::json::<Value>(
                            "POST",
                            "/me/notifications/app-devices",
                            Some(json!({})),
                        )
                        .await;
                        let config = result.ok().and_then(|view| {
                            Some(Config {
                                origin: crate::mobile::http::server_origin()?.to_owned(),
                                device: view.get("id")?.as_str()?.to_owned(),
                                owner: owner.to_string(),
                                cursor: view.get("cursor")?.as_i64()?,
                                agent: crate::mobile::http::USER_AGENT.to_owned(),
                            })
                        });
                        let ok = config
                            .as_ref()
                            .is_some_and(|config| notify::inbox_configure(channel, config));
                        if ok {
                            last_poll.set(crate::platform::now_ms() as i64);
                        }
                        // A failure is not retried until notifications are switched again,
                        // so a broken bridge cannot pile up server registrations.
                        set_server(server, if ok { Server::Active } else { Server::Failed });
                        registering.set(false);
                    });
                }
                Plan::Keep => {
                    set_server(server, Server::Active);
                    if visible.get() {
                        poll(&last_poll, false);
                    }
                }
                Plan::Unregister(device) => {
                    notify::inbox_stop();
                    set_server(server, Server::Off);
                    spawn(async move {
                        // Best effort: an unreachable server forgets the device after 30 days.
                        let _ = crate::api::send(
                            "DELETE",
                            &format!("/me/notifications/app-devices/{device}"),
                            None,
                        )
                        .await;
                    });
                }
                Plan::Forget => {
                    notify::inbox_stop();
                    set_server(server, Server::Off);
                }
                Plan::Idle => {
                    if !enabled {
                        set_server(server, Server::Off);
                    }
                }
            }
        });
    }
    let (eval_poll, eval_visible) = (last_poll.clone(), visible.clone());
    use_hook(move || {
        let (last_poll, visible) = (eval_poll, eval_visible);
        let mut eval = document::eval(
            "window.__ahdNotificationEvent = (kind) => dioxus.send(String(kind)); document.addEventListener('visibilitychange', () => dioxus.send(document.hidden ? 'hidden' : 'visible')); dioxus.send('start');",
        );
        spawn(async move {
            while let Ok(kind) = eval.recv::<String>().await {
                visible.set(kind != "hidden");
                if (kind == "visible" || kind == "start") && *server.peek() == Server::Active {
                    poll(&last_poll, true);
                }
                let value = notify::permission_granted();
                if *granted.peek() != value {
                    granted.set(value);
                }
                if let Some(target) = notify::take_open_target() {
                    let open = {
                        let read = state.read();
                        read.phase == Phase::SignedIn && read.ready()
                    };
                    if open {
                        state::set_page(state, navigation::notification_target(&target));
                    }
                }
            }
        });
    });
    // While visible, read the inbox every minute even when nothing on screen changes.
    {
        let (last_poll, visible) = (last_poll.clone(), visible.clone());
        use_hook(move || {
            spawn(async move {
                loop {
                    crate::platform::sleep_ms(FOREGROUND_POLL_MS as u32).await;
                    if visible.get() && *server.peek() == Server::Active {
                        poll(&last_poll, false);
                    }
                }
            })
        });
    }
    let device = DeviceNotifications {
        settings,
        granted,
        upcoming,
        server,
    };
    use_context_provider(|| device);
    device
}
