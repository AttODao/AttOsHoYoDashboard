//! Session-bound passkey registration and its user-locked persistence transaction.
#[cfg(test)]
#[path = "registration_tests.rs"]
mod tests;
use super::{
    MAX_PASSKEYS, PasskeyView, Passkeys, RegistrationChallenge,
    challenge::{CHALLENGE_TTL, Ceremony, Pending},
    credential::{
        MAX_CREDENTIAL_ID_BYTES, credential_fingerprint, decode_credential, load_credentials,
    },
    validate_label,
};
use crate::dashboard::{
    auth::{
        accounts::{self, Accounts},
        sessions::{Identity, Sessions},
    },
    db::entities::passkey,
    error::{Error, Result},
};
use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter, Set, SqlErr,
    TransactionTrait,
};
use std::time::Instant;
use uuid::Uuid;
use webauthn_rs::prelude::RegisterPublicKeyCredential;

impl Passkeys {
    pub(in crate::dashboard) async fn registration_start(
        &self,
        accounts: &Accounts,
        actor: &Identity,
        label: String,
    ) -> Result<RegistrationChallenge> {
        let label = validate_label(label)?;
        let fresh = Sessions::authenticate(&accounts.db, &actor.token_hash).await?;
        accounts::require_ready(&fresh.user)?;
        let rows = load_credentials(&accounts.db, fresh.user.id).await?;
        if rows.len() as u64 >= MAX_PASSKEYS {
            return Err(Error::Invalid("passkey limit reached"));
        }
        let owner = fresh.user.id;
        let username = fresh.user.username;
        let (options, state) = self
            .run_cpu(move |webauthn| {
                let exclude = rows
                    .into_iter()
                    .map(|row| decode_credential(&row).map(|key| key.cred_id().clone()))
                    .collect::<Result<Vec<_>>>()?;
                webauthn
                    .start_passkey_registration(owner, &username, &username, Some(exclude))
                    .map_err(|_| Error::Internal)
            })
            .await?;
        let challenge_id = self.insert_pending(Pending {
            owner,
            expires_at: Instant::now() + CHALLENGE_TTL,
            ceremony: Ceremony::Registration {
                session_hash: fresh.token_hash,
                label,
                state,
            },
        })?;
        Ok(RegistrationChallenge {
            challenge_id,
            options,
        })
    }

    pub(in crate::dashboard) async fn registration_finish(
        &self,
        accounts: &Accounts,
        actor: &Identity,
        challenge_id: Uuid,
        response: RegisterPublicKeyCredential,
    ) -> Result<PasskeyView> {
        // Consume first, including invalid or failed attempts: no replay after cancellation.
        let pending = self.take_pending(challenge_id)?;
        let Ceremony::Registration {
            session_hash,
            label,
            state,
        } = pending.ceremony
        else {
            return Err(Error::Unauthorized);
        };
        if pending.owner != actor.user.id || session_hash != actor.token_hash {
            return Err(Error::Unauthorized);
        }
        let fresh = Sessions::authenticate(&accounts.db, &session_hash).await?;
        accounts::require_ready(&fresh.user)?;
        let (credential_id, credential) = self
            .run_cpu(move |webauthn| {
                let key = webauthn
                    .finish_passkey_registration(&response, &state)
                    .map_err(|_| Error::Unauthorized)?;
                let credential_id: Vec<u8> = key.cred_id().as_ref().to_vec();
                if credential_id.is_empty() || credential_id.len() > MAX_CREDENTIAL_ID_BYTES {
                    return Err(Error::Invalid("unsupported passkey credential size"));
                }
                let credential = serde_json::to_value(key).map_err(|_| Error::Internal)?;
                credential_fingerprint(&credential)?;
                Ok((credential_id, credential))
            })
            .await?;
        let tx = accounts.db.begin().await?;
        accounts::lock_user(&tx, pending.owner).await?;
        let fresh = Sessions::authenticate(&tx, &session_hash).await?;
        accounts::require_ready(&fresh.user)?;
        if fresh.user.id != pending.owner || Instant::now() >= pending.expires_at {
            return Err(Error::Unauthorized);
        }
        if passkey::Entity::find()
            .filter(passkey::Column::UserId.eq(pending.owner))
            .count(&tx)
            .await?
            >= MAX_PASSKEYS
        {
            return Err(Error::Invalid("passkey limit reached"));
        }
        let row = passkey::ActiveModel {
            id: Set(Uuid::new_v4()),
            user_id: Set(pending.owner),
            credential_id: Set(credential_id),
            label: Set(label),
            credential: Set(credential),
            created_at: Set(Utc::now()),
            last_used_at: Set(None),
        }
        .insert(&tx)
        .await
        .map_err(|error| {
            if matches!(error.sql_err(), Some(SqlErr::UniqueConstraintViolation(_))) {
                Error::Invalid("passkey already registered")
            } else {
                Error::Database(error)
            }
        })?;
        accounts::record(
            &tx,
            Some(pending.owner),
            pending.owner,
            "passkey_registered",
        )
        .await?;
        tx.commit().await?;
        Ok(PasskeyView {
            id: row.id,
            label: row.label,
            created_at: row.created_at,
            last_used_at: row.last_used_at,
        })
    }
}
