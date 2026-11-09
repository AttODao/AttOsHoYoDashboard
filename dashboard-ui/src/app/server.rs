//! Changing the app's server from the login page (`pages/app_server.rs`): the typed address is
//! checked (`mobile::server`), probed at `/health`, then everything that belongs to the old
//! server is removed before the new one is selected. The browser build has one server, its
//! own origin, and never shows this.
use super::state::AppState;
use dioxus::prelude::*;

/// Sign-out reason after a change; one of the forgetting sign-outs
/// (`mobile::reminders::FORGETTING_SIGN_OUTS`), so the reminder schedule, the inbox
/// registration and the widgets are dropped like after a logout.
#[cfg_attr(not(feature = "mobile"), allow(dead_code))]
pub(super) const SERVER_CHANGED: &str = "messages.server_changed";

/// The selected server's origin and the build-time default, for display.
pub(super) fn origins() -> (Option<String>, Option<String>) {
    #[cfg(feature = "mobile")]
    {
        (
            crate::mobile::http::server_origin(),
            crate::mobile::http::default_origin(),
        )
    }
    #[cfg(not(feature = "mobile"))]
    {
        (None, None)
    }
}

/// What the typed address leads to.
#[cfg_attr(not(feature = "mobile"), allow(dead_code))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Outcome {
    /// The address names the server already in use: nothing to clear.
    Unchanged,
    /// Switched; the app is signed out and per-server data is gone.
    Changed,
    /// Not switched; the message key says why.
    Rejected(&'static str),
}

/// Checks `input`, probes it, and switches to it. Must run while signed out (login page).
pub(super) async fn change(state: Signal<AppState>, input: String) -> Outcome {
    #[cfg(feature = "mobile")]
    {
        use super::session::signed_out;
        use crate::mobile::{
            http,
            server::{ServerError, origin_from_input},
        };
        let origin = match origin_from_input(&input) {
            Ok(origin) => origin,
            Err(error) => return Outcome::Rejected(error.message_key()),
        };
        if http::server_origin().as_deref() == Some(origin.as_str()) {
            return Outcome::Unchanged;
        }
        if let Err(error) = http::probe(&origin).await {
            return Outcome::Rejected(error.message_key());
        }
        leave_current_server().await;
        // Everything of the old server goes before the new one counts as selected: if a step
        // fails or the app stops in between, the old server stays selected (with whatever is
        // left of its own state), so the new server never sees the old server's state or cookie.
        if !forget_local_server_data() {
            crate::mobile::log::warn("could not remove the old server's data for a server change");
            return Outcome::Rejected(ServerError::NotSaved.message_key());
        }
        if let Err(error) = http::switch_server(&origin) {
            return Outcome::Rejected(error.message_key());
        }
        crate::mobile::log::info("the server was changed");
        signed_out(state, SERVER_CHANGED);
        Outcome::Changed
    }
    #[cfg(not(feature = "mobile"))]
    {
        let _ = (state, input);
        Outcome::Rejected("auth.server_error_not_saved")
    }
}

/// Best effort, a few seconds at most: unregister this device's inbox and end the session on
/// the old server while its cookie still exists (the login page also shows after a failed
/// session check, with the cookie kept). Unanswered calls are dropped: the old server forgets
/// the device after 30 days and the session at its expiry.
#[cfg(feature = "mobile")]
async fn leave_current_server() {
    use crate::mobile::{http, inbox::Registered, notify};
    use std::time::Duration;
    if !http::has_session() {
        return;
    }
    let limit = Duration::from_secs(5);
    if let Some(registered) = Registered::from_json(notify::inbox_state().as_deref()) {
        let path = format!("/api/v1/me/notifications/app-devices/{}", registered.device);
        let _ = http::send_within("DELETE", &path, None, limit).await;
    }
    let _ = http::send_within(
        "POST",
        "/api/v1/auth/logout",
        Some(&serde_json::json!({})),
        limit,
    )
    .await;
}

/// Removes the old server's local state: the push registration (keys and distributor
/// token; the "receive instantly" switch stays), the inbox registration, the shown notices,
/// the widget data and `PER_SERVER_FILES`. Every step runs; false when any of them failed.
/// The cookie goes with `switch_server`; the in-memory dashboard state with `signed_out` (a
/// fresh `AppState`), the in-memory reminder schedule with the forgetting sign-out reason.
#[cfg(feature = "mobile")]
fn forget_local_server_data() -> bool {
    use crate::mobile::{notify, server::PER_SERVER_FILES, storage, widgets};
    let steps = [
        notify::push_stop(),
        notify::inbox_stop(),
        notify::forget_server(),
        widgets::sign_out(),
    ];
    let files = PER_SERVER_FILES.map(storage::remove);
    steps.into_iter().chain(files).all(|done| done)
}
