//! Session-cookie encoding/parsing and sign-in response; never sets session policy.
use super::{HttpState, users::UserView};
use crate::dashboard::{
    auth::{
        password,
        sessions::{IssuedSession, Sessions},
    },
    config::Config,
    db::entities::user,
    error::{Error, Result},
};
use axum::{
    Json,
    http::{HeaderMap, HeaderValue, header},
    response::{IntoResponse, Response},
};

pub(super) fn cookie_name(config: &Config) -> &'static str {
    if config.secure_cookie {
        "__Host-attos_session"
    } else {
        "attos_session"
    }
}

pub(super) fn cookie_hash(headers: &HeaderMap, name: &str) -> Result<String> {
    let mut token = None;
    for cookie in headers.get_all(header::COOKIE) {
        for part in cookie.to_str().map_err(|_| Error::Unauthorized)?.split(';') {
            if let Some((key, value)) = part.trim().split_once('=')
                && key == name
            {
                if token.is_some() {
                    return Err(Error::Unauthorized);
                }
                token = Some(value);
            }
        }
    }
    password::token_hash(token.ok_or(Error::Unauthorized)?)
}

pub(super) fn set_cookie(config: &Config, token: &str, max_age: i64) -> Result<HeaderValue> {
    HeaderValue::from_str(&format!(
        "{}={token}; Path=/; HttpOnly; SameSite=Strict; Max-Age={max_age}{}",
        cookie_name(config),
        if config.secure_cookie { "; Secure" } else { "" }
    ))
    .map_err(|_| Error::Internal)
}

/// The sign-in response: records which browser holds the session, then sets the cookie.
pub(super) async fn signed_in(
    app: &HttpState,
    headers: &HeaderMap,
    user: user::Model,
    session: IssuedSession,
) -> Result<Response> {
    let agent = headers
        .get(header::USER_AGENT)
        .and_then(|value| value.to_str().ok());
    // The browser label is a convenience; a failure to store it must not fail sign-in.
    if let Err(error) = Sessions::describe(&app.accounts.db, &session.token, agent).await {
        tracing::warn!(error = %error, "session browser not recorded");
    }
    let mut response = Json(UserView::from(user)).into_response();
    let remaining = session
        .expires_at
        .signed_duration_since(chrono::Utc::now())
        .num_seconds()
        .max(0);
    response.headers_mut().insert(
        header::SET_COOKIE,
        set_cookie(&app.config, &session.token, remaining)?,
    );
    Ok(response)
}
