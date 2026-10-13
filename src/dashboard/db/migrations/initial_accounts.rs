//! Initial users, sessions, and account audit schema.
use sea_orm_migration::{prelude::*, schema::*};

pub(super) struct InitialAccounts;

impl MigrationName for InitialAccounts {
    fn name(&self) -> &str {
        "m20261012_000001_accounts"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for InitialAccounts {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Users::Table)
                    .col(pk_uuid(Users::Id))
                    .col(string_len_uniq(Users::Username, 64))
                    .col(text(Users::PasswordHash))
                    .col(boolean(Users::IsAdmin))
                    .col(boolean(Users::Active))
                    .col(boolean(Users::MustChangePassword))
                    .col(timestamp_with_time_zone(Users::CreatedAt))
                    .to_owned(),
            )
            .await?;
        manager
            .create_table(
                Table::create()
                    .table(Sessions::Table)
                    .col(string_len(Sessions::TokenHash, 64).primary_key())
                    .col(uuid(Sessions::UserId))
                    .col(timestamp_with_time_zone(Sessions::ExpiresAt))
                    .foreign_key(
                        ForeignKey::create()
                            .name("sessions_user_fk")
                            .from(Sessions::Table, Sessions::UserId)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("sessions_user_idx")
                    .table(Sessions::Table)
                    .col(Sessions::UserId)
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("sessions_expiry_idx")
                    .table(Sessions::Table)
                    .col(Sessions::ExpiresAt)
                    .to_owned(),
            )
            .await?;
        manager
            .create_table(
                Table::create()
                    .table(AccountAudit::Table)
                    .col(pk_uuid(AccountAudit::Id))
                    .col(uuid_null(AccountAudit::ActorUserId))
                    .col(uuid(AccountAudit::TargetUserId))
                    .col(string_len(AccountAudit::Action, 32))
                    .col(timestamp_with_time_zone(AccountAudit::CreatedAt))
                    .foreign_key(
                        ForeignKey::create()
                            .name("audit_target_fk")
                            .from(AccountAudit::Table, AccountAudit::TargetUserId)
                            .to(Users::Table, Users::Id),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("audit_actor_fk")
                            .from(AccountAudit::Table, AccountAudit::ActorUserId)
                            .to(Users::Table, Users::Id),
                    )
                    .to_owned(),
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        for table in [
            AccountAudit::Table.into_iden(),
            Sessions::Table.into_iden(),
            Users::Table.into_iden(),
        ] {
            manager
                .drop_table(Table::drop().table(table).to_owned())
                .await?;
        }
        Ok(())
    }
}

#[derive(DeriveIden)]
enum Users {
    Table,
    Id,
    Username,
    PasswordHash,
    IsAdmin,
    Active,
    MustChangePassword,
    CreatedAt,
}
#[derive(DeriveIden)]
enum Sessions {
    Table,
    TokenHash,
    UserId,
    ExpiresAt,
}
#[derive(DeriveIden)]
enum AccountAudit {
    Table,
    Id,
    ActorUserId,
    TargetUserId,
    Action,
    CreatedAt,
}
