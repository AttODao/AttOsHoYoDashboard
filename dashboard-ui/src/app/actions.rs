use super::session::signed_out;
use super::state::{AppState, generation_matches, set_status};
use crate::api;
use dioxus::prelude::*;
use std::future::Future;

pub(super) fn spawn_root(future: impl Future<Output = ()> + 'static) {
    dioxus::core::spawn_forever(future);
}

pub(super) fn handle_api_error_for(state: Signal<AppState>, generation: u64, error: api::ApiError) {
    if !generation_matches(state, generation) {
        return;
    }
    if error.is_unauthorized() {
        signed_out(state, "Please sign in again.");
    } else {
        set_status(state, error.to_string(), "error");
    }
}

// Reserve before scheduling: epochs cannot undo a browser's already-received Set-Cookie.
pub(super) fn spawn_action<F: Future<Output = ()> + 'static>(
    state: Signal<AppState>,
    action: impl FnOnce(u64) -> F + 'static,
) {
    let generation = {
        let mut state = state;
        let mut write = state.write();
        if write.busy {
            return;
        }
        write.busy = true;
        write.generation = write.generation.saturating_add(1);
        write.generation
    };
    spawn_root(async move {
        action(generation).await;
        finish_action(state, generation);
    });
}

fn finish_action(mut state: Signal<AppState>, generation: u64) {
    if generation_matches(state, generation) {
        state.write().busy = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::state::Page;

    #[test]
    fn action_reservation_and_completion_reject_stale_generations() {
        fn check() -> Element {
            let mut state = use_signal(|| AppState::new(Page::Dashboard));
            spawn_action(state, |_| async {});
            let generation = state.read().generation;
            assert_eq!(generation, 1);
            assert!(state.read().busy);

            spawn_action(state, |_| async {});
            assert_eq!(state.read().generation, generation);
            finish_action(state, generation - 1);
            assert!(state.read().busy);
            handle_api_error_for(state, generation - 1, api::ApiError::browser("stale"));
            assert!(state.read().status.is_none());

            finish_action(state, generation);
            assert!(!state.read().busy);
            state.write().generation += 1;
            state.write().busy = true;
            finish_action(state, generation);
            assert!(state.read().busy);
            rsx! {}
        }

        VirtualDom::new(check).rebuild_in_place();
    }
}
