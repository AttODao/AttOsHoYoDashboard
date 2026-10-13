use super::*;
use crate::dashboard::error::Error;
use axum::response::IntoResponse;
use sea_orm::DbErr;

#[tokio::test]
async fn every_domain_error_has_an_exact_status_code_and_safe_message() {
    let cases = [
        (
            Error::Invalid("synthetic invalid input"),
            StatusCode::BAD_REQUEST,
            "invalid_input",
            "synthetic invalid input",
        ),
        (
            Error::Unauthorized,
            StatusCode::UNAUTHORIZED,
            "unauthorized",
            "invalid credentials or expired session",
        ),
        (
            Error::Forbidden,
            StatusCode::FORBIDDEN,
            "forbidden",
            "permission denied",
        ),
        (
            Error::PasswordChangeRequired,
            StatusCode::FORBIDDEN,
            "password_change_required",
            "password change required",
        ),
        (
            Error::NotFound,
            StatusCode::NOT_FOUND,
            "not_found",
            "resource not found",
        ),
        (
            Error::Conflict,
            StatusCode::CONFLICT,
            "conflict",
            "username already exists",
        ),
        (
            Error::RateLimited,
            StatusCode::TOO_MANY_REQUESTS,
            "rate_limited",
            "too many login attempts; try again later",
        ),
        (
            Error::Busy,
            StatusCode::SERVICE_UNAVAILABLE,
            "busy",
            "authentication service busy; try again later",
        ),
        (
            Error::Database(DbErr::Custom("synthetic-driver-secret".to_owned())),
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal_error",
            "database operation failed",
        ),
        (
            Error::Password,
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal_error",
            "password operation failed",
        ),
        (
            Error::Random,
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal_error",
            "secure random generation failed",
        ),
        (
            Error::Internal,
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal_error",
            "server operation failed",
        ),
    ];
    for (error, status, code, message) in cases {
        assert_eq!(format!("{error:?}"), message);
        let response = error.into_response();
        assert_eq!(response.status(), status);
        assert_eq!(response.headers()[header::CONTENT_TYPE], "application/json");
        assert_eq!(
            body(response).await,
            json!({"error": code, "message": message})
        );
    }
}

#[tokio::test]
async fn driver_source_details_do_not_leak_through_router_or_debug() {
    let error = Error::Database(DbErr::Custom("synthetic-driver-secret".to_owned()));
    // The source is retained for structured error handling, not exposed as response text.
    assert!(std::error::Error::source(&error).is_some());
    assert_eq!(format!("{error:?}"), "database operation failed");
    assert_eq!(format!("{error:#?}"), "database operation failed");
    assert_eq!(
        body(error.into_response()).await,
        json!({"error": "internal_error", "message": "database operation failed"})
    );
    let app = disconnected_app().await;
    error_response(call(&app, request("GET", "/health", None, None)).await).await;
}

async fn error_response(response: Response) {
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(
        body(response).await,
        json!({"error": "internal_error", "message": "database operation failed"})
    );
}
