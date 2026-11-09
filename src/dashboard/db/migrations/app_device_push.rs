//! UnifiedPush delivery: an app installation's sealed Web Push subscription.
use sea_orm_migration::prelude::*;

pub(super) struct AppDevicePush;
impl MigrationName for AppDevicePush {
    fn name(&self) -> &str {
        "m20261109_000013_app_device_push"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for AppDevicePush {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Endpoint URL and keys, sealed together like a Webhook URL; NULL means poll only.
        manager
            .get_connection()
            .execute_unprepared("ALTER TABLE app_devices ADD COLUMN push_subscription bytea NULL")
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared("ALTER TABLE app_devices DROP COLUMN push_subscription")
            .await?;
        Ok(())
    }
}
