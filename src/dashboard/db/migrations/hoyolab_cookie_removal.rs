//! A verified link may drop its stored Cookie while keeping its UID binding and settings.
use sea_orm_migration::prelude::*;

pub(super) struct HoyolabCookieRemoval;
impl MigrationName for HoyolabCookieRemoval {
    fn name(&self) -> &str {
        "m20261020_000005_hoyolab_cookie_removal"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for HoyolabCookieRemoval {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        db.execute_unprepared(
            "ALTER TABLE hoyolab_accounts \
             ADD COLUMN verified_at timestamptz NULL, \
             ALTER COLUMN encrypted_credentials DROP NOT NULL, \
             DROP CONSTRAINT hoyolab_accounts_state_check, \
             DROP CONSTRAINT hoyolab_accounts_ciphertext_check",
        )
        .await?;
        db.execute_unprepared(
            "UPDATE hoyolab_accounts SET verified_at = updated_at \
             WHERE auth_status IN ('verified', 'cookie_invalid')",
        )
        .await?;
        db.execute_unprepared(
            "ALTER TABLE hoyolab_accounts \
             ADD CONSTRAINT hoyolab_accounts_state_check CHECK \
             ((auth_status = 'pending_verification' AND hoyolab_uid IS NULL) OR \
              (auth_status IN ('verified', 'cookie_invalid', 'removed') AND hoyolab_uid IS NOT NULL)), \
             ADD CONSTRAINT hoyolab_accounts_ciphertext_check CHECK \
             ((auth_status = 'removed' AND encrypted_credentials IS NULL) OR \
              (auth_status <> 'removed' AND encrypted_credentials IS NOT NULL AND \
              octet_length(encrypted_credentials) BETWEEN 30 AND 8221))",
        )
        .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        // A removed Cookie cannot be restored; the link returns to pending setup.
        db.execute_unprepared("DELETE FROM hoyolab_accounts WHERE auth_status = 'removed'")
            .await?;
        db.execute_unprepared(
            "ALTER TABLE hoyolab_accounts \
             DROP CONSTRAINT hoyolab_accounts_state_check, \
             DROP CONSTRAINT hoyolab_accounts_ciphertext_check",
        )
        .await?;
        db.execute_unprepared(
            "ALTER TABLE hoyolab_accounts \
             DROP COLUMN verified_at, \
             ALTER COLUMN encrypted_credentials SET NOT NULL, \
             ADD CONSTRAINT hoyolab_accounts_state_check CHECK \
             ((auth_status = 'pending_verification' AND hoyolab_uid IS NULL) OR \
              (auth_status IN ('verified', 'cookie_invalid') AND hoyolab_uid IS NOT NULL)), \
             ADD CONSTRAINT hoyolab_accounts_ciphertext_check CHECK \
             (octet_length(encrypted_credentials) BETWEEN 30 AND 8221)",
        )
        .await?;
        Ok(())
    }
}
