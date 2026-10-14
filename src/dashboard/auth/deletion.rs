//! Dashboard user deletion request and trusted completion workflow.
#[cfg(test)]
#[path = "deletion_tests.rs"]
mod tests;

use super::{Accounts, Identity, Sessions, lock_user, record, require_admin, require_ready};
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
use uuid::Uuid;

const DELETE_LAST_ADMIN: &str = "cannot delete the last active administrator";
const CONFIRMATION_MISMATCH: &str = "confirmation must exactly match the target username";

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
        if !self
            .passwords
            .verify(current_password, Some(&actor_password_hash))
            .await?
        {
            return Err(Error::Unauthorized);
        }

        let tx = self.db.begin().await?;
        // Serialize administrator membership changes without blocking unrelated password updates.
        tx.execute_unprepared("SELECT pg_advisory_xact_lock(198642015)")
            .await?;

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

        if existing_request.is_none() {
            deletion_request::ActiveModel {
                user_id: Set(target),
                requested_by_user_id: Set(Some(actor_id)),
                created_at: Set(Utc::now()),
            }
            .insert(&tx)
            .await?;
        }
        record(&tx, Some(actor_id), target, "user_deletion_requested").await?;
        tx.commit().await?;
        Ok(())
    }

    pub(in crate::dashboard) async fn pending_deletions(&self) -> Result<Vec<Uuid>> {
        Ok(deletion_request::Entity::find()
            .order_by_asc(deletion_request::Column::CreatedAt)
            .limit(32)
            .all(&self.db)
            .await?
            .into_iter()
            .map(|request| request.user_id)
            .collect())
    }

    pub(in crate::dashboard) async fn finish_deletion(&self, owner: Uuid) -> Result<()> {
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
