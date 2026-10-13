//! Pending encrypted Cookie setup. No caller can promote a Cookie claim to a verified UID.
use super::credentials::{CookieKey, Credentials};
use crate::dashboard::{
    auth::{
        accounts::{lock_user, record, require_ready},
        sessions::{Identity, Sessions},
    },
    db::entities::hoyolab_account,
    error::{Error, Result},
};
use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, IntoActiveModel, QueryFilter,
    Set, TransactionTrait,
};
use std::sync::Arc;
use uuid::Uuid;

#[cfg(test)]
#[path = "store_tests.rs"]
mod tests;

pub(in crate::dashboard) struct CookieStore {
    db: DatabaseConnection,
    key: Arc<CookieKey>,
}
impl CookieStore {
    pub(in crate::dashboard) fn new(db: DatabaseConnection, key: Arc<CookieKey>) -> Self {
        Self { db, key }
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
            return Err(Error::Unavailable(
                "Cookie updates require verified HoYoLAB ownership",
            ));
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
