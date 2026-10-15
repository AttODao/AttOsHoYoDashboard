use super::actions::handle_api_error_for;
use super::session::signed_out;
use super::state::{
    AppState, DASHBOARD_GAMES, Page, Phase, current_generation, current_ready_generation,
    current_ready_page_generation, password_generation, ready_generation, select_account,
    set_status, signed_in_generation, upsert_status,
};
use crate::{
    api,
    models::{
        CookieStatus, GameAccount, GameStatus, PasskeyView, RewardRun, RewardSetting,
        ServerSettings, UserView,
    },
};
use dioxus::prelude::*;
use serde_json::json;
use uuid::Uuid;

pub(super) async fn load_page_data(state: Signal<AppState>, page: Page) {
    if state.read().phase != Phase::SignedIn {
        return;
    }
    if matches!(page, Page::Dashboard | Page::Uids | Page::Rewards) && !state.read().ready() {
        return;
    }
    match page {
        Page::Dashboard => load_dashboard_data(state, true).await,
        Page::Link => load_hoyolab(state).await,
        Page::Uids => load_accounts(state).await,
        Page::Rewards => load_rewards(state).await,
        Page::Security => load_passkeys(state).await,
        Page::Admin => load_admin_users(state).await,
        Page::Delete => {}
        Page::Docs => {}
    }
}

pub(super) async fn load_dashboard_data(state: Signal<AppState>, include_accounts: bool) {
    let Some(generation) = ready_generation(state) else {
        return;
    };
    if !current_ready_page_generation(state, generation, Page::Dashboard) {
        return;
    }
    if include_accounts {
        load_accounts(state).await;
    }
    if !current_ready_page_generation(state, generation, Page::Dashboard) {
        return;
    }
    load_selected_status(state).await;
    if !current_ready_page_generation(state, generation, Page::Dashboard) {
        return;
    }
    load_rewards(state).await;
}

pub(super) async fn load_hoyolab(mut state: Signal<AppState>) {
    let Some(generation) = signed_in_generation(state) else {
        return;
    };
    match api::get::<CookieStatus>("/me/hoyolab").await {
        Ok(status) if current_generation(state, generation) => {
            state.write().hoyolab = Some(status);
            state.write().hoyolab_error = None;
        }
        Ok(_) => {}
        Err(error) if current_generation(state, generation) && error.is_unauthorized() => {
            signed_out(state, "Please sign in again.");
        }
        Err(error) if current_generation(state, generation) => {
            state.write().hoyolab = None;
            state.write().hoyolab_error = Some(error.to_string());
        }
        Err(_) => {}
    }
}

pub(super) async fn load_accounts(mut state: Signal<AppState>) {
    let Some(generation) = ready_generation(state) else {
        return;
    };
    match api::get::<Vec<GameAccount>>("/game-accounts").await {
        Ok(accounts) if current_ready_generation(state, generation) => {
            let selected = select_account(state.read().selected_account, &accounts);
            let mut write = state.write();
            write.accounts = accounts;
            write.selected_account = selected;
            write.accounts_error = None;
            write.reconcile_dashboard_accounts();
        }
        Ok(_) => {}
        Err(error) if current_ready_generation(state, generation) && error.is_unauthorized() => {
            signed_out(state, "Please sign in again.");
        }
        Err(error) if current_ready_generation(state, generation) => {
            state.write().accounts_error = Some(error.to_string());
        }
        Err(_) => {}
    }
}

// Keep the polling entry point stable; each game has its own selection and request guard.
pub(super) async fn load_selected_status(state: Signal<AppState>) {
    let Some(generation) = ready_generation(state) else {
        return;
    };
    for game in DASHBOARD_GAMES {
        if !current_ready_page_generation(state, generation, Page::Dashboard) {
            return;
        }
        load_game_status(state, game).await;
    }
}

fn current_status_request(
    read: &AppState,
    generation: u64,
    game: &str,
    id: Uuid,
    request: u64,
) -> bool {
    read.generation == generation
        && read.phase == Phase::SignedIn
        && read.ready()
        && read.page == Page::Dashboard
        && read.selected_game_account(game) == Some(id)
        && read.selected_accounts.get(game).is_some_and(|selection| {
            selection.id == Some(id)
                && selection.request == request
                && selection.generation == generation
        })
}

pub(super) async fn load_game_status(mut state: Signal<AppState>, game: &'static str) {
    let (generation, id, request) = {
        let mut write = state.write();
        if write.phase != Phase::SignedIn
            || !write.ready()
            || write.page != Page::Dashboard
            || !DASHBOARD_GAMES.contains(&game)
        {
            return;
        }
        let Some(id) = write.selected_game_account(game) else {
            return;
        };
        write.select_game_account(game, Some(id));
        let generation = write.generation;
        let selection = write.selected_accounts.entry(game).or_default();
        selection.request = selection.request.saturating_add(1);
        selection.generation = generation;
        selection.loading = true;
        selection.error = None;
        (generation, id, selection.request)
    };
    let result = api::get::<GameStatus>(&format!("/game-accounts/{id}/status")).await;
    if !current_status_request(&state.read(), generation, game, id, request) {
        return;
    }
    state
        .write()
        .selected_accounts
        .entry(game)
        .or_default()
        .loading = false;
    match result {
        Ok(status) => {
            let matches_account = state.read().accounts.iter().any(|account| {
                account.id == id
                    && status.game_kind == account.game_kind
                    && status.game_uid == account.game_uid
                    && status.region == account.region
            });
            if matches_account {
                upsert_status(state, id, status);
            } else {
                state
                    .write()
                    .selected_accounts
                    .entry(game)
                    .or_default()
                    .error = Some("identity_mismatch".to_owned());
            }
        }
        Err(error) if error.is_unauthorized() => signed_out(state, "Please sign in again."),
        Err(_) => {
            // Retain the last payload, but show this refresh failure separately.
            state
                .write()
                .selected_accounts
                .entry(game)
                .or_default()
                .error = Some("refresh_failed".to_owned());
        }
    }
}

// Shared with the settings Save action: preserve its PATCH contract and generation boundary.
// Call inside spawn_action; the caller supplies localized acknowledgement copy.
pub(super) async fn save_reward_setting(
    state: Signal<AppState>,
    generation: u64,
    id: Uuid,
    enabled: bool,
    local_time: String,
    saved_message: &'static str,
) {
    if !current_ready_generation(state, generation) {
        return;
    }
    let body = json!({"enabled": enabled, "local_time": if local_time.is_empty() { "06:00".to_owned() } else { local_time }});
    match api::send("PATCH", &format!("/reward-settings/{id}"), Some(body)).await {
        Ok(()) if current_ready_generation(state, generation) => {
            set_status(state, saved_message, "success");
            load_rewards(state).await;
        }
        Ok(()) => {}
        Err(error) => handle_api_error_for(state, generation, error),
    }
}

pub(super) async fn load_rewards(mut state: Signal<AppState>) {
    let Some(generation) = ready_generation(state) else {
        return;
    };
    match api::get::<Vec<RewardSetting>>("/reward-settings").await {
        Ok(settings) if current_ready_generation(state, generation) => {
            state.write().rewards = settings;
            state.write().rewards_error = None;
        }
        Ok(_) => {}
        Err(error) if current_ready_generation(state, generation) && error.is_unauthorized() => {
            signed_out(state, "Please sign in again.");
        }
        Err(error) if current_ready_generation(state, generation) => {
            state.write().rewards.clear();
            state.write().rewards_error = Some(error.to_string());
        }
        Err(_) => {}
    }
    if !current_ready_generation(state, generation) {
        return;
    }
    match api::get::<Vec<RewardRun>>("/reward-runs?limit=100").await {
        Ok(runs) if current_ready_generation(state, generation) => {
            state.write().reward_runs = runs;
            state.write().reward_runs_error = None;
        }
        Ok(_) => {}
        Err(error) if current_ready_generation(state, generation) && error.is_unauthorized() => {
            signed_out(state, "Please sign in again.");
        }
        Err(error) if current_ready_generation(state, generation) => {
            state.write().reward_runs.clear();
            state.write().reward_runs_error = Some(error.to_string());
        }
        Err(_) => {}
    }
}

pub(super) async fn load_passkeys(mut state: Signal<AppState>) {
    let Some(generation) = password_generation(state) else {
        return;
    };
    match api::get::<Vec<PasskeyView>>("/me/passkeys").await {
        Ok(passkeys) if current_generation(state, generation) => {
            state.write().passkeys = passkeys;
            state.write().passkeys_error = None;
        }
        Ok(_) => {}
        Err(error) if current_generation(state, generation) && error.is_unauthorized() => {
            signed_out(state, "Please sign in again.");
        }
        Err(error) if current_generation(state, generation) => {
            state.write().passkeys.clear();
            state.write().passkeys_error = Some(error.to_string());
        }
        Err(_) => {}
    }
}

pub(super) async fn load_server_settings(mut state: Signal<AppState>) {
    let Some(generation) = password_generation(state) else {
        return;
    };
    match api::get::<ServerSettings>("/server-settings").await {
        Ok(settings) if current_generation(state, generation) => {
            state.write().server = Some(settings)
        }
        Ok(_) => {}
        Err(error) if current_generation(state, generation) && error.is_unauthorized() => {
            signed_out(state, "Please sign in again.");
        }
        Err(_) => {}
    }
}

pub(super) async fn load_admin_users(mut state: Signal<AppState>) {
    let Some(generation) = password_generation(state) else {
        return;
    };
    if !state.read().me.as_ref().is_some_and(|me| me.is_admin) {
        return;
    }
    match api::get::<Vec<UserView>>("/admin/users").await {
        Ok(users) if current_generation(state, generation) => {
            state.write().admin_users = users;
            state.write().admin_error = None;
        }
        Ok(_) => {}
        Err(error) if current_generation(state, generation) && error.is_unauthorized() => {
            signed_out(state, "Please sign in again.");
        }
        Err(error) if current_generation(state, generation) => {
            state.write().admin_users.clear();
            state.write().admin_error = Some(error.to_string());
        }
        Err(_) => {}
    }
}

pub(super) async fn load_docs(mut state: Signal<AppState>) {
    match api::openapi().await {
        Ok(value) => {
            state.write().openapi = Some(value);
            state.write().openapi_error = None;
        }
        Err(error) => state.write().openapi_error = Some(error.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Capabilities, MeView, Onboarding};

    fn ready_state() -> AppState {
        let mut state = AppState::new(Page::Dashboard);
        state.phase = Phase::SignedIn;
        state.generation = 7;
        state.me = Some(MeView {
            id: Uuid::from_u128(100),
            username: "synthetic".to_owned(),
            is_admin: false,
            active: true,
            must_change_password: false,
            onboarding: Onboarding {
                password_change_required: false,
                cookie_setup_complete: true,
                cookie_status: "verified".to_owned(),
                verification_available: false,
            },
            capabilities: Capabilities {
                reward_posting_available: false,
            },
        });
        for (n, game) in [(1, "genshin"), (2, "starrail"), (3, "genshin")] {
            state.accounts.push(GameAccount {
                id: Uuid::from_u128(n),
                hoyolab_account_id: Uuid::from_u128(101),
                game_kind: game.to_owned(),
                game_uid: format!("synthetic-{n}"),
                region: "synthetic".to_owned(),
                visible: true,
                detected: true,
                last_detected_at: "2026-01-01T00:00:00Z".to_owned(),
            });
        }
        state.reconcile_dashboard_accounts();
        let selection = state.selected_accounts.get_mut("genshin").unwrap();
        selection.request = 10;
        selection.generation = 7;
        state
    }

    #[test]
    fn status_responses_require_current_session_page_selection_and_request() {
        let mut state = ready_state();
        let id = Uuid::from_u128(1);
        assert!(current_status_request(&state, 7, "genshin", id, 10));
        assert!(!current_status_request(&state, 6, "genshin", id, 10));
        assert!(!current_status_request(&state, 7, "genshin", id, 9));
        assert!(!current_status_request(&state, 7, "starrail", id, 10));
        state.select_game_account("genshin", Some(Uuid::from_u128(3)));
        assert!(!current_status_request(&state, 7, "genshin", id, 10));
        state.select_game_account("genshin", Some(id));
        assert!(!current_status_request(&state, 7, "genshin", id, 10));
        let request = state.selected_accounts["genshin"].request;
        assert!(current_status_request(&state, 7, "genshin", id, request));
        state.accounts[0].visible = false;
        assert!(!current_status_request(&state, 7, "genshin", id, request));
        state.accounts[0].visible = true;
        state.accounts[0].detected = false;
        assert!(!current_status_request(&state, 7, "genshin", id, request));
        state.accounts[0].detected = true;
        state.accounts[0].game_uid.clear();
        assert!(!current_status_request(&state, 7, "genshin", id, request));
        state = ready_state();
        state.page = Page::Rewards;
        assert!(!current_status_request(&state, 7, "genshin", id, 10));
        state.page = Page::Dashboard;
        state.me.as_mut().unwrap().onboarding.cookie_setup_complete = false;
        assert!(!current_status_request(&state, 7, "genshin", id, 10));
        state = ready_state();
        state.phase = Phase::SignedOut;
        assert!(!current_status_request(&state, 7, "genshin", id, 10));
    }

    #[test]
    fn unavailable_game_selection_does_not_begin_a_status_request() {
        use std::future::Future;
        fn check() -> Element {
            let state = use_signal(|| {
                let mut state = ready_state();
                for account in &mut state.accounts {
                    account.visible = false;
                }
                state.reconcile_dashboard_accounts();
                state
            });
            let before = state.read().selected_accounts.clone();
            let mut future = std::pin::pin!(load_game_status(state, "genshin"));
            let mut context = std::task::Context::from_waker(std::task::Waker::noop());
            assert!(future.as_mut().poll(&mut context).is_ready());
            assert!(state.read().selected_accounts == before);
            assert!(state.read().account_statuses.is_empty());
            rsx! {}
        }
        VirtualDom::new(check).rebuild_in_place();
    }
}
