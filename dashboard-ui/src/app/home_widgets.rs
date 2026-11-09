//! Keeps Android home-screen widgets in step with the signed-in dashboard (see `mobile::widgets`).
use super::state::{AppState, DASHBOARD_GAMES, Phase};
use crate::mobile::{http, reminders, widgets};
use dioxus::prelude::*;
use std::{cell::RefCell, rc::Rc};

pub(super) fn use_home_widgets(state: Signal<AppState>) {
    let last = use_hook(|| Rc::new(RefCell::new(None::<String>)));
    use_effect(move || {
        let snapshot = state.read();
        match snapshot.phase {
            Phase::SignedIn if snapshot.me.is_some() && snapshot.accounts.is_some() => {
                let statuses: Vec<_> = snapshot
                    .account_statuses
                    .iter()
                    .map(|e| (e.id, &e.status))
                    .collect();
                let p = widgets::publication(
                    http::USER_AGENT,
                    &DASHBOARD_GAMES,
                    |g| snapshot.selected_game_account(g),
                    &statuses,
                );
                if let Ok(key) = serde_json::to_string(&p)
                    && last.borrow().as_ref() != Some(&key)
                    && let Some(origin) = http::server_origin()
                    && widgets::publish(&origin, &p)
                {
                    *last.borrow_mut() = Some(key);
                }
            }
            Phase::SignedOut
                if reminders::forgets_schedule(
                    true,
                    snapshot.status.as_ref().map(|s| s.message_key),
                ) && last.borrow().as_deref() != Some("signed-out") =>
            {
                widgets::sign_out();
                *last.borrow_mut() = Some("signed-out".into());
            }
            _ => {}
        }
    });
}
