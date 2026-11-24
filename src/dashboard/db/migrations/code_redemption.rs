//! Code redemption: whether a link's sealed Cookie carries redemption credentials and
//! whether redemption needs the owner's action, the per-UID redemption ledger (never
//! pruned with the catalogue, so a code is never sent twice) and the shared list of codes
//! HoYoLAB rejected as expired or invalid.
use sea_orm_migration::prelude::*;

pub(super) struct CodeRedemption;
impl MigrationName for CodeRedemption {
    fn name(&self) -> &str {
        "m20261124_000017_code_redemption"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for CodeRedemption {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        db.execute_unprepared(
            "ALTER TABLE hoyolab_accounts \
             ADD COLUMN redemption_credentials boolean NOT NULL DEFAULT false, \
             ADD COLUMN redemption_action varchar(32) NULL, \
             ADD COLUMN redemption_action_at timestamptz NULL, \
             ADD CONSTRAINT hoyolab_accounts_redemption_action_check CHECK \
             ((redemption_action IS NULL AND redemption_action_at IS NULL) OR \
              (redemption_action IN ('cookie_invalid', 'captcha_required') AND \
               redemption_action_at IS NOT NULL))",
        )
        .await?;
        db.execute_unprepared(
            "CREATE TABLE redemption_ledger ( \
             game_account_id uuid NOT NULL REFERENCES game_accounts (id) ON DELETE CASCADE, \
             code varchar(32) NOT NULL CHECK (code ~ '^[A-Z0-9]{4,32}$'), \
             source varchar(16) NOT NULL CHECK (source IN ('catalogue', 'manual')), \
             state varchar(32) NOT NULL CHECK (state IN ('pending', 'sending', 'redeemed', \
               'already_used', 'expired', 'invalid', 'rank_too_low', 'cookie_invalid', \
               'captcha_required', 'unknown')), \
             posts integer NOT NULL DEFAULT 0 CHECK (posts >= 0), \
             retcode integer NULL, \
             next_attempt_at timestamptz NULL, \
             last_sent_at timestamptz NULL, \
             created_at timestamptz NOT NULL, \
             updated_at timestamptz NOT NULL, \
             PRIMARY KEY (game_account_id, code), \
             CONSTRAINT redemption_ledger_sent_check CHECK ((posts = 0) = (last_sent_at IS NULL)))",
        )
        .await?;
        db.execute_unprepared(
            "CREATE INDEX redemption_ledger_state_idx ON redemption_ledger (state)",
        )
        .await?;
        db.execute_unprepared(
            "CREATE TABLE redemption_code_rejections ( \
             game_kind varchar(16) NOT NULL CHECK (game_kind IN ('genshin', 'starrail')), \
             code varchar(32) NOT NULL CHECK (code ~ '^[A-Z0-9]{4,32}$'), \
             reason varchar(16) NOT NULL CHECK (reason IN ('expired', 'invalid')), \
             rejected_at timestamptz NOT NULL, \
             PRIMARY KEY (game_kind, code))",
        )
        .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        db.execute_unprepared("DROP TABLE redemption_code_rejections")
            .await?;
        db.execute_unprepared("DROP TABLE redemption_ledger")
            .await?;
        db.execute_unprepared(
            "ALTER TABLE hoyolab_accounts \
             DROP CONSTRAINT hoyolab_accounts_redemption_action_check, \
             DROP COLUMN redemption_action_at, \
             DROP COLUMN redemption_action, \
             DROP COLUMN redemption_credentials",
        )
        .await?;
        Ok(())
    }
}
