//! Delivery of one notice to the routes that chose its kind, at most once per key.
use super::{
    Notice,
    channel::{ADMIN_SCOPE, Channel, Notifications},
};
use crate::dashboard::{db::entities::notification_log, error::Result};
use chrono::Utc;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, Set, sea_query::OnConflict};
use uuid::Uuid;

impl Notifications {
    /// Sends a notice once per key to every route that chose its kind: the webhook by
    /// `events`, a user's app inbox by `app_events`. True when a route took it now (the
    /// Webhook answered 2xx, or it entered the app inbox); false when no route wants it,
    /// none is set up, every attempt failed, or the key was already sent. The key is
    /// marked sent after any attempt, so the same key is never retried.
    pub(super) async fn deliver(
        &self,
        user: Option<Uuid>,
        channel: &Channel,
        notice: &Notice,
    ) -> Result<bool> {
        let webhook = channel.wants(notice.kind);
        let app = user.filter(|_| channel.wants_app(notice.kind));
        if !webhook && app.is_none() {
            return Ok(false);
        }
        let scope = user.unwrap_or(ADMIN_SCOPE);
        if self.already_sent(scope, &notice.key).await? {
            return Ok(false);
        }
        let mut sent = false;
        if webhook {
            sent |= self.send_webhook(user, channel, notice).await? == "sent";
        }
        if let Some(user) = app {
            sent |= self.send_to_app(user, notice, &channel.language).await?;
        }
        self.mark_sent(scope, &notice.key).await?;
        Ok(sent)
    }

    async fn already_sent(&self, scope: Uuid, key: &str) -> Result<bool> {
        Ok(notification_log::Entity::find()
            .filter(notification_log::Column::Scope.eq(scope))
            .filter(notification_log::Column::EventKey.eq(key))
            .one(&self.db)
            .await?
            .is_some())
    }

    async fn mark_sent(&self, scope: Uuid, key: &str) -> Result<()> {
        notification_log::Entity::insert(notification_log::ActiveModel {
            scope: Set(scope),
            event_key: Set(key.to_owned()),
            sent_at: Set(Utc::now()),
        })
        .on_conflict(
            OnConflict::columns([
                notification_log::Column::Scope,
                notification_log::Column::EventKey,
            ])
            .do_nothing()
            .to_owned(),
        )
        .exec(&self.db)
        .await?;
        Ok(())
    }
}
