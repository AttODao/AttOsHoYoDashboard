//! App delivery: registered app installations and the per-user inbox they pull from.
use sea_orm_migration::prelude::*;

pub(super) struct AppNotifications;
impl MigrationName for AppNotifications {
    fn name(&self) -> &str {
        "m20261028_000009_app_notifications"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for AppNotifications {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        // One row per app installation that asked for notifications; unseen ones expire.
        db.execute_unprepared(
            "CREATE TABLE app_devices ( \
             id uuid PRIMARY KEY, \
             user_id uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE, \
             created_at timestamptz NOT NULL DEFAULT now(), \
             last_seen_at timestamptz NOT NULL DEFAULT now())",
        )
        .await?;
        db.execute_unprepared("CREATE INDEX app_devices_user_idx ON app_devices (user_id)")
            .await?;
        // Rendered notices; the increasing id is each device's read cursor.
        db.execute_unprepared(
            "CREATE TABLE app_notifications ( \
             id bigserial PRIMARY KEY, \
             user_id uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE, \
             kind varchar(32) NOT NULL, \
             tag varchar(200) NOT NULL, \
             title varchar(200) NOT NULL, \
             body varchar(1000) NOT NULL, \
             url varchar(64) NOT NULL, \
             created_at timestamptz NOT NULL DEFAULT now())",
        )
        .await?;
        db.execute_unprepared(
            "CREATE INDEX app_notifications_user_idx ON app_notifications (user_id, id)",
        )
        .await?;
        db.execute_unprepared(
            "CREATE INDEX app_notifications_created_idx ON app_notifications (created_at)",
        )
        .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        for table in ["app_notifications", "app_devices"] {
            db.execute_unprepared(&format!("DROP TABLE {table}"))
                .await?;
        }
        Ok(())
    }
}
