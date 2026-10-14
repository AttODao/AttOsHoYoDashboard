use crate::{
    app::{self, AppState, Page, bool_status, count_pair, display_date, game_name, humanize},
    components::{
        Button, ButtonVariant, Card, CardContent, CardDescription, CardFooter, CardHeader,
        CardTitle,
    },
    models::*,
};
use dioxus::prelude::*;
use serde_json::{Value, json};
use uuid::Uuid;

#[component]
pub(crate) fn LoadingPage() -> Element {
    rsx! {
        main { class: "center-page", "aria-live": "polite",
            Card { class: "narrow-card",
                CardHeader {
                    CardTitle { "AttOs Game Dashboard" }
                    CardDescription { "Loading dashboard state…" }
                }
            }
        }
    }
}

#[component]
pub(crate) fn LoginPage(mut state: Signal<AppState>) -> Element {
    let mut username = use_signal(String::new);
    let mut password = use_signal(String::new);
    let mut passkey_username = use_signal(String::new);

    rsx! {
        main { class: "center-page",
            div { class: "brand-block",
                h1 { "AttOs Game Dashboard" }
                p { "Sign in with a password, or enter your username first to use a registered passkey." }
            }
            Status { state }
            if state.read().logout_unconfirmed {
                Button { variant: ButtonVariant::Secondary, r#type: "button", onclick: move |_| { app::spawn_action(state, move |generation| async move { app::logout(state, generation).await; }); }, "Retry server logout" }
            }
            div { class: "auth-grid",
                Card {
                    CardHeader {
                        CardTitle { "Password sign in" }
                        CardDescription { "Credentials are sent only to the same-origin dashboard API." }
                    }
                    CardContent { class: "form-stack",
                        Field { id: "login-username", label: "Username",
                            input { id: "login-username", autocomplete: "username", value: "{username}", oninput: move |event| username.set(event.value()) }
                        }
                        Field { id: "login-password", label: "Password",
                            input { id: "login-password", r#type: "password", autocomplete: "current-password", value: "{password}", oninput: move |event| password.set(event.value()) }
                        }
                    }
                    CardFooter {
                        Button { r#type: "button", onclick: move |_| {
                            let user = std::mem::take(&mut *username.write());
                            let pass = std::mem::take(&mut *password.write());
                            let state = state;
                            app::spawn_action(state, move |generation| async move {
                                match crate::api::send("POST", "/auth/login", Some(json!({"username": user, "password": pass}))).await {
                                    Ok(()) if app::generation_matches(state, generation) => app::bootstrap(state).await,
                                    Ok(()) => {}
                                    Err(error) => { app::handle_api_error_for(state, generation, error); app::finish_action(state, generation); }
                                }
                            });
                        }, "Sign in" }
                    }
                }
                Card {
                    CardHeader {
                        CardTitle { "Passkey sign in" }
                        CardDescription { "Enter your username, then approve the browser passkey prompt." }
                    }
                    CardContent { class: "form-stack",
                        Field { id: "passkey-login-username", label: "Username",
                            input { id: "passkey-login-username", autocomplete: "username webauthn", value: "{passkey_username}", oninput: move |event| passkey_username.set(event.value()) }
                        }
                    }
                    CardFooter {
                        Button { variant: ButtonVariant::Secondary, r#type: "button", onclick: move |_| {
                            let user = std::mem::take(&mut *passkey_username.write());
                            let state = state;
                            app::spawn_action(state, move |generation| async move {
                                match crate::webauthn::login(&user).await {
                                    Ok(()) if app::generation_matches(state, generation) => app::bootstrap(state).await,
                                    Ok(()) => {}
                                    Err(error) => { app::handle_api_error_for(state, generation, error); app::finish_action(state, generation); }
                                }
                            });
                        }, "Use passkey" }
                    }
                }
            }
        }
    }
}

#[component]
pub(crate) fn OnboardingPage(mut state: Signal<AppState>) -> Element {
    let me = state.read().me.clone();
    let onboarding = me.as_ref().map(|me| me.onboarding.clone());
    let password_title = if onboarding
        .as_ref()
        .is_some_and(|o| o.password_change_required)
    {
        "Change temporary password"
    } else {
        "Change password"
    };
    rsx! {
        main { class: "page-wrap",
            Header { state }
            Status { state }
            div { class: "setup-grid",
                Card {
                    CardHeader {
                        CardTitle { "Setup status" }
                        CardDescription { "Ordinary game status and reward screens stay locked until password and Cookie requirements are complete." }
                    }
                    CardContent {
                        dl { class: "metric-list",
                            Metric { label: "Password changed", value: if onboarding.as_ref().is_some_and(|o| o.password_change_required) { "Required".to_owned() } else { "Complete".to_owned() } }
                            Metric { label: "Cookie setup", value: onboarding.as_ref().map(|o| if o.cookie_setup_complete { "Complete".to_owned() } else { humanize(&o.cookie_status) }).unwrap_or_else(|| "Not loaded".to_owned()) }
                            Metric { label: "Server verification", value: onboarding.as_ref().map(|o| if o.verification_available { "Available".to_owned() } else { "Not available on this server".to_owned() }).unwrap_or_else(|| "Not reported".to_owned()) }
                        }
                        if let Some(error) = &state.read().hoyolab_error { p { class: "warning", "{error}" } }
                    }
                    CardFooter {
                        Button { variant: ButtonVariant::Secondary, r#type: "button", onclick: move |_| { let state = state; app::spawn_action(state, move |_| async move { app::bootstrap(state).await; }); }, "Refresh setup status" }
                    }
                }
                PasswordCard { state, title: password_title }
                if state.read().password_ok() {
                    ServerSettingsCard { state }
                    PasskeyCard { state }
                    DeleteAccountCard { state }
                }
                CookieCard { state }
                if me.as_ref().is_some_and(|me| me.is_admin) && state.read().password_ok() {
                    AdminCard { state, compact: true }
                }
            }
        }
    }
}

#[component]
pub(crate) fn Shell(mut state: Signal<AppState>) -> Element {
    let page = state.read().page;
    rsx! {
        Header { state }
        div { class: "dashboard-layout",
            nav { class: "side-nav", "aria-label": "Dashboard sections",
                NavButton { state, page: Page::Dashboard, current: page, label: "Dashboard" }
                NavButton { state, page: Page::Link, current: page, label: "HoYoLAB Link" }
                NavButton { state, page: Page::Uids, current: page, label: "UID Visibility" }
                NavButton { state, page: Page::Rewards, current: page, label: "Rewards" }
                NavButton { state, page: Page::Security, current: page, label: "Security" }
                if state.read().me.as_ref().is_some_and(|me| me.is_admin) {
                    NavButton { state, page: Page::Admin, current: page, label: "Admin" }
                }
                NavButton { state, page: Page::Delete, current: page, label: "Delete Account" }
            }
            main { class: "content-panel",
                Status { state }
                ServerSettingsSummary { state }
                match page {
                    Page::Dashboard => rsx! { DashboardPage { state } },
                    Page::Link => rsx! { LinkPage { state } },
                    Page::Uids => rsx! { UidsPage { state } },
                    Page::Rewards => rsx! { RewardsPage { state } },
                    Page::Security => rsx! { SecurityPage { state } },
                    Page::Admin => rsx! { AdminPage { state } },
                    Page::Delete => rsx! { DeletePage { state } },
                    Page::Docs => rsx! { DocsPage { state } },
                }
            }
        }
    }
}

#[component]
fn Header(mut state: Signal<AppState>) -> Element {
    let me = state.read().me.clone();
    rsx! {
        header { class: "topbar",
            div {
                p { class: "eyebrow", "Private dashboard" }
                h1 { "AttOs Game Dashboard" }
            }
            div { class: "topbar-actions",
                if let Some(me) = me {
                    span { class: "user-chip", "{me.username}" }
                    if me.is_admin { span { class: "role-chip", "Admin" } }
                    if let Some(server) = state.read().server.clone() {
                        span { class: "role-chip", "TZ {server.timezone_offset} · {server.session_ttl_days}d" }
                    }
                    Button { variant: ButtonVariant::Ghost, r#type: "button", onclick: move |_| { let state = state; app::spawn_action(state, move |generation| async move { app::logout(state, generation).await; }); }, "Logout" }
                } else {
                    Button { variant: ButtonVariant::Outline, r#type: "button", onclick: move |_| app::set_page(state, Page::Docs), "API docs" }
                }
            }
        }
    }
}

#[component]
fn Status(state: Signal<AppState>) -> Element {
    let status = state.read().status.clone();
    rsx! {
        div { class: "live-region", role: "status", "aria-live": "polite",
            if let Some(status) = status {
                p { class: "status {status.kind}", "{status.text}" }
            }
        }
    }
}

#[component]
fn ServerSettingsSummary(state: Signal<AppState>) -> Element {
    let server = state.read().server.clone();
    rsx! {
        if let Some(server) = server {
            p { class: "eyebrow", "Server timezone {server.timezone_offset}; session TTL {server.session_ttl_days} days" }
        }
    }
}

#[component]
fn ServerSettingsCard(state: Signal<AppState>) -> Element {
    let server = state.read().server.clone();
    rsx! {
        Card {
            CardHeader { CardTitle { "Server settings" } CardDescription { "Shared fixed metadata reported by the dashboard API." } }
            CardContent {
                dl { class: "metric-list",
                    Metric { label: "Fixed timezone", value: server.as_ref().map(|s| s.timezone_offset.clone()).unwrap_or_else(|| "Not loaded".to_owned()) }
                    Metric { label: "Session TTL", value: server.as_ref().map(|s| format!("{} days", s.session_ttl_days)).unwrap_or_else(|| "Not loaded".to_owned()) }
                }
            }
        }
    }
}

#[component]
fn NavButton(
    mut state: Signal<AppState>,
    page: Page,
    current: Page,
    label: &'static str,
) -> Element {
    rsx! {
        Button { variant: if page == current { ButtonVariant::Secondary } else { ButtonVariant::Ghost }, r#type: "button", "aria-current": if page == current { "page" } else { "false" }, onclick: move |_| app::set_page(state, page), "{label}" }
    }
}

#[component]
fn DashboardPage(mut state: Signal<AppState>) -> Element {
    let selected = state.read().selected_account;
    let visible = state.read().visible_accounts();
    let selected_status = selected.and_then(|id| {
        state
            .read()
            .account_statuses
            .iter()
            .find(|entry| entry.id == id)
            .map(|entry| entry.status.clone())
    });
    rsx! {
        div { class: "page-stack",
            h2 { "Dashboard" }
            div { class: "grid two",
                Card {
                    CardHeader { CardTitle { "Visible game UIDs" } CardDescription { "Only visible, detected UIDs are offered for status polling." } }
                    CardContent {
                        if let Some(error) = &state.read().accounts_error { p { class: "warning", "{error}" } }
                        AccountTable { state, controls: false }
                    }
                }
                Card {
                    CardHeader { CardTitle { "Selected game status" } CardDescription { "Fetched from the server cache/upstream path; missing fields are not invented." } }
                    CardContent {
                        if visible.is_empty() {
                            p { "No visible detected UIDs. Use UID Visibility to choose retained game accounts." }
                        } else {
                            label { r#for: "selected-account", "Selected UID" }
                            select { id: "selected-account", value: selected.map(|id| id.to_string()).unwrap_or_default(), onchange: move |event| {
                                if let Ok(id) = event.value().parse::<Uuid>() {
                                    state.write().selected_account = Some(id);
                                    let state = state;
                                    app::spawn_root(async move { app::load_selected_status(state).await; });
                                }
                            },
                                for account in visible {
                                    option { value: "{account.id}", "{game_name(&account.game_kind)} {account.game_uid}" }
                                }
                            }
                            if let Some(status) = selected_status {
                                GameStatusCard { status }
                            } else {
                                p { "Status has not been fetched yet, or the server returned no usable status." }
                                Button { variant: ButtonVariant::Secondary, r#type: "button", onclick: move |_| { let state = state; app::spawn_root(async move { app::load_selected_status(state).await; }); }, "Fetch status" }
                            }
                        }
                    }
                }
            }
            Card {
                CardHeader { CardTitle { "Login bonuses" } CardDescription { "Reward status is checked independently of the selected UID." } }
                CardContent {
                    if let Some(error) = &state.read().rewards_error { p { class: "warning", "{error}" } }
                    div { class: "reward-badges",
                        for setting in state.read().rewards.clone() {
                            div { class: "reward-badge",
                                strong { "{game_name(&setting.game_kind)}" }
                                span { "{bool_status(setting.is_sign)}" }
                                small { "Checked: {display_date(&setting.last_checked_at)}" }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn GameStatusCard(status: GameStatus) -> Element {
    let data = status.data.clone();
    rsx! {
        div { class: "status-card",
            dl { class: "metric-list",
                Metric { label: "Game", value: game_name(&status.game_kind).to_owned() }
                Metric { label: "UID", value: if status.game_uid.is_empty() { "Missing".to_owned() } else { status.game_uid.clone() } }
                Metric { label: "Region", value: status.region.clone() }
                Metric { label: "Fetched", value: display_date(&status.fetched_at) }
                Metric { label: "Stale", value: if status.stale { "Yes".to_owned() } else { "No".to_owned() } }
                Metric { label: "Error", value: status.error_code.clone().unwrap_or_else(|| "None".to_owned()) }
                Metric { label: "Error at", value: display_date(&status.error_at) }
            }
            if let Some(data) = data {
                dl { class: "metric-list wide",
                    if status.game_kind == "genshin" {
                        Metric { label: "Resin", value: count_pair(data.current_resin, data.max_resin) }
                        Metric { label: "Resin full at", value: display_date(&data.resin_full_at) }
                        Metric { label: "Home coin", value: count_pair(data.current_home_coin, data.max_home_coin) }
                        Metric { label: "Home coin full at", value: display_date(&data.home_coin_full_at) }
                        Metric { label: "Commissions", value: count_pair(data.finished_task_num, data.total_task_num) }
                        Metric { label: "Extra commission reward", value: bool_status(data.is_extra_task_reward_received).to_owned() }
                    } else {
                        Metric { label: "Stamina", value: count_pair(data.current_stamina, data.max_stamina) }
                        Metric { label: "Stamina full at", value: display_date(&data.stamina_full_at) }
                        Metric { label: "Reserve stamina", value: data.current_reserve_stamina.map(|n| n.to_string()).unwrap_or_else(|| "Not reported".to_owned()) }
                        Metric { label: "Training", value: count_pair(data.current_train_score, data.max_train_score) }
                    }
                }
                ExpeditionList { expeditions: data.expeditions.clone().unwrap_or_default() }
            } else {
                p { "No daily summary payload returned." }
            }
        }
    }
}

#[component]
fn ExpeditionList(expeditions: Vec<Expedition>) -> Element {
    let rows: Vec<(String, String, String)> = expeditions
        .into_iter()
        .map(|expedition| {
            (
                expedition.name.unwrap_or_else(|| "Unnamed".to_owned()),
                expedition.status.unwrap_or_else(|| "unknown".to_owned()),
                expedition
                    .remained_time
                    .or(expedition.remaining_time)
                    .map(|duration| duration.to_string())
                    .unwrap_or_else(|| "not reported".to_owned()),
            )
        })
        .collect();
    rsx! {
        div { class: "expeditions",
            h3 { "Expeditions" }
            if rows.is_empty() {
                p { "Not reported" }
            } else {
                ul {
                    for (name, status, remaining) in rows {
                        li { "{name} — {status} — {remaining}" }
                    }
                }
            }
        }
    }
}

#[component]
fn LinkPage(state: Signal<AppState>) -> Element {
    rsx! {
        div { class: "page-stack",
            h2 { "HoYoLAB Link" }
            CookieCard { state }
        }
    }
}

#[component]
fn CookieCard(mut state: Signal<AppState>) -> Element {
    let mut cookie = use_signal(String::new);
    let hoyolab = state.read().hoyolab.clone();
    let onboarding = state.read().me.as_ref().map(|me| me.onboarding.clone());
    rsx! {
        Card {
            CardHeader { CardTitle { "HoYoLAB Cookie" } CardDescription { "One Dashboard user links to one HoYoLAB account. Save a replacement Cookie for the same account and ask the server to verify/sync when available." } }
            CardContent { class: "form-stack",
                dl { class: "metric-list",
                    Metric { label: "Status", value: onboarding.as_ref().map(|o| humanize(&o.cookie_status)).unwrap_or_else(|| hoyolab.as_ref().map(|h| humanize(&h.status)).unwrap_or_else(|| "Not loaded".to_owned())) }
                    Metric { label: "HoYoLAB UID", value: hoyolab.as_ref().and_then(|h| h.hoyolab_uid.clone()).unwrap_or_else(|| "Only shown after backend verification".to_owned()) }
                    Metric { label: "Verification", value: if onboarding.as_ref().is_some_and(|o| o.verification_available) || hoyolab.as_ref().is_some_and(|h| h.verification_available) { "Available".to_owned() } else { "Unavailable; ordinary UI remains blocked until server proof succeeds".to_owned() } }
                    Metric { label: "Updated", value: hoyolab.as_ref().map(|h| display_date(&h.updated_at)).unwrap_or_else(|| "Not reported".to_owned()) }
                }
                Field { id: "hoyolab-cookie", label: "Paste one HoYoLAB Cookie",
                    textarea { id: "hoyolab-cookie", autocomplete: "off", value: "{cookie}", oninput: move |event| cookie.set(event.value()) }
                }
            }
            CardFooter { class: "button-row",
                Button { r#type: "button", onclick: move |_| {
                    let secret = std::mem::take(&mut *cookie.write());
                    let state = state;
                    app::spawn_action(state, move |generation| async move {
                                match crate::api::json::<CookieSubmission>("PUT", "/me/hoyolab/cookie", Some(json!({"cookie": secret}))).await {
                            Ok(_) if app::generation_matches(state, generation) => { app::set_status(state, "Cookie stored for server verification.", "success"); app::bootstrap(state).await; }
                            Ok(_) => {}
                            Err(error) => { app::handle_api_error_for(state, generation, error); app::finish_action(state, generation); }
                        }
                    });
                }, "Save Cookie" }
                Button { variant: ButtonVariant::Secondary, r#type: "button", onclick: move |_| { let state = state; app::spawn_action(state, move |generation| async move {
                                match crate::api::send("POST", "/me/hoyolab/verify", Some(json!({}))).await {
                        Ok(()) if app::generation_matches(state, generation) => { app::set_status(state, "Verification requested.", "success"); app::bootstrap(state).await; }
                        Ok(()) => {}
                        Err(error) => { app::handle_api_error_for(state, generation, error); app::finish_action(state, generation); }
                    }
                }); }, "Verify binding" }
                Button { variant: ButtonVariant::Secondary, r#type: "button", onclick: move |_| { let state = state; app::spawn_action(state, move |generation| async move {
                                match crate::api::send("POST", "/me/hoyolab/sync", Some(json!({}))).await {
                        Ok(()) if app::generation_matches(state, generation) => { app::set_status(state, "Sync requested.", "success"); app::bootstrap(state).await; }
                        Ok(()) => {}
                        Err(error) => { app::handle_api_error_for(state, generation, error); app::finish_action(state, generation); }
                    }
                }); }, "Sync metadata" }
            }
        }
    }
}

#[component]
fn UidsPage(state: Signal<AppState>) -> Element {
    rsx! {
        div { class: "page-stack",
            h2 { "UID Visibility" }
            Card {
                CardHeader { CardTitle { "Retained game accounts" } CardDescription { "Hidden and missing UIDs stay listed here for management but are never fetched for home status." } }
                CardContent {
                    if let Some(error) = &state.read().accounts_error { p { class: "warning", "{error}" } }
                    AccountTable { state, controls: true }
                }
            }
        }
    }
}

#[component]
fn AccountTable(mut state: Signal<AppState>, controls: bool) -> Element {
    let accounts = if controls {
        state.read().accounts.clone()
    } else {
        state.read().visible_accounts()
    };
    rsx! {
        if accounts.is_empty() {
            p { "No game accounts returned." }
        } else {
            div { class: "table-wrap",
                table {
                    thead { tr { th { "Game" } th { "UID" } th { "Region" } th { "Detected" } th { "Visible" } } }
                    tbody {
                        for account in accounts {
                            tr {
                                td { "{game_name(&account.game_kind)}" }
                                td { if account.game_uid.is_empty() { "Missing" } else { "{account.game_uid}" } }
                                td { "{account.region}" }
                                td { if account.detected { "Yes" } else { "No" } }
                                td {
                                    if controls {
                                        input { r#type: "checkbox", checked: account.visible, "aria-label": "Visible {account.game_uid}", onchange: move |event| {
                                            let id = account.id;
                                            let visible = event.checked();
                                            let state = state;
                                            app::spawn_action(state, move |generation| async move {
                                match crate::api::send("PATCH", &format!("/game-accounts/{id}"), Some(json!({"visible": visible}))).await {
                                                    Ok(()) if app::generation_matches(state, generation) => { app::set_status(state, "Visibility updated.", "success"); app::load_accounts(state).await; app::finish_action(state, generation); }
                                                    Ok(()) => {}
                                                    Err(error) => { app::handle_api_error_for(state, generation, error); app::finish_action(state, generation); }
                                                }
                                            });
                                        } }
                                    } else if account.visible { "Yes" } else { "No" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn RewardsPage(state: Signal<AppState>) -> Element {
    rsx! {
        div { class: "page-stack",
            h2 { "Rewards" }
            if !state.read().me.as_ref().is_some_and(|me| me.capabilities.reward_posting_available) {
                p { class: "warning", "Reward posting is not available on this server. Settings and history are shown only when the API exposes them; no UI state is treated as proof of availability." }
            }
            if let Some(error) = &state.read().rewards_error { p { class: "warning", "{error}" } }
            div { class: "grid two",
                for setting in state.read().rewards.clone() {
                    RewardSettingCard { key: "{setting.id}", state, setting }
                }
            }
            RewardHistory { state }
        }
    }
}

#[component]
fn RewardSettingCard(mut state: Signal<AppState>, setting: RewardSetting) -> Element {
    let mut enabled = use_signal(|| setting.enabled);
    let mut local_time = use_signal(|| setting.local_time.clone());
    rsx! {
        Card {
            CardHeader { CardTitle { "{game_name(&setting.game_kind)}" } CardDescription { "Default is off. Manual runs enqueue server work and return accepted asynchronously." } }
            CardContent { class: "form-stack",
                label { class: "checkbox-row", input { r#type: "checkbox", checked: enabled(), onchange: move |event| enabled.set(event.checked()) } "Enabled" }
                Field { id: "reward-time-{setting.id}", label: "Local run time",
                    input { id: "reward-time-{setting.id}", r#type: "time", value: "{local_time}", oninput: move |event| local_time.set(event.value()) }
                }
                dl { class: "metric-list",
                    Metric { label: "Auth status", value: humanize(&setting.auth_status) }
                    Metric { label: "Current reward", value: bool_status(setting.is_sign).to_owned() }
                    Metric { label: "Checked", value: display_date(&setting.last_checked_at) }
                    Metric { label: "Next run", value: display_date(&setting.next_run_at) }
                    Metric { label: "Last run", value: setting.last_run.as_ref().map(|run| humanize(&run.state)).unwrap_or_else(|| "Never".to_owned()) }
                    Metric { label: "Error", value: setting.error_code.clone().unwrap_or_else(|| "None".to_owned()) }
                }
            }
            CardFooter { class: "button-row",
                Button { r#type: "button", onclick: move |_| {
                    let id = setting.id;
                    let body = json!({"enabled": enabled(), "local_time": if local_time().is_empty() { "06:00".to_owned() } else { local_time() }});
                    let state = state;
                    app::spawn_action(state, move |generation| async move {
                                match crate::api::send("PATCH", &format!("/reward-settings/{id}"), Some(body)).await {
                            Ok(()) if app::generation_matches(state, generation) => { app::set_status(state, "Reward setting saved.", "success"); app::load_rewards(state).await; app::finish_action(state, generation); }
                            Ok(()) => {}
                            Err(error) => { app::handle_api_error_for(state, generation, error); app::finish_action(state, generation); }
                        }
                    });
                }, "Save" }
                Button { variant: ButtonVariant::Secondary, r#type: "button", onclick: move |_| {
                    let id = setting.id;
                    let state = state;
                    app::spawn_action(state, move |generation| async move {
                                match crate::api::send("POST", &format!("/reward-settings/{id}/run"), Some(json!({}))).await {
                            Ok(()) if app::generation_matches(state, generation) => { app::set_status(state, "Manual reward run queued.", "success"); app::load_rewards(state).await; app::finish_action(state, generation); }
                            Ok(()) => {}
                            Err(error) => { app::handle_api_error_for(state, generation, error); app::finish_action(state, generation); }
                        }
                    });
                }, "Run now" }
            }
        }
    }
}

#[component]
fn RewardHistory(state: Signal<AppState>) -> Element {
    let rows = state.read().reward_runs.clone();
    rsx! {
        Card {
            CardHeader { CardTitle { "Reward history: last 5 days" } CardDescription { "Attempts show backend outcomes without exposing secrets." } }
            CardContent {
                if let Some(error) = &state.read().reward_runs_error { p { class: "warning", "{error}" } }
                if rows.is_empty() {
                    p { "No reward runs returned for the last 5 days." }
                } else {
                    div { class: "table-wrap",
                        table {
                            thead { tr { th { "Game" } th { "Trigger" } th { "State" } th { "Created" } th { "Attempts" } } }
                            tbody { for run in rows { tr {
                                td { "{game_name(&run.game_kind)}" }
                                td { "{humanize(&run.trigger)}" }
                                td { "{humanize(&run.state)}" }
                                td { "{run.created_at}" }
                                td { "{attempt_summary(&run)}" }
                            } } }
                        }
                    }
                }
            }
        }
    }
}

fn attempt_summary(run: &RewardRun) -> String {
    let outcomes = run
        .attempts
        .iter()
        .map(|attempt| humanize(&attempt.outcome))
        .collect::<Vec<_>>()
        .join(", ");
    if outcomes.is_empty() {
        run.attempt_count.to_string()
    } else {
        format!("{} ({outcomes})", run.attempt_count)
    }
}

#[component]
fn SecurityPage(state: Signal<AppState>) -> Element {
    rsx! {
        div { class: "page-stack",
            h2 { "Security" }
            div { class: "grid two",
                PasswordCard { state, title: "Change password" }
                PasskeyCard { state }
            }
        }
    }
}

#[component]
fn PasswordCard(mut state: Signal<AppState>, title: &'static str) -> Element {
    let mut current = use_signal(String::new);
    let mut new_password = use_signal(String::new);
    rsx! {
        Card {
            CardHeader { CardTitle { "{title}" } CardDescription { "Password changes rotate the server session and refresh onboarding metadata." } }
            CardContent { class: "form-stack",
                Field { id: "current-password", label: "Current password", input { id: "current-password", r#type: "password", autocomplete: "current-password", value: "{current}", oninput: move |event| current.set(event.value()) } }
                Field { id: "new-password", label: "New password", input { id: "new-password", r#type: "password", autocomplete: "new-password", value: "{new_password}", minlength: "12", oninput: move |event| new_password.set(event.value()) } }
            }
            CardFooter { Button { r#type: "button", onclick: move |_| {
                let current_password = std::mem::take(&mut *current.write());
                let next = std::mem::take(&mut *new_password.write());
                let state = state;
                app::spawn_action(state, move |generation| async move {
                                match crate::api::send("PUT", "/me/password", Some(json!({"current_password": current_password, "new_password": next}))).await {
                        Ok(()) if app::generation_matches(state, generation) => { app::set_status(state, "Password changed.", "success"); app::bootstrap(state).await; }
                        Ok(()) => {}
                        Err(error) => { app::handle_api_error_for(state, generation, error); app::finish_action(state, generation); }
                    }
                });
            }, "Change password" } }
        }
    }
}

#[component]
fn PasskeyCard(mut state: Signal<AppState>) -> Element {
    let mut label = use_signal(String::new);
    rsx! {
        Card {
            CardHeader { CardTitle { "Passkeys" } CardDescription { "Optional browser passkeys can be added after password setup. Password sign-in remains available." } }
            CardContent { class: "form-stack",
                if let Some(error) = &state.read().passkeys_error { p { class: "warning", "{error}" } }
                Field { id: "passkey-label", label: "New passkey label", input { id: "passkey-label", maxlength: "100", autocomplete: "off", value: "{label}", oninput: move |event| label.set(event.value()) } }
                if state.read().passkeys.is_empty() {
                    p { "No passkeys registered." }
                } else {
                    ul { class: "item-list",
                        for passkey in state.read().passkeys.clone() {
                            li { span { "{passkey.label} — created {passkey.created_at}" } Button { variant: ButtonVariant::Destructive, r#type: "button", onclick: move |_| { let id = passkey.id; let state = state; app::spawn_action(state, move |generation| async move {
                                match crate::api::send("DELETE", &format!("/me/passkeys/{id}"), None).await {
                                    Ok(()) if app::generation_matches(state, generation) => { app::set_status(state, "Passkey deleted.", "success"); app::load_passkeys(state).await; app::finish_action(state, generation); }
                                    Ok(()) => {}
                                    Err(error) => { app::handle_api_error_for(state, generation, error); app::finish_action(state, generation); }
                                }
                            }); }, "Delete" } }
                        }
                    }
                }
            }
            CardFooter { Button { variant: ButtonVariant::Secondary, r#type: "button", onclick: move |_| {
                let label_value = std::mem::take(&mut *label.write());
                let state = state;
                app::spawn_action(state, move |generation| async move {
                                match crate::webauthn::register(&label_value).await {
                        Ok(()) if app::generation_matches(state, generation) => { app::set_status(state, "Passkey registered.", "success"); app::load_passkeys(state).await; app::finish_action(state, generation); }
                        Ok(()) => {}
                        Err(error) => { app::handle_api_error_for(state, generation, error); app::finish_action(state, generation); }
                    }
                });
            }, "Register passkey" } }
        }
    }
}

#[component]
fn AdminPage(state: Signal<AppState>) -> Element {
    rsx! { div { class: "page-stack", h2 { "Admin" } AdminCard { state, compact: false } } }
}

#[component]
fn AdminCard(mut state: Signal<AppState>, compact: bool) -> Element {
    let mut username = use_signal(String::new);
    let mut temp_password = use_signal(String::new);
    rsx! {
        Card {
            CardHeader { CardTitle { if compact { "Admin management" } else { "Users" } } CardDescription { "Create users, set temporary recovery passwords, disable/re-enable accounts, or request deletion with exact confirmation." } }
            CardContent { class: "form-stack",
                if let Some(error) = &state.read().admin_error { p { class: "warning", "{error}" } }
                div { class: "inline-form",
                    Field { id: "admin-create-username", label: "Username", input { id: "admin-create-username", autocomplete: "off", value: "{username}", oninput: move |event| username.set(event.value()) } }
                    Field { id: "admin-create-temp", label: "Temporary password", input { id: "admin-create-temp", r#type: "password", autocomplete: "new-password", value: "{temp_password}", oninput: move |event| temp_password.set(event.value()) } }
                    Button { r#type: "button", onclick: move |_| {
                        let user = std::mem::take(&mut *username.write());
                        let temp = std::mem::take(&mut *temp_password.write());
                        let state = state;
                        app::spawn_action(state, move |generation| async move {
                                match crate::api::json::<UserView>("POST", "/admin/users", Some(json!({"username": user, "temporary_password": temp}))).await {
                                Ok(_) if app::generation_matches(state, generation) => { app::set_status(state, "User created.", "success"); app::load_admin_users(state).await; app::finish_action(state, generation); }
                                Ok(_) => {}
                                Err(error) => { app::handle_api_error_for(state, generation, error); app::finish_action(state, generation); }
                            }
                        });
                    }, "Create user" }
                }
                AdminTable { state }
            }
        }
    }
}

#[component]
fn AdminTable(state: Signal<AppState>) -> Element {
    rsx! {
        div { class: "table-wrap",
            table {
                thead { tr { th { "Username" } th { "Admin" } th { "Active" } th { "Recovery" } th { "Delete" } } }
                tbody { for user in state.read().admin_users.clone() { AdminUserRow { key: "{user.id}", state, user } } }
            }
        }
    }
}

#[component]
fn AdminUserRow(mut state: Signal<AppState>, user: UserView) -> Element {
    let mut temp = use_signal(String::new);
    let mut current = use_signal(String::new);
    let mut confirmation = use_signal(String::new);
    rsx! {
        tr {
            td { "{user.username}" }
            td { if user.is_admin { "Yes" } else { "No" } }
            td { Button { variant: ButtonVariant::Secondary, r#type: "button", onclick: move |_| { let id = user.id; let active = !user.active; let state = state; app::spawn_action(state, move |generation| async move {
                                match crate::api::send("PATCH", &format!("/admin/users/{id}"), Some(json!({"active": active}))).await {
                    Ok(()) if app::generation_matches(state, generation) => { app::set_status(state, "User updated.", "success"); if app::refresh_me(state).await { app::load_admin_users(state).await; } app::finish_action(state, generation); }
                    Ok(()) => {}
                    Err(error) => { app::handle_api_error_for(state, generation, error); app::finish_action(state, generation); }
                }
            }); }, if user.active { "Disable" } else { "Re-enable" } } }
            td { class: "cell-form",
                input { r#type: "password", autocomplete: "new-password", "aria-label": "Temporary password for {user.username}", value: "{temp}", oninput: move |event| temp.set(event.value()) }
                Button { variant: ButtonVariant::Secondary, r#type: "button", onclick: move |_| { let secret = std::mem::take(&mut *temp.write()); let id = user.id; let state = state; app::spawn_action(state, move |generation| async move {
                                match crate::api::send("PUT", &format!("/admin/users/{id}/temporary-password"), Some(json!({"temporary_password": secret}))).await {
                        Ok(()) if app::generation_matches(state, generation) => { app::set_status(state, "Recovery password set.", "success"); if app::refresh_me(state).await { app::load_admin_users(state).await; } app::finish_action(state, generation); }
                        Ok(()) => {}
                        Err(error) => { app::handle_api_error_for(state, generation, error); app::finish_action(state, generation); }
                    }
                }); }, "Set recovery password" }
            }
            td { class: "cell-form danger-zone",
                input { r#type: "password", autocomplete: "current-password", "aria-label": "Your current password to delete {user.username}", value: "{current}", oninput: move |event| current.set(event.value()) }
                input { autocomplete: "off", "aria-label": "Type username to delete {user.username}", value: "{confirmation}", oninput: move |event| confirmation.set(event.value()) }
                Button { variant: ButtonVariant::Destructive, r#type: "button", onclick: move |_| { let pass = std::mem::take(&mut *current.write()); let confirm = std::mem::take(&mut *confirmation.write()); let id = user.id; let state = state; app::spawn_action(state, move |generation| async move {
                                match crate::api::send("DELETE", &format!("/admin/users/{id}"), Some(json!({"current_password": pass, "confirmation": confirm}))).await {
                        Ok(()) if app::generation_matches(state, generation) => { app::set_status(state, "User deletion accepted.", "success"); if app::refresh_me(state).await { app::load_admin_users(state).await; } app::finish_action(state, generation); }
                        Ok(()) => {}
                        Err(error) => { app::handle_api_error_for(state, generation, error); app::finish_action(state, generation); }
                    }
                }); }, "Delete user" }
            }
        }
    }
}

#[component]
fn DeletePage(state: Signal<AppState>) -> Element {
    rsx! {
        div { class: "page-stack",
            h2 { "Delete your account" }
            DeleteAccountCard { state }
        }
    }
}

#[component]
fn DeleteAccountCard(mut state: Signal<AppState>) -> Element {
    let username = state
        .read()
        .me
        .as_ref()
        .map(|me| me.username.clone())
        .unwrap_or_default();
    let mut password = use_signal(String::new);
    let mut confirmation = use_signal(String::new);
    rsx! {
        Card { class: "danger-zone",
            CardHeader { CardTitle { "Irreversible account deletion" } CardDescription { "Type your username exactly and enter your current password. The backend protects the last active admin." } }
            CardContent { class: "form-stack",
                Field { id: "delete-confirmation", label: "Type username: {username}", input { id: "delete-confirmation", autocomplete: "off", value: "{confirmation}", oninput: move |event| confirmation.set(event.value()) } }
                Field { id: "delete-password", label: "Current password", input { id: "delete-password", r#type: "password", autocomplete: "current-password", value: "{password}", oninput: move |event| password.set(event.value()) } }
            }
            CardFooter { Button { variant: ButtonVariant::Destructive, r#type: "button", onclick: move |_| { let pass = std::mem::take(&mut *password.write()); let confirm = std::mem::take(&mut *confirmation.write()); let state = state; app::spawn_action(state, move |generation| async move {
                                match crate::api::send("DELETE", "/me", Some(json!({"current_password": pass, "confirmation": confirm}))).await {
                    Ok(()) if app::generation_matches(state, generation) => app::signed_out(state, "Account deletion accepted."),
                    Ok(()) => {}
                    Err(error) => { app::handle_api_error_for(state, generation, error); app::finish_action(state, generation); }
                }
            }); }, "Delete my account" } }
        }
    }
}

#[component]
pub(crate) fn DocsPage(mut state: Signal<AppState>) -> Element {
    use_future(move || async move {
        app::load_docs(state).await;
    });
    let doc = state.read().openapi.clone();
    rsx! {
        main { class: "page-wrap docs-page",
            Header { state }
            Status { state }
            Card {
                CardHeader { CardTitle { "API documentation" } CardDescription { "Read-only OpenAPI summary. This page has no try-it console and sends no credential-bearing API calls other than fetching /openapi.json." } }
                CardContent {
                    if let Some(error) = &state.read().openapi_error { p { class: "warning", "{error}" } }
                    if let Some(doc) = doc {
                        OpenApiSummary { doc }
                    } else {
                        p { "Loading OpenAPI document…" }
                    }
                }
            }
        }
    }
}

#[component]
fn OpenApiSummary(doc: Value) -> Element {
    let title = doc
        .pointer("/info/title")
        .and_then(Value::as_str)
        .unwrap_or("Dashboard API")
        .to_owned();
    let version = doc
        .pointer("/info/version")
        .and_then(Value::as_str)
        .unwrap_or("unknown")
        .to_owned();
    let paths: Vec<(String, Vec<String>)> = doc
        .get("paths")
        .and_then(Value::as_object)
        .map(|map| {
            let mut rows: Vec<_> = map
                .iter()
                .map(|(path, methods)| {
                    let methods = methods
                        .as_object()
                        .map(|obj| obj.keys().map(|k| k.to_uppercase()).collect())
                        .unwrap_or_default();
                    (path.clone(), methods)
                })
                .collect();
            rows.sort_by(|a, b| a.0.cmp(&b.0));
            rows
        })
        .unwrap_or_default();
    rsx! {
        div { class: "docs-summary",
            h2 { "{title}" }
            p { "Version {version}" }
            div { class: "table-wrap",
                table {
                    thead { tr { th { "Path" } th { "Methods" } } }
                    tbody { for (path, methods) in paths { tr { td { code { "{path}" } } td { "{methods_text(&methods)}" } } } }
                }
            }
        }
    }
}

fn methods_text(methods: &[String]) -> String {
    methods.join(", ")
}

#[component]
fn Field(id: String, label: String, children: Element) -> Element {
    rsx! { div { class: "field", label { r#for: "{id}", "{label}" } {children} } }
}

#[component]
fn Metric(label: &'static str, value: String) -> Element {
    rsx! { div { class: "metric", dt { "{label}" } dd { "{value}" } } }
}
