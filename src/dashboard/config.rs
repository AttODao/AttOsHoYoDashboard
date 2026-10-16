use super::error::{Error, Result};
use super::hoyolab::credentials::CookieKey;
use chrono::{Duration, FixedOffset, Utc};
use reqwest::Url;
use std::{env, net::SocketAddr, path::PathBuf, sync::Arc};

// Deliberately not Debug: the database URL contains credentials.
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

fn setting(
    read: &mut impl FnMut(&str) -> std::result::Result<String, env::VarError>,
    name: &str,
    default: &str,
) -> Result<String> {
    match read(name) {
        Ok(value) => Ok(value),
        Err(env::VarError::NotPresent) => Ok(default.to_owned()),
        Err(_) => Err(Error::Invalid("configuration must be UTF-8")),
    }
}

fn database_url(
    read: &mut impl FnMut(&str) -> std::result::Result<String, env::VarError>,
) -> Result<String> {
    let name = setting(read, "DB_NAME", "")?;
    let user = setting(read, "DB_USER", "")?;
    let password = setting(read, "DB_PASSWORD", "")?;
    if !name.is_empty() || !user.is_empty() || !password.is_empty() {
        if name.is_empty() || user.is_empty() || password.is_empty() {
            return Err(Error::Invalid(
                "DB_NAME, DB_USER and DB_PASSWORD are required together",
            ));
        }
        if name.len() > 63 || user.len() > 63 || name.contains('\0') || user.contains('\0') {
            return Err(Error::Invalid(
                "database and role names must be 1 to 63 bytes without NUL",
            ));
        }
        let host = setting(read, "DB_HOST", "127.0.0.1")?;
        if host.is_empty() || host.contains('\0') {
            return Err(Error::Invalid("invalid DB_HOST"));
        }
        let port: u16 = setting(read, "DB_PORT", "5432")?
            .parse()
            .ok()
            .filter(|port| *port != 0)
            .ok_or(Error::Invalid("invalid DB_PORT"))?;
        let mut url = Url::parse("postgres://localhost").expect("static PostgreSQL URL");
        url.set_port(Some(port))
            .map_err(|_| Error::Invalid("invalid DB_PORT"))?;
        // libpq/SQLx accept these query parameters. The serializer escapes all
        // credentials, including literal %, +, &, quotes and Unicode.
        url.query_pairs_mut()
            .append_pair("host", &host)
            .append_pair("dbname", &name)
            .append_pair("user", &user)
            .append_pair("password", &password);
        return Ok(url.into());
    }
    let raw =
        read("DATABASE_URL").map_err(|_| Error::Invalid("database configuration is required"))?;
    let url =
        Url::parse(&raw).map_err(|_| Error::Invalid("DATABASE_URL must be a PostgreSQL URL"))?;
    if !matches!(url.scheme(), "postgres" | "postgresql") {
        return Err(Error::Invalid("DATABASE_URL must be a PostgreSQL URL"));
    }
    Ok(raw)
}

impl Config {
    pub fn load() -> Result<Self> {
        Self::load_from(|name| env::var(name))
    }

    fn load_from(
        mut read: impl FnMut(&str) -> std::result::Result<String, env::VarError>,
    ) -> Result<Self> {
        let database_url = database_url(&mut read)?;
        let listen = setting(&mut read, "BIND_ADDR", "127.0.0.1:8080")?
            .parse()
            .map_err(|_| Error::Invalid("invalid BIND_ADDR"))?;
        let url = Url::parse(&setting(
            &mut read,
            "PUBLIC_ORIGIN",
            "http://localhost:8080",
        )?)
        .map_err(|_| Error::Invalid("invalid PUBLIC_ORIGIN"))?;
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
        let rp_id = setting(
            &mut read,
            "RP_ID",
            url.host_str()
                .ok_or(Error::Invalid("PUBLIC_ORIGIN requires a host"))?,
        )?;
        let days: i64 = setting(&mut read, "SESSION_TTL_DAYS", "7")?
            .parse()
            .map_err(|_| Error::Invalid("invalid SESSION_TTL_DAYS"))?;
        let session_ttl = Duration::try_days(days)
            .filter(|ttl| days > 0 && Utc::now().checked_add_signed(*ttl).is_some())
            .ok_or(Error::Invalid(
                "SESSION_TTL_DAYS must be positive and representable",
            ))?;
        let timezone = setting(&mut read, "TIMEZONE_OFFSET", "+09:00")?
            .parse()
            .map_err(|_| Error::Invalid("TIMEZONE_OFFSET must be a fixed UTC offset"))?;
        let ui_dir = setting(
            &mut read,
            "DASHBOARD_UI_DIR",
            "target/dx/attos-dashboard-ui/release/web/public",
        )?;
        if ui_dir.trim().is_empty() || ui_dir.contains('\0') {
            return Err(Error::Invalid(
                "DASHBOARD_UI_DIR must be a nonempty directory path",
            ));
        }
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
            ui_dir: ui_dir.into(),
        })
    }
}

#[cfg(test)]
#[path = "config_tests.rs"]
mod tests;
