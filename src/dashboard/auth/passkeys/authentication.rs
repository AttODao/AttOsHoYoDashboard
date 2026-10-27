//! Username-first and discoverable passkey login with current credential checks and atomic session issuance.
#[cfg(test)]
#[path = "authentication_tests.rs"]
mod tests;
use super::{
    AuthenticationChallenge, Passkeys,
    challenge::{CHALLENGE_TTL, Ceremony, Pending},
    credential::{OfferedCredential, credential_fingerprint, decode_credential, load_credentials},
    decoy,
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
use webauthn_rs::prelude::{AuthenticationResult, DiscoverableKey, PublicKeyCredential};

impl Passkeys {
    /// Username-first login for an explicitly supplied name.
    ///
    /// Names without a usable passkey get decoy options of the same shape and status, so
    /// this endpoint does not reveal which accounts exist or have passkeys.
    pub(in crate::dashboard) async fn authentication_start(
        &self,
        accounts: &Accounts,
        username: String,
    ) -> Result<AuthenticationChallenge> {
        accounts.admit_login(&username)?;
        // No account can have an invalid name, so rejecting one reveals nothing.
        accounts::validate_username(&username).map_err(|_| Error::Unauthorized)?;
        let owner = user::Entity::find()
            .filter(user::Column::Username.eq(&username))
            .one(&accounts.db)
            .await?
            .filter(|user| accounts::require_ready(user).is_ok())
            .map(|user| user.id);
        // Every path runs the same queries and one WebAuthn task; a decoy is not a shortcut.
        let rows = load_credentials(&accounts.db, owner.unwrap_or_else(Uuid::nil)).await?;
        let (owner, options, ceremony) = match owner.filter(|_| !rows.is_empty()) {
            Some(owner) => {
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
                let ceremony = Ceremony::Authentication {
                    username,
                    offered,
                    state,
                };
                (owner, options, ceremony)
            }
            None => {
                let credential_id = self.decoy.credential_id(&username)?;
                let options = self
                    .run_cpu(move |webauthn| decoy::options(webauthn, credential_id))
                    .await?;
                (Uuid::nil(), options, Ceremony::Decoy { username })
            }
        };
        let challenge_id = self.insert_pending(Pending {
            owner,
            expires_at: Instant::now() + CHALLENGE_TTL,
            ceremony,
        })?;
        Ok(AuthenticationChallenge {
            challenge_id,
            options,
        })
    }

    /// Username-less modal login; the bounded pending map is the only start limit.
    pub(in crate::dashboard) async fn discoverable_authentication_start(
        &self,
    ) -> Result<AuthenticationChallenge> {
        let (mut options, state) = self
            .run_cpu(|webauthn| {
                webauthn
                    .start_discoverable_authentication()
                    .map_err(|_| Error::Internal)
            })
            .await?;
        options.mediation = None;
        let challenge_id = self.insert_pending(Pending {
            owner: Uuid::nil(),
            expires_at: Instant::now() + CHALLENGE_TTL,
            ceremony: Ceremony::DiscoverableAuthentication { state },
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
        let (username, offered, state) = match pending.ceremony {
            Ceremony::Authentication {
                username,
                offered,
                state,
            } => (username, offered, state),
            Ceremony::Decoy { username } => {
                // Same budget and the same 401 as an assertion that fails verification.
                accounts.admit_login(&username)?;
                return Err(Error::Unauthorized);
            }
            Ceremony::DiscoverableAuthentication { state } => {
                // Identification is an untrusted hint: only cheap lookups precede admission.
                let (owner, credential_id) = self
                    .webauthn
                    .identify_discoverable_authentication(&response)
                    .map_err(|_| Error::Unauthorized)?;
                let user = user::Entity::find_by_id(owner)
                    .one(&accounts.db)
                    .await?
                    .ok_or(Error::Unauthorized)?;
                accounts::require_ready(&user).map_err(|_| Error::Unauthorized)?;
                let row = load_credentials(&accounts.db, owner)
                    .await?
                    .into_iter()
                    .find(|row| row.credential_id == credential_id)
                    .ok_or(Error::Unauthorized)?;
                accounts.admit_login(&user.username)?;
                let (result, offered) = self
                    .run_cpu(move |webauthn| {
                        let key = decode_credential(&row)?;
                        let offered = OfferedCredential {
                            id: row.id,
                            credential_id: row.credential_id,
                            fingerprint: credential_fingerprint(&row.credential)?,
                        };
                        let result = webauthn
                            .finish_discoverable_authentication(
                                &response,
                                state,
                                &[DiscoverableKey::from(&key)],
                            )
                            .map_err(|_| Error::Unauthorized)?;
                        Ok((result, offered))
                    })
                    .await?;
                return self
                    .finish_verified(accounts, owner, pending.expires_at, offered, result)
                    .await;
            }
            Ceremony::Registration { .. } => return Err(Error::Unauthorized),
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
        self.finish_verified(accounts, pending.owner, pending.expires_at, offered, result)
            .await
    }

    /// Shared atomic issuance tail; verification and counter updates stay outside the lock.
    async fn finish_verified(
        &self,
        accounts: &Accounts,
        owner: Uuid,
        expires_at: Instant,
        offered: OfferedCredential,
        result: AuthenticationResult,
    ) -> Result<(user::Model, IssuedSession)> {
        // Compute the counter/flag update from an unlocked snapshot so no CPU work runs while
        // the user row is locked. The locked re-check below makes the snapshot authoritative.
        let snapshot = find_offered(&accounts.db, owner, &offered).await?;
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
        let user = accounts::lock_user(&tx, owner).await.map_err(|error| {
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
        if Instant::now() >= expires_at {
            return Err(Error::Unauthorized);
        }
        let mut update = row.into_active_model();
        if let Some(credential) = updated {
            update.credential = Set(credential);
        }
        update.last_used_at = Set(Some(chrono::Utc::now()));
        update.update(&tx).await?;
        let session = accounts.sessions.issue(&tx, user.id).await?;
        let user = accounts::mark_signed_in(&tx, user).await?;
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
