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
        ApiTokenView, AuditEntry, CookieStatus, GameAccount, GameStatus, NotificationChannelView,
        PasskeyView, PersonalNotificationView, RewardRun, RewardSetting, ServerSettings,
        SessionView, UserView,
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
        Page::Security => {
            load_passkeys(state).await;
            load_sessions(state).await;
            // The app leaves API tokens to the web version.
            if !crate::platform::APP {
                load_api_tokens(state).await;
            }
        }
        Page::Notifications => load_notifications(state).await,
        Page::Admin if !crate::platform::APP => {
            load_admin_users(state).await;
            load_admin_audit(state, false).await;
            load_admin_notifications(state).await;
        }
        Page::Admin | Page::Delete | Page::Docs => {}
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

/// Resources refreshed by overlapping loads (page entry, polling and the
/// refresh after an action). Game status has its own per-game request guard.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Resource {
    Hoyolab,
    Accounts,
    Rewards,
    RewardRuns,
    Passkeys,
    Sessions,
    ApiTokens,
    ServerSettings,
    AdminAudit,
    AdminUsers,
    Notifications,
    AdminNotifications,
}

const RESOURCES: usize = Resource::AdminNotifications as usize + 1;

/// Latest request issued per resource. Actions keep the session generation, so
/// an older in-flight GET may finish after a newer refresh; only the reply to the
/// latest request for a resource may be applied.
struct RequestSequence {
    next: AtomicU64,
    latest: [AtomicU64; RESOURCES],
}

impl RequestSequence {
    const fn new() -> Self {
        Self {
            next: AtomicU64::new(1),
            latest: [const { AtomicU64::new(0) }; RESOURCES],
        }
    }

    fn issue(&self, resource: Resource) -> u64 {
        let ticket = self.next.fetch_add(1, Ordering::Relaxed);
        self.latest[resource as usize].store(ticket, Ordering::Relaxed);
        ticket
    }

    fn is_latest(&self, resource: Resource, ticket: u64) -> bool {
        self.latest[resource as usize].load(Ordering::Relaxed) == ticket
    }
}

// Tickets are process-unique, so they stay ordered across sign-out and state resets.
static REQUESTS: RequestSequence = RequestSequence::new();

// Session-scoped GET: a reply for an older session (or a lost gate), or one
// superseded by a newer request for the same resource, is dropped and a 401
// signs out; otherwise the caller gets the value or a message key.
async fn fetch_current<T: DeserializeOwned>(
    state: Signal<AppState>,
    resource: Resource,
    path: &str,
    generation: u64,
    current: fn(Signal<AppState>, u64) -> bool,
) -> Option<Result<T, &'static str>> {
    let ticket = REQUESTS.issue(resource);
    let result = api::get::<T>(path).await;
    if !current(state, generation) || !REQUESTS.is_latest(resource, ticket) {
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
    let Some(result) = fetch_current::<CookieStatus>(
        state,
        Resource::Hoyolab,
        "/me/hoyolab",
        generation,
        current_generation,
    )
    .await
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
        Resource::Accounts,
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
        Resource::Rewards,
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
        Resource::RewardRuns,
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

// UI duplicate/uncertain-send guard only; the backend remains the policy authority.
// Manual runs are valid even when automation is OFF.
pub(super) fn run_eligible(auth_status: &str, last_state: Option<&str>) -> bool {
    auth_status == "ready"
        && !matches!(
            last_state,
            Some("pending" | "queued" | "running" | "retry_wait" | "unknown")
        )
}

/// Why the single "claim now" control can or cannot run, from the loaded settings.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum RunAll {
    /// These settings still need today's claim and may be run.
    Ready(Vec<Uuid>),
    /// Message key explaining why nothing is runnable.
    Blocked(&'static str),
}

pub(super) fn run_all_targets(settings: &[RewardSetting], posting_available: bool) -> RunAll {
    if !posting_available {
        return RunAll::Blocked("rewards.reward_posting_unavailable");
    }
    if settings.is_empty() {
        return RunAll::Blocked("common.not_fetched");
    }
    let unclaimed: Vec<_> = settings
        .iter()
        .filter(|setting| setting.is_sign != Some(true))
        .collect();
    if unclaimed.is_empty() {
        return RunAll::Blocked("rewards.all_claimed_today");
    }
    let ready: Vec<_> = unclaimed
        .iter()
        .filter(|setting| {
            run_eligible(
                &setting.auth_status,
                setting.last_run.as_ref().map(|run| run.state.as_str()),
            )
        })
        .map(|setting| setting.id)
        .collect();
    if !ready.is_empty() {
        return RunAll::Ready(ready);
    }
    let last = |setting: &&RewardSetting| setting.last_run.as_ref().map(|run| run.state.clone());
    if unclaimed
        .iter()
        .any(|setting| last(setting).is_some_and(|state| reward_run_in_flight(&state)))
    {
        RunAll::Blocked("rewards.run_in_progress")
    } else if unclaimed
        .iter()
        .any(|setting| last(setting).as_deref() == Some("unknown"))
    {
        RunAll::Blocked("rewards.run_outcome_unknown")
    } else {
        RunAll::Blocked("rewards.run_needs_link_check")
    }
}

/// Claim every runnable game once. Each POST is sent at most once; a failed
/// acknowledgement is never retried, only followed by a GET refresh.
pub(super) fn run_rewards_now(state: Signal<AppState>, page: Page) {
    let posting = state
        .read()
        .me
        .as_ref()
        .is_some_and(|me| me.capabilities.reward_posting_available);
    let RunAll::Ready(targets) = run_all_targets(&state.read().rewards, posting) else {
        return;
    };
    super::actions::spawn_action(state, move |generation| async move {
        let mut accepted = Vec::new();
        let mut failure = None;
        for id in targets {
            if !super::state::generation_matches(state, generation) {
                return;
            }
            match api::send(
                "POST",
                &format!("/reward-settings/{id}/run"),
                Some(json!({})),
            )
            .await
            {
                Ok(()) => accepted.push(id),
                Err(error) => failure = Some(error),
            }
        }
        if !super::state::generation_matches(state, generation) {
            return;
        }
        match failure {
            Some(error) => handle_api_error_for(state, generation, error),
            None => set_status(
                state,
                "rewards.run_accepted_queued_not_completed",
                "success",
            ),
        }
        // A failed acknowledgement may still have enqueued work: refresh either way.
        load_rewards(state).await;
        if !accepted.is_empty() {
            start_reward_poll(state, generation, accepted, page);
        }
    });
}

/// Completion announcement (message key, status kind) for settled run states.
fn reward_runs_finished(states: &[String]) -> (&'static str, &'static str) {
    let settled = |ok: &[&str]| states.iter().all(|state| ok.contains(&state.as_str()));
    if settled(&["success"]) {
        ("rewards.run_finished_success", "success")
    } else if settled(&["already_claimed"]) {
        ("rewards.run_finished_already_claimed", "success")
    } else if settled(&["success", "already_claimed"]) {
        ("rewards.run_finished_success", "success")
    } else if states
        .iter()
        .any(|state| matches!(state.as_str(), "failed" | "stopped"))
    {
        ("rewards.run_finished_failed", "error")
    } else {
        ("rewards.run_finished_unconfirmed", "error")
    }
}

/// Follow accepted manual runs for about a minute. Each poll is an ordinary
/// GET refresh: it never re-sends a POST, skips while an action holds the UI,
/// and stops on completion, page leave, session change or a newer poll.
pub(super) fn start_reward_poll(
    mut state: Signal<AppState>,
    generation: u64,
    settings: Vec<Uuid>,
    page: Page,
) {
    if !current_ready_page_generation(state, generation, page) {
        return;
    }
    let token = NEXT_REWARD_POLL.fetch_add(1, Ordering::Relaxed);
    state.write().reward_poll = Some(RewardPoll {
        token,
        settings: settings.clone(),
        page,
    });
    spawn_root(async move {
        poll_reward_runs(state, generation, token, settings, page).await;
        if state
            .read()
            .reward_poll
            .as_ref()
            .is_some_and(|poll| poll.token == token)
        {
            state.write().reward_poll = None;
        }
    });
}

async fn poll_reward_runs(
    state: Signal<AppState>,
    generation: u64,
    token: u64,
    settings: Vec<Uuid>,
    page: Page,
) {
    let following = move || {
        current_ready_page_generation(state, generation, page)
            && state
                .read()
                .reward_poll
                .as_ref()
                .is_some_and(|poll| poll.token == token)
    };
    let run_states = || -> Option<Vec<String>> {
        let read = state.read();
        settings
            .iter()
            .map(|id| {
                let row = read.rewards.iter().find(|row| row.id == *id)?;
                row.last_run.as_ref().map(|run| run.state.clone())
            })
            .collect()
    };
    let settled = |states: &Option<Vec<String>>| {
        states
            .as_ref()
            .is_some_and(|states| states.iter().all(|state| !reward_run_in_flight(state)))
    };
    for _ in 0..REWARD_POLL_ATTEMPTS {
        if settled(&run_states()) {
            break;
        }
        crate::platform::sleep_ms(REWARD_POLL_INTERVAL_MS).await;
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
    match run_states() {
        Some(states) if settled(&Some(states.clone())) => {
            let (key, kind) = reward_runs_finished(&states);
            set_status(state, key, kind);
        }
        _ => set_status(state, "rewards.run_still_pending", "info"),
    }
}

pub(super) async fn load_sessions(mut state: Signal<AppState>) {
    let Some(generation) = password_generation(state) else {
        return;
    };
    let Some(result) = fetch_current::<Vec<SessionView>>(
        state,
        Resource::Sessions,
        "/me/sessions",
        generation,
        current_generation,
    )
    .await
    else {
        return;
    };
    let mut write = state.write();
    match result {
        Ok(sessions) => {
            write.sessions = Some(sessions);
            write.sessions_error = None;
        }
        Err(key) => {
            write.sessions = None;
            write.sessions_error = Some(key.to_owned());
        }
    }
}

pub(super) async fn load_api_tokens(mut state: Signal<AppState>) {
    let Some(generation) = password_generation(state) else {
        return;
    };
    let Some(result) = fetch_current::<Vec<ApiTokenView>>(
        state,
        Resource::ApiTokens,
        "/me/api-tokens",
        generation,
        current_generation,
    )
    .await
    else {
        return;
    };
    let mut write = state.write();
    match result {
        Ok(tokens) => {
            write.api_tokens = Some(tokens);
            write.api_tokens_error = None;
        }
        Err(key) => {
            write.api_tokens = None;
            write.api_tokens_error = Some(key.to_owned());
        }
    }
}

pub(super) async fn load_passkeys(mut state: Signal<AppState>) {
    let Some(generation) = password_generation(state) else {
        return;
    };
    let Some(result) = fetch_current::<Vec<PasskeyView>>(
        state,
        Resource::Passkeys,
        "/me/passkeys",
        generation,
        current_generation,
    )
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
    if let Some(Ok(settings)) = fetch_current::<ServerSettings>(
        state,
        Resource::ServerSettings,
        "/server-settings",
        generation,
        current_generation,
    )
    .await
    {
        state.write().server = Some(settings);
    }
}

pub(super) async fn load_notifications(mut state: Signal<AppState>) {
    let Some(generation) = password_generation(state) else {
        return;
    };
    let Some(result) = fetch_current::<PersonalNotificationView>(
        state,
        Resource::Notifications,
        "/me/notifications",
        generation,
        current_generation,
    )
    .await
    else {
        return;
    };
    let mut write = state.write();
    match result {
        Ok(view) => {
            write.notifications = Some(view);
            write.notifications_error = None;
        }
        Err(key) => {
            write.notifications = None;
            write.notifications_error = Some(key.to_owned());
        }
    }
}

pub(super) async fn load_admin_notifications(mut state: Signal<AppState>) {
    let Some(generation) = password_generation(state) else {
        return;
    };
    if !state.read().me.as_ref().is_some_and(|me| me.is_admin) {
        return;
    }
    let Some(result) = fetch_current::<NotificationChannelView>(
        state,
        Resource::AdminNotifications,
        "/admin/notifications",
        generation,
        current_generation,
    )
    .await
    else {
        return;
    };
    let mut write = state.write();
    match result {
        Ok(view) => {
            write.admin_notifications = Some(view);
            write.admin_notifications_error = None;
        }
        Err(key) => {
            write.admin_notifications = None;
            write.admin_notifications_error = Some(key.to_owned());
        }
    }
}

pub(super) async fn load_admin_users(mut state: Signal<AppState>) {
    let Some(generation) = password_generation(state) else {
        return;
    };
    if !state.read().me.as_ref().is_some_and(|me| me.is_admin) {
        return;
    }
    let Some(result) = fetch_current::<Vec<UserView>>(
        state,
        Resource::AdminUsers,
        "/admin/users",
        generation,
        current_generation,
    )
    .await
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

pub(super) const AUDIT_PAGE: usize = 50;

/// The newest audit page, or (with `older`) the page before the oldest shown entry.
pub(super) async fn load_admin_audit(mut state: Signal<AppState>, older: bool) {
    let Some(generation) = password_generation(state) else {
        return;
    };
    if !state.read().me.as_ref().is_some_and(|me| me.is_admin) {
        return;
    }
    let before = older
        .then(|| {
            state
                .read()
                .admin_audit
                .as_ref()
                .and_then(|rows| rows.last().map(|row| row.created_at.clone()))
        })
        .flatten();
    let path = match &before {
        Some(before) => format!(
            "/admin/audit?limit={AUDIT_PAGE}&before={}",
            query_value(before)
        ),
        None => format!("/admin/audit?limit={AUDIT_PAGE}"),
    };
    let Some(result) = fetch_current::<Vec<AuditEntry>>(
        state,
        Resource::AdminAudit,
        &path,
        generation,
        current_generation,
    )
    .await
    else {
        return;
    };
    let mut write = state.write();
    match result {
        Ok(rows) => {
            write.admin_audit_more = rows.len() == AUDIT_PAGE;
            match (&mut write.admin_audit, before.is_some()) {
                (Some(existing), true) => existing.extend(rows),
                _ => write.admin_audit = Some(rows),
            }
            write.admin_audit_error = None;
        }
        Err(key) => write.admin_audit_error = Some(key.to_owned()),
    }
}

/// Percent-encodes the characters an RFC 3339 timestamp may carry into a query value.
fn query_value(value: &str) -> String {
    value.replace('+', "%2B").replace(':', "%3A")
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
    fn only_the_latest_request_per_resource_may_apply_its_reply() {
        let requests = RequestSequence::new();
        let first = requests.issue(Resource::Rewards);
        assert!(requests.is_latest(Resource::Rewards, first));
        // A refresh after an action supersedes the in-flight page load.
        let refresh = requests.issue(Resource::Rewards);
        assert!(refresh > first);
        assert!(!requests.is_latest(Resource::Rewards, first));
        assert!(requests.is_latest(Resource::Rewards, refresh));
        // Other resources are sequenced independently.
        let runs = requests.issue(Resource::RewardRuns);
        let accounts = requests.issue(Resource::Accounts);
        assert!(requests.is_latest(Resource::Rewards, refresh));
        assert!(requests.is_latest(Resource::RewardRuns, runs));
        assert!(requests.is_latest(Resource::Accounts, accounts));
        for resource in [
            Resource::Hoyolab,
            Resource::Passkeys,
            Resource::Sessions,
            Resource::ServerSettings,
            Resource::AdminAudit,
            Resource::AdminUsers,
            Resource::Notifications,
            Resource::AdminNotifications,
        ] {
            let ticket = requests.issue(resource);
            assert!(requests.is_latest(resource, ticket), "{resource:?}");
        }
        assert!(requests.is_latest(Resource::Accounts, accounts));
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
        let states = |list: &[&str]| list.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
        assert_eq!(
            reward_runs_finished(&states(&["success"])),
            ("rewards.run_finished_success", "success")
        );
        assert_eq!(
            reward_runs_finished(&states(&["success", "already_claimed"])).1,
            "success"
        );
        assert_eq!(
            reward_runs_finished(&states(&["already_claimed"])).0,
            "rewards.run_finished_already_claimed"
        );
        assert_eq!(
            reward_runs_finished(&states(&["success", "stopped"])).1,
            "error"
        );
        assert_eq!(
            reward_runs_finished(&states(&["unknown"])),
            ("rewards.run_finished_unconfirmed", "error")
        );
    }

    #[test]
    fn reward_polling_starts_only_on_its_current_page() {
        fn check() -> Element {
            let state = use_signal(ready_state);
            let ids = vec![Uuid::from_u128(20)];
            start_reward_poll(state, 7, ids.clone(), Page::Rewards);
            assert!(
                state.read().reward_poll.is_none(),
                "dashboard is not the rewards page"
            );
            start_reward_poll(state, 6, ids.clone(), Page::Dashboard);
            assert!(state.read().reward_poll.is_none(), "stale session");
            rsx! {}
        }
        VirtualDom::new(check).rebuild_in_place();
    }

    fn reward(n: u128, is_sign: Option<bool>, auth: &str, last: Option<&str>) -> RewardSetting {
        RewardSetting {
            id: Uuid::from_u128(n),
            game_kind: "genshin".to_owned(),
            enabled: false,
            local_time: "06:00".to_owned(),
            auth_status: auth.to_owned(),
            next_run_at: None,
            last_run: last.map(|state| crate::models::LastRun {
                id: Uuid::from_u128(n + 100),
                state: state.to_owned(),
                ended_at: None,
            }),
            is_sign,
            total_sign_day: None,
            today_award: None,
            last_checked_at: None,
            error_code: None,
            error_at: None,
        }
    }

    #[test]
    fn claim_now_runs_unclaimed_eligible_games_and_explains_otherwise() {
        let id = Uuid::from_u128;
        let open = reward(1, Some(false), "ready", None);
        let done = reward(2, Some(true), "ready", Some("success"));
        assert_eq!(
            run_all_targets(&[open.clone(), done.clone()], true),
            RunAll::Ready(vec![id(1)])
        );
        assert_eq!(
            run_all_targets(std::slice::from_ref(&open), false),
            RunAll::Blocked("rewards.reward_posting_unavailable")
        );
        assert_eq!(
            run_all_targets(std::slice::from_ref(&done), true),
            RunAll::Blocked("rewards.all_claimed_today")
        );
        assert_eq!(
            run_all_targets(
                &[reward(3, Some(false), "ready", Some("running")), done],
                true
            ),
            RunAll::Blocked("rewards.run_in_progress")
        );
        assert_eq!(
            run_all_targets(&[reward(4, None, "ready", Some("unknown"))], true),
            RunAll::Blocked("rewards.run_outcome_unknown")
        );
        assert_eq!(
            run_all_targets(&[reward(5, Some(false), "action_required", None)], true),
            RunAll::Blocked("rewards.run_needs_link_check")
        );
        assert_eq!(
            run_all_targets(&[], true),
            RunAll::Blocked("common.not_fetched")
        );
        // Automation OFF never blocks a manual claim.
        assert!(run_eligible("ready", Some("failed")));
        assert!(!run_eligible("ready", Some("retry_wait")));
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
