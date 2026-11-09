//! Platform-free view of the UnifiedPush state the Kotlin side (`AhdPush`) reports: what the
//! Notifications page says about instant delivery.
use serde::Deserialize;

#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
pub(crate) struct PushState {
    /// The "receive instantly" switch.
    #[serde(default)]
    pub(crate) wanted: bool,
    /// off, no_distributor, waiting, registering, active or failed.
    #[serde(default)]
    status: String,
    /// Why registration failed: "rejected" (endpoint refused by the server), "server",
    /// "unregistered" or a distributor reason such as "NETWORK".
    #[serde(default)]
    reason: String,
    /// The distributor app's name.
    #[serde(default)]
    distributor: Option<String>,
}

impl PushState {
    pub(crate) fn from_json(text: Option<&str>) -> Self {
        text.and_then(|text| serde_json::from_str(text).ok())
            .unwrap_or_default()
    }

    /// The status line under the switch: catalog key, `{app}` value and CSS class.
    pub(crate) fn line(&self) -> Option<(&'static str, String, &'static str)> {
        let app = self.distributor.clone().unwrap_or_default();
        match self.status.as_str() {
            _ if !self.wanted => None,
            "no_distributor" => Some(("notifications.device_push_no_distributor", app, "warning")),
            "registering" => Some(("notifications.device_push_pending", app, "")),
            "active" => Some(("notifications.device_push_active", app, "status")),
            "failed" if self.reason == "rejected" => {
                Some(("notifications.device_push_rejected", app, "warning"))
            }
            "failed" => Some(("notifications.device_push_failed", app, "warning")),
            // Waiting for the server registration, which has its own line above.
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_page_explains_every_push_state() {
        let state = |json: &str| PushState::from_json(Some(json));
        assert_eq!(PushState::from_json(None), PushState::default());
        assert_eq!(PushState::from_json(Some("not json")), PushState::default());
        assert_eq!(state(r#"{"wanted":false,"status":"off"}"#).line(), None);
        assert_eq!(
            state(r#"{"wanted":true,"status":"no_distributor"}"#).line(),
            Some((
                "notifications.device_push_no_distributor",
                String::new(),
                "warning"
            ))
        );
        assert_eq!(
            state(r#"{"wanted":true,"status":"registering","distributor":"ntfy"}"#).line(),
            Some(("notifications.device_push_pending", "ntfy".into(), ""))
        );
        assert_eq!(
            state(r#"{"wanted":true,"status":"active","distributor":"ntfy"}"#).line(),
            Some(("notifications.device_push_active", "ntfy".into(), "status"))
        );
        assert_eq!(
            state(r#"{"wanted":true,"status":"failed","reason":"rejected","distributor":"ntfy"}"#)
                .line()
                .map(|line| line.0),
            Some("notifications.device_push_rejected")
        );
        assert_eq!(
            state(r#"{"wanted":true,"status":"failed","reason":"NETWORK"}"#)
                .line()
                .map(|line| line.0),
            Some("notifications.device_push_failed")
        );
        assert_eq!(state(r#"{"wanted":true,"status":"waiting"}"#).line(), None);
        // A switched-off state never shows a stale failure.
        assert_eq!(state(r#"{"wanted":false,"status":"failed"}"#).line(), None);
    }
}
