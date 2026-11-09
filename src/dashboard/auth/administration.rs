//! Administrator read models: user overview and the account audit log.
use super::{
    accounts::{Accounts, require_admin},
    sessions::{Identity, Sessions},
};
use crate::dashboard::{
    db::entities::{audit, user},
    error::Result,
    hoyolab::health::{self, LinkHealth},
};
use chrono::{DateTime, Utc};
use sea_orm::{
    AccessMode, ColumnTrait, EntityTrait, IsolationLevel, QueryFilter, QueryOrder, QuerySelect,
    TransactionTrait,
};
use std::collections::HashMap;
use uuid::Uuid;

#[cfg(test)]
#[path = "administration_tests.rs"]
mod tests;

/// One user as administrators see them, with linkage health but no secrets.
pub(in crate::dashboard) struct UserOverview {
    pub(in crate::dashboard) user: user::Model,
    /// Stored link state, last status fetch and last reward runs; no upstream calls.
    pub(in crate::dashboard) hoyolab: LinkHealth,
}

pub(in crate::dashboard) struct AuditEntry {
    pub(in crate::dashboard) id: Uuid,
    pub(in crate::dashboard) action: String,
    pub(in crate::dashboard) created_at: DateTime<Utc>,
    /// Usernames when the users still exist; the actor is None for system actions.
    pub(in crate::dashboard) actor: Option<String>,
    pub(in crate::dashboard) target: Option<String>,
}

pub(in crate::dashboard) const AUDIT_PAGE_MAX: u64 = 200;

impl Accounts {
    pub(in crate::dashboard) async fn user_overview(
        &self,
        actor: &Identity,
    ) -> Result<Vec<UserOverview>> {
        let fresh = Sessions::authenticate(&self.db, &actor.token_hash).await?;
        require_admin(&fresh.user)?;
        // One read-only snapshot, so users, links, settings and runs agree.
        let tx = self
            .db
            .begin_with_config(
                Some(IsolationLevel::RepeatableRead),
                Some(AccessMode::ReadOnly),
            )
            .await?;
        let users = user::Entity::find()
            .order_by_asc(user::Column::Username)
            .all(&tx)
            .await?;
        let mut links = health::by_owner(&tx).await?;
        tx.commit().await?;
        Ok(users
            .into_iter()
            .map(|user| UserOverview {
                hoyolab: links
                    .remove(&user.id)
                    .unwrap_or_else(LinkHealth::not_configured),
                user,
            })
            .collect())
    }

    /// Newest entries first; `before` pages backwards through older ones.
    pub(in crate::dashboard) async fn audit_log(
        &self,
        actor: &Identity,
        limit: u64,
        before: Option<DateTime<Utc>>,
    ) -> Result<Vec<AuditEntry>> {
        let fresh = Sessions::authenticate(&self.db, &actor.token_hash).await?;
        require_admin(&fresh.user)?;
        let mut query = audit::Entity::find();
        if let Some(before) = before {
            query = query.filter(audit::Column::CreatedAt.lt(before));
        }
        let rows = query
            .order_by_desc(audit::Column::CreatedAt)
            .order_by_desc(audit::Column::Id)
            .limit(limit.clamp(1, AUDIT_PAGE_MAX))
            .all(&self.db)
            .await?;
        let ids: Vec<Uuid> = rows
            .iter()
            .flat_map(|row| [row.actor_user_id, Some(row.target_user_id)])
            .flatten()
            .collect();
        let names: HashMap<Uuid, String> = user::Entity::find()
            .filter(user::Column::Id.is_in(ids))
            .all(&self.db)
            .await?
            .into_iter()
            .map(|user| (user.id, user.username))
            .collect();
        Ok(rows
            .into_iter()
            .map(|row| AuditEntry {
                id: row.id,
                actor: row.actor_user_id.and_then(|id| names.get(&id).cloned()),
                target: names.get(&row.target_user_id).cloned(),
                action: row.action,
                created_at: row.created_at,
            })
            .collect())
    }
}
