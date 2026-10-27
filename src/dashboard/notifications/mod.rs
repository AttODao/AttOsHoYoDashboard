//! Webhook, browser-push and app notifications: channels, conditions and delivery.
pub(super) mod app;
pub(super) mod channel;
#[cfg(test)]
#[path = "channel_tests.rs"]
mod channel_tests;
mod conditions;
mod delivery;
mod messages;
pub(super) mod push;
mod secret;
pub(super) mod worker;
#[cfg(test)]
#[path = "worker_tests.rs"]
mod worker_tests;

/// Notification kinds a user can enable, with their defaults.
pub(super) const PERSONAL_EVENTS: [(&str, bool); 11] = [
    ("reward_failed", true),
    ("reward_claimed", false),
    ("cookie_invalid", true),
    ("signed_in", false),
    ("sign_in_failed", true),
    ("resource_near_cap", true),
    ("resource_full", true),
    ("expeditions_done", true),
    ("transformer_ready", true),
    ("daily_unfinished", true),
    ("weekly_unfinished", true),
];

/// Notification kinds for the administrator channel, with their defaults.
pub(super) const ADMIN_EVENTS: [(&str, bool); 5] = [
    ("user_attention", true),
    ("worker_stalled", true),
    ("deletion_stuck", true),
    ("sign_in_failed", true),
    ("signed_in", false),
];

/// One message to deliver: its kind, a deduplication key and text parameters.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Notice {
    pub(super) kind: &'static str,
    pub(super) key: String,
    pub(super) params: Vec<(&'static str, String)>,
}

impl Notice {
    pub(super) fn new(kind: &'static str, key: impl Into<String>) -> Self {
        Self {
            kind,
            key: key.into(),
            params: Vec::new(),
        }
    }

    pub(super) fn with(mut self, name: &'static str, value: impl Into<String>) -> Self {
        self.params.push((name, value.into()));
        self
    }
}
