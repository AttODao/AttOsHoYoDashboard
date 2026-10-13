use sea_orm::DbErr;

#[derive(thiserror::Error)]
pub(crate) enum Error {
    #[error("{0}")]
    Invalid(&'static str),
    #[error("invalid credentials or expired session")]
    Unauthorized,
    #[error("permission denied")]
    Forbidden,
    #[error("password change required")]
    PasswordChangeRequired,
    #[error("resource not found")]
    NotFound,
    #[error("username already exists")]
    Conflict,
    #[error("too many login attempts; try again later")]
    RateLimited,
    #[error("authentication service busy; try again later")]
    Busy,
    #[error("database operation failed")]
    Database(#[from] DbErr),
    #[error("password operation failed")]
    Password,
    #[error("secure random generation failed")]
    Random,
    #[error("server operation failed")]
    Internal,
}

// Driver Debug/source chains may contain credentials or a rejected row's hash.
impl std::fmt::Debug for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(self, f)
    }
}

pub(super) type Result<T> = std::result::Result<T, Error>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn database_display_and_debug_never_echo_driver_content() {
        let error = Error::Database(DbErr::Custom("synthetic-password-hash-and-url".into()));
        assert_eq!(error.to_string(), "database operation failed");
        assert_eq!(format!("{error:?}"), "database operation failed");
    }
}
