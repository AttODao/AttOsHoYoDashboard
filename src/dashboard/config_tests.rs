use super::*;

fn load(overrides: &[(&str, &str)]) -> Result<Config> {
    Config::load_from(|key| {
        overrides
            .iter()
            .rev()
            .find(|(name, _)| *name == key)
            .map(|(_, value)| (*value).to_owned())
            .or_else(|| {
                (key == "DATABASE_URL").then(|| "postgres://fixture@localhost/test".to_owned())
            })
            .ok_or(env::VarError::NotPresent)
    })
}

#[test]
fn defaults_and_explicit_settings_share_the_production_parser() {
    let config = load(&[]).unwrap();
    assert_eq!(
        config.listen,
        "127.0.0.1:8080".parse::<SocketAddr>().unwrap()
    );
    assert_eq!(config.origin, "http://localhost:8080");
    assert_eq!(config.rp_id, "localhost");
    assert!(!config.secure_cookie);
    assert_eq!(config.session_ttl, Duration::days(7));
    assert_eq!(config.timezone.to_string(), "+09:00");
    let config = load(&[
        ("PUBLIC_ORIGIN", "https://app.example.test"),
        ("RP_ID", "example.test"),
        ("BIND_ADDR", "[::1]:9010"),
        ("SESSION_TTL_DAYS", "2"),
        ("TIMEZONE_OFFSET", "-04:30"),
        ("DASHBOARD_UI_DIR", "/srv/attos/public"),
    ])
    .unwrap();
    assert_eq!(config.ui_dir, PathBuf::from("/srv/attos/public"));
    assert_eq!(config.rp_id, "example.test");
    assert_eq!(config.listen, "[::1]:9010".parse::<SocketAddr>().unwrap());
    assert!(config.secure_cookie);
    assert_eq!(config.session_ttl, Duration::days(2));
    assert_eq!(config.timezone.to_string(), "-04:30");
    // A named zone takes precedence over the fixed offset.
    let config = load(&[("TIMEZONE", "Europe/Berlin"), ("TIMEZONE_OFFSET", "+09:00")]).unwrap();
    assert_eq!(config.timezone.name(), Some("Europe/Berlin"));
}

#[test]
fn invalid_ui_directory_is_rejected_without_echoing_path() {
    for directory in ["", " ", "synthetic-private-path\0suffix"] {
        let error = load(&[("DASHBOARD_UI_DIR", directory)])
            .err()
            .expect("invalid UI path");
        assert_eq!(
            error.to_string(),
            "DASHBOARD_UI_DIR must be a nonempty directory path"
        );
    }
}

#[test]
fn structured_database_settings_escape_credentials_and_override_legacy_url() {
    let password = "synthetic %2F+&/@'\" 日本語";
    let config = load(&[
        ("DB_NAME", "synthetic-db"),
        ("DB_USER", "synthetic-user"),
        ("DB_PASSWORD", password),
        ("DB_HOST", "/tmp/synthetic-pg"),
        ("DB_PORT", "5544"),
    ])
    .unwrap();
    let url = Url::parse(&config.database_url).unwrap();
    let values: std::collections::HashMap<_, _> = url.query_pairs().collect();
    assert!(
        values
            .get("password")
            .is_some_and(|value| value == password)
    );
    assert_eq!(values.get("user").unwrap(), "synthetic-user");
    assert_eq!(values.get("dbname").unwrap(), "synthetic-db");
    assert_eq!(values.get("host").unwrap(), "/tmp/synthetic-pg");
    assert_eq!(url.port(), Some(5544));
}

#[test]
fn partial_or_invalid_database_fields_fail_without_secret_echoes() {
    let base = [
        ("DB_NAME", "test"),
        ("DB_USER", "test"),
        ("DB_PASSWORD", "synthetic-private-password"),
    ];
    assert!(load(&[("DB_NAME", "test")]).is_err());
    for pair in [
        ("DB_PASSWORD", ""),
        ("DB_PORT", "0"),
        ("DB_PORT", "65536"),
        ("DB_HOST", ""),
    ] {
        let mut fields = base.to_vec();
        fields.push(pair);
        let error = load(&fields)
            .err()
            .expect("invalid structured database setting");
        assert!(!format!("{error:?}").contains("synthetic-private-password"));
    }
}

#[test]
fn untrusted_origins_are_rejected_without_echoing_input() {
    for origin in [
        "http://example.test",
        "http://127.0.0.1:8080",
        "https://127.0.0.1",
        "https://[::1]",
        "https://secret:credential@example.test",
        "https://app.example.test/path",
        "https://app.example.test?q=secret",
        "https://app.example.test#secret",
        "file:///tmp/site",
    ] {
        let error = load(&[("PUBLIC_ORIGIN", origin)])
            .err()
            .expect("origin must fail");
        assert!(!error.to_string().contains("credential"));
        assert!(!format!("{error:?}").contains(origin));
    }
}

#[test]
fn invalid_database_address_lifetime_and_timezone_fail_closed() {
    assert!(Config::load_from(|_| Err(env::VarError::NotPresent)).is_err());
    for pair in [
        ("DATABASE_URL", "sqlite://secret"),
        ("DATABASE_URL", "garbage"),
        ("BIND_ADDR", "localhost:8080"),
        ("SESSION_TTL_DAYS", "0"),
        ("SESSION_TTL_DAYS", "-1"),
        ("SESSION_TTL_DAYS", "9223372036854775807"),
        ("SESSION_TTL_DAYS", "NaN"),
        ("TIMEZONE_OFFSET", "Asia/Tokyo"),
        ("TIMEZONE_OFFSET", "+25:00"),
        ("TIMEZONE", "Mars/Base"),
    ] {
        assert!(load(&[pair]).is_err(), "invalid {} accepted", pair.0);
    }
}

#[test]
fn cookie_storage_key_is_optional_but_invalid_present_values_fail_without_echoing() {
    assert!(load(&[]).unwrap().cookie_key.is_none());
    assert!(
        load(&[("HOYOLAB_COOKIE_KEY", &"11".repeat(32))])
            .unwrap()
            .cookie_key
            .is_some()
    );
    for raw in [
        "synthetic-secret-key",
        "",
        &"ff".repeat(31),
        &"z".repeat(64),
    ] {
        let error = load(&[("HOYOLAB_COOKIE_KEY", raw)]).err().unwrap();
        assert!(matches!(error, Error::Invalid(_)));
        assert!(!error.to_string().contains("synthetic-secret-key"));
    }
    let result = Config::load_from(|name| match name {
        "DATABASE_URL" => Ok("postgres://fixture@localhost/test".into()),
        "HOYOLAB_COOKIE_KEY" => Err(env::VarError::NotUnicode(std::ffi::OsString::from(
            "synthetic-private-key",
        ))),
        _ => Err(env::VarError::NotPresent),
    });
    assert!(matches!(
        result,
        Err(Error::Invalid("HOYOLAB_COOKIE_KEY must be UTF-8"))
    ));
}

#[test]
fn non_unicode_configuration_is_not_silently_defaulted() {
    use std::ffi::OsString;
    let result = Config::load_from(|key| match key {
        "DATABASE_URL" => Ok("postgres://fixture@localhost/test".to_owned()),
        "BIND_ADDR" => Err(env::VarError::NotUnicode(OsString::from(
            "synthetic-unicode-error",
        ))),
        _ => Err(env::VarError::NotPresent),
    });
    assert!(matches!(
        result,
        Err(Error::Invalid("configuration must be UTF-8"))
    ));
}

#[test]
fn tuning_defaults_and_bounds() {
    assert_eq!(load(&[]).unwrap().tuning, Tuning::default());
    let tuning = load(&[
        ("REWARD_HISTORY_DAYS", "30"),
        ("STATUS_CACHE_TTL_MINUTES", "15"),
        ("REWARD_MAX_ATTEMPTS", "2"),
        ("REWARD_RETRY_DELAY_SECONDS", "60"),
    ])
    .unwrap()
    .tuning;
    assert_eq!(tuning.history_days, 30);
    assert_eq!(tuning.status_ttl, Duration::minutes(15));
    assert_eq!(tuning.max_attempts, 2);
    assert_eq!(tuning.retry_delay, Duration::seconds(60));
    for pair in [
        ("REWARD_HISTORY_DAYS", "0"),
        ("REWARD_HISTORY_DAYS", "91"),
        ("STATUS_CACHE_TTL_MINUTES", "0"),
        ("REWARD_MAX_ATTEMPTS", "5"),
        ("REWARD_RETRY_DELAY_SECONDS", "x"),
    ] {
        assert!(load(&[pair]).is_err(), "{} accepted", pair.0);
    }
}
