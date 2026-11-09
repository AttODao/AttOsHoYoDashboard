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
    let (generation, token, status_before) = {
        let mut state = state;
        let mut write = state.write();
        if write.busy {
            return;
        }
        let token = NEXT_ACTION.fetch_add(1, Ordering::Relaxed);
        write.busy = true;
        write.action = token;
        (
            write.generation,
            token,
            write.status.as_ref().map(|status| status.serial),
        )
    };
    // The busy shell disables the control that started the action, which drops its focus.
    let trigger = browser_focus::active();
    spawn_root(async move {
        action(generation).await;
        finish_action(state, token);
        let new_error =
            state.peek().status.as_ref().is_some_and(|status| {
                status.kind == "error" && Some(status.serial) != status_before
            });
        browser_focus::settle(trigger, new_error).await;
    });
}

/// Where focus goes once an action has finished and the shell is enabled again.
/// Only the browser build moves focus; the app and native tests use the decision in tests.
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
#[derive(Clone, Copy, PartialEq, Debug)]
enum FocusAfter {
    /// Focus is still on a control (the action or a dialog already placed it).
    Keep,
    /// The action failed: show its message, which may sit above the visible part of a long form.
    Status,
    /// Back to the control that started the action.
    Trigger,
}

#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
fn focus_after(focus_lost: bool, new_error: bool) -> FocusAfter {
    match (focus_lost, new_error) {
        (false, _) => FocusAfter::Keep,
        (true, true) => FocusAfter::Status,
        (true, false) => FocusAfter::Trigger,
    }
}

#[cfg(target_arch = "wasm32")]
mod browser_focus {
    use super::{FocusAfter, focus_after};
    use crate::app::state::STATUS_ID;
    use wasm_bindgen::JsCast;
    use web_sys::HtmlElement;

    pub(super) type Trigger = Option<HtmlElement>;

    pub(super) fn active() -> Trigger {
        web_sys::window()?
            .document()?
            .active_element()?
            .dyn_into()
            .ok()
    }

    /// Focus fell back to the page itself, e.g. because its control was disabled.
    fn lost() -> bool {
        let Some(document) = web_sys::window().and_then(|window| window.document()) else {
            return false;
        };
        match document.active_element() {
            None => true,
            Some(element) => document
                .body()
                .is_some_and(|body| body.is_same_node(Some(&element))),
        }
    }

    fn try_focus(element: Option<HtmlElement>) -> bool {
        element.is_some_and(|element| {
            element.is_connected()
                && !element.matches(":disabled").unwrap_or(true)
                && element.focus().is_ok()
                && !lost()
        })
    }

    pub(super) async fn settle(trigger: Trigger, new_error: bool) {
        // The shell re-enables on the next render; give it a few frames.
        for _ in 0..10 {
            crate::platform::sleep_ms(30).await;
            let enabled = trigger
                .as_ref()
                .is_none_or(|element| !element.matches(":disabled").unwrap_or(false));
            if enabled {
                break;
            }
        }
        match focus_after(lost(), new_error) {
            FocusAfter::Keep => {}
            FocusAfter::Status => {
                let status = web_sys::window()
                    .and_then(|window| window.document())
                    .and_then(|document| document.get_element_by_id(STATUS_ID))
                    .and_then(|element| element.dyn_into().ok());
                // A message outside an open modal cannot take focus; stay on the trigger then.
                if !try_focus(status) {
                    try_focus(trigger);
                }
            }
            FocusAfter::Trigger => {
                try_focus(trigger);
            }
        }
    }
}

// The app and native tests leave focus to the platform.
#[cfg(not(target_arch = "wasm32"))]
mod browser_focus {
    pub(super) struct Trigger;

    pub(super) fn active() -> Trigger {
        Trigger
    }

    pub(super) async fn settle(_trigger: Trigger, _new_error: bool) {}
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
    fn finished_actions_return_lost_focus_to_the_trigger_or_their_error() {
        assert_eq!(focus_after(false, true), FocusAfter::Keep);
        assert_eq!(focus_after(false, false), FocusAfter::Keep);
        assert_eq!(focus_after(true, true), FocusAfter::Status);
        assert_eq!(focus_after(true, false), FocusAfter::Trigger);
    }

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
