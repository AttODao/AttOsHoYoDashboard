//! Per-user API tokens: only the SHA-256 of the bearer value is stored.
use sea_orm_migration::prelude::*;

pub(super) struct ApiTokens;
impl MigrationName for ApiTokens {
    fn name(&self) -> &str {
        "m20261027_000008_api_tokens"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for ApiTokens {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        // Revocation deletes the row; the account audit keeps the history.
        db.execute_unprepared(
            "CREATE TABLE api_tokens ( \
             id uuid PRIMARY KEY, \
             user_id uuid NOT NULL, \
             name varchar(100) NOT NULL, \
             scope varchar(8) NOT NULL, \
             token_hash varchar(64) NOT NULL, \
             created_at timestamptz NOT NULL, \
             last_used_at timestamptz NULL, \
             CONSTRAINT api_tokens_user_fk FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE, \
             CONSTRAINT api_tokens_scope_check CHECK (scope IN ('read', 'run')), \
             CONSTRAINT api_tokens_token_hash_key UNIQUE (token_hash))",
        )
        .await?;
        db.execute_unprepared("CREATE INDEX api_tokens_user_idx ON api_tokens (user_id)")
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared("DROP TABLE api_tokens")
            .await?;
        Ok(())
    }
}
