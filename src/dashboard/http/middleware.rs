//! Shared Origin/cache boundary and session extraction for protected routes.
use super::{
    App,
    session::{cookie_hash, cookie_name},
};
use crate::dashboard::{auth::sessions::Sessions, error::Error};
use axum::{
    extract::{Request, State},
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
    let response = if !safe && !valid_origin(request.headers(), &app.config.origin) {
        Error::Forbidden.into_response()
    } else {
        next.run(request).await
    };
    harden(response)
}

pub(super) fn harden(mut response: Response) -> Response {
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response.headers_mut().insert(
        "x-content-type-options",
        HeaderValue::from_static("nosniff"),
    );
    response
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
    let result = match cookie_hash(request.headers(), cookie_name(&app.config)) {
        Ok(hash) => Sessions::authenticate(&app.accounts.db, &hash).await,
        Err(error) => Err(error),
    };
    match result {
        Ok(identity) => {
            request.extensions_mut().insert(Arc::new(identity));
            next.run(request).await
        }
        Err(error) => error.into_response(),
    }
}
