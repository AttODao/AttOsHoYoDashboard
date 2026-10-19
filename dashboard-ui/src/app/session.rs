use super::data::{load_hoyolab, load_page_data, load_server_settings};
use super::state::{
    AppState, Page, Phase, StatusLine, current_generation, current_ready_generation,
    generation_matches, set_status, signed_in_generation,
};
use crate::{api, models::MeView};
use dioxus::prelude::*;
use serde_json::json;

pub(super) async fn complete_login(mut state: Signal<AppState>, generation: u64) {
    if !generation_matches(state, generation) {
        return;
    }
    // Explicit authentication starts at Dashboard; ordinary bootstrap/reload
    // retains the authenticated user's requested settings page.
    state.write().page = Page::Dashboard;
    bootstrap(state).await;
}

pub(super) async fn bootstrap(mut state: Signal<AppState>) {
    // Public docs make no authenticated request; restore_page bootstraps on leaving.
    if state.read().page == Page::Docs {
        state.write().phase = Phase::Unchecked;
        return;
    }
    let generation = {
        let mut write = state.write();
        write.generation = write.generation.saturating_add(1);
        write.phase = Phase::Loading;
        write.busy = true;
        write.status = None;
        write.generation
    };
    match api::get::<MeView>("/me").await {
        Ok(me) if generation_matches(state, generation) => {
            {
                let mut write = state.write();
                let page = write.page;
                *write = AppState::new(page);
                write.phase = Phase::SignedIn;
                write.busy = true;
                write.generation = generation;
                write.me = Some(me);
            }
            load_gate_data(state, generation).await;
        }
        Ok(_) => {}
        Err(error) if generation_matches(state, generation) && error.is_unauthorized() => {
            signed_out(state, "messages.please_sign_in");
        }
        Err(error) if generation_matches(state, generation) => {
            signed_out(state, "messages.the_dashboard_could_not_load_your_session");
            set_status(state, error.message_key(), "error");
        }
        Err(_) => {}
    }
    if generation_matches(state, generation) {
        state.write().busy = false;
    }
}

// Load only what the current gate renders: onboarding shows the HoYoLAB linkage;
// the ready shell needs server settings plus the current page.
async fn load_gate_data(state: Signal<AppState>, generation: u64) {
    if !current_generation(state, generation) || state.read().page == Page::Docs {
        return;
    }
    if !state.read().ready() {
        load_hoyolab(state).await;
        return;
    }
    load_server_settings(state).await;
    if current_ready_generation(state, generation) {
        let page = state.read().page;
        load_page_data(state, page).await;
    }
}

/// Re-read /me after a gate-changing mutation and load what the resulting gate
/// renders, in place: no Loading screen, and the caller's status message stays.
pub(super) async fn refresh_gate(state: Signal<AppState>, generation: u64) {
    if refresh_me(state).await && current_generation(state, generation) {
        load_gate_data(state, generation).await;
    }
}

pub(super) async fn refresh_me(mut state: Signal<AppState>) -> bool {
    let Some(generation) = signed_in_generation(state) else {
        return false;
    };
    match api::get::<MeView>("/me").await {
        Ok(me) if current_generation(state, generation) => {
            let changed_owner = apply_fresh_me(&mut state.write(), me);
            if changed_owner {
                let generation = state.read().generation;
                load_gate_data(state, generation).await;
            }
            true
        }
        Ok(_) => false,
        Err(error) if current_generation(state, generation) && error.is_unauthorized() => {
            signed_out(state, "messages.please_sign_in_again");
            false
        }
        Err(error) if current_generation(state, generation) => {
            set_status(state, error.message_key(), "error");
            false
        }
        Err(_) => false,
    }
}

fn apply_fresh_me(write: &mut AppState, me: MeView) -> bool {
    let changed_owner = write.me.as_ref().is_some_and(|current| current.id != me.id);
    if changed_owner {
        write.generation = write.generation.saturating_add(1);
        write.clear_owner_data();
        write.page = Page::Dashboard;
        write.busy = false;
        write.status = None;
    }
    write.me = Some(me);
    changed_owner
}

pub(super) fn signed_out(mut state: Signal<AppState>, message_key: &'static str) {
    let page = state.read().page;
    let generation = state.read().generation.saturating_add(1);
    let mut next = AppState::new(if page == Page::Docs {
        Page::Docs
    } else {
        Page::Dashboard
    });
    next.phase = Phase::SignedOut;
    next.generation = generation;
    next.status = Some(StatusLine {
        message_key,
        kind: "info",
    });
    state.set(next);
}

pub(super) async fn logout(mut state: Signal<AppState>, generation: u64) {
    let result = api::send("POST", "/auth/logout", Some(json!({}))).await;
    if !generation_matches(state, generation) {
        return;
    }
    match result {
        Ok(()) => signed_out(state, "messages.signed_out"),
        Err(error) if error.is_unauthorized() => signed_out(state, "messages.signed_out"),
        Err(_) => {
            signed_out(state, "messages.logout_unconfirmed");
            state.write().logout_unconfirmed = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Capabilities, GameAccount, Onboarding, ServerSettings, UserView};
    use uuid::Uuid;

    fn uuid(n: u128) -> Uuid {
        Uuid::from_u128(n)
    }

    fn me(password_change_required: bool, cookie_setup_complete: bool) -> MeView {
        MeView {
            id: uuid(5),
            username: "admin".to_owned(),
            is_admin: true,
            active: true,
            must_change_password: password_change_required,
            onboarding: Onboarding {
                password_change_required,
                cookie_setup_complete,
                cookie_status: "pending".to_owned(),
                verification_available: false,
            },
            capabilities: Capabilities {
                reward_posting_available: false,
            },
        }
    }

    fn account(id: Uuid, visible: bool, detected: bool, uid: &str) -> GameAccount {
        GameAccount {
            id,
            hoyolab_account_id: uuid(100),
            game_kind: "genshin".to_owned(),
            game_uid: uid.to_owned(),
            region: "os_usa".to_owned(),
            visible,
            detected,
            last_detected_at: "2026-01-01T00:00:00Z".to_owned(),
        }
    }

    #[test]
    fn stale_login_cannot_change_route_and_sign_out_preserves_public_docs() {
        fn check() -> Element {
            let mut state = use_signal(|| AppState::new(Page::Security));
            state.write().generation = 2;
            let mut login = std::pin::pin!(complete_login(state, 1));
            let mut context = std::task::Context::from_waker(std::task::Waker::noop());
            assert!(login.as_mut().poll(&mut context).is_ready());
            assert!(state.read().page == Page::Security);
            assert_eq!(state.read().generation, 2);
            signed_out(state, "messages.signed_out");
            assert!(state.read().page == Page::Dashboard);
            assert!(state.read().phase == Phase::SignedOut);
            assert_eq!(state.read().generation, 3);
            state.write().page = Page::Docs;
            signed_out(state, "messages.signed_out");
            assert!(state.read().page == Page::Docs);
            rsx! {}
        }
        VirtualDom::new(check).rebuild_in_place();
    }

    #[test]
    fn gate_refresh_stays_in_place_and_ignores_stale_sessions() {
        fn check() -> Element {
            let mut state = use_signal(|| AppState::new(Page::Link));
            state.write().phase = Phase::SignedIn;
            state.write().generation = 3;
            state.write().me = Some(me(false, false));
            set_status(state, "hoyolab.cookie_save_accepted", "success");
            let mut context = std::task::Context::from_waker(std::task::Waker::noop());
            let mut stale = std::pin::pin!(refresh_gate(state, 2));
            assert!(stale.as_mut().poll(&mut context).is_ready());
            assert!(
                state.read().phase == Phase::SignedIn,
                "never enters Loading"
            );
            assert!(state.read().page == Page::Link);
            assert_eq!(state.read().generation, 3);
            assert!(state.read().me.is_some());
            rsx! {}
        }
        VirtualDom::new(check).rebuild_in_place();
    }

    #[test]
    fn owner_data_reset_clears_old_shell_state() {
        let mut state = AppState::new(Page::Dashboard);
        state.phase = Phase::SignedIn;
        state.generation = 42;
        state.page = Page::Security;
        state.me = Some(me(false, true));
        state.server = Some(ServerSettings {
            timezone_offset: "+08:00".to_owned(),
            session_ttl_days: 7,
        });
        state.accounts = Some(vec![account(uuid(1), true, true, "600000000")]);
        state.rewards_error = Some("old owner".to_owned());
        state.admin_users = Some(vec![UserView {
            id: uuid(2),
            username: "other".to_owned(),
            is_admin: false,
            active: true,
            must_change_password: false,
        }]);

        let mut next = me(false, true);
        next.id = uuid(6);
        assert!(apply_fresh_me(&mut state, next.clone()));
        assert_eq!(state.generation, 43);
        assert!(state.page == Page::Dashboard);
        assert_eq!(state.me.as_ref().unwrap().id, next.id);
        assert!(!apply_fresh_me(&mut state, next));
        assert_eq!(state.generation, 43);
        assert!(state.server.is_none());
        assert!(state.accounts.is_none());
        assert!(state.rewards_error.is_none());
        assert!(state.admin_users.is_none());
    }
}
