use super::error::{Error, Result};
use super::hoyolab::credentials::CookieKey;
use super::rewards::schedule::ScheduleZone;
use chrono::{Duration, Utc};
use reqwest::Url;
use std::{env, net::SocketAddr, path::PathBuf, sync::Arc};

/// Operational limits shared by status caching and the reward worker.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Tuning {
    /// Finished reward runs older than this are deleted.
    pub history_days: i64,
    /// Game and reward status younger than this is served without an upstream GET.
    pub status_ttl: Duration,
    /// Initial try plus retries for one reward run (the schema allows at most 4).
    pub max_attempts: i32,
    /// Wait before retrying a transient reward GET failure.
    pub retry_delay: Duration,
}

impl Default for Tuning {
    fn default() -> Self {
        Self {
            history_days: 5,
            status_ttl: Duration::minutes(5),
            max_attempts: 4,
            retry_delay: Duration::seconds(10),
        }
    }
}

fn bounded(
    read: &mut impl FnMut(&str) -> std::result::Result<String, env::VarError>,
    name: &str,
    default: i64,
    range: std::ops::RangeInclusive<i64>,
    message: &'static str,
) -> Result<i64> {
    setting(read, name, &default.to_string())?
        .parse::<i64>()
        .ok()
        .filter(|value| range.contains(value))
        .ok_or(Error::Invalid(message))
}

impl Tuning {
    fn load(
        read: &mut impl FnMut(&str) -> std::result::Result<String, env::VarError>,
    ) -> Result<Self> {
        let defaults = Self::default();
        Ok(Self {
            history_days: bounded(
                read,
                "REWARD_HISTORY_DAYS",
                defaults.history_days,
                1..=90,
                "REWARD_HISTORY_DAYS must be 1 to 90",
            )?,
            status_ttl: Duration::minutes(bounded(
                read,
                "STATUS_CACHE_TTL_MINUTES",
                defaults.status_ttl.num_minutes(),
                1..=60,
                "STATUS_CACHE_TTL_MINUTES must be 1 to 60",
            )?),
            max_attempts: bounded(
                read,
                "REWARD_MAX_ATTEMPTS",
                defaults.max_attempts.into(),
                1..=4,
                "REWARD_MAX_ATTEMPTS must be 1 to 4",
            )? as i32,
            retry_delay: Duration::seconds(bounded(
                read,
                "REWARD_RETRY_DELAY_SECONDS",
                defaults.retry_delay.num_seconds(),
                1..=600,
                "REWARD_RETRY_DELAY_SECONDS must be 1 to 600",
            )?),
        })
    }
}

// Deliberately not Debug: the database URL contains credentials.
pub(super) struct Config {
    pub database_url: String,
    pub listen: SocketAddr,
    pub origin: String,
    pub rp_id: String,
    pub secure_cookie: bool,
    pub session_ttl: Duration,
    pub timezone: ScheduleZone,
    pub tuning: Tuning,
    /// Bearer token for `/metrics`; the endpoint is absent without one.
    pub metrics_token: Option<String>,
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
        let zone_name = setting(&mut read, "TIMEZONE", "")?;
        let timezone = ScheduleZone::parse(
            Some(zone_name.as_str()),
            &setting(&mut read, "TIMEZONE_OFFSET", "+09:00")?,
        )?;
        let tuning = Tuning::load(&mut read)?;
        let metrics_token = match setting(&mut read, "METRICS_TOKEN", "")? {
            token if token.is_empty() => None,
            token if token.len() >= 16 && token.bytes().all(|b| b.is_ascii_graphic()) => {
                Some(token)
            }
            _ => {
                return Err(Error::Invalid(
                    "METRICS_TOKEN must be at least 16 visible ASCII characters",
                ));
            }
        };
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
            tuning,
            metrics_token,
            cookie_key,
            ui_dir: ui_dir.into(),
        })
    }
}

#[cfg(test)]
#[path = "config_tests.rs"]
mod tests;
