//! Administrator read models: user overview and the account audit log.
use super::{
    accounts::{Accounts, require_admin},
    sessions::{Identity, Sessions},
};
use crate::dashboard::{
    db::entities::{audit, hoyolab_account, reward_setting, user},
    error::Result,
};
use chrono::{DateTime, Utc};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, QueryOrder, QuerySelect};
use std::collections::HashMap;
use uuid::Uuid;

#[cfg(test)]
#[path = "administration_tests.rs"]
mod tests;

/// One user as administrators see them, with linkage health but no secrets.
pub(in crate::dashboard) struct UserOverview {
    pub(in crate::dashboard) user: user::Model,
    /// The HoYoLAB link state, or `not_configured`.
    pub(in crate::dashboard) link_status: String,
    /// Games with automatic claiming ON.
    pub(in crate::dashboard) automatic_claims: u64,
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
        let users = user::Entity::find()
            .order_by_asc(user::Column::Username)
            .all(&self.db)
            .await?;
        let links: HashMap<Uuid, hoyolab_account::Model> = hoyolab_account::Entity::find()
            .all(&self.db)
            .await?
            .into_iter()
            .map(|link| (link.owner_user_id, link))
            .collect();
        let mut enabled: HashMap<Uuid, u64> = HashMap::new();
        for setting in reward_setting::Entity::find()
            .filter(reward_setting::Column::Enabled.eq(true))
            .all(&self.db)
            .await?
        {
            *enabled.entry(setting.hoyolab_account_id).or_default() += 1;
        }
        Ok(users
            .into_iter()
            .map(|user| {
                let link = links.get(&user.id);
                UserOverview {
                    link_status: link
                        .map_or_else(|| "not_configured".to_owned(), |l| l.auth_status.clone()),
                    automatic_claims: link
                        .and_then(|link| enabled.get(&link.id).copied())
                        .unwrap_or_default(),
                    user,
                }
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
