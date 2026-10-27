//! App delivery: registered app installations pull the notices the web channels get.
//!
//! The server keeps no push-service credentials: each installation polls its user's inbox
//! with its session cookie and shows new entries as native notifications.
use super::{Notice, channel::Notifications, messages::render};
use crate::dashboard::{
    auth::{
        accounts::require_ready,
        sessions::{Identity, Sessions},
    },
    db::entities::{app_device, app_notification},
    error::{Error, Result},
};
use chrono::{DateTime, Duration, Utc};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter, QueryOrder,
    QuerySelect, Set,
};
use serde::Serialize;
use uuid::Uuid;

const MAX_DEVICES: u64 = 10;
/// Entries returned by one inbox read; the cursor lets the device fetch the rest.
const PAGE: u64 = 50;
/// Undelivered entries older than this are dropped, like a push service's TTL.
const KEEP_ENTRIES: i64 = 3;
/// Installations that have not polled for this long stop receiving (and costing) notices.
const KEEP_DEVICES: i64 = 30;

/// The app screen for a notice, as the web path the app maps to its page.
pub(super) fn target(kind: &str) -> &'static str {
    match kind {
        "reward_failed" | "reward_claimed" => "/settings/bonus",
        "cookie_invalid" => "/settings/link",
        "signed_in" | "sign_in_failed" => "/settings/security",
        "test" => "/settings/notifications",
        _ => "/dashboard",
    }
}

#[derive(Serialize)]
pub(in crate::dashboard) struct DeviceView {
    id: Uuid,
    /// Read position: entries stored before registration are not shown.
    cursor: i64,
}

#[derive(Serialize)]
pub(in crate::dashboard) struct InboxEntry {
    id: i64,
    kind: String,
    /// Replaces an earlier notification with the same tag (the deduplication key).
    tag: String,
    title: String,
    body: String,
    url: String,
    created_at: DateTime<Utc>,
}

#[derive(Serialize)]
pub(in crate::dashboard) struct InboxView {
    items: Vec<InboxEntry>,
    cursor: i64,
    /// More entries follow after `cursor`.
    more: bool,
}

impl Notifications {
    async fn app_ready(&self, actor: &Identity) -> Result<Identity> {
        let fresh = Sessions::authenticate(&self.db, &actor.token_hash).await?;
        require_ready(&fresh.user)?;
        Ok(fresh)
    }

    async fn newest_entry(&self, user: Uuid) -> Result<i64> {
        Ok(app_notification::Entity::find()
            .filter(app_notification::Column::UserId.eq(user))
            .order_by_desc(app_notification::Column::Id)
            .one(&self.db)
            .await?
            .map_or(0, |row| row.id))
    }

    pub(in crate::dashboard) async fn register_app_device(
        &self,
        actor: &Identity,
    ) -> Result<DeviceView> {
        let fresh = self.app_ready(actor).await?;
        let count = app_device::Entity::find()
            .filter(app_device::Column::UserId.eq(fresh.user.id))
            .count(&self.db)
            .await?;
        if count >= MAX_DEVICES {
            return Err(Error::Invalid("too many app devices"));
        }
        let now = Utc::now();
        let row = app_device::ActiveModel {
            id: Set(Uuid::new_v4()),
            user_id: Set(fresh.user.id),
            created_at: Set(now),
            last_seen_at: Set(now),
        }
        .insert(&self.db)
        .await?;
        Ok(DeviceView {
            id: row.id,
            cursor: self.newest_entry(fresh.user.id).await?,
        })
    }

    pub(in crate::dashboard) async fn unregister_app_device(
        &self,
        actor: &Identity,
        device: Uuid,
    ) -> Result<()> {
        let fresh = self.app_ready(actor).await?;
        app_device::Entity::delete_many()
            .filter(app_device::Column::Id.eq(device))
            .filter(app_device::Column::UserId.eq(fresh.user.id))
            .exec(&self.db)
            .await?;
        Ok(())
    }

    /// Entries after `after` for the actor's own registered device; marks it as seen.
    pub(in crate::dashboard) async fn app_inbox(
        &self,
        actor: &Identity,
        device: Uuid,
        after: i64,
    ) -> Result<InboxView> {
        let fresh = self.app_ready(actor).await?;
        let updated = app_device::Entity::update_many()
            .col_expr(
                app_device::Column::LastSeenAt,
                sea_orm::sea_query::Expr::value(Utc::now()),
            )
            .filter(app_device::Column::Id.eq(device))
            .filter(app_device::Column::UserId.eq(fresh.user.id))
            .exec(&self.db)
            .await?;
        if updated.rows_affected == 0 {
            return Err(Error::NotFound);
        }
        let mut rows = app_notification::Entity::find()
            .filter(app_notification::Column::UserId.eq(fresh.user.id))
            .filter(app_notification::Column::Id.gt(after))
            .order_by_asc(app_notification::Column::Id)
            .limit(PAGE + 1)
            .all(&self.db)
            .await?;
        let more = rows.len() as u64 > PAGE;
        rows.truncate(PAGE as usize);
        let cursor = rows.last().map_or(after.max(0), |row| row.id);
        Ok(InboxView {
            items: rows
                .into_iter()
                .map(|row| InboxEntry {
                    id: row.id,
                    kind: row.kind,
                    tag: row.tag,
                    title: row.title,
                    body: row.body,
                    url: row.url,
                    created_at: row.created_at,
                })
                .collect(),
            cursor,
            more,
        })
    }

    pub(super) async fn has_app_device(&self, user: Uuid) -> Result<bool> {
        Ok(app_device::Entity::find()
            .filter(app_device::Column::UserId.eq(user))
            .count(&self.db)
            .await?
            > 0)
    }

    /// Queues a notice for the user's app installations; false when none is registered.
    pub(super) async fn send_to_app(
        &self,
        user: Uuid,
        notice: &Notice,
        language: &str,
    ) -> Result<bool> {
        if !self.has_app_device(user).await? {
            return Ok(false);
        }
        let (title, body) = render(notice, language);
        app_notification::ActiveModel {
            user_id: Set(user),
            kind: Set(notice.kind.to_owned()),
            tag: Set(truncate(&notice.key, 200)),
            title: Set(truncate(&title, 200)),
            body: Set(truncate(&body, 1000)),
            url: Set(target(notice.kind).to_owned()),
            created_at: Set(Utc::now()),
            ..Default::default()
        }
        .insert(&self.db)
        .await?;
        Ok(true)
    }

    /// Drops old inbox entries and installations that stopped polling.
    pub(super) async fn cleanup_app(&self, now: DateTime<Utc>) -> Result<()> {
        app_notification::Entity::delete_many()
            .filter(app_notification::Column::CreatedAt.lt(now - Duration::days(KEEP_ENTRIES)))
            .exec(&self.db)
            .await?;
        app_device::Entity::delete_many()
            .filter(app_device::Column::LastSeenAt.lt(now - Duration::days(KEEP_DEVICES)))
            .exec(&self.db)
            .await?;
        Ok(())
    }
}

fn truncate(text: &str, max: usize) -> String {
    text.chars().take(max).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notices_open_the_screen_that_handles_them() {
        assert_eq!(target("reward_failed"), "/settings/bonus");
        assert_eq!(target("reward_claimed"), "/settings/bonus");
        assert_eq!(target("cookie_invalid"), "/settings/link");
        assert_eq!(target("signed_in"), "/settings/security");
        assert_eq!(target("sign_in_failed"), "/settings/security");
        assert_eq!(target("test"), "/settings/notifications");
        for kind in [
            "resource_near_cap",
            "resource_full",
            "expeditions_done",
            "transformer_ready",
            "daily_unfinished",
            "weekly_unfinished",
        ] {
            assert_eq!(target(kind), "/dashboard", "{kind}");
        }
        assert_eq!(truncate("あいう", 2), "あい");
    }
}
