//! Bounded, process-local, single-use pending ceremonies; restart cancels all challenges.
#[cfg(test)]
#[path = "challenge_tests.rs"]
mod tests;
use super::{Passkeys, credential::OfferedCredential};
use crate::dashboard::error::{Error, Result};
use std::time::{Duration, Instant};
use uuid::Uuid;
use webauthn_rs::prelude::{PasskeyAuthentication, PasskeyRegistration};

pub(super) const CHALLENGE_TTL: Duration = Duration::from_secs(300);
const MAX_PENDING: usize = 128;

// No Debug/Serialize on pending state, credentials, or authentication results.
pub(super) struct Pending {
    pub(super) owner: Uuid,
    pub(super) expires_at: Instant,
    pub(super) ceremony: Ceremony,
}

pub(super) enum Ceremony {
    Registration {
        session_hash: String,
        label: String,
        state: PasskeyRegistration,
    },
    Authentication {
        offered: Vec<OfferedCredential>,
        state: PasskeyAuthentication,
    },
}

impl Passkeys {
    pub(super) fn insert_pending(&self, pending: Pending) -> Result<Uuid> {
        let mut entries = self.pending.lock().map_err(|_| Error::Internal)?;
        let now = Instant::now();
        entries.retain(|_, entry| entry.expires_at > now);
        if entries.len() >= MAX_PENDING {
            return Err(Error::Busy);
        }
        let id = Uuid::new_v4();
        entries.insert(id, pending);
        Ok(id)
    }

    pub(super) fn take_pending(&self, id: Uuid) -> Result<Pending> {
        let mut entries = self.pending.lock().map_err(|_| Error::Internal)?;
        let now = Instant::now();
        entries.retain(|_, entry| entry.expires_at > now);
        entries.remove(&id).ok_or(Error::Unauthorized)
    }
}
