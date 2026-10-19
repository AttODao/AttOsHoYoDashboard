use super::actions::{handle_api_error_for, spawn_root};
use super::session::signed_out;
use super::state::{
    AppState, DASHBOARD_GAMES, Page, Phase, RewardPoll, current_generation,
    current_ready_generation, current_ready_page_generation, password_generation, ready_generation,
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
use serde::de::DeserializeOwned;
use serde_json::json;
use std::sync::atomic::{AtomicU64, Ordering};
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

// Session-scoped GET: a reply for an older session (or a lost gate) is dropped
// and a 401 signs out; otherwise the caller gets the value or a message key.
async fn fetch_current<T: DeserializeOwned>(
    state: Signal<AppState>,
    path: &str,
    generation: u64,
    current: fn(Signal<AppState>, u64) -> bool,
) -> Option<Result<T, &'static str>> {
    let result = api::get::<T>(path).await;
    if !current(state, generation) {
        return None;
    }
    match result {
        Ok(value) => Some(Ok(value)),
        Err(error) if error.is_unauthorized() => {
            signed_out(state, "messages.please_sign_in_again");
            None
        }
        Err(error) => Some(Err(error.message_key())),
    }
}

pub(super) async fn load_hoyolab(mut state: Signal<AppState>) {
    let Some(generation) = signed_in_generation(state) else {
        return;
    };
    let Some(result) =
        fetch_current::<CookieStatus>(state, "/me/hoyolab", generation, current_generation).await
    else {
        return;
    };
    let mut write = state.write();
    match result {
        Ok(status) => {
            write.hoyolab = Some(status);
            write.hoyolab_error = None;
        }
        Err(key) => {
            write.hoyolab = None;
            write.hoyolab_error = Some(key.to_owned());
        }
    }
}

pub(super) async fn load_accounts(mut state: Signal<AppState>) {
    let Some(generation) = ready_generation(state) else {
        return;
    };
    let Some(result) = fetch_current::<Vec<GameAccount>>(
        state,
        "/game-accounts",
        generation,
        current_ready_generation,
    )
    .await
    else {
        return;
    };
    let mut write = state.write();
    match result {
        Ok(accounts) => {
            write.accounts = Some(accounts);
            write.accounts_error = None;
            write.reconcile_dashboard_accounts();
        }
        // Keep the last list; the page shows the refresh failure beside it.
        Err(key) => write.accounts_error = Some(key.to_owned()),
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
            let matches_account = state.read().account_rows().iter().any(|account| {
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
                    .error = Some("dashboard.status_identity_mismatch");
            }
        }
        Err(error) if error.is_unauthorized() => signed_out(state, "messages.please_sign_in_again"),
        Err(_) => {
            // Retain the last payload, but show this refresh failure separately.
            state
                .write()
                .selected_accounts
                .entry(game)
                .or_default()
                .error = Some("dashboard.refresh_failed");
        }
    }
}

// Shared with the settings Save action: preserve its PATCH contract and generation boundary.
// Call inside spawn_action; the caller supplies an acknowledgement message key.
pub(super) async fn save_reward_setting(
    state: Signal<AppState>,
    generation: u64,
    id: Uuid,
    enabled: bool,
    local_time: String,
    saved_message_key: &'static str,
) {
    if !current_ready_generation(state, generation) {
        return;
    }
    let body = json!({"enabled": enabled, "local_time": if local_time.is_empty() { "06:00".to_owned() } else { local_time }});
    match api::send("PATCH", &format!("/reward-settings/{id}"), Some(body)).await {
        Ok(()) if current_ready_generation(state, generation) => {
            set_status(state, saved_message_key, "success");
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
    let Some(result) = fetch_current::<Vec<RewardSetting>>(
        state,
        "/reward-settings",
        generation,
        current_ready_generation,
    )
    .await
    else {
        return;
    };
    {
        let mut write = state.write();
        match result {
            Ok(settings) => {
                write.rewards = settings;
                write.rewards_error = None;
            }
            Err(key) => {
                write.rewards.clear();
                write.rewards_error = Some(key.to_owned());
            }
        }
    }
    let Some(result) = fetch_current::<Vec<RewardRun>>(
        state,
        "/reward-runs?limit=100",
        generation,
        current_ready_generation,
    )
    .await
    else {
        return;
    };
    let mut write = state.write();
    match result {
        Ok(runs) => {
            write.reward_runs = Some(runs);
            write.reward_runs_error = None;
        }
        Err(key) => {
            write.reward_runs = None;
            write.reward_runs_error = Some(key.to_owned());
        }
    }
}

const REWARD_POLL_INTERVAL_MS: u32 = 3_000;
const REWARD_POLL_ATTEMPTS: u32 = 20;
static NEXT_REWARD_POLL: AtomicU64 = AtomicU64::new(1);

/// States in which a run may still change without user action.
pub(super) fn reward_run_in_flight(state: &str) -> bool {
    matches!(state, "pending" | "queued" | "running" | "retry_wait")
}

/// Completion announcement (message key, status kind) for a settled run state.
fn reward_run_finished(state: &str) -> (&'static str, &'static str) {
    match state {
        "success" => ("rewards.run_finished_success", "success"),
        "already_claimed" => ("rewards.run_finished_already_claimed", "success"),
        "failed" | "stopped" => ("rewards.run_finished_failed", "error"),
        _ => ("rewards.run_finished_unconfirmed", "error"),
    }
}

/// Follow an accepted manual run for about a minute. Each poll is an ordinary
/// GET refresh: it never re-sends the POST, skips while an action holds the UI,
/// and stops on completion, page leave, session change or a newer poll.
pub(super) fn start_reward_poll(mut state: Signal<AppState>, generation: u64, setting: Uuid) {
    if !current_ready_page_generation(state, generation, Page::Rewards) {
        return;
    }
    let token = NEXT_REWARD_POLL.fetch_add(1, Ordering::Relaxed);
    state.write().reward_poll = Some(RewardPoll { token, setting });
    spawn_root(async move {
        poll_reward_run(state, generation, token, setting).await;
        if state
            .read()
            .reward_poll
            .is_some_and(|poll| poll.token == token)
        {
            state.write().reward_poll = None;
        }
    });
}

async fn poll_reward_run(state: Signal<AppState>, generation: u64, token: u64, setting: Uuid) {
    let following = move || {
        current_ready_page_generation(state, generation, Page::Rewards)
            && state
                .read()
                .reward_poll
                .is_some_and(|poll| poll.token == token)
    };
    let run_state = move || {
        let read = state.read();
        let row = read.rewards.iter().find(|row| row.id == setting)?;
        row.last_run.as_ref().map(|run| run.state.clone())
    };
    for _ in 0..REWARD_POLL_ATTEMPTS {
        if run_state().is_some_and(|run| !reward_run_in_flight(&run)) {
            break;
        }
        gloo_timers::future::TimeoutFuture::new(REWARD_POLL_INTERVAL_MS).await;
        if !following() {
            return;
        }
        if !state.read().busy {
            load_rewards(state).await;
        }
        if !following() {
            return;
        }
    }
    match run_state() {
        Some(run) if !reward_run_in_flight(&run) => {
            let (key, kind) = reward_run_finished(&run);
            set_status(state, key, kind);
        }
        _ => set_status(state, "rewards.run_still_pending", "info"),
    }
}

pub(super) async fn load_passkeys(mut state: Signal<AppState>) {
    let Some(generation) = password_generation(state) else {
        return;
    };
    let Some(result) =
        fetch_current::<Vec<PasskeyView>>(state, "/me/passkeys", generation, current_generation)
            .await
    else {
        return;
    };
    let mut write = state.write();
    match result {
        Ok(passkeys) => {
            write.passkeys = Some(passkeys);
            write.passkeys_error = None;
        }
        Err(key) => {
            write.passkeys = None;
            write.passkeys_error = Some(key.to_owned());
        }
    }
}

pub(super) async fn load_server_settings(mut state: Signal<AppState>) {
    let Some(generation) = password_generation(state) else {
        return;
    };
    // Optional context: a failure leaves the "not loaded" placeholders.
    if let Some(Ok(settings)) =
        fetch_current::<ServerSettings>(state, "/server-settings", generation, current_generation)
            .await
    {
        state.write().server = Some(settings);
    }
}

pub(super) async fn load_admin_users(mut state: Signal<AppState>) {
    let Some(generation) = password_generation(state) else {
        return;
    };
    if !state.read().me.as_ref().is_some_and(|me| me.is_admin) {
        return;
    }
    let Some(result) =
        fetch_current::<Vec<UserView>>(state, "/admin/users", generation, current_generation).await
    else {
        return;
    };
    let mut write = state.write();
    match result {
        Ok(users) => {
            write.admin_users = Some(users);
            write.admin_error = None;
        }
        Err(key) => {
            write.admin_users = None;
            write.admin_error = Some(key.to_owned());
        }
    }
}

pub(super) async fn load_docs(mut state: Signal<AppState>) {
    match api::openapi().await {
        Ok(value) => {
            state.write().openapi = Some(value);
            state.write().openapi_error = None;
        }
        Err(error) => state.write().openapi_error = Some(error.message_key().to_owned()),
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
            state.accounts.get_or_insert_default().push(GameAccount {
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
        state.accounts.as_mut().unwrap()[0].visible = false;
        assert!(!current_status_request(&state, 7, "genshin", id, request));
        state.accounts.as_mut().unwrap()[0].visible = true;
        state.accounts.as_mut().unwrap()[0].detected = false;
        assert!(!current_status_request(&state, 7, "genshin", id, request));
        state.accounts.as_mut().unwrap()[0].detected = true;
        state.accounts.as_mut().unwrap()[0].game_uid.clear();
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
    fn reward_polling_stops_on_settled_states_and_announces_the_outcome() {
        for state in ["pending", "queued", "running", "retry_wait"] {
            assert!(reward_run_in_flight(state), "{state}");
        }
        for state in [
            "success",
            "already_claimed",
            "failed",
            "stopped",
            "unknown",
            "action_required",
        ] {
            assert!(!reward_run_in_flight(state), "{state}");
        }
        assert_eq!(
            reward_run_finished("success"),
            ("rewards.run_finished_success", "success")
        );
        assert_eq!(reward_run_finished("stopped").1, "error");
        assert_eq!(
            reward_run_finished("unknown"),
            ("rewards.run_finished_unconfirmed", "error")
        );
    }

    #[test]
    fn reward_polling_starts_only_on_the_current_rewards_page() {
        fn check() -> Element {
            let state = use_signal(ready_state);
            start_reward_poll(state, 7, Uuid::from_u128(20));
            assert!(
                state.read().reward_poll.is_none(),
                "dashboard is not the rewards page"
            );
            let mut state = state;
            state.write().page = Page::Rewards;
            start_reward_poll(state, 6, Uuid::from_u128(20));
            assert!(state.read().reward_poll.is_none(), "stale session");
            rsx! {}
        }
        VirtualDom::new(check).rebuild_in_place();
    }

    #[test]
    fn unavailable_game_selection_does_not_begin_a_status_request() {
        use std::future::Future;
        fn check() -> Element {
            let state = use_signal(|| {
                let mut state = ready_state();
                for account in state.accounts.iter_mut().flatten() {
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
