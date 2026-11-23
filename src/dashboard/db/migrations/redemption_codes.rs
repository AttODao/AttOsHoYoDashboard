//! Redemption codes: the shared catalogue, per-UID modes, per-UID results and the
//! single code-source row (administrator controls and fetch status).
use sea_orm_migration::prelude::*;

pub(super) struct RedemptionCodes;
impl MigrationName for RedemptionCodes {
    fn name(&self) -> &str {
        "m20261123_000016_redemption_codes"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for RedemptionCodes {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        db.execute_unprepared(
            "CREATE TABLE redemption_codes ( \
             id uuid PRIMARY KEY, \
             game_kind varchar(16) NOT NULL CHECK (game_kind IN ('genshin', 'starrail')), \
             code varchar(32) NOT NULL CHECK (code ~ '^[A-Z0-9]{4,32}$'), \
             rewards varchar(300) NOT NULL DEFAULT '', \
             first_seen_at timestamptz NOT NULL, \
             last_seen_at timestamptz NOT NULL, \
             listed boolean NOT NULL DEFAULT true, \
             seeded boolean NOT NULL DEFAULT false, \
             CONSTRAINT redemption_codes_game_code_key UNIQUE (game_kind, code), \
             CONSTRAINT redemption_codes_seen_check CHECK (last_seen_at >= first_seen_at))",
        )
        .await?;
        // Pruning deletes long-unlisted codes by age.
        db.execute_unprepared(
            "CREATE INDEX redemption_codes_last_seen_idx ON redemption_codes (last_seen_at)",
        )
        .await?;
        db.execute_unprepared(
            "CREATE TABLE redemption_code_settings ( \
             game_account_id uuid PRIMARY KEY REFERENCES game_accounts (id) ON DELETE CASCADE, \
             mode varchar(16) NOT NULL DEFAULT 'off' CHECK (mode IN ('off', 'notify', 'auto')), \
             enabled_at timestamptz NULL, \
             updated_at timestamptz NOT NULL, \
             CONSTRAINT redemption_code_settings_enabled_check CHECK ((mode = 'off') = (enabled_at IS NULL)))",
        )
        .await?;
        db.execute_unprepared(
            "CREATE TABLE redemption_code_results ( \
             game_account_id uuid NOT NULL REFERENCES game_accounts (id) ON DELETE CASCADE, \
             code_id uuid NOT NULL REFERENCES redemption_codes (id) ON DELETE CASCADE, \
             state varchar(32) NOT NULL CHECK (state IN ('announced', 'pending', 'redeemed', \
               'already_used', 'expired', 'invalid', 'rank_too_low', 'cookie_invalid', \
               'captcha_required', 'unknown')), \
             created_at timestamptz NOT NULL, \
             updated_at timestamptz NOT NULL, \
             PRIMARY KEY (game_account_id, code_id))",
        )
        .await?;
        db.execute_unprepared(
            "CREATE INDEX redemption_code_results_code_idx ON redemption_code_results (code_id)",
        )
        .await?;
        db.execute_unprepared(
            "CREATE TABLE redemption_code_source ( \
             id smallint PRIMARY KEY CHECK (id = 1), \
             enabled boolean NOT NULL DEFAULT true, \
             interval_minutes integer NOT NULL DEFAULT 180 CHECK (interval_minutes BETWEEN 60 AND 1440), \
             last_attempt_at timestamptz NULL, \
             last_success_at timestamptz NULL, \
             consecutive_failures integer NOT NULL DEFAULT 0 CHECK (consecutive_failures >= 0), \
             last_error varchar(32) NULL CHECK (last_error IS NULL OR last_error IN ('timeout', \
               'connect', 'http_status', 'redirect', 'too_large', 'invalid_payload', 'storage')), \
             updated_at timestamptz NOT NULL DEFAULT now())",
        )
        .await?;
        db.execute_unprepared("INSERT INTO redemption_code_source (id) VALUES (1)")
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        for table in [
            "redemption_code_source",
            "redemption_code_results",
            "redemption_code_settings",
            "redemption_codes",
        ] {
            db.execute_unprepared(&format!("DROP TABLE {table}"))
                .await?;
        }
        Ok(())
    }
}
