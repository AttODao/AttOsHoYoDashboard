//! Session, sign-in and passkey activity for self-service and administration views.
use sea_orm_migration::prelude::*;

pub(super) struct AccountActivity;
impl MigrationName for AccountActivity {
    fn name(&self) -> &str {
        "m20261020_000006_account_activity"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for AccountActivity {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        // The public session id never reveals or derives from the token hash.
        db.execute_unprepared(
            "ALTER TABLE sessions \
             ADD COLUMN id uuid NOT NULL DEFAULT gen_random_uuid(), \
             ADD COLUMN created_at timestamptz NOT NULL DEFAULT now(), \
             ADD COLUMN last_seen_at timestamptz NULL, \
             ADD COLUMN user_agent varchar(256) NULL, \
             ADD CONSTRAINT sessions_id_key UNIQUE (id)",
        )
        .await?;
        db.execute_unprepared("ALTER TABLE users ADD COLUMN last_login_at timestamptz NULL")
            .await?;
        db.execute_unprepared("ALTER TABLE passkeys ADD COLUMN last_used_at timestamptz NULL")
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        db.execute_unprepared("ALTER TABLE passkeys DROP COLUMN last_used_at")
            .await?;
        db.execute_unprepared("ALTER TABLE users DROP COLUMN last_login_at")
            .await?;
        db.execute_unprepared(
            "ALTER TABLE sessions DROP CONSTRAINT sessions_id_key, DROP COLUMN user_agent, \
             DROP COLUMN last_seen_at, DROP COLUMN created_at, DROP COLUMN id",
        )
        .await?;
        Ok(())
    }
}
