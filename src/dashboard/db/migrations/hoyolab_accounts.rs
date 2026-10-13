//! Unique owner and verified UID, pending credentials remain explicitly unverified.
use sea_orm_migration::{prelude::*, schema::*};

pub(super) struct HoyolabAccounts;
impl MigrationName for HoyolabAccounts {
    fn name(&self) -> &str {
        "m20261013_000003_hoyolab_accounts"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for HoyolabAccounts {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(HoyolabAccount::Table)
                    .col(pk_uuid(HoyolabAccount::Id))
                    .col(uuid(HoyolabAccount::OwnerUserId).unique_key())
                    .col(string_len_null(HoyolabAccount::HoyolabUid, 20).unique_key())
                    .col(binary(HoyolabAccount::EncryptedCredentials))
                    .col(big_integer(HoyolabAccount::CredentialVersion))
                    .col(string_len(HoyolabAccount::AuthStatus, 32))
                    .col(timestamp_with_time_zone(HoyolabAccount::CreatedAt))
                    .col(timestamp_with_time_zone(HoyolabAccount::UpdatedAt))
                    .foreign_key(
                        ForeignKey::create()
                            .name("hoyolab_accounts_owner_fk")
                            .from(HoyolabAccount::Table, HoyolabAccount::OwnerUserId)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .get_connection()
            .execute_unprepared(
                "ALTER TABLE hoyolab_accounts \
             ADD CONSTRAINT hoyolab_accounts_state_check CHECK \
             ((auth_status = 'pending_verification' AND hoyolab_uid IS NULL) OR \
              (auth_status = 'verified' AND hoyolab_uid IS NOT NULL)), \
             ADD CONSTRAINT hoyolab_accounts_uid_check CHECK \
             (hoyolab_uid IS NULL OR hoyolab_uid ~ '^[1-9][0-9]{0,19}$'), \
             ADD CONSTRAINT hoyolab_accounts_version_check CHECK (credential_version > 0), \
             ADD CONSTRAINT hoyolab_accounts_ciphertext_check CHECK \
             (octet_length(encrypted_credentials) BETWEEN 30 AND 8221)",
            )
            .await?;
        Ok(())
    }
    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(HoyolabAccount::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum HoyolabAccount {
    #[sea_orm(iden = "hoyolab_accounts")]
    Table,
    Id,
    OwnerUserId,
    HoyolabUid,
    EncryptedCredentials,
    CredentialVersion,
    AuthStatus,
    CreatedAt,
    UpdatedAt,
}
#[derive(DeriveIden)]
enum Users {
    Table,
    Id,
}
