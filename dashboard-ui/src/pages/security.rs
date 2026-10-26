use super::common::{Field, LoadError, Metric, ModalDialog, PasswordInput, focus_element};
use crate::app::actions::{handle_api_error_for, handle_password_error_for, spawn_action};
use crate::app::credentials::{self, CopyState, CredentialResult};
use crate::app::data::{load_api_tokens, load_passkeys, load_sessions};
use crate::app::display::{browser_label, datetime_text};
use crate::app::preferences::use_preferences;
use crate::app::session::refresh_gate;
use crate::app::state::{AppState, generation_matches, set_status};
use crate::components::{Button, ButtonVariant};
use crate::models::{ApiTokenView, IssuedApiToken, PasskeyView, SessionView};
use dioxus::prelude::*;
use serde_json::{Value, json};

#[component]
pub(super) fn SecurityPage(state: Signal<AppState>) -> Element {
    let ui = use_preferences();
    let server = state.read().server.clone();
    rsx! {
        div { class: "page-stack",
            h2 { {ui.text("security.security")} }
            // The app leaves password change and API tokens to the web version.
            if !crate::platform::APP {
                PasswordCard { state, title: ui.text("auth.change_password") }
            }
            PasskeyCard { state }
            section { class: "panel page-stack",
                h3 { {ui.text("security.sessions")} }
                dl { class: "metric-list",
                    Metric { label: ui.text("shell.session_lifetime"), value: server.as_ref().map(|s| ui.format("session.duration_days", &[("count", &s.session_ttl_days.to_string())])).unwrap_or_else(|| ui.text("common.not_fetched").to_owned()) }
                }
                SessionList { state }
            }
            if !crate::platform::APP {
                ApiTokenCard { state }
            }
        }
    }
}

/// The API rejects names that are empty after trimming, longer than 100 characters, or
/// contain control characters.
fn api_token_name_ok(name: &str) -> bool {
    let name = name.trim();
    (1..=100).contains(&name.chars().count()) && !name.chars().any(char::is_control)
}

fn api_token_scope_key(scope: &str) -> &'static str {
    if scope == "run" {
        "security.api_token_scope_run"
    } else {
        "security.api_token_scope_read"
    }
}

/// Personal API tokens for external tools; the plaintext is shown once after issuance.
#[component]
fn ApiTokenCard(mut state: Signal<AppState>) -> Element {
    let ui = use_preferences();
    let mut name = use_signal(String::new);
    let mut scope = use_signal(|| "read".to_owned());
    let mut submitted = use_signal(|| false);
    let mut issued = use_signal(|| None::<CredentialResult>);
    let mut revoking = use_signal(|| None::<ApiTokenView>);
    use_drop(move || {
        name.write().clear();
    });
    let error = state.read().api_tokens_error.clone();
    let tokens = state.read().api_tokens.clone();
    let name_error = submitted() && !api_token_name_ok(&name.read());
    let at_limit = tokens.as_ref().is_some_and(|tokens| tokens.len() >= 10);
    rsx! {
        section { class: "panel",
            h3 { {ui.text("security.api_tokens")} }
            p { class: "field-hint", {ui.text("security.api_tokens_hint")} }
            if let Some(error) = error {
                LoadError { state, message: ui.text(&error) }
            } else if let Some(tokens) = tokens {
                if tokens.is_empty() {
                    p { {ui.text("security.no_api_tokens")} }
                } else {
                    ul { class: "item-list",
                        for token in tokens {
                            li { key: "{token.id}",
                                div {
                                    strong { "{token.name}" }
                                    " " span { class: "status", {ui.text(api_token_scope_key(&token.scope))} }
                                    p { class: "field-hint",
                                        {ui.text("security.created")} " " time { datetime: "{token.created_at}", {datetime_text(Some(&token.created_at), ui)} }
                                        " · " {ui.text("security.last_used")} " "
                                        if token.last_used_at.is_some() { {datetime_text(token.last_used_at.as_deref(), ui)} } else { {ui.text("security.never_used")} }
                                    }
                                }
                                Button { variant: ButtonVariant::Destructive, r#type: "button", id: "api-token-revoke-{token.id}",
                                    "aria-label": ui.format("security.revoke_api_token_label", &[("name", &token.name)]),
                                    onclick: { let target = token.clone(); move |_| { if !state.read().busy { revoking.set(Some(target.clone())); } } },
                                    {ui.text("security.revoke")}
                                }
                            }
                        }
                    }
                }
            } else {
                p { role: "status", {ui.text("security.loading_api_tokens")} }
            }
            form { novalidate: true, onsubmit: move |event| {
                event.prevent_default();
                if state.read().busy { return; }
                if !api_token_name_ok(&name.read()) {
                    submitted.set(true);
                    focus_element("api-token-name");
                    return;
                }
                let body = json!({"name": name.read().trim(), "scope": scope.read().clone()});
                name.write().clear();
                submitted.set(false);
                spawn_action(state, move |generation| async move {
                    match crate::api::json::<IssuedApiToken>("POST", "/me/api-tokens", Some(body)).await {
                        Ok(token) if generation_matches(state, generation) => {
                            issued.set(Some(CredentialResult { username: token.name, secret: token.token, generation, copy: CopyState::Ready }));
                            load_api_tokens(state).await;
                        }
                        Ok(_) => {}
                        Err(error) => handle_api_error_for(state, generation, error),
                    }
                });
            },
                div { class: "form-grid",
                    Field { id: "api-token-name", label: ui.text("security.api_token_name"),
                        input { id: "api-token-name", maxlength: "100", autocomplete: "off", value: "{name}",
                            "aria-describedby": if name_error { "api-token-name-hint api-token-name-error" } else { "api-token-name-hint" },
                            "aria-invalid": if name_error { "true" } else { "false" },
                            oninput: move |event| name.set(event.value()) }
                        p { id: "api-token-name-hint", class: "field-hint", {ui.text("security.api_token_name_hint")} }
                        if name_error { p { id: "api-token-name-error", class: "field-error", {ui.text("security.api_token_name_error")} } }
                    }
                    Field { id: "api-token-scope", label: ui.text("security.api_token_scope"),
                        select { id: "api-token-scope", value: "{scope}", onchange: move |event| scope.set(event.value()),
                            option { value: "read", {ui.text("security.api_token_scope_read")} }
                            option { value: "run", {ui.text("security.api_token_scope_run")} }
                        }
                    }
                }
                if at_limit { p { class: "field-hint", {ui.text("security.api_token_limit_reached")} } }
                div { class: "auth-submit",
                    Button { variant: ButtonVariant::Secondary, r#type: "submit", disabled: at_limit, {ui.text("security.issue_api_token")} }
                }
            }
            if issued.read().is_some() {
                ApiTokenIssuedDialog { state, issued }
            }
            if let Some(token) = revoking() {
                RevokeApiTokenDialog { state, token, revoking }
            }
        }
    }
}

#[component]
fn ApiTokenIssuedDialog(
    state: Signal<AppState>,
    mut issued: Signal<Option<CredentialResult>>,
) -> Element {
    let ui = use_preferences();
    // The plaintext leaves memory with the dialog, whichever way it closes.
    use_drop(move || {
        if let Ok(mut value) = issued.try_write() {
            *value = None;
        }
    });
    rsx! {
        ModalDialog { id: "api-token-issued", title: ui.text("security.api_token_issued"), onclose: move |_| issued.set(None),
            if let Some(value) = issued.read().as_ref() {
                dl { class: "metric-list", dt { {ui.text("security.api_token_name")} } dd { "{value.username}" } }
                Field { id: "api-token-secret", label: ui.text("security.api_token"),
                    output { id: "api-token-secret", class: "generated-password", "aria-describedby": "api-token-secret-note", "{value.secret}" }
                    p { id: "api-token-secret-note", class: "field-hint", {ui.text("security.api_token_shown_once")} }
                }
                p { role: "status", "aria-live": "polite",
                    {match value.copy {
                        CopyState::Ready => "",
                        CopyState::Pending => ui.text("admin.copying"),
                        CopyState::Copied => ui.text("admin.copied"),
                        CopyState::Failed => ui.text("admin.could_not_copy_try_again"),
                    }}
                }
                div { class: "actions",
                    Button { variant: ButtonVariant::Secondary, r#type: "button", disabled: value.copy == CopyState::Pending, onclick: move |_| {
                        let generation = state.read().generation;
                        if !generation_matches(state, generation) { issued.set(None); return; }
                        let candidate = {
                            let mut write = issued.write();
                            let Some(value) = write.as_mut() else { return; };
                            if value.generation != generation || value.copy == CopyState::Pending { return; }
                            value.copy = CopyState::Pending;
                            value.secret.clone()
                        };
                        // Scope-bound task is cancelled when this dialog unmounts.
                        spawn(async move {
                            let outcome = credentials::copy(&candidate).await;
                            if !generation_matches(state, generation) { return; }
                            if let Some(value) = issued.write().as_mut().filter(|value| value.generation == generation) {
                                value.copy = CopyState::after_write(outcome);
                            }
                        });
                    }, {ui.text("admin.copy")} }
                    Button { r#type: "button", onclick: move |_| issued.set(None), {ui.text("admin.close")} }
                }
            }
        }
    }
}

#[component]
fn RevokeApiTokenDialog(
    state: Signal<AppState>,
    token: ApiTokenView,
    mut revoking: Signal<Option<ApiTokenView>>,
) -> Element {
    let ui = use_preferences();
    let id = token.id;
    rsx! {
        ModalDialog { id: "api-token-revoke-dialog", title: ui.text("security.revoke_api_token_title"), onclose: move |_| revoking.set(None),
            p { {ui.format("security.revoke_api_token_consequences", &[("name", &token.name)])} }
            div { class: "actions",
                Button { variant: ButtonVariant::Secondary, r#type: "button", onclick: move |_| revoking.set(None), {ui.text("admin.cancel")} }
                Button { variant: ButtonVariant::Destructive, r#type: "button", onclick: move |_| {
                    revoking.set(None);
                    spawn_action(state, move |generation| async move {
                        match crate::api::send("DELETE", &format!("/me/api-tokens/{id}"), None).await {
                            Ok(()) if generation_matches(state, generation) => {
                                set_status(state, "security.api_token_revoked", "success");
                                load_api_tokens(state).await;
                            }
                            Ok(()) => {}
                            Err(error) => handle_api_error_for(state, generation, error),
                        }
                    });
                }, {ui.text("security.revoke")} }
            }
        }
    }
}

// Only the mandatory setup path may omit current_password. Never retain a login secret.
fn password_body(mandatory: bool, current: String, next: String) -> Value {
    if mandatory {
        json!({"new_password": next})
    } else {
        json!({"current_password": current, "new_password": next})
    }
}

/// The backend counts UTF-8 bytes (`auth::password`), not characters.
fn password_length_ok(next: &str) -> bool {
    (12..=256).contains(&next.len())
}

fn valid_password_change(mandatory: bool, current: &str, next: &str, confirm: &str) -> bool {
    (mandatory || !current.is_empty()) && password_length_ok(next) && next == confirm
}

#[component]
pub(super) fn PasswordCard(mut state: Signal<AppState>, title: &'static str) -> Element {
    let ui = use_preferences();
    let mut current = use_signal(String::new);
    let mut new_password = use_signal(String::new);
    let mut confirmation = use_signal(String::new);
    // Inline errors appear after leaving a filled field or after a submit attempt.
    let mut submitted = use_signal(|| false);
    let mut left_new = use_signal(|| false);
    let mut left_confirm = use_signal(|| false);
    let mandatory = state
        .read()
        .me
        .as_ref()
        .is_some_and(|me| me.onboarding.password_change_required);
    use_drop(move || {
        current.write().clear();
        new_password.write().clear();
        confirmation.write().clear();
    });
    let next_bytes = new_password.read().len();
    let length_error = !password_length_ok(&new_password.read())
        && (submitted() || (left_new() && next_bytes > 0));
    let mismatch_error = *confirmation.read() != *new_password.read()
        && (submitted() || (left_confirm() && !confirmation.read().is_empty()));
    let current_error = !mandatory && submitted() && current.read().is_empty();
    let new_described = if length_error {
        "new-password-rules new-password-error"
    } else {
        "new-password-rules"
    }
    .to_owned();
    rsx! {
        section { class: "panel",
            h3 { "{title}" }
            form { novalidate: true, onsubmit: move |event| {
                event.prevent_default();
                if state.read().busy { return; }
                // Read the current server gate at submission, not a captured setup reason.
                let mandatory = state.read().me.as_ref().is_some_and(|me| me.onboarding.password_change_required);
                if !valid_password_change(mandatory, &current.read(), &new_password.read(), &confirmation.read()) {
                    // Keep the typed values so the inline errors can be fixed in place.
                    submitted.set(true);
                    set_status(state, "security.invalid_password_change", "error");
                    let first = if !mandatory && current.read().is_empty() { "current-password" }
                        else if !password_length_ok(&new_password.read()) { "new-password" }
                        else { "confirm-password" };
                    focus_element(first);
                    return;
                }
                let current_password = std::mem::take(&mut *current.write());
                let next = std::mem::take(&mut *new_password.write());
                confirmation.write().clear();
                submitted.set(false);
                left_new.set(false);
                left_confirm.set(false);
                let body = password_body(mandatory, current_password, next);
                spawn_action(state, move |generation| async move {
                    match crate::api::send("PUT", "/me/password", Some(body)).await {
                        Ok(()) if generation_matches(state, generation) => {
                            // The change may open the setup gate; refresh it in place.
                            set_status(state, "security.password_changed", "success");
                            refresh_gate(state, generation).await;
                        }
                        Ok(()) => {}
                        Err(error) if mandatory => { handle_api_error_for(state, generation, error); }
                        Err(error) => { handle_password_error_for(state, generation, error).await; }
                    }
                });
            },
                div { class: "form-grid",
                    if !mandatory {
                        Field { id: "current-password", label: ui.text("deletion.current_password"),
                            PasswordInput { id: "current-password", value: current, autocomplete: "current-password",
                                describedby: current_error.then(|| "current-password-error".to_owned()), invalid: current_error }
                            if current_error { p { id: "current-password-error", class: "field-error", {ui.text("security.current_password_required")} } }
                        }
                    }
                    Field { id: "new-password", label: ui.text("security.new_password"),
                        PasswordInput { id: "new-password", value: new_password, autocomplete: "new-password",
                            describedby: new_described,
                            invalid: length_error, onblur: move |_| left_new.set(true) }
                        p { id: "new-password-rules", class: "field-hint", {ui.text("security.password_rules")} }
                        if length_error {
                            p { id: "new-password-error", class: "field-error",
                                if next_bytes == 0 { {ui.text("security.new_password_required")} }
                                else { {ui.format("security.password_length_error", &[("count", &next_bytes.to_string())])} }
                            }
                        }
                    }
                    Field { id: "confirm-password", label: ui.text("security.confirm_new_password"),
                        PasswordInput { id: "confirm-password", value: confirmation, autocomplete: "new-password",
                            describedby: mismatch_error.then(|| "confirm-password-error".to_owned()),
                            invalid: mismatch_error, onblur: move |_| left_confirm.set(true) }
                        if mismatch_error { p { id: "confirm-password-error", class: "field-error", {ui.text("security.password_mismatch")} } }
                    }
                }
                div { class: "auth-submit",
                    Button { r#type: "submit", {ui.text("auth.change_password")} }
                }
            }
        }
    }
}

#[component]
pub(super) fn PasskeyCard(mut state: Signal<AppState>) -> Element {
    let ui = use_preferences();
    let mut label = use_signal(String::new);
    use_drop(move || {
        label.write().clear();
    });
    let mut confirming = use_signal(|| None::<PasskeyView>);
    let mut renaming = use_signal(|| None::<PasskeyView>);
    let error = state.read().passkeys_error.clone();
    let passkeys = state.read().passkeys.clone();
    rsx! {
        section { class: "panel",
            h3 { {ui.text("security.passkeys")} }
            if let Some(error) = error {
                LoadError { state, message: ui.text(&error) }
            } else if let Some(passkeys) = passkeys {
                if passkeys.is_empty() {
                    p { {ui.text("security.no_passkeys_registered")} }
                } else {
                ul { class: "item-list",
                    for (passkey, target) in passkeys.into_iter().map(|passkey| (passkey.clone(), passkey)) {
                        li { key: "{passkey.id}",
                            div {
                                strong { "{passkey.label}" }
                                p { class: "field-hint",
                                    {ui.text("security.created")} " " time { datetime: "{passkey.created_at}", {datetime_text(Some(&passkey.created_at), ui)} }
                                    " · " {ui.text("security.last_used")} " "
                                    if passkey.last_used_at.is_some() { {datetime_text(passkey.last_used_at.as_deref(), ui)} } else { {ui.text("security.never_used")} }
                                }
                            }
                            div { class: "button-row",
                                Button { variant: ButtonVariant::Secondary, r#type: "button", id: "passkey-rename-{passkey.id}",
                                    "aria-label": ui.format("security.rename_passkey_label", &[("label", &passkey.label)]),
                                    onclick: { let target = target.clone(); move |_| { if !state.read().busy { renaming.set(Some(target.clone())); } } },
                                    {ui.text("security.rename")}
                                }
                                Button { variant: ButtonVariant::Destructive, r#type: "button", id: "passkey-delete-{passkey.id}",
                                    "aria-label": ui.format("security.delete_passkey_label", &[("label", &passkey.label)]),
                                    onclick: move |_| { if !state.read().busy { confirming.set(Some(target.clone())); } },
                                    {ui.text("security.delete")}
                                }
                            }
                        }
                    }
                }
                }
            } else {
                p { role: "status", {ui.text("security.loading_passkeys")} }
            }
            // WebAuthn needs a browser origin; the app lists, renames and deletes passkeys only.
            if !crate::platform::APP {
                form { onsubmit: move |event| {
                    event.prevent_default();
                    if state.read().busy { return; }
                    // An empty name registers under a generic one the user can rename later.
                    let typed = std::mem::take(&mut *label.write());
                    let label_value = if typed.trim().is_empty() { ui.text("security.default_passkey_label").to_owned() } else { typed };
                    spawn_action(state, move |generation| async move {
                        match crate::webauthn::register(&label_value).await {
                            Ok(()) if generation_matches(state, generation) => {
                                set_status(state, "security.passkey_registered", "success");
                                load_passkeys(state).await;
                            }
                            Ok(()) => {}
                            Err(error) => { handle_api_error_for(state, generation, error); }
                        }
                    });
                },
                    Field { id: "passkey-label", label: ui.text("security.new_passkey_label"),
                        input { id: "passkey-label", maxlength: "100", autocomplete: "off", "aria-describedby": "passkey-label-hint", value: "{label}", oninput: move |event| label.set(event.value()) }
                        p { id: "passkey-label-hint", class: "field-hint", {ui.text("security.passkey_label_hint")} }
                    }
                    div { class: "auth-submit",
                        Button { variant: ButtonVariant::Secondary, r#type: "submit", {ui.text("security.register_passkey")} }
                    }
                }
            }
            if let Some(passkey) = confirming() {
                DeletePasskeyDialog { state, passkey, confirming }
            }
            if let Some(passkey) = renaming() {
                RenamePasskeyDialog { state, passkey, renaming }
            }
        }
    }
}

#[component]
fn RenamePasskeyDialog(
    state: Signal<AppState>,
    passkey: PasskeyView,
    mut renaming: Signal<Option<PasskeyView>>,
) -> Element {
    let ui = use_preferences();
    let mut name = use_signal(|| passkey.label.clone());
    let id = passkey.id;
    let valid = !name().trim().is_empty() && name().len() <= 100;
    rsx! {
        ModalDialog { id: "passkey-rename-dialog", title: ui.text("security.rename_passkey_title"), onclose: move |_| renaming.set(None),
            form { class: "form-stack", onsubmit: move |event| {
                event.prevent_default();
                let label = name().trim().to_owned();
                if label.is_empty() || state.read().busy { return; }
                renaming.set(None);
                spawn_action(state, move |generation| async move {
                    match crate::api::send("PATCH", &format!("/me/passkeys/{id}"), Some(json!({"label": label}))).await {
                        Ok(()) if generation_matches(state, generation) => {
                            set_status(state, "security.passkey_renamed", "success");
                            load_passkeys(state).await;
                        }
                        Ok(()) => {}
                        Err(error) => handle_api_error_for(state, generation, error),
                    }
                });
            },
                Field { id: "passkey-rename-input", label: ui.text("security.passkey_name"),
                    input { id: "passkey-rename-input", maxlength: "100", autocomplete: "off", value: "{name}", oninput: move |event| name.set(event.value()) }
                }
                div { class: "actions",
                    Button { variant: ButtonVariant::Secondary, r#type: "button", onclick: move |_| renaming.set(None), {ui.text("admin.cancel")} }
                    Button { r#type: "submit", disabled: !valid, {ui.text("security.rename")} }
                }
            }
        }
    }
}

/// Every unexpired session, with this browser marked; others can be signed out.
#[component]
fn SessionList(state: Signal<AppState>) -> Element {
    let ui = use_preferences();
    let mut confirming_all = use_signal(|| false);
    let error = state.read().sessions_error.clone();
    let sessions = state.read().sessions.clone();
    let others = sessions
        .as_ref()
        .map_or(0, |rows| rows.iter().filter(|row| !row.current).count());
    rsx! {
        if let Some(error) = error {
            LoadError { state, message: ui.text(&error) }
        } else if let Some(rows) = sessions {
            ul { class: "item-list sessions",
                for session in rows {
                    SessionRow { key: "{session.id}", state, session }
                }
            }
            if others > 0 {
                div { class: "button-row",
                    Button { variant: ButtonVariant::Secondary, r#type: "button", onclick: move |_| confirming_all.set(true), {ui.text("security.sign_out_others")} }
                }
            }
        } else {
            p { role: "status", {ui.text("security.loading_sessions")} }
        }
        if confirming_all() {
            ModalDialog { id: "sessions-revoke-dialog", title: ui.text("security.sign_out_others"), onclose: move |_| confirming_all.set(false),
                p { {ui.format("security.sign_out_others_consequences", &[("count", &others.to_string())])} }
                div { class: "actions",
                    Button { variant: ButtonVariant::Secondary, r#type: "button", onclick: move |_| confirming_all.set(false), {ui.text("admin.cancel")} }
                    Button { variant: ButtonVariant::Destructive, r#type: "button", onclick: move |_| {
                        confirming_all.set(false);
                        spawn_action(state, move |generation| async move {
                            match crate::api::send("POST", "/me/sessions/revoke-others", Some(json!({}))).await {
                                Ok(()) if generation_matches(state, generation) => {
                                    set_status(state, "security.other_sessions_signed_out", "success");
                                    load_sessions(state).await;
                                }
                                Ok(()) => {}
                                Err(error) => handle_api_error_for(state, generation, error),
                            }
                        });
                    }, {ui.text("security.sign_out_others")} }
                }
            }
        }
    }
}

#[component]
fn SessionRow(state: Signal<AppState>, session: SessionView) -> Element {
    let ui = use_preferences();
    let label = session
        .user_agent
        .as_deref()
        .and_then(browser_label)
        .unwrap_or_else(|| ui.text("security.unknown_browser").to_owned());
    let id = session.id;
    rsx! {
        li {
            div {
                strong { "{label}" }
                if session.current { " " span { class: "status", {ui.text("security.this_browser")} } }
                p { class: "field-hint",
                    {ui.text("security.signed_in_at")} " " {datetime_text(Some(&session.created_at), ui)}
                    if let Some(seen) = &session.last_seen_at { " · " {ui.text("security.last_active")} " " {datetime_text(Some(seen), ui)} }
                }
            }
            if !session.current {
                Button { variant: ButtonVariant::Secondary, r#type: "button",
                    "aria-label": ui.format("security.sign_out_session_label", &[("browser", &label)]),
                    onclick: move |_| {
                        spawn_action(state, move |generation| async move {
                            match crate::api::send("DELETE", &format!("/me/sessions/{id}"), None).await {
                                Ok(()) if generation_matches(state, generation) => {
                                    set_status(state, "security.session_signed_out", "success");
                                    load_sessions(state).await;
                                }
                                Ok(()) => {}
                                Err(error) => handle_api_error_for(state, generation, error),
                            }
                        });
                    },
                    {ui.text("shell.sign_out")}
                }
            }
        }
    }
}

#[component]
fn DeletePasskeyDialog(
    state: Signal<AppState>,
    passkey: PasskeyView,
    mut confirming: Signal<Option<PasskeyView>>,
) -> Element {
    let ui = use_preferences();
    let id = passkey.id;
    rsx! {
        ModalDialog { id: "passkey-delete-dialog", title: ui.text("security.delete_passkey_title"), onclose: move |_| confirming.set(None),
            p { {ui.format("security.delete_passkey_consequences", &[("label", &passkey.label)])} }
            div { class: "actions",
                Button { variant: ButtonVariant::Secondary, r#type: "button", onclick: move |_| confirming.set(None), {ui.text("admin.cancel")} }
                Button { variant: ButtonVariant::Destructive, r#type: "button", onclick: move |_| {
                    confirming.set(None);
                    spawn_action(state, move |generation| async move {
                        match crate::api::send("DELETE", &format!("/me/passkeys/{id}"), None).await {
                            Ok(()) if generation_matches(state, generation) => {
                                set_status(state, "security.passkey_deleted", "success");
                                load_passkeys(state).await;
                            }
                            Ok(()) => {}
                            Err(error) => { handle_api_error_for(state, generation, error); }
                        }
                    });
                }, {ui.text("security.delete")} }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mandatory_password_payload_omits_current_but_security_keeps_it() {
        let setup = password_body(true, String::new(), "synthetic-new-password".to_owned());
        assert!(setup.get("current_password").is_none());
        assert_eq!(setup["new_password"], "synthetic-new-password");
        let ordinary = password_body(
            false,
            "synthetic-current".to_owned(),
            "synthetic-new-password".to_owned(),
        );
        assert_eq!(ordinary["current_password"], "synthetic-current");
    }

    #[test]
    fn api_token_names_match_the_server_rules() {
        assert!(api_token_name_ok("deck"));
        assert!(api_token_name_ok("  deck  "));
        assert!(api_token_name_ok(&"あ".repeat(100)));
        assert!(!api_token_name_ok(&"あ".repeat(101)));
        assert!(!api_token_name_ok("   "));
        assert!(!api_token_name_ok("line\nbreak"));
        assert_eq!(api_token_scope_key("run"), "security.api_token_scope_run");
        assert_eq!(api_token_scope_key("read"), "security.api_token_scope_read");
    }

    #[test]
    fn confirmation_and_utf8_byte_limits_apply_to_both_password_flows() {
        let next = "synthetic-password";
        assert!(valid_password_change(true, "", next, next));
        assert!(!valid_password_change(false, "", next, next));
        assert!(valid_password_change(false, "current", next, next));
        assert!(!valid_password_change(true, "", next, "different"));
        for length in [11, 257] {
            let next = "a".repeat(length);
            assert!(!valid_password_change(true, "", &next, &next));
        }
        for length in [12, 256] {
            let next = "a".repeat(length);
            assert!(valid_password_change(true, "", &next, &next));
        }
        let unicode = "あ".repeat(86);
        assert!(!valid_password_change(true, "", &unicode, &unicode));
        // Four full-width characters are 12 bytes, so they meet the minimum.
        assert!(password_length_ok(&"あ".repeat(4)));
        assert!(password_length_ok(&"あ".repeat(85)));
        assert!(!password_length_ok("あいう"));
    }
}
