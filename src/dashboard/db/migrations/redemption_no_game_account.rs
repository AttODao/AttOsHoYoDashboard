//! The ledger state `no_game_account`: HoYoLAB answered that the Cookie's account has no
//! game account for the UID and server (-1073). It stops that UID only, where it used to be
//! read as an invalid Cookie that paused the whole owner.
use sea_orm_migration::prelude::*;

pub(super) struct RedemptionNoGameAccount;
impl MigrationName for RedemptionNoGameAccount {
    fn name(&self) -> &str {
        "m20261124_000018_redemption_no_game_account"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for RedemptionNoGameAccount {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(
                "ALTER TABLE redemption_ledger \
                 DROP CONSTRAINT redemption_ledger_state_check, \
                 ADD CONSTRAINT redemption_ledger_state_check CHECK (state IN ('pending', \
                   'sending', 'redeemed', 'already_used', 'expired', 'invalid', 'rank_too_low', \
                   'cookie_invalid', 'captcha_required', 'no_game_account', 'unknown'))",
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        // Such rows were answered without redeeming: waiting again is what a Cookie update
        // would have made of them.
        db.execute_unprepared(
            "UPDATE redemption_ledger SET state = 'pending', next_attempt_at = NULL \
             WHERE state = 'no_game_account'",
        )
        .await?;
        db.execute_unprepared(
            "ALTER TABLE redemption_ledger \
             DROP CONSTRAINT redemption_ledger_state_check, \
             ADD CONSTRAINT redemption_ledger_state_check CHECK (state IN ('pending', \
               'sending', 'redeemed', 'already_used', 'expired', 'invalid', 'rank_too_low', \
               'cookie_invalid', 'captcha_required', 'unknown'))",
        )
        .await?;
        Ok(())
    }
}
