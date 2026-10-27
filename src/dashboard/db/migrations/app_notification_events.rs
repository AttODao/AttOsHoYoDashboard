//! Separate app-notification choices: each personal event can be on for the app and off for the webhook.
use sea_orm_migration::prelude::*;

pub(super) struct AppNotificationEvents;
impl MigrationName for AppNotificationEvents {
    fn name(&self) -> &str {
        "m20261028_000011_app_notification_events"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for AppNotificationEvents {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        db.execute_unprepared(
            "ALTER TABLE notification_settings \
             ADD COLUMN app_events jsonb NOT NULL DEFAULT '{}'::jsonb \
             CHECK (jsonb_typeof(app_events) = 'object')",
        )
        .await?;
        // Until now the app followed the webhook choices: keep what users already receive.
        db.execute_unprepared("UPDATE notification_settings SET app_events = events")
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared("ALTER TABLE notification_settings DROP COLUMN app_events")
            .await?;
        Ok(())
    }
}
