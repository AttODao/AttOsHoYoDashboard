//! Session state of the app's API client, kept free of I/O so it is unit-tested in the
//! browser build's native test run too.
use std::collections::BTreeMap;

/// The server's origin (`scheme://host[:port]`), or None if it is unusable: it must be
/// HTTPS, or HTTP on localhost, without a path, query or credentials.
pub(crate) fn parse_origin(value: &str) -> Option<String> {
    let url = url::Url::parse(value.trim()).ok()?;
    let local = matches!(url.host_str(), Some("localhost" | "127.0.0.1"));
    let allowed = url.scheme() == "https" || (url.scheme() == "http" && local);
    let bare = matches!(url.path(), "" | "/")
        && url.query().is_none()
        && url.fragment().is_none()
        && url.username().is_empty()
        && url.password().is_none();
    (allowed && bare && url.host_str().is_some()).then(|| url.origin().ascii_serialization())
}

/// Cookies set by the one configured server, by name. Values are never logged.
#[derive(Default, PartialEq)]
pub(crate) struct Jar {
    cookies: BTreeMap<String, String>,
}

impl Jar {
    /// A saved jar; anything unreadable starts empty (signed out).
    pub(crate) fn from_json(text: Option<&str>) -> Self {
        let cookies = text
            .and_then(|text| serde_json::from_str(text).ok())
            .unwrap_or_default();
        Self { cookies }
    }

    /// None when empty: there is nothing to keep.
    pub(crate) fn to_json(&self) -> Option<String> {
        if self.cookies.is_empty() {
            return None;
        }
        serde_json::to_string(&self.cookies).ok()
    }

    pub(crate) fn header(&self) -> Option<String> {
        (!self.cookies.is_empty()).then(|| {
            self.cookies
                .iter()
                .map(|(name, value)| format!("{name}={value}"))
                .collect::<Vec<_>>()
                .join("; ")
        })
    }

    /// Applies one Set-Cookie line; `Max-Age` of zero or less deletes. Returns whether
    /// the jar changed.
    pub(crate) fn apply(&mut self, line: &str) -> bool {
        let mut parts = line.split(';');
        let Some((name, value)) = parts.next().and_then(|pair| pair.split_once('=')) else {
            return false;
        };
        let (name, value) = (name.trim(), value.trim());
        if name.is_empty() || !name.bytes().all(|b| b.is_ascii_graphic() && b != b'=') {
            return false;
        }
        let expired = parts.any(|attribute| {
            attribute.trim().split_once('=').is_some_and(|(key, age)| {
                key.trim().eq_ignore_ascii_case("max-age")
                    && age.trim().parse::<i64>().map_or(true, |age| age <= 0)
            })
        });
        if expired || value.is_empty() {
            self.cookies.remove(name).is_some()
        } else {
            self.cookies
                .insert(name.to_owned(), value.to_owned())
                .as_deref()
                != Some(value)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn server_must_be_https_or_local_http_without_a_path() {
        assert_eq!(
            parse_origin("https://ahd.example.test/").as_deref(),
            Some("https://ahd.example.test")
        );
        assert_eq!(
            parse_origin("http://localhost:8080").as_deref(),
            Some("http://localhost:8080")
        );
        assert!(parse_origin("http://ahd.example.test").is_none());
        assert!(parse_origin("https://ahd.example.test/sub").is_none());
        assert!(parse_origin("https://user:pw@ahd.example.test").is_none());
        assert!(parse_origin("ahd.example.test").is_none());
    }

    #[test]
    fn jar_roundtrips_and_unreadable_saves_start_signed_out() {
        let mut jar = Jar::default();
        assert_eq!(jar.to_json(), None);
        jar.apply("attos_session=synthetic; Max-Age=60");
        let saved = jar.to_json().unwrap();
        assert!(Jar::from_json(Some(&saved)) == jar);
        assert!(Jar::from_json(Some("{")) == Jar::default());
        assert!(Jar::from_json(None) == Jar::default());
    }

    #[test]
    fn jar_stores_replaces_and_deletes_session_cookies() {
        let mut jar = Jar::default();
        assert!(
            jar.apply("__Host-attos_session=synthetic; Path=/; HttpOnly; Secure; Max-Age=604800")
        );
        assert!(!jar.apply("__Host-attos_session=synthetic; Path=/"));
        assert_eq!(
            jar.header().as_deref(),
            Some("__Host-attos_session=synthetic")
        );
        assert!(jar.apply("__Host-attos_session=; Path=/; Max-Age=0"));
        assert_eq!(jar.header(), None);
        assert!(!jar.apply("not-a-cookie"));
        assert!(!jar.apply("bad name=value"));
    }
}
