//! Optional WebAuthn construction, bounded CPU work, and credential management.
mod authentication;
mod challenge;
mod credential;
mod decoy;
mod registration;
#[cfg(test)]
mod test_support;
#[cfg(test)]
#[path = "management_tests.rs"]
mod tests;

use super::{
    accounts::{self, Accounts},
    sessions::{Identity, Sessions},
};
use crate::dashboard::{
    db::entities::passkey,
    error::{Error, Result},
    hoyolab::credentials::CookieKey,
};
use challenge::{CHALLENGE_TTL, Pending};
use chrono::{DateTime, Utc};
use decoy::DecoyKey;
use sea_orm::{
    ColumnTrait, ConnectionTrait, EntityTrait, FromQueryResult, QueryFilter, QueryOrder,
    QuerySelect, TransactionTrait, sea_query::Expr,
};
use serde::Serialize;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};
use tokio::sync::Semaphore;
use uuid::Uuid;
use webauthn_rs::prelude::{
    CreationChallengeResponse, RequestChallengeResponse, Url, Webauthn, WebauthnBuilder,
};

// Fixed limits suffice for a private, single-instance dashboard. No waiting CPU queue.
const MAX_PASSKEYS: u64 = 20;

pub(in crate::dashboard) struct Passkeys {
    webauthn: Arc<Webauthn>,
    cpu: Arc<Semaphore>,
    pending: Mutex<HashMap<Uuid, Pending>>,
    decoy: DecoyKey,
}

// These are the only persisted credential fields allowed in an HTTP list response.
#[derive(Serialize, FromQueryResult)]
pub(in crate::dashboard) struct PasskeyView {
    pub(in crate::dashboard) id: Uuid,
    pub(in crate::dashboard) label: String,
    pub(in crate::dashboard) created_at: DateTime<Utc>,
    pub(in crate::dashboard) last_used_at: Option<DateTime<Utc>>,
}

#[derive(Serialize)]
pub(in crate::dashboard) struct RegistrationChallenge {
    pub(in crate::dashboard) challenge_id: Uuid,
    pub(in crate::dashboard) options: CreationChallengeResponse,
}

#[derive(Serialize)]
pub(in crate::dashboard) struct AuthenticationChallenge {
    pub(in crate::dashboard) challenge_id: Uuid,
    pub(in crate::dashboard) options: RequestChallengeResponse,
}

impl Passkeys {
    /// Origin and RP relationship checks are delegated to WebAuthn, with no extra origins.
    /// `decoy_seed` keeps decoy login options stable across restarts when configured.
    pub(in crate::dashboard) fn new(
        rp_id: &str,
        origin: &str,
        decoy_seed: Option<&CookieKey>,
    ) -> Result<Self> {
        let origin = Url::parse(origin).map_err(|_| Error::Invalid("invalid passkey origin"))?;
        let webauthn =
            tracing::subscriber::with_default(tracing::subscriber::NoSubscriber::default(), || {
                WebauthnBuilder::new(rp_id, &origin).and_then(|builder| {
                    builder
                        .rp_name("AttOsHoYoDashboard")
                        .timeout(CHALLENGE_TTL)
                        .build()
                })
            })
            .map_err(|_| Error::Invalid("invalid passkey RP configuration"))?;
        Ok(Self {
            webauthn: Arc::new(webauthn),
            cpu: Arc::new(Semaphore::new(4)),
            pending: Mutex::new(HashMap::new()),
            decoy: DecoyKey::new(decoy_seed)?,
        })
    }

    pub(in crate::dashboard) async fn list(
        &self,
        accounts: &Accounts,
        actor: &Identity,
    ) -> Result<Vec<PasskeyView>> {
        let fresh = Sessions::authenticate(&accounts.db, &actor.token_hash).await?;
        accounts::require_ready(&fresh.user)?;
        Ok(passkey::Entity::find()
            .select_only()
            .columns([
                passkey::Column::Id,
                passkey::Column::Label,
                passkey::Column::CreatedAt,
                passkey::Column::LastUsedAt,
            ])
            .filter(passkey::Column::UserId.eq(fresh.user.id))
            .order_by_asc(passkey::Column::CreatedAt)
            .limit(MAX_PASSKEYS)
            .into_model::<PasskeyView>()
            .all(&accounts.db)
            .await?)
    }

    pub(in crate::dashboard) async fn delete(
        &self,
        accounts: &Accounts,
        actor: &Identity,
        id: Uuid,
    ) -> Result<()> {
        let tx = accounts.db.begin().await?;
        accounts::lock_user(&tx, actor.user.id).await?;
        let fresh = Sessions::authenticate(&tx, &actor.token_hash).await?;
        accounts::require_ready(&fresh.user)?;
        let deleted = passkey::Entity::delete_many()
            .filter(passkey::Column::Id.eq(id))
            .filter(passkey::Column::UserId.eq(fresh.user.id))
            .exec(&tx)
            .await?;
        if deleted.rows_affected != 1 {
            return Err(Error::NotFound);
        }
        accounts::record(&tx, Some(fresh.user.id), fresh.user.id, "passkey_deleted").await?;
        tx.commit().await?;
        Ok(())
    }

    pub(in crate::dashboard) async fn rename(
        &self,
        accounts: &Accounts,
        actor: &Identity,
        id: Uuid,
        label: String,
    ) -> Result<()> {
        let label = validate_label(label)?;
        let tx = accounts.db.begin().await?;
        accounts::lock_user(&tx, actor.user.id).await?;
        let fresh = Sessions::authenticate(&tx, &actor.token_hash).await?;
        accounts::require_ready(&fresh.user)?;
        let renamed = passkey::Entity::update_many()
            .col_expr(passkey::Column::Label, Expr::value(label))
            .filter(passkey::Column::Id.eq(id))
            .filter(passkey::Column::UserId.eq(fresh.user.id))
            .exec(&tx)
            .await?;
        if renamed.rows_affected != 1 {
            return Err(Error::NotFound);
        }
        tx.commit().await?;
        Ok(())
    }

    /// Call from administrator recovery inside its target-user-locked transaction.
    /// This invalidates even in-flight login challenges: finish requires the same live row UUID.
    /// Registration challenges are invalidated by revoking their original session in that tx.
    pub(in crate::dashboard) async fn revoke_all(
        db: &impl ConnectionTrait,
        owner: Uuid,
    ) -> Result<()> {
        passkey::Entity::delete_many()
            .filter(passkey::Column::UserId.eq(owner))
            .exec(db)
            .await?;
        Ok(())
    }

    async fn run_cpu<T: Send + 'static>(
        &self,
        work: impl FnOnce(&Webauthn) -> Result<T> + Send + 'static,
    ) -> Result<T> {
        let permit = self
            .cpu
            .clone()
            .try_acquire_owned()
            .map_err(|_| Error::Busy)?;
        let webauthn = self.webauthn.clone();
        tokio::task::spawn_blocking(move || {
            // Permit belongs to the real CPU work even if the awaiting request is cancelled.
            let _permit = permit;
            // The library's TRACE diagnostics can include credential details.
            tracing::subscriber::with_default(tracing::subscriber::NoSubscriber::default(), || {
                work(&webauthn)
            })
        })
        .await
        .map_err(|_| Error::Internal)?
    }
}

fn validate_label(label: String) -> Result<String> {
    let label = label.trim();
    if label.is_empty() || label.len() > 100 || label.chars().any(char::is_control) {
        return Err(Error::Invalid(
            "passkey label must contain 1 to 100 bytes without control characters",
        ));
    }
    Ok(label.to_owned())
}
