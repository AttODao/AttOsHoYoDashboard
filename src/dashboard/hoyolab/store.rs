//! Encrypted Cookie setup and proof-gated linkage; Cookie claims alone are never proof.
use super::{
    credentials::{CookieKey, Credentials},
    gateway::{Gateway, LiveGateway},
    operations::Operations,
};
use crate::dashboard::{
    auth::{
        accounts::{lock_user, record, require_ready},
        sessions::{Identity, Sessions},
    },
    db::entities::hoyolab_account,
    error::{Error, Result},
};
use chrono::{FixedOffset, Utc};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, DatabaseTransaction, EntityTrait,
    IntoActiveModel, QueryFilter, Set, TransactionTrait,
};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use uuid::Uuid;

#[cfg(test)]
#[path = "store_tests.rs"]
mod tests;

// Remains false until live token-to-UID binding and roles principal consistency are independently established.
const OWNER_BINDING_ESTABLISHED: bool = false;

pub(in crate::dashboard) struct CookieStore<G: Gateway = LiveGateway> {
    pub(in crate::dashboard) db: DatabaseConnection,
    pub(in crate::dashboard) gateway: G,
    pub(in crate::dashboard) operations: Operations,
    pub(in crate::dashboard) timezone: FixedOffset,
    pub(super) key: Arc<CookieKey>,
    pub(super) binding_established: bool,
    pub(super) reward_contract_established: bool,
    pub(super) closing: AtomicBool,
}
impl CookieStore<LiveGateway> {
    pub(in crate::dashboard) fn new(db: DatabaseConnection, key: Arc<CookieKey>) -> Self {
        Self {
            db,
            key,
            gateway: LiveGateway,
            operations: Operations::default(),
            timezone: FixedOffset::east_opt(9 * 3600).expect("fixed valid offset"),
            binding_established: OWNER_BINDING_ESTABLISHED,
            reward_contract_established: false,
            closing: AtomicBool::new(false),
        }
    }
}
impl<G: Gateway> CookieStore<G> {
    pub(in crate::dashboard) fn stop_new_operations(&self) {
        self.closing.store(true, Ordering::Release);
    }
    pub(in crate::dashboard) fn is_closing(&self) -> bool {
        self.closing.load(Ordering::Acquire)
    }
    fn require_running(&self) -> Result<()> {
        if self.closing.load(Ordering::Acquire) {
            return Err(Error::Unavailable("server is shutting down"));
        }
        Ok(())
    }
    pub(in crate::dashboard) fn verification_available(&self) -> bool {
        self.binding_established
    }
    pub(in crate::dashboard) fn reward_available(&self) -> bool {
        self.reward_contract_established
    }
    pub(in crate::dashboard) fn require_reward_contract(&self) -> Result<()> {
        if !self.reward_contract_established {
            return Err(Error::Unavailable(
                "upstream reward period contract is not yet confirmed",
            ));
        }
        Ok(())
    }
    pub(in crate::dashboard) fn require_binding(&self) -> Result<()> {
        if !self.binding_established {
            return Err(Error::Unavailable(
                "upstream identity binding is not yet confirmed",
            ));
        }
        Ok(())
    }
    pub(in crate::dashboard) async fn actor_locked(
        &self,
        tx: &DatabaseTransaction,
        actor: &Identity,
    ) -> Result<Identity> {
        self.require_running()?;
        lock_user(tx, actor.user.id).await?;
        let fresh = Sessions::authenticate(tx, &actor.token_hash).await?;
        if fresh.user.id != actor.user.id {
            return Err(Error::Unauthorized);
        }
        require_ready(&fresh.user)?;
        Ok(fresh)
    }
    pub(in crate::dashboard) async fn ready_link(
        &self,
        actor: &Identity,
    ) -> Result<(hoyolab_account::Model, Credentials)> {
        self.require_binding()?;
        let tx = self.db.begin().await?;
        let fresh = self.actor_locked(&tx, actor).await?;
        let result = self.owner_link_in(&tx, fresh.user.id).await?;
        tx.commit().await?;
        Ok(result)
    }
    pub(in crate::dashboard) async fn owner_link_in(
        &self,
        tx: &DatabaseTransaction,
        owner: Uuid,
    ) -> Result<(hoyolab_account::Model, Credentials)> {
        self.require_running()?;
        self.require_binding()?;
        let user = lock_user(tx, owner).await?;
        require_ready(&user)?;
        let row = hoyolab_account::Entity::find()
            .filter(hoyolab_account::Column::OwnerUserId.eq(owner))
            .one(tx)
            .await?
            .ok_or(Error::InitialSetupRequired)?;
        if row.auth_status != "verified" || row.hoyolab_uid.is_none() {
            return Err(Error::InitialSetupRequired);
        }
        let cookie = self.key.open(
            owner,
            row.hoyolab_uid.as_deref(),
            &row.encrypted_credentials,
        )?;
        Ok((row, cookie))
    }
    // Internal deletion/restart confirmation only. No user-facing route may use this to bypass active/ready checks.
    pub(in crate::dashboard) async fn stored_cookie(
        &self,
        owner: Uuid,
    ) -> Result<Option<(hoyolab_account::Model, Credentials)>> {
        let row = hoyolab_account::Entity::find()
            .filter(hoyolab_account::Column::OwnerUserId.eq(owner))
            .one(&self.db)
            .await?;
        row.map(|row| {
            self.key
                .open(
                    owner,
                    row.hoyolab_uid.as_deref(),
                    &row.encrypted_credentials,
                )
                .map(|cookie| (row, cookie))
        })
        .transpose()
    }
    pub(in crate::dashboard) async fn mark_cookie_invalid(&self, owner: Uuid) -> Result<()> {
        let tx = self.db.begin().await?;
        lock_user(&tx, owner).await?;
        let row = hoyolab_account::Entity::find()
            .filter(hoyolab_account::Column::OwnerUserId.eq(owner))
            .one(&tx)
            .await?;
        if let Some(row) = row.filter(|row| row.hoyolab_uid.is_some()) {
            let mut row = row.into_active_model();
            row.auth_status = Set("cookie_invalid".into());
            row.updated_at = Set(Utc::now());
            row.update(&tx).await?;
        }
        tx.commit().await?;
        Ok(())
    }

    #[cfg(test)]
    pub(in crate::dashboard) fn for_test(
        db: DatabaseConnection,
        key: Arc<CookieKey>,
        gateway: G,
    ) -> Self {
        Self {
            db,
            key,
            gateway,
            operations: Operations::default(),
            timezone: FixedOffset::east_opt(9 * 3600).unwrap(),
            binding_established: true,
            reward_contract_established: true,
            closing: AtomicBool::new(false),
        }
    }

    pub(in crate::dashboard) async fn status(
        &self,
        actor: &Identity,
    ) -> Result<Option<hoyolab_account::Model>> {
        let tx = self.db.begin().await?;
        lock_user(&tx, actor.user.id).await?;
        let fresh = Sessions::authenticate(&tx, &actor.token_hash).await?;
        if fresh.user.id != actor.user.id {
            return Err(Error::Unauthorized);
        }
        let row = hoyolab_account::Entity::find()
            .filter(hoyolab_account::Column::OwnerUserId.eq(fresh.user.id))
            .one(&tx)
            .await?;
        // Fail closed on corruption or a wrong key, never declare an unreadable Cookie usable.
        if let Some(row) = &row {
            self.key.open(
                row.owner_user_id,
                row.hoyolab_uid.as_deref(),
                &row.encrypted_credentials,
            )?;
        }
        tx.commit().await?;
        Ok(row)
    }

    pub(in crate::dashboard) async fn submit(&self, actor: &Identity, raw: &str) -> Result<()> {
        let cookie = Credentials::parse(raw)?;
        let _operation = self.operations.acquire(actor.user.id).await;
        self.require_running()?;
        // Syntactic normalization is not server-authenticated ownership verification.
        let tx = self.db.begin().await?;
        lock_user(&tx, actor.user.id).await?;
        let fresh = Sessions::authenticate(&tx, &actor.token_hash).await?;
        if fresh.user.id != actor.user.id {
            return Err(Error::Unauthorized);
        }
        require_ready(&fresh.user)?;
        let existing = hoyolab_account::Entity::find()
            .filter(hoyolab_account::Column::OwnerUserId.eq(fresh.user.id))
            .one(&tx)
            .await?;
        if existing
            .as_ref()
            .is_some_and(|row| row.hoyolab_uid.is_some())
        {
            let uid = existing
                .as_ref()
                .and_then(|row| row.hoyolab_uid.clone())
                .ok_or(Error::Internal)?;
            tx.commit().await?;
            return self.accept_verified(actor, &cookie, Some(&uid)).await;
        }
        let encrypted = self.key.seal(fresh.user.id, None, &cookie)?;
        let now = Utc::now();
        if let Some(existing) = existing {
            let next = existing
                .credential_version
                .checked_add(1)
                .ok_or(Error::Internal)?;
            let mut row = existing.into_active_model();
            row.encrypted_credentials = Set(encrypted);
            row.credential_version = Set(next);
            row.auth_status = Set("pending_verification".to_owned());
            row.updated_at = Set(now);
            row.update(&tx).await?;
        } else {
            hoyolab_account::ActiveModel {
                id: Set(Uuid::new_v4()),
                owner_user_id: Set(fresh.user.id),
                hoyolab_uid: Set(None),
                encrypted_credentials: Set(encrypted),
                credential_version: Set(1),
                auth_status: Set("pending_verification".to_owned()),
                created_at: Set(now),
                updated_at: Set(now),
            }
            .insert(&tx)
            .await?;
        }
        record(
            &tx,
            Some(fresh.user.id),
            fresh.user.id,
            "hoyolab_cookie_submitted",
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }
}
