//! Shared Origin/cache boundary and session extraction for protected routes.
use super::{
    App,
    session::{cookie_hash, cookie_name},
};
use crate::dashboard::{
    auth::{
        api_tokens::{self, ApiTokens},
        sessions::{Credential, Identity, Sessions},
    },
    error::{Error, Result},
};
use axum::{
    extract::{MatchedPath, Request, State},
    http::{HeaderMap, HeaderValue, Method, header},
    middleware::Next,
    response::{IntoResponse, Response},
};
use std::sync::Arc;

pub(super) async fn boundary(State(app): State<App>, request: Request, next: Next) -> Response {
    let safe = matches!(
        *request.method(),
        Method::GET | Method::HEAD | Method::OPTIONS
    );
    let response = if !safe
        && !bearer_without_cookie(request.headers())
        && !valid_origin(request.headers(), &app.config.origin)
    {
        Error::Forbidden.into_response()
    } else {
        next.run(request).await
    };
    harden(response)
}

pub(super) fn harden(mut response: Response) -> Response {
    // Successful `/assets` responses choose their own cache policy (see `ui.rs`).
    response
        .headers_mut()
        .entry(header::CACHE_CONTROL)
        .or_insert(HeaderValue::from_static("no-store"));
    response.headers_mut().insert(
        "x-content-type-options",
        HeaderValue::from_static("nosniff"),
    );
    response
}

/// CSRF needs ambient credentials: browsers attach cookies automatically, but never a
/// Bearer header cross-site without a CORS preflight, which AHD never answers. Such a
/// request is authenticated by its token alone (see `authenticate`).
fn bearer_without_cookie(headers: &HeaderMap) -> bool {
    let mut values = headers.get_all(header::AUTHORIZATION).iter();
    let bearer = values
        .next()
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.starts_with("Bearer "));
    bearer && values.next().is_none() && !headers.contains_key(header::COOKIE)
}

fn valid_origin(headers: &HeaderMap, expected: &str) -> bool {
    let mut origins = headers.get_all(header::ORIGIN).iter();
    origins.next().and_then(|v| v.to_str().ok()) == Some(expected) && origins.next().is_none()
}

pub(super) async fn authenticate(
    State(app): State<App>,
    mut request: Request,
    next: Next,
) -> Response {
    let result = if request.headers().contains_key(header::AUTHORIZATION) {
        let route = request
            .extensions()
            .get::<MatchedPath>()
            .map(|path| path.as_str().to_owned());
        let method = request.method().clone();
        token_identity(&app, request.headers(), &method, route.as_deref()).await
    } else {
        session_identity(&app, request.headers()).await
    };
    match result {
        Ok(identity) => {
            request.extensions_mut().insert(Arc::new(identity));
            next.run(request).await
        }
        Err(error) => error.into_response(),
    }
}

async fn session_identity(app: &App, headers: &HeaderMap) -> Result<Identity> {
    let hash = cookie_hash(headers, cookie_name(&app.config))?;
    let identity = Sessions::authenticate(&app.accounts.db, &hash).await?;
    // Activity is a convenience for the session list; never fail a request over it.
    if let Err(error) = Sessions::touch(&app.accounts.db, &identity.token_hash).await {
        tracing::warn!(error = %error, "session activity not recorded");
    }
    Ok(identity)
}

/// An `Authorization` header selects token authentication only; the token's scope must
/// permit the matched route.
async fn token_identity(
    app: &App,
    headers: &HeaderMap,
    method: &Method,
    route: Option<&str>,
) -> Result<Identity> {
    // One request carries one credential; a session cookie beside a token is ambiguous.
    let name = cookie_name(&app.config);
    let session_cookie = headers.get_all(header::COOKIE).iter().any(|value| {
        value.to_str().map_or(true, |value| {
            value.split(';').any(|part| {
                part.trim()
                    .split_once('=')
                    .is_some_and(|(key, _)| key == name)
            })
        })
    });
    if session_cookie {
        return Err(Error::Unauthorized);
    }
    let mut values = headers.get_all(header::AUTHORIZATION).iter();
    let token = match (values.next(), values.next()) {
        (Some(value), None) => value
            .to_str()
            .ok()
            .and_then(|value| value.strip_prefix("Bearer "))
            .ok_or(Error::Unauthorized)?,
        _ => return Err(Error::Unauthorized),
    };
    let identity = ApiTokens::authenticate(&app.accounts.db, &api_tokens::hash(token)?).await?;
    let Credential::ApiToken { id, scope } = identity.credential else {
        return Err(Error::Unauthorized);
    };
    let route = route.ok_or(Error::Forbidden)?;
    if !api_tokens::permits(scope, method, route) {
        return Err(Error::Forbidden);
    }
    // Like session activity, last use is a convenience; never fail a request over it.
    if let Err(error) = ApiTokens::touch(&app.accounts.db, id).await {
        tracing::warn!(error = %error, "API token use not recorded");
    }
    Ok(identity)
}
