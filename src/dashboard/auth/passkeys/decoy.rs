//! Passkey login options for names without a usable passkey, shaped like a real user's.
//!
//! Unknown, inactive, setup-pending, and passkey-less users receive a stored challenge that
//! can never verify, with one credential ID derived from the username by HMAC, so repeated
//! probes of one name stay consistent. The key comes from `HOYOLAB_COOKIE_KEY` (domain-
//! separated, stable across restarts) or, without it, a random per-process key.
//!
//! Residual differences: a user with several passkeys lists several IDs; real ID lengths
//! follow the user's authenticators, and transports appear only for credentials registered
//! with packed/TPM attestation. A decoy always lists one 32-byte ID without transports,
//! matching the common case of one passkey registered without attestation.
#[cfg(test)]
#[path = "decoy_tests.rs"]
mod tests;

use crate::dashboard::{
    error::{Error, Result},
    hoyolab::credentials::CookieKey,
};
use openssl::{hash::MessageDigest, pkey::PKey, sign::Signer};
use rand::TryRngCore;
use serde_json::json;
use webauthn_rs::prelude::{Base64UrlSafeData, RequestChallengeResponse, Webauthn};

const PURPOSE: &[u8] = b"attoshoyodashboard:dashboard:passkey-decoy-credential:v1";

pub(super) struct DecoyKey([u8; 32]);

impl DecoyKey {
    pub(super) fn new(seed: Option<&CookieKey>) -> Result<Self> {
        match seed {
            Some(seed) => Ok(Self(seed.derive(PURPOSE)?)),
            None => {
                let mut key = [0; 32];
                rand::rngs::OsRng
                    .try_fill_bytes(&mut key)
                    .map_err(|_| Error::Random)?;
                Ok(Self(key))
            }
        }
    }

    /// Lookup is exact, so the exact username is the HMAC input.
    pub(super) fn credential_id(&self, username: &str) -> Result<Vec<u8>> {
        let key = PKey::hmac(&self.0).map_err(|_| Error::Internal)?;
        let mut signer = Signer::new(MessageDigest::sha256(), &key).map_err(|_| Error::Internal)?;
        signer
            .update(username.as_bytes())
            .map_err(|_| Error::Internal)?;
        signer.sign_to_vec().map_err(|_| Error::Internal)
    }
}

/// Options matching `start_passkey_authentication` for one credential; the challenge is
/// fresh from WebAuthn and its state is dropped, so no assertion can complete it.
pub(super) fn options(
    webauthn: &Webauthn,
    credential_id: Vec<u8>,
) -> Result<RequestChallengeResponse> {
    let (mut options, _unusable) = webauthn
        .start_discoverable_authentication()
        .map_err(|_| Error::Internal)?;
    // Discoverable login adds conditional mediation and the uvm extension; passkey login does not.
    options.mediation = None;
    options.public_key.extensions = None;
    options.public_key.allow_credentials = serde_json::from_value(json!([{
        "type": "public-key",
        "id": Base64UrlSafeData::from(credential_id),
    }]))
    .map_err(|_| Error::Internal)?;
    Ok(options)
}
