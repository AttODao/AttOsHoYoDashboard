//! Redacted domain-error to HTTP mapping; no driver/source diagnostics are emitted.
use crate::dashboard::error::Error;
use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde_json::json;

impl IntoResponse for Error {
    fn into_response(self) -> Response {
        let (status, code) = match &self {
            Self::Invalid(_) => (StatusCode::BAD_REQUEST, "invalid_input"),
            Self::Unauthorized => (StatusCode::UNAUTHORIZED, "unauthorized"),
            Self::Forbidden => (StatusCode::FORBIDDEN, "forbidden"),
            Self::PasswordChangeRequired => (StatusCode::FORBIDDEN, "password_change_required"),
            Self::NotFound => (StatusCode::NOT_FOUND, "not_found"),
            Self::Conflict => (StatusCode::CONFLICT, "conflict"),
            Self::RateLimited => (StatusCode::TOO_MANY_REQUESTS, "rate_limited"),
            Self::Busy => (StatusCode::SERVICE_UNAVAILABLE, "busy"),
            Self::Unavailable(_) => (StatusCode::SERVICE_UNAVAILABLE, "unavailable"),
            _ => (StatusCode::INTERNAL_SERVER_ERROR, "internal_error"),
        };
        if status.is_server_error() {
            tracing::error!(kind = code, "backend request failed");
        }
        (
            status,
            Json(json!({"error": code, "message": self.to_string()})),
        )
            .into_response()
    }
}
