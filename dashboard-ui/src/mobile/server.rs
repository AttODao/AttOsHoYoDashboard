//! The server the app connects to: the build-time `AHD_SERVER_URL` by default, or the origin
//! the user chose on the login page (`server-origin.txt`). Kept free of I/O so the rules are
//! unit-tested in every native test run; `mobile::http` holds the live value.
use super::session::parse_origin;

/// App-private file with the chosen origin (`scheme://host[:port]`, nothing else).
pub(crate) const SERVER_FILE: &str = "server-origin.txt";

/// Why an address typed on the login page cannot be used.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ServerError {
    Empty,
    Invalid,
    /// `http://` anywhere but localhost.
    Insecure,
    /// No answer from `/health` (DNS, TLS, timeout, refused).
    Unreachable,
    /// An answer, but not the health document of an AHD server.
    NotAhd,
    /// The old server's session or the new choice could not be stored.
    NotSaved,
}

impl ServerError {
    pub(crate) fn message_key(self) -> &'static str {
        match self {
            Self::Empty => "auth.server_error_empty",
            Self::Invalid => "auth.server_error_invalid",
            Self::Insecure => "auth.server_error_insecure",
            Self::Unreachable => "auth.server_error_unreachable",
            Self::NotAhd => "auth.server_error_not_ahd",
            Self::NotSaved => "auth.server_error_not_saved",
        }
    }
}

/// The origin of an address the user typed. A missing scheme means `https://`; a path,
/// query or fragment (a copied page URL) is dropped, because only the origin is stored.
/// HTTPS only, except `http://localhost` / `http://127.0.0.1` for development.
pub(crate) fn origin_from_input(input: &str) -> Result<String, ServerError> {
    let input = input.trim();
    if input.is_empty() {
        return Err(ServerError::Empty);
    }
    // A scheme only counts before the first `/`, `?` or `#` (not inside a pasted query).
    let has_scheme = input
        .split_once("://")
        .is_some_and(|(scheme, _)| !scheme.contains(['/', '?', '#']));
    let with_scheme = if has_scheme {
        input.to_owned()
    } else {
        format!("https://{input}")
    };
    let mut url = url::Url::parse(&with_scheme).map_err(|_| ServerError::Invalid)?;
    if !matches!(url.scheme(), "https" | "http")
        || url.host_str().is_none_or(str::is_empty)
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err(ServerError::Invalid);
    }
    url.set_path("");
    url.set_query(None);
    url.set_fragment(None);
    parse_origin(url.as_str()).ok_or(if url.scheme() == "http" {
        ServerError::Insecure
    } else {
        ServerError::Invalid
    })
}

/// The origin in use: the saved choice when it is still valid, else the build-time default.
/// None when neither is usable (the login page then asks for a server).
pub(crate) fn configured(saved: Option<&str>, default: &str) -> Option<String> {
    saved
        .and_then(parse_origin)
        .or_else(|| parse_origin(default))
}

/// What the saved file should hold for `origin`: nothing when it is the default, so a later
/// build with another default moves users who never chose a server along with it.
pub(crate) fn saved_value(origin: &str, default: &str) -> Option<String> {
    (parse_origin(default).as_deref() != Some(origin)).then(|| origin.to_owned())
}

/// Whether `/health` answered like an AHD server: 200 (`ok`) or 503 (`degraded`, the
/// background worker stalled) with the JSON `status` field.
pub(crate) fn health_answer(status: u16, body: &[u8]) -> Result<(), ServerError> {
    let document: serde_json::Value =
        serde_json::from_slice(body).map_err(|_| ServerError::NotAhd)?;
    let state = document.get("status").and_then(|value| value.as_str());
    match (status, state) {
        (200, Some("ok")) | (503, Some("degraded")) => Ok(()),
        _ => Err(ServerError::NotAhd),
    }
}

/// Files in app-private storage that belong to one server and are removed when it changes.
/// The session cookie (`AhdSessionStore`), the inbox registration and the widget data are
/// cleared through their Kotlin owners; device-wide choices (language, theme, which device
/// notifications are on, the skipped update) stay.
pub(crate) const PER_SERVER_FILES: [&str; 2] = [
    super::browser_login::PENDING_FILE,
    super::reminders::SCHEDULE_FILE,
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_addresses_keep_only_a_secure_origin() {
        let ok = |input: &str| origin_from_input(input).unwrap();
        assert_eq!(ok("https://ahd.example.test"), "https://ahd.example.test");
        assert_eq!(ok("  ahd.example.test  "), "https://ahd.example.test");
        assert_eq!(
            ok("https://AHD.example.test:8443/dashboard?game=genshin#x"),
            "https://ahd.example.test:8443"
        );
        assert_eq!(
            ok("https://ahd.example.test:443/"),
            "https://ahd.example.test"
        );
        assert_eq!(ok("http://localhost:18080/login"), "http://localhost:18080");
        assert_eq!(ok("http://127.0.0.1:8080"), "http://127.0.0.1:8080");
        assert_eq!(
            ok("ahd.example.test/login?next=https://other.example.test"),
            "https://ahd.example.test"
        );
        assert_eq!(origin_from_input(""), Err(ServerError::Empty));
        assert_eq!(origin_from_input("   "), Err(ServerError::Empty));
        assert_eq!(
            origin_from_input("http://ahd.example.test"),
            Err(ServerError::Insecure)
        );
        assert_eq!(
            origin_from_input("http://192.168.0.10:8080"),
            Err(ServerError::Insecure)
        );
        for bad in [
            "ftp://ahd.example.test",
            "https://user:pw@ahd.example.test",
            "https://user@ahd.example.test",
            "https://",
            "https://exa mple.test",
            "file:///etc/hosts",
            "javascript:alert(1)",
        ] {
            assert_eq!(origin_from_input(bad), Err(ServerError::Invalid), "{bad}");
        }
    }

    #[test]
    fn the_saved_choice_wins_over_the_build_default_while_it_is_valid() {
        let default = "https://default.example.test";
        assert_eq!(configured(None, default).as_deref(), Some(default));
        assert_eq!(
            configured(Some("https://other.example.test\n"), default).as_deref(),
            Some("https://other.example.test")
        );
        assert_eq!(
            configured(Some("http://other.example.test"), default).as_deref(),
            Some(default)
        );
        assert_eq!(
            configured(Some("garbage"), default).as_deref(),
            Some(default)
        );
        assert_eq!(configured(None, "not a url"), None);
        assert_eq!(saved_value(default, default), None);
        assert_eq!(
            saved_value("https://other.example.test", default).as_deref(),
            Some("https://other.example.test")
        );
        assert_eq!(
            saved_value("https://other.example.test", "not a url").as_deref(),
            Some("https://other.example.test")
        );
    }

    #[test]
    fn only_the_ahd_health_document_counts_as_an_ahd_server() {
        assert_eq!(
            health_answer(200, br#"{"status":"ok","worker":"ok"}"#),
            Ok(())
        );
        assert_eq!(
            health_answer(503, br#"{"status":"degraded","worker":"stalled"}"#),
            Ok(())
        );
        for (status, body) in [
            (200, &b"<html>ok</html>"[..]),
            (200, br#"{"ok":true}"#),
            (404, br#"{"status":"ok"}"#),
            (500, br#"{"code":"internal"}"#),
            (301, b""),
        ] {
            assert_eq!(health_answer(status, body), Err(ServerError::NotAhd));
        }
    }

    #[test]
    fn a_server_change_forgets_server_data_but_keeps_device_choices() {
        assert!(PER_SERVER_FILES.contains(&"browser-login.json"));
        assert!(PER_SERVER_FILES.contains(&"notification-schedule.json"));
        for kept in [
            "preferences.json",
            "device-notifications.json",
            "update-skip.json",
            SERVER_FILE,
        ] {
            assert!(!PER_SERVER_FILES.contains(&kept), "{kept}");
        }
        // The sign-out reason of a change drops the schedule, the inbox and the widgets.
        let reason = "messages.server_changed";
        assert!(super::super::reminders::forgets_schedule(
            true,
            Some(reason)
        ));
        let registered = super::super::inbox::Registered {
            device: "d1".into(),
            owner: uuid::Uuid::from_u128(1).to_string(),
        };
        assert_eq!(
            super::super::inbox::plan(true, None, true, Some(&registered)),
            super::super::inbox::Plan::Forget
        );
    }

    #[test]
    fn every_error_has_its_own_message() {
        let errors = [
            ServerError::Empty,
            ServerError::Invalid,
            ServerError::Insecure,
            ServerError::Unreachable,
            ServerError::NotAhd,
            ServerError::NotSaved,
        ];
        let keys: std::collections::BTreeSet<_> =
            errors.iter().map(|error| error.message_key()).collect();
        assert_eq!(keys.len(), errors.len());
    }
}
