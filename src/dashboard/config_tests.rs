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
    assert_eq!(config.timezone.local_minus_utc(), 9 * 3600);
    let config = load(&[
        ("PUBLIC_ORIGIN", "https://app.example.test"),
        ("RP_ID", "example.test"),
        ("BIND_ADDR", "[::1]:9010"),
        ("SESSION_TTL_DAYS", "2"),
        ("TIMEZONE_OFFSET", "-04:30"),
    ])
    .unwrap();
    assert_eq!(config.rp_id, "example.test");
    assert!(config.secure_cookie);
    assert_eq!(config.session_ttl, Duration::days(2));
    assert_eq!(config.timezone.local_minus_utc(), -16200);
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
    ] {
        assert!(load(&[pair]).is_err(), "invalid {} accepted", pair.0);
    }
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
