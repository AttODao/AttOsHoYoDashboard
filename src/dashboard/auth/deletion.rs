//! Dashboard user deletion request and trusted completion workflow.
#[cfg(test)]
#[path = "deletion_tests.rs"]
mod tests;

use super::{
    Accounts, Identity, Sessions, lock_membership, lock_user, record, require_admin, require_ready,
};
use crate::dashboard::{
    db::entities::{audit, deletion_request, user},
    error::{Error, Result},
};
use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseTransaction, EntityTrait,
    IntoActiveModel, PaginatorTrait, QueryFilter, QueryOrder, QuerySelect, Set, TransactionTrait,
    sea_query::Expr,
};
use std::{
    collections::HashMap,
    sync::Mutex,
    time::{Duration, Instant},
};
use uuid::Uuid;

const DELETE_LAST_ADMIN: &str = "cannot delete the last active administrator";
const CONFIRMATION_MISMATCH: &str = "confirmation must exactly match the target username";
const PENDING_BATCH: u64 = 32;
const RETRY_FIRST: Duration = Duration::from_secs(10);
const RETRY_MAX: Duration = Duration::from_secs(3600);

/// Process-local completion backoff; the single instance restarts it from the first delay.
#[derive(Default)]
pub(super) struct DeletionRetries(Mutex<HashMap<Uuid, Retry>>);

struct Retry {
    failures: u32,
    not_before: Instant,
}

impl DeletionRetries {
    fn waiting(&self, now: Instant) -> Result<Vec<Uuid>> {
        let entries = self.0.lock().map_err(|_| Error::Internal)?;
        Ok(entries
            .iter()
            .filter(|(_, retry)| retry.not_before > now)
            .map(|(owner, _)| *owner)
            .collect())
    }

    fn postpone(&self, owner: Uuid, now: Instant) -> Result<Duration> {
        let mut entries = self.0.lock().map_err(|_| Error::Internal)?;
        let retry = entries.entry(owner).or_insert(Retry {
            failures: 0,
            not_before: now,
        });
        retry.failures = retry.failures.saturating_add(1);
        let delay = RETRY_FIRST
            .saturating_mul(1 << (retry.failures - 1).min(16))
            .min(RETRY_MAX);
        retry.not_before = now + delay;
        Ok(delay)
    }

    fn forget(&self, owner: Uuid) -> Result<()> {
        self.0.lock().map_err(|_| Error::Internal)?.remove(&owner);
        Ok(())
    }
}

impl Accounts {
    pub(in crate::dashboard) async fn request_deletion(
        &self,
        actor: &Identity,
        target: Uuid,
        current_password: String,
        confirmation: String,
    ) -> Result<()> {
        let actor_id = actor.user.id;
        let actor_password_hash = actor.user.password_hash.clone();
        // Reject another user's deletion before password work; locked checks below stay final.
        if target != actor_id {
            require_admin(&actor.user)?;
        }
        self.admit_reauth(actor_id)?;
        if !self
            .passwords
            .verify(current_password, Some(&actor_password_hash))
            .await?
        {
            return Err(Error::Unauthorized);
        }

        let tx = self.db.begin().await?;
        // Serialize administrator membership changes without blocking unrelated password updates.
        lock_membership(&tx).await?;

        let fresh = Sessions::authenticate(&tx, &actor.token_hash).await?;
        if fresh.user.id != actor_id || fresh.user.password_hash != actor_password_hash {
            return Err(Error::Unauthorized);
        }
        if target == actor_id {
            require_ready(&fresh.user)?;
        } else {
            require_admin(&fresh.user)?;
        }

        let (locked_actor, locked_target) = lock_actor_and_target(&tx, actor_id, target).await?;
        let fresh = Sessions::authenticate(&tx, &actor.token_hash).await?;
        if fresh.user.id != actor_id
            || fresh.user.password_hash != actor_password_hash
            || fresh.user.password_hash != locked_actor.password_hash
        {
            return Err(Error::Unauthorized);
        }
        if target == actor_id {
            require_ready(&fresh.user)?;
        } else {
            require_admin(&fresh.user)?;
        }

        if locked_target.username != confirmation {
            return Err(Error::Invalid(CONFIRMATION_MISMATCH));
        }

        let existing_request = deletion_request::Entity::find_by_id(target)
            .lock_exclusive()
            .one(&tx)
            .await?;
        if existing_request.is_some() {
            tx.commit().await?;
            return Ok(());
        }

        if locked_target.active && locked_target.is_admin && active_admin_count(&tx).await? <= 1 {
            return Err(Error::Invalid(DELETE_LAST_ADMIN));
        }

        let mut update = locked_target.into_active_model();
        update.active = Set(false);
        update.update(&tx).await?;
        Sessions::revoke_all(&tx, target).await?;
        super::super::passkeys::Passkeys::revoke_all(&tx, target).await?;
        super::super::api_tokens::ApiTokens::revoke_all(&tx, target).await?;

        deletion_request::ActiveModel {
            user_id: Set(target),
            requested_by_user_id: Set(Some(actor_id)),
            created_at: Set(Utc::now()),
        }
        .insert(&tx)
        .await?;
        record(&tx, Some(actor_id), target, "user_deletion_requested").await?;
        tx.commit().await?;
        Ok(())
    }

    /// Oldest pending owners, skipping `active` ones and those still in retry backoff.
    pub(in crate::dashboard) async fn pending_deletions(
        &self,
        active: &[Uuid],
        now: Instant,
    ) -> Result<Vec<Uuid>> {
        let mut skip = active.to_vec();
        skip.extend(self.deletion_retries.waiting(now)?);
        let mut query = deletion_request::Entity::find()
            .order_by_asc(deletion_request::Column::CreatedAt)
            .limit(PENDING_BATCH);
        if !skip.is_empty() {
            query = query.filter(deletion_request::Column::UserId.is_not_in(skip));
        }
        Ok(query
            .all(&self.db)
            .await?
            .into_iter()
            .map(|request| request.user_id)
            .collect())
    }

    /// Records a failed completion and returns how long the owner is skipped.
    pub(in crate::dashboard) fn postpone_deletion(
        &self,
        owner: Uuid,
        now: Instant,
    ) -> Result<Duration> {
        self.deletion_retries.postpone(owner, now)
    }

    pub(in crate::dashboard) async fn finish_deletion(&self, owner: Uuid) -> Result<()> {
        self.delete_pending_owner(owner).await?;
        self.deletion_retries.forget(owner)
    }

    async fn delete_pending_owner(&self, owner: Uuid) -> Result<()> {
        let tx = self.db.begin().await?;
        let user = user::Entity::find_by_id(owner)
            .lock_exclusive()
            .one(&tx)
            .await?;
        if user.is_none() {
            tx.commit().await?;
            return Ok(());
        }
        if user.as_ref().is_some_and(|user| user.active) {
            return Err(Error::Internal);
        }
        let pending = deletion_request::Entity::find_by_id(owner)
            .lock_exclusive()
            .one(&tx)
            .await?;
        if pending.is_none() {
            tx.commit().await?;
            return Ok(());
        }

        audit::Entity::delete_many()
            .filter(audit::Column::TargetUserId.eq(owner))
            .exec(&tx)
            .await?;
        audit::Entity::update_many()
            .col_expr(
                audit::Column::ActorUserId,
                Expr::value(Option::<Uuid>::None),
            )
            .filter(audit::Column::ActorUserId.eq(owner))
            .exec(&tx)
            .await?;
        user::Entity::delete_by_id(owner).exec(&tx).await?;
        tx.commit().await?;
        Ok(())
    }
}

async fn lock_actor_and_target(
    tx: &DatabaseTransaction,
    actor: Uuid,
    target: Uuid,
) -> Result<(user::Model, user::Model)> {
    if actor == target {
        let user = lock_user(tx, actor).await?;
        return Ok((user.clone(), user));
    }

    let (first, second) = if actor < target {
        (actor, target)
    } else {
        (target, actor)
    };
    let first_user = lock_user(tx, first).await?;
    let second_user = lock_user(tx, second).await?;
    if actor < target {
        Ok((first_user, second_user))
    } else {
        Ok((second_user, first_user))
    }
}

async fn active_admin_count(db: &impl ConnectionTrait) -> Result<u64> {
    Ok(user::Entity::find()
        .filter(user::Column::Active.eq(true))
        .filter(user::Column::IsAdmin.eq(true))
        .count(db)
        .await?)
}
