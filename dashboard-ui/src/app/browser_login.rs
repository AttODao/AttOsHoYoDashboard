//! Browser-mediated app authentication; the verifier stays in app-private storage.
use super::state::{AppState, set_status};
use dioxus::prelude::*;

pub(super) fn start(state: Signal<AppState>) {
    #[cfg(feature = "mobile")]
    {
        use crate::mobile::{
            browser_login::{PENDING_FILE, Pending, challenge, start_url, verifier_from},
            storage,
        };
        let mut bytes = [0; 32];
        if getrandom::fill(&mut bytes).is_err() {
            set_status(state, "messages.could_not_open_the_browser", "error");
            return;
        }
        let verifier = verifier_from(&bytes);
        let pending = Pending {
            verifier: verifier.clone(),
            created_ms: crate::platform::now_ms() as i64,
        };
        if storage::write(PENDING_FILE, &pending.to_json()).is_err() {
            set_status(state, "messages.could_not_open_the_browser", "error");
            return;
        }
        let Some(origin) = crate::mobile::http::server_origin() else {
            set_status(state, "messages.could_not_open_the_browser", "error");
            return;
        };
        if crate::platform::open_external(&start_url(origin, &challenge(&verifier))).is_err() {
            storage::remove(PENDING_FILE);
            set_status(state, "messages.could_not_open_the_browser", "error");
        } else {
            set_status(state, "messages.continue_in_browser", "info");
        }
    }
    #[cfg(not(feature = "mobile"))]
    set_status(state, "messages.could_not_open_the_browser", "error");
}

#[cfg(feature = "mobile")]
pub(super) fn use_browser_login(state: Signal<AppState>) {
    use super::{
        actions::{handle_api_error_for, spawn_action},
        session::complete_login,
        state::{Phase, generation_matches},
    };
    use crate::mobile::{
        browser_login::{PENDING_FILE, Pending, callback_code},
        login_intent, storage,
    };
    use serde_json::json;

    // A cold start from the callback intent has it waiting before the first render.
    let mut received = use_signal(login_intent::take_callback);
    login_intent::use_callback_events(move || {
        if let Some(uri) = login_intent::take_callback() {
            received.set(Some(uri));
        }
    });
    use_effect(move || {
        let snapshot = state.read();
        let uri = received.read().clone();
        if uri.is_none()
            || snapshot.busy
            || !matches!(snapshot.phase, Phase::SignedOut | Phase::SignedIn)
        {
            return;
        }
        let signed_in = snapshot.phase == Phase::SignedIn;
        drop(snapshot);
        let Some(uri) = uri else {
            return;
        };
        received.set(None);
        // The handoff only signs a signed-out app in; never swap an existing session's account.
        if signed_in {
            storage::remove(PENDING_FILE);
            return;
        }
        let Some(code) = callback_code(&uri) else {
            set_status(state, "messages.app_login_expired", "error");
            return;
        };
        let pending = Pending::from_json(storage::read(PENDING_FILE).as_deref());
        storage::remove(PENDING_FILE);
        let Some(pending) =
            pending.filter(|pending| pending.usable(crate::platform::now_ms() as i64))
        else {
            set_status(state, "messages.app_login_expired", "error");
            return;
        };
        let verifier = pending.verifier;
        spawn_action(state, move |generation| async move {
            match crate::api::send(
                "POST",
                "/auth/app-login/exchange",
                Some(json!({"code": code, "code_verifier": verifier})),
            )
            .await
            {
                Ok(()) if generation_matches(state, generation) => {
                    complete_login(state, generation).await
                }
                Ok(()) => {}
                Err(error) if error.is_unauthorized() && generation_matches(state, generation) => {
                    set_status(state, "messages.app_login_expired", "error")
                }
                Err(error) => handle_api_error_for(state, generation, error),
            }
        });
    });
}
