use super::session::{refresh_me, signed_out};
use super::state::{AppState, generation_matches, set_status};
use crate::api;
use dioxus::prelude::*;
use std::future::Future;
use std::sync::atomic::{AtomicU64, Ordering};

pub(super) fn spawn_root(future: impl Future<Output = ()> + 'static) {
    dioxus::core::spawn_forever(future);
}

pub(super) fn handle_api_error_for(state: Signal<AppState>, generation: u64, error: api::ApiError) {
    if !generation_matches(state, generation) {
        return;
    }
    if error.is_unauthorized() {
        signed_out(state, "messages.please_sign_in_again");
    } else {
        set_status(state, error.message_key(), "error");
    }
}

pub(super) async fn handle_password_error_for(
    state: Signal<AppState>,
    generation: u64,
    error: api::ApiError,
) {
    if !generation_matches(state, generation) {
        return;
    }
    if !error.is_unauthorized() {
        handle_api_error_for(state, generation, error);
        return;
    }
    // Password confirmation and session expiry both return 401. Check the
    // session without retrying the mutation or discarding a valid login.
    if refresh_me(state).await && generation_matches(state, generation) {
        set_status(
            state,
            "messages.the_current_password_was_not_accepted_please_try_again",
            "error",
        );
    }
}

// Reserve before scheduling so a second click cannot start a parallel mutation.
// The reservation token is separate from the session generation: starting an
// action must not discard in-flight page loads for the same session.
static NEXT_ACTION: AtomicU64 = AtomicU64::new(1);

pub(super) fn spawn_action<F: Future<Output = ()> + 'static>(
    state: Signal<AppState>,
    action: impl FnOnce(u64) -> F + 'static,
) {
    let (generation, token) = {
        let mut state = state;
        let mut write = state.write();
        if write.busy {
            return;
        }
        let token = NEXT_ACTION.fetch_add(1, Ordering::Relaxed);
        write.busy = true;
        write.action = token;
        (write.generation, token)
    };
    spawn_root(async move {
        action(generation).await;
        finish_action(state, token);
    });
}

// Tokens are process-unique, so a stale action cannot release a later reservation
// even after sign-out replaced the whole state.
fn finish_action(mut state: Signal<AppState>, token: u64) {
    if state.read().action == token {
        state.write().busy = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::state::Page;

    #[test]
    fn action_reservation_keeps_the_session_generation_and_rejects_stale_tokens() {
        fn check() -> Element {
            let mut state = use_signal(|| AppState::new(Page::Dashboard));
            state.write().generation = 4;
            spawn_action(state, |_| async {});
            assert_eq!(
                state.read().generation,
                4,
                "actions do not invalidate page loads"
            );
            assert!(state.read().busy);
            let token = state.read().action;

            spawn_action(state, |_| async {});
            assert_eq!(
                state.read().action,
                token,
                "busy rejects a second reservation"
            );
            finish_action(state, token - 1);
            assert!(state.read().busy);
            handle_api_error_for(state, 3, api::ApiError::browser("stale"));
            let mut password_error = std::pin::pin!(handle_password_error_for(
                state,
                3,
                api::ApiError::browser("stale password confirmation"),
            ));
            let mut context = std::task::Context::from_waker(std::task::Waker::noop());
            assert!(password_error.as_mut().poll(&mut context).is_ready());
            assert!(state.read().status.is_none());

            finish_action(state, token);
            assert!(!state.read().busy);
            spawn_action(state, |_| async {});
            let next = state.read().action;
            assert_ne!(next, token);
            finish_action(state, token);
            assert!(
                state.read().busy,
                "an older action cannot release a newer one"
            );
            rsx! {}
        }

        VirtualDom::new(check).rebuild_in_place();
    }
}
