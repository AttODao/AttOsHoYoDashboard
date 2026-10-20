use super::actions::spawn_root;
use super::data::load_page_data;
use crate::models::{
    CookieStatus, GameAccount, GameStatus, MeView, PasskeyView, RewardRun, RewardSetting,
    ServerSettings, UserView,
};
use dioxus::prelude::*;
use std::collections::BTreeMap;
use uuid::Uuid;

pub(super) const DASHBOARD_GAMES: [&str; 2] = ["genshin", "starrail"];

#[derive(Clone, Default, PartialEq)]
pub(super) struct GameSelection {
    pub(super) id: Option<Uuid>,
    pub(super) request: u64,
    pub(super) generation: u64,
    pub(super) loading: bool,
    /// Message key of the last refresh problem for this selection.
    pub(super) error: Option<&'static str>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Page {
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
pub(super) enum Phase {
    /// Public docs entry: /me has not been consulted, so leaving Docs must bootstrap.
    Unchecked,
    Loading,
    SignedOut,
    SignedIn,
}

#[derive(Clone, PartialEq)]
pub(super) struct StatusLine {
    pub(super) message_key: &'static str,
    pub(super) kind: &'static str,
    /// Distinguishes repeated messages, so a timer only clears the line it showed.
    pub(super) serial: u64,
}

/// How long a success or info line stays before clearing itself.
const STATUS_DISMISS_MS: u32 = 8_000;

/// Manual reward runs being followed after their 202s; see `data::start_reward_poll`.
#[derive(Clone, PartialEq)]
pub(super) struct RewardPoll {
    pub(super) token: u64,
    pub(super) settings: Vec<Uuid>,
    /// Following stops when the user leaves this page.
    pub(super) page: Page,
}

#[derive(Clone, PartialEq)]
pub(super) struct AccountStatusEntry {
    pub(super) id: Uuid,
    pub(super) status: GameStatus,
}

#[derive(Clone, PartialEq)]
pub(super) struct AppState {
    pub(super) phase: Phase,
    /// Session epoch: changes on bootstrap, sign-out and owner change only.
    pub(super) generation: u64,
    /// Token of the action holding `busy`; see `actions::spawn_action`.
    pub(super) action: u64,
    pub(super) page: Page,
    pub(super) me: Option<MeView>,
    pub(super) status: Option<StatusLine>,
    pub(super) busy: bool,
    pub(super) logout_unconfirmed: bool,
    pub(super) hoyolab: Option<CookieStatus>,
    pub(super) hoyolab_error: Option<String>,
    /// `None` until the first reply: lists distinguish "loading" from "empty".
    pub(super) accounts: Option<Vec<GameAccount>>,
    pub(super) accounts_error: Option<String>,
    pub(super) selected_accounts: BTreeMap<&'static str, GameSelection>,
    pub(super) account_statuses: Vec<AccountStatusEntry>,
    pub(super) rewards: Vec<RewardSetting>,
    pub(super) rewards_error: Option<String>,
    pub(super) reward_runs: Option<Vec<RewardRun>>,
    pub(super) reward_runs_error: Option<String>,
    pub(super) reward_poll: Option<RewardPoll>,
    pub(super) passkeys: Option<Vec<PasskeyView>>,
    pub(super) passkeys_error: Option<String>,
    pub(super) server: Option<ServerSettings>,
    pub(super) admin_users: Option<Vec<UserView>>,
    pub(super) admin_error: Option<String>,
    pub(super) openapi: Option<serde_json::Value>,
    pub(super) openapi_error: Option<String>,
}

impl AppState {
    pub(super) fn new(page: Page) -> Self {
        Self {
            phase: Phase::Loading,
            generation: 0,
            action: 0,
            page,
            me: None,
            status: None,
            busy: false,
            logout_unconfirmed: false,
            hoyolab: None,
            hoyolab_error: None,
            accounts: None,
            accounts_error: None,
            selected_accounts: BTreeMap::new(),
            account_statuses: Vec::new(),
            rewards: Vec::new(),
            rewards_error: None,
            reward_runs: None,
            reward_runs_error: None,
            reward_poll: None,
            passkeys: None,
            passkeys_error: None,
            server: None,
            admin_users: None,
            admin_error: None,
            openapi: None,
            openapi_error: None,
        }
    }

    pub(super) fn ready(&self) -> bool {
        self.me.as_ref().is_some_and(|me| {
            !me.onboarding.password_change_required && me.onboarding.cookie_setup_complete
        })
    }

    pub(super) fn password_ok(&self) -> bool {
        self.me
            .as_ref()
            .is_some_and(|me| !me.onboarding.password_change_required)
    }

    pub(super) fn account_rows(&self) -> &[GameAccount] {
        self.accounts.as_deref().unwrap_or_default()
    }

    pub(super) fn visible_accounts(&self) -> Vec<GameAccount> {
        visible_detected_accounts(self.account_rows())
    }

    pub(super) fn selected_game_account(&self, game: &str) -> Option<Uuid> {
        let current = self
            .selected_accounts
            .get(game)
            .and_then(|selection| selection.id);
        select_game_account(current, self.account_rows(), game)
    }

    pub(super) fn select_game_account(&mut self, game: &'static str, id: Option<Uuid>) {
        if !DASHBOARD_GAMES.contains(&game) {
            return;
        }
        let id = select_game_account(id, self.account_rows(), game);
        let selection = self.selected_accounts.entry(game).or_default();
        if selection.id != id {
            selection.id = id;
            selection.request = selection.request.saturating_add(1);
            selection.loading = false;
            selection.error = None;
        }
    }

    pub(super) fn reconcile_dashboard_accounts(&mut self) {
        for game in DASHBOARD_GAMES {
            let id = self.selected_game_account(game);
            self.select_game_account(game, id);
        }
        let selected: Vec<_> = self
            .selected_accounts
            .values()
            .filter_map(|selection| selection.id)
            .collect();
        self.account_statuses
            .retain(|entry| selected.contains(&entry.id));
    }

    pub(super) fn clear_owner_data(&mut self) {
        self.hoyolab = None;
        self.hoyolab_error = None;
        self.accounts = None;
        self.accounts_error = None;
        self.selected_accounts.clear();
        self.account_statuses.clear();
        self.rewards.clear();
        self.rewards_error = None;
        self.reward_runs = None;
        self.reward_runs_error = None;
        self.reward_poll = None;
        self.passkeys = None;
        self.passkeys_error = None;
        self.server = None;
        self.admin_users = None;
        self.admin_error = None;
    }
}

pub(super) fn set_page(state: Signal<AppState>, page: Page) {
    if state.read().busy {
        return;
    }
    if state.read().phase != Phase::SignedOut || page == Page::Docs {
        super::navigation::push_page(page);
    }
    restore_page(state, page);
}

pub(super) fn restore_page(mut state: Signal<AppState>, page: Page) {
    let page = if state.read().phase == Phase::SignedOut && page != Page::Docs {
        Page::Dashboard
    } else {
        page
    };
    if state.read().page == page {
        return;
    }
    let check_session = state.read().phase == Phase::Unchecked && page != Page::Docs;
    {
        let mut write = state.write();
        write.page = page;
        // A message belongs to the page it was raised on.
        write.status = None;
        if check_session {
            write.phase = Phase::Loading;
        }
    }
    spawn_root(async move {
        if check_session {
            super::session::bootstrap(state).await;
        } else {
            load_page_data(state, page).await;
        }
    });
}

pub(super) fn set_status(
    mut state: Signal<AppState>,
    message_key: &'static str,
    kind: &'static str,
) {
    static NEXT_STATUS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    let serial = NEXT_STATUS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    state.write().status = Some(StatusLine {
        message_key,
        kind,
        serial,
    });
    // Errors need reading and a decision; they stay until dismissed or replaced.
    if kind != "error" {
        spawn_root(async move {
            #[cfg(target_arch = "wasm32")]
            gloo_timers::future::TimeoutFuture::new(STATUS_DISMISS_MS).await;
            #[cfg(not(target_arch = "wasm32"))]
            let _ = STATUS_DISMISS_MS;
            if state
                .read()
                .status
                .as_ref()
                .is_some_and(|status| status.serial == serial)
                && cfg!(target_arch = "wasm32")
            {
                state.write().status = None;
            }
        });
    }
}

pub(super) fn clear_status(mut state: Signal<AppState>) {
    state.write().status = None;
}

pub(super) fn generation_matches(state: Signal<AppState>, generation: u64) -> bool {
    state.read().generation == generation
}

pub(super) fn upsert_status(mut state: Signal<AppState>, id: Uuid, status: GameStatus) {
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

pub(super) fn signed_in_generation(state: Signal<AppState>) -> Option<u64> {
    let read = state.read();
    (read.phase == Phase::SignedIn).then_some(read.generation)
}

pub(super) fn ready_generation(state: Signal<AppState>) -> Option<u64> {
    let read = state.read();
    (read.phase == Phase::SignedIn && read.ready()).then_some(read.generation)
}

pub(super) fn password_generation(state: Signal<AppState>) -> Option<u64> {
    let read = state.read();
    (read.phase == Phase::SignedIn && read.password_ok()).then_some(read.generation)
}

pub(super) fn current_generation(state: Signal<AppState>, generation: u64) -> bool {
    let read = state.read();
    read.generation == generation && read.phase == Phase::SignedIn
}

pub(super) fn current_ready_generation(state: Signal<AppState>, generation: u64) -> bool {
    let read = state.read();
    read.generation == generation && read.phase == Phase::SignedIn && read.ready()
}

pub(super) fn current_ready_page(state: Signal<AppState>, page: Page) -> bool {
    let read = state.read();
    read.phase == Phase::SignedIn && read.ready() && read.page == page
}

pub(super) fn current_ready_page_generation(
    state: Signal<AppState>,
    generation: u64,
    page: Page,
) -> bool {
    let read = state.read();
    read.generation == generation
        && read.phase == Phase::SignedIn
        && read.ready()
        && read.page == page
}

pub(super) fn select_game_account(
    current: Option<Uuid>,
    accounts: &[GameAccount],
    game: &str,
) -> Option<Uuid> {
    let mut eligible = accounts.iter().filter(|account| {
        account.game_kind == game
            && account.visible
            && account.detected
            && !account.game_uid.is_empty()
    });
    if current.is_some_and(|id| eligible.clone().any(|account| account.id == id)) {
        current
    } else {
        eligible.next().map(|account| account.id)
    }
}

pub(super) fn visible_detected_accounts(accounts: &[GameAccount]) -> Vec<GameAccount> {
    accounts
        .iter()
        .filter(|account| account.visible && account.detected && !account.game_uid.is_empty())
        .cloned()
        .collect()
}

pub(super) fn initial_page() -> Page {
    super::navigation::initial_page()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Capabilities, Onboarding};

    fn uuid(n: u128) -> Uuid {
        Uuid::from_u128(n)
    }

    #[test]
    fn history_restoration_preserves_authenticated_deep_links_not_signed_out_routes() {
        fn check() -> Element {
            let mut state = use_signal(|| AppState::new(Page::Dashboard));
            state.write().phase = Phase::SignedOut;
            restore_page(state, Page::Security);
            assert!(state.read().page == Page::Dashboard);
            restore_page(state, Page::Docs);
            assert!(state.read().page == Page::Docs);
            restore_page(state, Page::Admin);
            assert!(state.read().page == Page::Dashboard);
            // Docs never consulted /me; leaving it checks the session, not the login gate.
            state.write().page = Page::Docs;
            state.write().phase = Phase::Unchecked;
            restore_page(state, Page::Security);
            assert!(state.read().page == Page::Security);
            assert!(state.read().phase == Phase::Loading);
            state.write().phase = Phase::Loading;
            state.write().busy = true;
            restore_page(state, Page::Security);
            assert!(state.read().page == Page::Security);
            state.write().phase = Phase::SignedIn;
            restore_page(state, Page::Rewards);
            assert!(state.read().page == Page::Rewards);
            assert!(state.read().busy);
            rsx! {}
        }
        VirtualDom::new(check).rebuild_in_place();
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
        let visible = visible_detected_accounts(&rows);
        assert_eq!(visible.len(), 1);
        assert_eq!(visible[0].id, keep);
    }

    #[test]
    fn game_selections_are_independent_and_reconcile_visibility() {
        let mut state = AppState::new(Page::Dashboard);
        let mut starrail = account(uuid(3), true, true, "700000001");
        starrail.game_kind = "starrail".to_owned();
        state.accounts = Some(vec![
            account(uuid(1), true, true, "600000001"),
            account(uuid(2), true, true, "600000002"),
            starrail,
            account(uuid(4), false, true, "600000004"),
            account(uuid(5), true, false, "600000005"),
            account(uuid(6), true, true, ""),
        ]);
        state.reconcile_dashboard_accounts();
        state.select_game_account("genshin", Some(uuid(2)));
        assert_eq!(state.selected_game_account("genshin"), Some(uuid(2)));
        assert_eq!(state.selected_game_account("starrail"), Some(uuid(3)));
        assert_eq!(
            select_game_account(Some(uuid(3)), state.account_rows(), "genshin"),
            Some(uuid(1))
        );
        state.accounts.as_mut().unwrap()[1].visible = false;
        state.reconcile_dashboard_accounts();
        assert_eq!(state.selected_game_account("genshin"), Some(uuid(1)));
        state.accounts.as_mut().unwrap()[0].detected = false;
        state.reconcile_dashboard_accounts();
        assert_eq!(state.selected_game_account("genshin"), None);
        assert_eq!(state.selected_game_account("starrail"), Some(uuid(3)));
        state.clear_owner_data();
        assert!(state.selected_accounts.is_empty());
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
}
