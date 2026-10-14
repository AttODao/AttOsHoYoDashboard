use super::error::{Error, Result};
use super::hoyolab::credentials::CookieKey;
use chrono::{Duration, FixedOffset, Utc};
use reqwest::Url;
use std::{env, net::SocketAddr, path::PathBuf, sync::Arc};

// Deliberately not Debug: DATABASE_URL may contain credentials.
pub(super) struct Config {
    pub database_url: String,
    pub listen: SocketAddr,
    pub origin: String,
    pub rp_id: String,
    pub secure_cookie: bool,
    pub session_ttl: Duration,
    pub timezone: FixedOffset,
    pub cookie_key: Option<Arc<CookieKey>>,
    pub ui_dir: PathBuf,
}

impl Config {
    pub fn load() -> Result<Self> {
        Self::load_from(|name| env::var(name))
    }

    // The same parser is tested without mutating the process-wide environment.
    fn load_from(
        mut read: impl FnMut(&str) -> std::result::Result<String, env::VarError>,
    ) -> Result<Self> {
        let database_url =
            read("DATABASE_URL").map_err(|_| Error::Invalid("DATABASE_URL is required"))?;
        let mut value = |name: &str, default: &str| match read(name) {
            Ok(value) => Ok(value),
            Err(env::VarError::NotPresent) => Ok(default.to_owned()),
            Err(_) => Err(Error::Invalid("configuration must be UTF-8")),
        };
        let database = Url::parse(&database_url)
            .map_err(|_| Error::Invalid("DATABASE_URL must be a PostgreSQL URL"))?;
        if !matches!(database.scheme(), "postgres" | "postgresql") {
            return Err(Error::Invalid("DATABASE_URL must be a PostgreSQL URL"));
        }
        let listen = value("BIND_ADDR", "127.0.0.1:8080")?
            .parse()
            .map_err(|_| Error::Invalid("invalid BIND_ADDR"))?;
        let url = Url::parse(&value("PUBLIC_ORIGIN", "http://localhost:8080")?)
            .map_err(|_| Error::Invalid("invalid PUBLIC_ORIGIN"))?;
        // WebAuthn requires a domain RP ID; IP-literal origins cannot serve passkeys.
        if !(url.scheme() == "https" || url.scheme() == "http" && url.domain() == Some("localhost"))
            || !url.username().is_empty()
            || url.password().is_some()
            || url.path() != "/"
            || url.query().is_some()
            || url.fragment().is_some()
            || url.domain().is_none()
        {
            return Err(Error::Invalid(
                "PUBLIC_ORIGIN must be an HTTPS DNS origin (HTTP only on localhost)",
            ));
        }
        let rp_id = value(
            "RP_ID",
            url.host_str()
                .ok_or(Error::Invalid("PUBLIC_ORIGIN requires a host"))?,
        )?;
        let days: i64 = value("SESSION_TTL_DAYS", "7")?
            .parse()
            .map_err(|_| Error::Invalid("invalid SESSION_TTL_DAYS"))?;
        let session_ttl = Duration::try_days(days)
            .filter(|ttl| days > 0 && Utc::now().checked_add_signed(*ttl).is_some())
            .ok_or(Error::Invalid(
                "SESSION_TTL_DAYS must be positive and representable",
            ))?;
        let timezone = value("TIMEZONE_OFFSET", "+09:00")?
            .parse()
            .map_err(|_| Error::Invalid("TIMEZONE_OFFSET must be a fixed UTC offset"))?;
        let ui_dir = value(
            "DASHBOARD_UI_DIR",
            "target/dx/attos-dashboard-ui/release/web/public",
        )?;
        if ui_dir.trim().is_empty() || ui_dir.contains('\0') {
            return Err(Error::Invalid(
                "DASHBOARD_UI_DIR must be a nonempty directory path",
            ));
        }
        let ui_dir = PathBuf::from(ui_dir);
        let cookie_key = match read("HOYOLAB_COOKIE_KEY") {
            Ok(value) => Some(Arc::new(CookieKey::from_hex(&value)?)),
            Err(env::VarError::NotPresent) => None,
            Err(_) => return Err(Error::Invalid("HOYOLAB_COOKIE_KEY must be UTF-8")),
        };
        Ok(Self {
            database_url,
            listen,
            origin: url.origin().ascii_serialization(),
            rp_id,
            secure_cookie: url.scheme() == "https",
            session_ttl,
            timezone,
            cookie_key,
            ui_dir,
        })
    }
}

#[cfg(test)]
#[path = "config_tests.rs"]
mod tests;
