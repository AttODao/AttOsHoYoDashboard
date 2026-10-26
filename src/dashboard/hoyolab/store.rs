//! Encrypted Cookie setup and upstream identity consistency; Cookie claims alone are never proof.
use super::{
    credentials::{CookieKey, Credentials},
    gateway::{Gateway, LiveGateway},
    operations::Operations,
};
use crate::dashboard::{
    auth::{
        accounts::{lock_user, record, require_ready},
        sessions::{Identity, Sessions, reauthenticate},
    },
    config::Tuning,
    db::entities::{hoyolab_account, reward_setting},
    error::{Error, Result},
    rewards::schedule::ScheduleZone,
};
use chrono::{FixedOffset, Utc};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, DatabaseTransaction, EntityTrait,
    IntoActiveModel, QueryFilter, Set, TransactionTrait, sea_query::Expr,
};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use uuid::Uuid;

#[cfg(test)]
#[path = "store_tests.rs"]
mod tests;

pub(in crate::dashboard) struct CookieStore<G: Gateway = LiveGateway> {
    pub(in crate::dashboard) db: DatabaseConnection,
    pub(in crate::dashboard) gateway: G,
    pub(in crate::dashboard) operations: Operations,
    pub(in crate::dashboard) timezone: ScheduleZone,
    pub(in crate::dashboard) tuning: Tuning,
    pub(super) key: Arc<CookieKey>,
    pub(super) closing: AtomicBool,
}
impl CookieStore<LiveGateway> {
    pub(in crate::dashboard) fn new(db: DatabaseConnection, key: Arc<CookieKey>) -> Self {
        Self {
            db,
            key,
            gateway: LiveGateway,
            operations: Operations::default(),
            timezone: ScheduleZone::Fixed(
                FixedOffset::east_opt(9 * 3600).expect("fixed valid offset"),
            ),
            tuning: Tuning::default(),
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
    pub(in crate::dashboard) async fn actor_locked(
        &self,
        tx: &DatabaseTransaction,
        actor: &Identity,
    ) -> Result<Identity> {
        self.require_running()?;
        lock_user(tx, actor.user.id).await?;
        let fresh = reauthenticate(tx, actor).await?;
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
        let cookie = self
            .key
            .open(owner, row.hoyolab_uid.as_deref(), row.sealed_credentials()?)?;
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
        // A removed Cookie leaves nothing to confirm with.
        row.filter(|row| row.encrypted_credentials.is_some())
            .map(|row| {
                self.key
                    .open(owner, row.hoyolab_uid.as_deref(), row.sealed_credentials()?)
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
        if let Some(row) =
            row.filter(|row| row.hoyolab_uid.is_some() && row.auth_status == "verified")
        {
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
            timezone: ScheduleZone::Fixed(FixedOffset::east_opt(9 * 3600).unwrap()),
            tuning: Tuning::default(),
            closing: AtomicBool::new(false),
        }
    }

    pub(in crate::dashboard) async fn status(
        &self,
        actor: &Identity,
    ) -> Result<Option<hoyolab_account::Model>> {
        let tx = self.db.begin().await?;
        lock_user(&tx, actor.user.id).await?;
        let fresh = reauthenticate(&tx, actor).await?;
        let row = hoyolab_account::Entity::find()
            .filter(hoyolab_account::Column::OwnerUserId.eq(fresh.user.id))
            .one(&tx)
            .await?;
        // Fail closed on corruption or a wrong key, never declare an unreadable Cookie usable.
        if let Some(row) = row.as_ref().filter(|row| row.auth_status != "removed") {
            self.key.open(
                row.owner_user_id,
                row.hoyolab_uid.as_deref(),
                row.sealed_credentials()?,
            )?;
        }
        tx.commit().await?;
        Ok(row)
    }

    pub(in crate::dashboard) async fn submit(&self, actor: &Identity, raw: &str) -> Result<()> {
        let cookie = Credentials::parse_submitted(raw)?;
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
            row.encrypted_credentials = Set(Some(encrypted));
            row.credential_version = Set(next);
            row.auth_status = Set("pending_verification".to_owned());
            row.updated_at = Set(now);
            row.update(&tx).await?;
        } else {
            hoyolab_account::ActiveModel {
                id: Set(Uuid::new_v4()),
                owner_user_id: Set(fresh.user.id),
                hoyolab_uid: Set(None),
                encrypted_credentials: Set(Some(encrypted)),
                credential_version: Set(1),
                auth_status: Set("pending_verification".to_owned()),
                created_at: Set(now),
                updated_at: Set(now),
                verified_at: Set(None),
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

    /// Forget the stored Cookie. A verified link keeps its UID binding, game
    /// visibility, history and send guards so the same account can be restored;
    /// automatic claiming turns off. An unverified link is deleted outright.
    pub(in crate::dashboard) async fn remove_cookie(&self, actor: &Identity) -> Result<()> {
        // Serialize with reward attempts so none is mid-flight with the old Cookie.
        let _operation = self.operations.acquire(actor.user.id).await;
        let tx = self.db.begin().await?;
        let fresh = self.actor_locked(&tx, actor).await?;
        let Some(row) = hoyolab_account::Entity::find()
            .filter(hoyolab_account::Column::OwnerUserId.eq(fresh.user.id))
            .one(&tx)
            .await?
            .filter(|row| row.auth_status != "removed")
        else {
            return Err(Error::NotFound);
        };
        if row.hoyolab_uid.is_none() {
            hoyolab_account::Entity::delete_by_id(row.id)
                .exec(&tx)
                .await?;
        } else {
            let next = row
                .credential_version
                .checked_add(1)
                .ok_or(Error::Internal)?;
            let link = row.id;
            let mut row = row.into_active_model();
            row.encrypted_credentials = Set(None);
            row.credential_version = Set(next);
            row.auth_status = Set("removed".to_owned());
            row.updated_at = Set(Utc::now());
            row.update(&tx).await?;
            reward_setting::Entity::update_many()
                .col_expr(reward_setting::Column::Enabled, Expr::value(false))
                .col_expr(
                    reward_setting::Column::NextRunAt,
                    Expr::value(Option::<chrono::DateTime<Utc>>::None),
                )
                .filter(reward_setting::Column::HoyolabAccountId.eq(link))
                .exec(&tx)
                .await?;
        }
        record(
            &tx,
            Some(fresh.user.id),
            fresh.user.id,
            "hoyolab_cookie_removed",
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }
}
