//! Resin and Trailblaze Power snapshots, one per status fetch, removed with their game
//! account and pruned by `RESOURCE_HISTORY_DAYS`.
use sea_orm_migration::prelude::*;

pub(super) struct ResourceHistory;
impl MigrationName for ResourceHistory {
    fn name(&self) -> &str {
        "m20261109_000015_resource_history"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for ResourceHistory {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        db.execute_unprepared(
            "CREATE TABLE resource_history ( \
             game_account_id uuid NOT NULL, \
             fetched_at timestamptz NOT NULL, \
             current integer NOT NULL, \
             maximum integer NOT NULL, \
             full_at timestamptz NULL, \
             CONSTRAINT resource_history_pkey PRIMARY KEY (game_account_id, fetched_at), \
             CONSTRAINT resource_history_game_account_fk FOREIGN KEY (game_account_id) \
               REFERENCES game_accounts (id) ON DELETE CASCADE, \
             CONSTRAINT resource_history_values_check CHECK \
               (current >= 0 AND current <= 100000 AND maximum > 0 AND maximum <= 100000))",
        )
        .await?;
        // The hourly retention cleanup deletes by age across all accounts.
        db.execute_unprepared(
            "CREATE INDEX resource_history_fetched_at_idx ON resource_history (fetched_at)",
        )
        .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared("DROP TABLE resource_history")
            .await?;
        Ok(())
    }
}
