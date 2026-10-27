//! Browser-to-app sign-in handoff: a signed-in browser session approves the app's PKCE (S256) challenge and
//! gets a one-time code; the app redeems it with its verifier for a session of its own.
#[cfg(test)]
#[path = "app_logins_tests.rs"]
mod tests;

use super::{
    accounts::{self, Accounts},
    password,
    sessions::{Identity, IssuedSession, Sessions},
};
use crate::dashboard::{
    db::entities::user,
    error::{Error, Result},
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use sea_orm::TransactionTrait;
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    sync::Mutex,
    time::{Duration, Instant},
};
use uuid::Uuid;

pub(in crate::dashboard) const CODE_TTL: Duration = Duration::from_secs(120);
const MAX_PENDING: usize = 256;

struct Grant {
    user_id: Uuid,
    session_hash: String,
    challenge: String,
    expires_at: Instant,
}

pub(in crate::dashboard) struct AppLogins {
    ttl: Duration,
    pending: Mutex<HashMap<String, Grant>>,
}

impl AppLogins {
    pub(in crate::dashboard) fn new() -> Self {
        Self {
            ttl: CODE_TTL,
            pending: Mutex::new(HashMap::new()),
        }
    }

    #[cfg(test)]
    fn with_ttl(ttl: Duration) -> Self {
        Self {
            ttl,
            pending: Mutex::new(HashMap::new()),
        }
    }

    pub(in crate::dashboard) async fn approve(
        &self,
        accounts: &Accounts,
        actor: &Identity,
        challenge: String,
    ) -> Result<String> {
        if !valid_challenge(&challenge) {
            return Err(Error::Invalid(
                "code_challenge must be a base64url SHA-256 value",
            ));
        }
        actor.session_expires_at()?;
        let fresh = Sessions::authenticate(&accounts.db, &actor.token_hash).await?;
        let code = password::random_secret()?;
        let key = password::token_hash(&code)?;
        let mut pending = self.pending.lock().map_err(|_| Error::Internal)?;
        let now = Instant::now();
        pending.retain(|_, grant| grant.expires_at > now);
        if pending.len() >= MAX_PENDING {
            return Err(Error::Busy);
        }
        pending.insert(
            key,
            Grant {
                user_id: fresh.user.id,
                session_hash: actor.token_hash.clone(),
                challenge,
                expires_at: Instant::now() + self.ttl,
            },
        );
        Ok(code)
    }

    pub(in crate::dashboard) async fn exchange(
        &self,
        accounts: &Accounts,
        code: &str,
        verifier: &str,
    ) -> Result<(user::Model, IssuedSession)> {
        let key = password::token_hash(code)?;
        let grant = {
            let mut pending = self.pending.lock().map_err(|_| Error::Internal)?;
            let grant = pending.remove(&key);
            let now = Instant::now();
            pending.retain(|_, grant| grant.expires_at > now);
            grant.ok_or(Error::Unauthorized)?
        };
        if grant.expires_at <= Instant::now() {
            return Err(Error::Unauthorized);
        }
        if !(43..=128).contains(&verifier.len())
            || !verifier
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~'))
        {
            return Err(Error::Unauthorized);
        }
        let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
        if challenge.len() != grant.challenge.len()
            || challenge
                .bytes()
                .zip(grant.challenge.bytes())
                .fold(0u8, |diff, (a, b)| diff | (a ^ b))
                != 0
        {
            return Err(Error::Unauthorized);
        }
        let tx = accounts.db.begin().await?;
        let user = accounts::lock_user(&tx, grant.user_id)
            .await
            .map_err(|_| Error::Unauthorized)?;
        let browser = Sessions::authenticate(&tx, &grant.session_hash).await?;
        if browser.user.id != user.id || !user.active {
            return Err(Error::Unauthorized);
        }
        let session = accounts.sessions.issue(&tx, user.id).await?;
        let user = accounts::mark_signed_in(&tx, user).await?;
        tx.commit().await?;
        Ok((user, session))
    }
}

fn valid_challenge(challenge: &str) -> bool {
    challenge.len() == 43
        && challenge
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
}
