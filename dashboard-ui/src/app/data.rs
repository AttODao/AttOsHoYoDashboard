use super::session::signed_out;
use super::state::{
    AppState, Page, Phase, current_generation, current_ready_generation, current_ready_page,
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
    if !current_ready_page(state, Page::Dashboard) {
        return;
    }
    if include_accounts {
        load_accounts(state).await;
    }
    if !current_ready_page(state, Page::Dashboard) {
        return;
    }
    load_selected_status(state).await;
    if !current_ready_page(state, Page::Dashboard) {
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
            write
                .account_statuses
                .retain(|entry| Some(entry.id) == selected);
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

pub(super) async fn load_selected_status(mut state: Signal<AppState>) {
    let (generation, selected) = {
        let read = state.read();
        if read.phase != Phase::SignedIn || !read.ready() || read.page != Page::Dashboard {
            return;
        }
        (read.generation, read.selected_account)
    };
    let Some(id) = selected else {
        return;
    };
    if !state.read().accounts.iter().any(|account| {
        account.id == id && account.visible && account.detected && !account.game_uid.is_empty()
    }) {
        return;
    }
    match api::get::<GameStatus>(&format!("/game-accounts/{id}/status")).await {
        Ok(status) if current_ready_page_generation(state, generation, Page::Dashboard) => {
            upsert_status(state, id, status);
        }
        Ok(_) => {}
        Err(error)
            if current_ready_page_generation(state, generation, Page::Dashboard)
                && error.is_unauthorized() =>
        {
            signed_out(state, "Please sign in again.");
        }
        Err(error) if current_ready_page_generation(state, generation, Page::Dashboard) => {
            state
                .write()
                .account_statuses
                .retain(|entry| entry.id != id);
            set_status(state, error.to_string(), "error");
        }
        Err(_) => {}
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
