//! Webhook delivery: validated HTTPS endpoints, Discord embeds or plain JSON.
use super::{Notice, messages::render};
use crate::dashboard::error::{Error, Result};
use chrono::Utc;
use reqwest::{Client, Url};
use serde_json::json;
use std::{sync::OnceLock, time::Duration};

const MAX_URL: usize = 1024;

/// HTTPS to a DNS name only: no credentials, IP literals, or local names, so a
/// webhook cannot be pointed at the server's own network by address.
pub(super) fn validate_webhook_url(raw: &str) -> Result<String> {
    let invalid = || Error::Invalid("webhook URL must be an HTTPS URL on a public host name");
    let raw = raw.trim();
    if raw.is_empty() || raw.len() > MAX_URL {
        return Err(invalid());
    }
    let url = Url::parse(raw).map_err(|_| invalid())?;
    let host = url.domain().ok_or_else(invalid)?.to_ascii_lowercase();
    let local = host == "localhost"
        || [".localhost", ".local", ".internal", ".lan", ".home.arpa"]
            .iter()
            .any(|suffix| host.ends_with(suffix))
        || !host.contains('.');
    if url.scheme() != "https" || !url.username().is_empty() || url.password().is_some() || local {
        return Err(invalid());
    }
    Ok(url.into())
}

/// A short, non-secret description of a stored URL, e.g. "discord.com/…/a1b2".
pub(super) fn hint(url: &str) -> String {
    let Ok(parsed) = Url::parse(url) else {
        return "…".to_owned();
    };
    let tail: String = url
        .chars()
        .rev()
        .take(4)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    format!("{}/…/{tail}", parsed.host_str().unwrap_or_default())
}

fn client() -> Option<&'static Client> {
    static CLIENT: OnceLock<Option<Client>> = OnceLock::new();
    CLIENT
        .get_or_init(|| {
            Client::builder()
                .timeout(Duration::from_secs(10))
                .redirect(reqwest::redirect::Policy::none())
                .user_agent("AHD notifications")
                .build()
                .ok()
        })
        .as_ref()
}

/// The request body for a notice in the channel's format.
pub(super) fn payload(notice: &Notice, format: &str, language: &str) -> serde_json::Value {
    let (title, body) = render(notice, language);
    let occurred_at = Utc::now().to_rfc3339();
    if format == "discord" {
        // Embed color: warning for problems, cyan otherwise.
        let color = match notice.kind {
            "reward_failed"
            | "cookie_invalid"
            | "sign_in_failed"
            | "resource_full"
            | "user_attention"
            | "worker_stalled"
            | "deletion_stuck"
            | "admin_sign_in_failed" => 0xFFD585,
            _ => 0x49EEFF,
        };
        json!({
            "username": "AHD",
            "allowed_mentions": {"parse": []},
            "embeds": [{"title": title, "description": body, "color": color, "timestamp": occurred_at}],
        })
    } else {
        json!({"source": "ahd", "event": notice.kind, "title": title, "message": body, "occurred_at": occurred_at})
    }
}

/// Posts once; any 2xx counts as delivered. Errors never include the URL.
pub(super) async fn post(url: &str, body: &serde_json::Value) -> bool {
    let Some(client) = client() else {
        return false;
    };
    let request = client
        .post(url)
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .body(body.to_string());
    match request.send().await {
        Ok(response) => response.status().is_success(),
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn webhook_urls_must_be_public_https_names() {
        assert_eq!(
            validate_webhook_url(" https://discord.com/api/webhooks/1/abc ").unwrap(),
            "https://discord.com/api/webhooks/1/abc"
        );
        for bad in [
            "",
            "http://discord.com/api/webhooks/1/abc",
            "https://user:pass@discord.com/x",
            "https://127.0.0.1/hook",
            "https://[::1]/hook",
            "https://localhost/hook",
            "https://router.lan/hook",
            "https://nas.local/hook",
            "https://intranet/hook",
            "ftp://example.com/x",
            "not a url",
        ] {
            assert!(validate_webhook_url(bad).is_err(), "{bad}");
        }
        assert!(
            validate_webhook_url(&format!("https://example.com/{}", "a".repeat(1100))).is_err()
        );
        assert_eq!(
            hint("https://discord.com/api/webhooks/1/secrettoken"),
            "discord.com/…/oken"
        );
    }

    #[test]
    fn discord_and_json_payloads_carry_the_rendered_text() {
        let notice = Notice::new("test", "k");
        let discord = payload(&notice, "discord", "en");
        assert_eq!(discord["embeds"][0]["title"], "Test notification");
        assert_eq!(discord["allowed_mentions"]["parse"], json!([]));
        let plain = payload(&notice, "json", "ja");
        assert_eq!(plain["event"], "test");
        assert_eq!(plain["title"], "テスト通知");
    }
}
