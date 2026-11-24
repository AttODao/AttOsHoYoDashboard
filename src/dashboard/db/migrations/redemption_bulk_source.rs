//! The ledger source `bulk`: a listed code the owner asked to redeem once for a UID outside
//! `auto` (`off` or `notify`), without changing its mode.
use sea_orm_migration::prelude::*;

pub(super) struct RedemptionBulkSource;
impl MigrationName for RedemptionBulkSource {
    fn name(&self) -> &str {
        "m20261124_000019_redemption_bulk_source"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for RedemptionBulkSource {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(
                "ALTER TABLE redemption_ledger \
                 DROP CONSTRAINT redemption_ledger_source_check, \
                 ADD CONSTRAINT redemption_ledger_source_check CHECK \
                   (source IN ('catalogue', 'manual', 'bulk'))",
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        // Queued rows that were never answered with a result go; answered ones stay as
        // catalogue rows, so nothing they recorded is ever sent again.
        db.execute_unprepared(
            "DELETE FROM redemption_ledger WHERE source = 'bulk' AND state = 'pending'",
        )
        .await?;
        db.execute_unprepared(
            "UPDATE redemption_ledger SET source = 'catalogue' WHERE source = 'bulk'",
        )
        .await?;
        db.execute_unprepared(
            "ALTER TABLE redemption_ledger \
             DROP CONSTRAINT redemption_ledger_source_check, \
             ADD CONSTRAINT redemption_ledger_source_check CHECK \
               (source IN ('catalogue', 'manual'))",
        )
        .await?;
        Ok(())
    }
}
