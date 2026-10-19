//! Fixed-window attempt budgets for password and passkey checks.
//!
//! Requests arrive through a reverse proxy, so the peer address cannot identify a client.
//! Sign-in is keyed by the attempted username so one user's failures do not lock out others;
//! in-session current-password checks are keyed by the signed-in user's id in a separate map,
//! so a stolen session cannot exhaust its owner's sign-in budget (or the reverse).
//! Overall CPU stays bounded by the password and WebAuthn worker limits.
#[cfg(test)]
#[path = "login_limit_tests.rs"]
mod tests;

use super::accounts::validate_username;
use crate::dashboard::error::{Error, Result};
use std::{
    collections::HashMap,
    hash::Hash,
    sync::Mutex,
    time::{Duration, Instant},
};

const WINDOW: Duration = Duration::from_secs(60);
const ATTEMPTS: u32 = 20;
const MAX_BUCKETS: usize = 1024;
/// Shared key for names no account can have, so they cannot grow the map.
const INVALID: &str = "";

struct Bucket {
    started: Instant,
    attempts: u32,
}

pub(super) struct AttemptLimiter<K>(Mutex<HashMap<K, Bucket>>);

impl<K> Default for AttemptLimiter<K> {
    fn default() -> Self {
        Self(Mutex::new(HashMap::new()))
    }
}

/// Sign-in attempts per attempted username.
pub(super) type LoginLimiter = AttemptLimiter<String>;

impl LoginLimiter {
    pub(super) fn admit_username(&self, username: &str, now: Instant) -> Result<()> {
        self.admit(key(username), now)
    }

    #[cfg(test)]
    pub(super) fn username_attempts(&self, username: &str) -> u32 {
        self.attempts(&key(username))
    }
}

impl<K: Hash + Eq + Clone> AttemptLimiter<K> {
    pub(super) fn admit(&self, key: K, now: Instant) -> Result<()> {
        let mut buckets = self.0.lock().map_err(|_| Error::Internal)?;
        if !buckets.contains_key(&key) && buckets.len() >= MAX_BUCKETS {
            buckets.retain(|_, bucket| now.duration_since(bucket.started) < WINDOW);
            if buckets.len() >= MAX_BUCKETS {
                // Evicting the oldest window costs a sprayer one attempt per evicted name.
                let oldest = buckets
                    .iter()
                    .min_by_key(|(_, bucket)| bucket.started)
                    .map(|(name, _)| name.clone());
                if let Some(oldest) = oldest {
                    buckets.remove(&oldest);
                }
            }
        }
        let bucket = buckets.entry(key).or_insert(Bucket {
            started: now,
            attempts: 0,
        });
        if now.duration_since(bucket.started) >= WINDOW {
            bucket.started = now;
            bucket.attempts = 0;
        }
        if bucket.attempts >= ATTEMPTS {
            return Err(Error::RateLimited);
        }
        bucket.attempts += 1;
        Ok(())
    }

    #[cfg(test)]
    pub(super) fn attempts(&self, key: &K) -> u32 {
        self.0
            .lock()
            .unwrap()
            .get(key)
            .map_or(0, |bucket| bucket.attempts)
    }
}

/// Lookup is exact, so the key is too; names that fail account validation share one bucket.
fn key(username: &str) -> String {
    match validate_username(username) {
        Ok(()) => username.to_owned(),
        Err(_) => INVALID.to_owned(),
    }
}
