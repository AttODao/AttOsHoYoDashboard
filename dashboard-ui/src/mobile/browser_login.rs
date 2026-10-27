//! Browser sign-in handoff for the app (PKCE S256, RFC 7636): the app keeps the verifier, the browser
//! approves the challenge, and the callback carries only a single-use code.
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub(crate) const PENDING_FILE: &str = "browser-login.json";
const CALLBACK_SCHEME: &str = "cc.attodao.ahd";
const CALLBACK_HOST: &str = "login";
pub(crate) const PENDING_TTL_MS: i64 = 10 * 60 * 1000;

pub(crate) fn verifier_from(bytes: &[u8; 32]) -> String {
    URL_SAFE_NO_PAD.encode(bytes)
}

pub(crate) fn challenge(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

pub(crate) fn start_url(origin: &str, challenge: &str) -> String {
    format!("{origin}/app-login?challenge={challenge}")
}

pub(crate) fn callback_code(uri: &str) -> Option<String> {
    let url = url::Url::parse(uri).ok()?;
    if url.scheme() != CALLBACK_SCHEME
        || url.host_str() != Some(CALLBACK_HOST)
        || !matches!(url.path(), "" | "/")
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some()
        || url.fragment().is_some()
    {
        return None;
    }
    let mut pairs = url.query_pairs();
    let (name, code) = pairs.next()?;
    if name != "code"
        || pairs.next().is_some()
        || code.len() != 64
        || !code
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return None;
    }
    Some(code.into_owned())
}

#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub(crate) struct Pending {
    pub(crate) verifier: String,
    pub(crate) created_ms: i64,
}

impl Pending {
    pub(crate) fn to_json(&self) -> String {
        serde_json::to_string(self).expect("pending login contains only a string and timestamp")
    }

    pub(crate) fn from_json(text: Option<&str>) -> Option<Self> {
        serde_json::from_str(text?).ok()
    }

    pub(crate) fn usable(&self, now_ms: i64) -> bool {
        self.verifier.len() == 43
            && now_ms
                .checked_sub(self.created_ms)
                .is_some_and(|age| (0..PENDING_TTL_MS).contains(&age))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pkce_matches_rfc7636_and_uses_url_safe_verifiers() {
        assert_eq!(
            challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
        let verifier = verifier_from(&[0; 32]);
        assert_eq!(verifier.len(), 43);
        assert!(
            verifier
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
        );
        assert_eq!(
            start_url("https://ahd.example", &challenge(&verifier)),
            format!(
                "https://ahd.example/app-login?challenge={}",
                challenge(&verifier)
            )
        );
        assert_eq!(PENDING_FILE, "browser-login.json");
    }

    #[test]
    fn callbacks_accept_only_one_lowercase_hex_code_for_our_app() {
        let code = "0123456789abcdef".repeat(4);
        for path in ["", "/"] {
            assert_eq!(
                callback_code(&format!("cc.attodao.ahd://login{path}?code={code}")),
                Some(code.clone())
            );
        }
        for uri in [
            format!("other://login?code={code}"),
            format!("cc.attodao.ahd://other?code={code}"),
            format!("https://login?code={code}"),
            format!("cc.attodao.ahd://login?code={code}&code={code}"),
            format!("cc.attodao.ahd://login?code={code}&other=x"),
            format!("cc.attodao.ahd://login?code={}", code.to_uppercase()),
            "cc.attodao.ahd://login?code=0123".into(),
            format!("cc.attodao.ahd://login?code={code}#fragment"),
            format!("cc.attodao.ahd://login/path?code={code}"),
            format!("cc.attodao.ahd://user:pass@login?code={code}"),
            format!("cc.attodao.ahd://login:123?code={code}"),
            "not a URL".into(),
        ] {
            assert_eq!(callback_code(&uri), None, "{uri}");
        }
    }

    #[test]
    fn pending_roundtrips_and_expires_without_accepting_future_timestamps() {
        let pending = Pending {
            verifier: verifier_from(&[0; 32]),
            created_ms: 10,
        };
        assert_eq!(
            Pending::from_json(Some(&pending.to_json())),
            Some(Pending {
                verifier: pending.verifier.clone(),
                created_ms: 10
            })
        );
        for text in [None, Some("garbage"), Some("{}")] {
            assert_eq!(Pending::from_json(text), None);
        }
        assert!(pending.usable(10));
        assert!(pending.usable(10 + PENDING_TTL_MS - 1));
        assert!(!pending.usable(10 + PENDING_TTL_MS));
        assert!(!pending.usable(9));
        assert!(!pending.usable(i64::MIN));
        assert!(
            !Pending {
                verifier: "short".into(),
                created_ms: 10
            }
            .usable(10)
        );
    }
}
