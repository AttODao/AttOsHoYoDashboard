//! Cached endgame challenge summaries, removed with their game account.
use sea_orm_migration::prelude::*;

pub(super) struct GameChallengeCache;
impl MigrationName for GameChallengeCache {
    fn name(&self) -> &str {
        "m20261109_000012_game_challenge_cache"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for GameChallengeCache {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(
                "CREATE TABLE game_challenge_cache ( \
                 game_account_id uuid PRIMARY KEY, \
                 fetched_at timestamptz NULL, \
                 updated_at timestamptz NOT NULL, \
                 payload jsonb NULL, \
                 payload_version integer NOT NULL, \
                 error_code varchar(64) NULL, \
                 error_at timestamptz NULL, \
                 CONSTRAINT game_challenge_cache_game_account_fk FOREIGN KEY (game_account_id) \
                   REFERENCES game_accounts (id) ON DELETE CASCADE, \
                 CONSTRAINT game_challenge_cache_payload_version_check CHECK (payload_version > 0), \
                 CONSTRAINT game_challenge_cache_error_code_check CHECK \
                   (error_code IS NULL OR error_code ~ '^[A-Za-z0-9_.:-]{1,64}$'))",
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared("DROP TABLE game_challenge_cache")
            .await?;
        Ok(())
    }
}
