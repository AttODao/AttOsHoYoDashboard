use crate::{api, models::*};
use dioxus::prelude::*;
use serde_json::json;
use std::future::Future;
use uuid::Uuid;

#[path = "pages/mod.rs"]
mod pages;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Page {
    Dashboard,
    Link,
    Uids,
    Rewards,
    Security,
    Admin,
    Delete,
    Docs,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Phase {
    Loading,
    SignedOut,
    SignedIn,
}

#[derive(Clone, PartialEq)]
pub(crate) struct StatusLine {
    pub text: String,
    pub kind: &'static str,
}

#[derive(Clone, PartialEq)]
pub(crate) struct AccountStatusEntry {
    pub id: Uuid,
    pub status: GameStatus,
}

#[derive(Clone, PartialEq)]
pub(crate) struct AppState {
    pub phase: Phase,
    pub generation: u64,
    pub page: Page,
    pub me: Option<MeView>,
    pub status: Option<StatusLine>,
    pub busy: bool,
    pub logout_unconfirmed: bool,
    pub hoyolab: Option<CookieStatus>,
    pub hoyolab_error: Option<String>,
    pub accounts: Vec<GameAccount>,
    pub accounts_error: Option<String>,
    pub selected_account: Option<Uuid>,
    pub account_statuses: Vec<AccountStatusEntry>,
    pub rewards: Vec<RewardSetting>,
    pub rewards_error: Option<String>,
    pub reward_runs: Vec<RewardRun>,
    pub reward_runs_error: Option<String>,
    pub passkeys: Vec<PasskeyView>,
    pub passkeys_error: Option<String>,
    pub server: Option<ServerSettings>,
    pub admin_users: Vec<UserView>,
    pub admin_error: Option<String>,
    pub openapi: Option<serde_json::Value>,
    pub openapi_error: Option<String>,
}

impl AppState {
    fn new(page: Page) -> Self {
        Self {
            phase: Phase::Loading,
            generation: 0,
            page,
            me: None,
            status: None,
            busy: false,
            logout_unconfirmed: false,
            hoyolab: None,
            hoyolab_error: None,
            accounts: Vec::new(),
            accounts_error: None,
            selected_account: None,
            account_statuses: Vec::new(),
            rewards: Vec::new(),
            rewards_error: None,
            reward_runs: Vec::new(),
            reward_runs_error: None,
            passkeys: Vec::new(),
            passkeys_error: None,
            server: None,
            admin_users: Vec::new(),
            admin_error: None,
            openapi: None,
            openapi_error: None,
        }
    }

    pub(crate) fn ready(&self) -> bool {
        self.me.as_ref().is_some_and(|me| {
            !me.onboarding.password_change_required && me.onboarding.cookie_setup_complete
        })
    }

    pub(crate) fn password_ok(&self) -> bool {
        self.me
            .as_ref()
            .is_some_and(|me| !me.onboarding.password_change_required)
    }

    pub(crate) fn visible_accounts(&self) -> Vec<GameAccount> {
        visible_detected_accounts(&self.accounts)
    }

    fn clear_owner_data(&mut self) {
        self.hoyolab = None;
        self.hoyolab_error = None;
        self.accounts.clear();
        self.accounts_error = None;
        self.selected_account = None;
        self.account_statuses.clear();
        self.rewards.clear();
        self.rewards_error = None;
        self.reward_runs.clear();
        self.reward_runs_error = None;
        self.passkeys.clear();
        self.passkeys_error = None;
        self.server = None;
        self.admin_users.clear();
        self.admin_error = None;
    }
}

#[component]
pub fn App() -> Element {
    let state = use_signal(|| AppState::new(initial_page()));

    use_future(move || async move {
        bootstrap(state).await;
    });

    use_future(move || async move {
        loop {
            gloo_timers::future::TimeoutFuture::new(60_000).await;
            let snapshot = state.read().clone();
            if snapshot.busy
                || snapshot.phase != Phase::SignedIn
                || snapshot.page != Page::Dashboard
            {
                continue;
            }
            if refresh_me(state).await && current_ready_page(state, Page::Dashboard) {
                load_dashboard_data(state, false).await;
            }
        }
    });

    let owner_key = state
        .read()
        .me
        .as_ref()
        .map(|me| me.id.to_string())
        .unwrap_or_default();
    rsx! {
        document::Stylesheet { href: asset!("/assets/dx-components-theme.css") }
        document::Stylesheet { href: asset!("/assets/dashboard.css") }
        document::Link { rel: "license", href: asset!("/src/components/LICENSE-MIT") }
        fieldset { class: "app-shell", disabled: state.read().busy, "aria-label": "Dashboard actions",
            if state.read().page == Page::Docs {
                pages::DocsPage { state }
            } else {
                match state.read().phase {
                    Phase::Loading => rsx! { pages::LoadingPage {} },
                    Phase::SignedOut => rsx! { pages::LoginPage { state } },
                    Phase::SignedIn => rsx! {
                        if !state.read().ready() {
                            pages::OnboardingPage { key: "{owner_key}", state }
                        } else {
                            pages::Shell { key: "{owner_key}", state }
                        }
                    },
                }
            }
        }
    }
}

pub(crate) fn set_page(mut state: Signal<AppState>, page: Page) {
    if state.read().busy {
        return;
    }
    state.write().page = page;
    let state_for_task = state;
    spawn_root(async move {
        load_page_data(state_for_task, page).await;
    });
}

pub(crate) fn spawn_root(future: impl Future<Output = ()> + 'static) {
    dioxus::core::spawn_forever(future);
}

pub(crate) fn set_status(mut state: Signal<AppState>, text: impl Into<String>, kind: &'static str) {
    state.write().status = Some(StatusLine {
        text: text.into(),
        kind,
    });
}

pub(crate) fn handle_api_error_for(state: Signal<AppState>, generation: u64, error: api::ApiError) {
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
pub(crate) fn spawn_action<F: Future<Output = ()> + 'static>(
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

pub(crate) fn finish_action(mut state: Signal<AppState>, generation: u64) {
    if generation_matches(state, generation) {
        state.write().busy = false;
    }
}

pub(crate) fn generation_matches(state: Signal<AppState>, generation: u64) -> bool {
    state.read().generation == generation
}

pub(crate) async fn bootstrap(mut state: Signal<AppState>) {
    if state.read().page == Page::Docs {
        state.write().phase = Phase::SignedOut;
        load_docs(state).await;
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
            load_bootstrap_data(state, generation).await;
        }
        Ok(_) => {}
        Err(error) if generation_matches(state, generation) && error.is_unauthorized() => {
            signed_out(state, "Please sign in.");
        }
        Err(error) if generation_matches(state, generation) => {
            signed_out(state, "The dashboard could not load your session.");
            set_status(state, error.to_string(), "error");
        }
        Err(_) => {}
    }
    if generation_matches(state, generation) {
        state.write().busy = false;
    }
}

async fn load_bootstrap_data(state: Signal<AppState>, generation: u64) {
    if !current_generation(state, generation) {
        return;
    }
    if state.read().page == Page::Docs {
        load_docs(state).await;
        return;
    }
    let password_ok = state.read().password_ok();
    if !state.read().ready() {
        load_hoyolab(state).await;
        if current_generation(state, generation) && password_ok {
            load_passkeys(state).await;
            if current_generation(state, generation) {
                load_server_settings(state).await;
            }
            if current_generation(state, generation)
                && state.read().me.as_ref().is_some_and(|me| me.is_admin)
            {
                load_admin_users(state).await;
            }
        }
        return;
    }
    load_server_settings(state).await;
    if current_ready_generation(state, generation) {
        let page = state.read().page;
        load_page_data(state, page).await;
    }
}

pub(crate) async fn load_page_data(state: Signal<AppState>, page: Page) {
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
        Page::Docs => load_docs(state).await,
    }
}

pub(crate) async fn load_dashboard_data(state: Signal<AppState>, include_accounts: bool) {
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

pub(crate) async fn load_hoyolab(mut state: Signal<AppState>) {
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

pub(crate) async fn load_accounts(mut state: Signal<AppState>) {
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

pub(crate) async fn load_selected_status(mut state: Signal<AppState>) {
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

pub(crate) async fn load_rewards(mut state: Signal<AppState>) {
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

pub(crate) async fn load_passkeys(mut state: Signal<AppState>) {
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

pub(crate) async fn load_server_settings(mut state: Signal<AppState>) {
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

pub(crate) async fn load_admin_users(mut state: Signal<AppState>) {
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

pub(crate) async fn load_docs(mut state: Signal<AppState>) {
    match api::openapi().await {
        Ok(value) => {
            state.write().openapi = Some(value);
            state.write().openapi_error = None;
        }
        Err(error) => state.write().openapi_error = Some(error.to_string()),
    }
}

pub(crate) async fn refresh_me(mut state: Signal<AppState>) -> bool {
    let Some(generation) = signed_in_generation(state) else {
        return false;
    };
    match api::get::<MeView>("/me").await {
        Ok(me) if current_generation(state, generation) => {
            let changed_owner = apply_fresh_me(&mut state.write(), me);
            if changed_owner {
                let generation = state.read().generation;
                load_bootstrap_data(state, generation).await;
            }
            true
        }
        Ok(_) => false,
        Err(error) if current_generation(state, generation) && error.is_unauthorized() => {
            signed_out(state, "Please sign in again.");
            false
        }
        Err(error) if current_generation(state, generation) => {
            set_status(state, error.to_string(), "error");
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

pub(crate) fn signed_out(mut state: Signal<AppState>, message: &str) {
    let page = state.read().page;
    let generation = state.read().generation + 1;
    let mut next = AppState::new(if page == Page::Docs {
        Page::Docs
    } else {
        Page::Dashboard
    });
    next.phase = Phase::SignedOut;
    next.generation = generation;
    next.status = Some(StatusLine {
        text: message.to_owned(),
        kind: "info",
    });
    state.set(next);
}

pub(crate) async fn logout(mut state: Signal<AppState>, generation: u64) {
    let result = api::send("POST", "/auth/logout", Some(json!({}))).await;
    if !generation_matches(state, generation) {
        return;
    }
    match result {
        Ok(()) => signed_out(state, "Signed out."),
        Err(error) if error.is_unauthorized() => signed_out(state, "Signed out."),
        Err(_) => {
            signed_out(
                state,
                "Local data cleared, but server logout is unconfirmed. Retry server logout before leaving this browser.",
            );
            state.write().logout_unconfirmed = true;
        }
    }
}

fn upsert_status(mut state: Signal<AppState>, id: Uuid, status: GameStatus) {
    let mut write = state.write();
    if let Some(existing) = write
        .account_statuses
        .iter_mut()
        .find(|entry| entry.id == id)
    {
        existing.status = status;
    } else {
        write
            .account_statuses
            .push(AccountStatusEntry { id, status });
    }
}

fn signed_in_generation(state: Signal<AppState>) -> Option<u64> {
    let read = state.read();
    (read.phase == Phase::SignedIn).then_some(read.generation)
}

fn ready_generation(state: Signal<AppState>) -> Option<u64> {
    let read = state.read();
    (read.phase == Phase::SignedIn && read.ready()).then_some(read.generation)
}

fn password_generation(state: Signal<AppState>) -> Option<u64> {
    let read = state.read();
    (read.phase == Phase::SignedIn && read.password_ok()).then_some(read.generation)
}

fn current_generation(state: Signal<AppState>, generation: u64) -> bool {
    let read = state.read();
    read.generation == generation && read.phase == Phase::SignedIn
}

fn current_ready_generation(state: Signal<AppState>, generation: u64) -> bool {
    let read = state.read();
    read.generation == generation && read.phase == Phase::SignedIn && read.ready()
}

fn current_ready_page(state: Signal<AppState>, page: Page) -> bool {
    let read = state.read();
    read.phase == Phase::SignedIn && read.ready() && read.page == page
}

fn current_ready_page_generation(state: Signal<AppState>, generation: u64, page: Page) -> bool {
    let read = state.read();
    read.generation == generation
        && read.phase == Phase::SignedIn
        && read.ready()
        && read.page == page
}

fn select_account(current: Option<Uuid>, accounts: &[GameAccount]) -> Option<Uuid> {
    let visible = visible_detected_accounts(accounts);
    if current.is_some_and(|id| visible.iter().any(|account| account.id == id)) {
        current
    } else {
        visible.first().map(|account| account.id)
    }
}

pub(crate) fn visible_detected_accounts(accounts: &[GameAccount]) -> Vec<GameAccount> {
    accounts
        .iter()
        .filter(|account| account.visible && account.detected && !account.game_uid.is_empty())
        .cloned()
        .collect()
}

fn initial_page() -> Page {
    #[cfg(target_arch = "wasm32")]
    {
        if web_sys::window()
            .and_then(|window| window.location().pathname().ok())
            .as_deref()
            == Some("/docs")
        {
            return Page::Docs;
        }
    }
    Page::Dashboard
}

pub(crate) fn game_name(kind: &str) -> &str {
    match kind {
        "genshin" => "Genshin Impact",
        "starrail" => "Honkai: Star Rail",
        other => other,
    }
}

pub(crate) fn humanize(value: &str) -> String {
    value.replace(['_', '-'], " ")
}

pub(crate) fn display_date(value: &Option<String>) -> String {
    value.as_deref().unwrap_or("Not reported").to_owned()
}

pub(crate) fn count_pair(current: Option<i64>, max: Option<i64>) -> String {
    match (current, max) {
        (Some(current), Some(max)) => format!("{current}/{max}"),
        (Some(current), None) => current.to_string(),
        _ => "Not reported".to_owned(),
    }
}

pub(crate) fn bool_status(value: Option<bool>) -> &'static str {
    match value {
        Some(true) => "Received",
        Some(false) => "Not received",
        None => "Not fetched",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn visible_home_accounts_exclude_hidden_missing_and_undetected() {
        let keep = uuid(1);
        let rows = vec![
            account(keep, true, true, "600000000"),
            account(uuid(2), false, true, "700000000"),
            account(uuid(3), true, false, "800000000"),
            account(uuid(4), true, true, ""),
        ];
        assert_eq!(visible_detected_accounts(&rows)[0].id, keep);
        assert_eq!(select_account(None, &rows), Some(keep));
    }

    #[test]
    fn readiness_requires_password_and_cookie_gates() {
        let mut state = AppState::new(Page::Dashboard);
        state.me = Some(me(false, false));
        assert!(!state.ready());
        state.me.as_mut().unwrap().onboarding.cookie_setup_complete = true;
        assert!(state.ready());
    }

    #[test]
    fn password_ok_does_not_require_cookie_setup() {
        let mut state = AppState::new(Page::Dashboard);
        state.me = Some(me(false, false));
        assert!(state.password_ok());
        assert!(!state.ready());
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
        state.accounts = vec![account(uuid(1), true, true, "600000000")];
        state.selected_account = Some(uuid(1));
        state.rewards_error = Some("old owner".to_owned());
        state.admin_users = vec![UserView {
            id: uuid(2),
            username: "other".to_owned(),
            is_admin: false,
            active: true,
            must_change_password: false,
        }];

        let mut next = me(false, true);
        next.id = uuid(6);
        assert!(apply_fresh_me(&mut state, next.clone()));
        assert_eq!(state.generation, 43);
        assert!(state.page == Page::Dashboard);
        assert_eq!(state.me.as_ref().unwrap().id, next.id);
        assert!(!apply_fresh_me(&mut state, next));
        assert_eq!(state.generation, 43);
        assert!(state.server.is_none());
        assert!(state.accounts.is_empty());
        assert!(state.selected_account.is_none());
        assert!(state.rewards_error.is_none());
        assert!(state.admin_users.is_empty());
    }
}
