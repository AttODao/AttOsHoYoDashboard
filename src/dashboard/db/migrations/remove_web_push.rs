//! Drops browser push subscriptions; Web Push was removed in favour of Webhook and the app inbox.
use sea_orm_migration::prelude::*;

pub(super) struct RemoveWebPush;
impl MigrationName for RemoveWebPush {
    fn name(&self) -> &str {
        "m20261028_000010_remove_web_push"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for RemoveWebPush {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared("DROP TABLE push_subscriptions")
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        db.execute_unprepared(
            "CREATE TABLE push_subscriptions ( \
             id uuid PRIMARY KEY, \
             user_id uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE, \
             endpoint varchar(1024) NOT NULL UNIQUE CHECK (endpoint ~ '^https://'), \
             p256dh varchar(128) NOT NULL, \
             auth varchar(64) NOT NULL, \
             created_at timestamptz NOT NULL DEFAULT now())",
        )
        .await?;
        db.execute_unprepared(
            "CREATE INDEX push_subscriptions_user_idx ON push_subscriptions (user_id)",
        )
        .await?;
        Ok(())
    }
}
