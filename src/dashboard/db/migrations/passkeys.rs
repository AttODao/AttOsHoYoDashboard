//! Optional passkey credential schema and owner index.
use sea_orm_migration::{prelude::*, schema::*};

pub(super) struct Passkeys;

impl MigrationName for Passkeys {
    fn name(&self) -> &str {
        "m20261012_000002_passkeys"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Passkeys {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Passkey::Table)
                    .col(pk_uuid(Passkey::Id))
                    .col(uuid(Passkey::UserId))
                    .col(binary(Passkey::CredentialId).unique_key())
                    .col(string_len(Passkey::Label, 100))
                    .col(json_binary(Passkey::Credential))
                    .col(timestamp_with_time_zone(Passkey::CreatedAt))
                    .foreign_key(
                        ForeignKey::create()
                            .name("passkeys_user_fk")
                            .from(Passkey::Table, Passkey::UserId)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("passkeys_user_idx")
                    .table(Passkey::Table)
                    .col(Passkey::UserId)
                    .to_owned(),
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Passkey::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum Passkey {
    #[sea_orm(iden = "passkeys")]
    Table,
    Id,
    UserId,
    CredentialId,
    Label,
    Credential,
    CreatedAt,
}

#[derive(DeriveIden)]
enum Users {
    Table,
    Id,
}
