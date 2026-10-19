//! Username-first passkey login with current credential checks and atomic session issuance.
#[cfg(test)]
#[path = "authentication_tests.rs"]
mod tests;
use super::{
    AuthenticationChallenge, Passkeys,
    challenge::{CHALLENGE_TTL, Ceremony, Pending},
    credential::{OfferedCredential, credential_fingerprint, decode_credential, load_credentials},
};
use crate::dashboard::{
    auth::{
        accounts::{self, Accounts},
        sessions::{IssuedSession, Sessions},
    },
    db::entities::{passkey, user},
    error::{Error, Result},
};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, EntityTrait, IntoActiveModel, QueryFilter, Set,
    TransactionTrait,
};
use std::time::Instant;
use uuid::Uuid;
use webauthn_rs::prelude::PublicKeyCredential;

impl Passkeys {
    /// Username-first login, not discoverable/username-less credentials.
    pub(in crate::dashboard) async fn authentication_start(
        &self,
        accounts: &Accounts,
        username: String,
    ) -> Result<AuthenticationChallenge> {
        accounts.admit_login(&username)?;
        if username.is_empty() || username.len() > 64 {
            return Err(Error::Unauthorized);
        }
        let user = user::Entity::find()
            .filter(user::Column::Username.eq(&username))
            .one(&accounts.db)
            .await?
            .ok_or(Error::Unauthorized)?;
        accounts::require_ready(&user).map_err(|_| Error::Unauthorized)?;
        let rows = load_credentials(&accounts.db, user.id).await?;
        if rows.is_empty() {
            return Err(Error::Unauthorized);
        }
        let (options, state, offered) = self
            .run_cpu(move |webauthn| {
                let mut keys = Vec::with_capacity(rows.len());
                let mut offered = Vec::with_capacity(rows.len());
                for row in rows {
                    keys.push(decode_credential(&row)?);
                    offered.push(OfferedCredential {
                        id: row.id,
                        credential_id: row.credential_id,
                        fingerprint: credential_fingerprint(&row.credential)?,
                    });
                }
                let (options, state) = webauthn
                    .start_passkey_authentication(&keys)
                    .map_err(|_| Error::Unauthorized)?;
                Ok((options, state, offered))
            })
            .await?;
        let challenge_id = self.insert_pending(Pending {
            owner: user.id,
            expires_at: Instant::now() + CHALLENGE_TTL,
            ceremony: Ceremony::Authentication {
                username,
                offered,
                state,
            },
        })?;
        Ok(AuthenticationChallenge {
            challenge_id,
            options,
        })
    }

    pub(in crate::dashboard) async fn authentication_finish(
        &self,
        accounts: &Accounts,
        challenge_id: Uuid,
        response: PublicKeyCredential,
    ) -> Result<(user::Model, IssuedSession)> {
        let pending = self.take_pending(challenge_id)?;
        let Ceremony::Authentication {
            username,
            offered,
            state,
        } = pending.ceremony
        else {
            return Err(Error::Unauthorized);
        };
        // An unknown challenge costs no verification work; a known one spends its user's budget.
        accounts.admit_login(&username)?;
        let result = self
            .run_cpu(move |webauthn| {
                webauthn
                    .finish_passkey_authentication(&response, &state)
                    .map_err(|_| Error::Unauthorized)
            })
            .await?;
        let offered = offered
            .into_iter()
            .find(|key| key.credential_id.as_slice() == result.cred_id().as_ref())
            .ok_or(Error::Unauthorized)?;
        // Compute the counter/flag update from an unlocked snapshot so no CPU work runs while
        // the user row is locked. The locked re-check below makes the snapshot authoritative.
        let snapshot = find_offered(&accounts.db, pending.owner, &offered).await?;
        let fingerprint = offered.fingerprint;
        let updated = self
            .run_cpu(move |_| {
                // A changed credential means the verification used stale counters/flags.
                // Conservatively retry concurrent logins rather than accepting a stale assertion.
                if credential_fingerprint(&snapshot.credential)? != fingerprint {
                    return Err(Error::Unauthorized);
                }
                let mut updated = decode_credential(&snapshot)?;
                if !updated
                    .update_credential(&result)
                    .ok_or(Error::Unauthorized)?
                {
                    return Ok(None);
                }
                let credential = serde_json::to_value(updated).map_err(|_| Error::Internal)?;
                credential_fingerprint(&credential)?;
                Ok(Some(credential))
            })
            .await?;
        Sessions::purge_expired(&accounts.db).await?;
        let tx = accounts.db.begin().await?;
        let user = accounts::lock_user(&tx, pending.owner)
            .await
            .map_err(|error| {
                if matches!(error, Error::NotFound) {
                    Error::Unauthorized
                } else {
                    error
                }
            })?;
        accounts::require_ready(&user).map_err(|_| Error::Unauthorized)?;
        // All deletion, recovery, and credential updates use this same user lock.
        // Row UUID also rejects deletion followed by re-registration of the same key.
        let row = find_offered(&tx, user.id, &offered).await?;
        // Unchanged stored bytes mean the precomputed update applies to the current state;
        // any concurrent login, deletion, or recovery since the snapshot is rejected here.
        if credential_fingerprint(&row.credential)? != offered.fingerprint {
            return Err(Error::Unauthorized);
        }
        if Instant::now() >= pending.expires_at {
            return Err(Error::Unauthorized);
        }
        if let Some(credential) = updated {
            let mut update = row.into_active_model();
            update.credential = Set(credential);
            update.update(&tx).await?;
        }
        let session = accounts.sessions.issue(&tx, user.id).await?;
        tx.commit().await?;
        Ok((user, session))
    }
}

async fn find_offered(
    db: &impl ConnectionTrait,
    owner: Uuid,
    offered: &OfferedCredential,
) -> Result<passkey::Model> {
    passkey::Entity::find_by_id(offered.id)
        .filter(passkey::Column::UserId.eq(owner))
        .filter(passkey::Column::CredentialId.eq(offered.credential_id.clone()))
        .one(db)
        .await?
        .ok_or(Error::Unauthorized)
}
