//! Bounded password hashing and secret generation.
use crate::dashboard::error::{Error, Result};
use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier, password_hash::SaltString};
use rand::TryRngCore;
use sha2::{Digest, Sha256};
use std::sync::Arc;
use tokio::sync::Semaphore;

// Password work is bounded independently of Tokio's blocking pool.
pub(in crate::dashboard) struct Passwords {
    workers: Arc<Semaphore>,
    dummy_hash: String,
}

impl Passwords {
    pub(in crate::dashboard) async fn new() -> Result<Self> {
        let dummy_hash = tokio::task::spawn_blocking(|| hash_sync(&random_secret()?))
            .await
            .map_err(|_| Error::Internal)??;
        Ok(Self {
            workers: Arc::new(Semaphore::new(4)),
            dummy_hash,
        })
    }

    pub(in crate::dashboard) async fn hash(&self, password: String) -> Result<String> {
        if !(12..=256).contains(&password.len()) {
            return Err(Error::Invalid(
                "password must contain 12 to 256 UTF-8 bytes",
            ));
        }
        let permit = self
            .workers
            .clone()
            .try_acquire_owned()
            .map_err(|_| Error::Busy)?;
        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            hash_sync(&password)
        })
        .await
        .map_err(|_| Error::Internal)?
    }

    pub(in crate::dashboard) async fn verify(
        &self,
        password: String,
        hash: Option<&str>,
    ) -> Result<bool> {
        if password.len() > 256 {
            return Err(Error::Unauthorized);
        }
        let hash = hash.unwrap_or(&self.dummy_hash).to_owned();
        let permit = self
            .workers
            .clone()
            .try_acquire_owned()
            .map_err(|_| Error::Busy)?;
        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            let hash = PasswordHash::new(&hash).map_err(|_| Error::Password)?;
            match Argon2::default().verify_password(password.as_bytes(), &hash) {
                Ok(()) => Ok(true),
                Err(argon2::password_hash::Error::Password) => Ok(false),
                Err(_) => Err(Error::Password),
            }
        })
        .await
        .map_err(|_| Error::Internal)?
    }
}

fn hash_sync(password: &str) -> Result<String> {
    let salt = SaltString::encode_b64(&random_bytes()?).map_err(|_| Error::Password)?;
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|hash| hash.to_string())
        .map_err(|_| Error::Password)
}

fn random_bytes() -> Result<[u8; 32]> {
    let mut bytes = [0; 32];
    rand::rngs::OsRng
        .try_fill_bytes(&mut bytes)
        .map_err(|_| Error::Random)?;
    Ok(bytes)
}

pub(in crate::dashboard) fn random_secret() -> Result<String> {
    Ok(format!("{:x}", Sha256::digest(random_bytes()?)))
}

pub(in crate::dashboard) fn token_hash(raw: &str) -> Result<String> {
    if raw.len() != 64 || !raw.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(Error::Unauthorized);
    }
    Ok(format!("{:x}", Sha256::digest(raw.as_bytes())))
}
