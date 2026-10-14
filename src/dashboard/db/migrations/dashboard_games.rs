//! Dashboard game UID, reward execution, status cache, and deletion request schema.
use sea_orm_migration::{prelude::*, schema::*};

pub(super) struct DashboardGames;
impl MigrationName for DashboardGames {
    fn name(&self) -> &str {
        "m20261013_000004_dashboard_games"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for DashboardGames {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(
                "ALTER TABLE hoyolab_accounts \
                 DROP CONSTRAINT hoyolab_accounts_state_check, \
                 ADD CONSTRAINT hoyolab_accounts_state_check CHECK \
                 ((auth_status = 'pending_verification' AND hoyolab_uid IS NULL) OR \
                  (auth_status IN ('verified', 'cookie_invalid') AND hoyolab_uid IS NOT NULL))",
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(DeletionRequest::Table)
                    .col(
                        ColumnDef::new(DeletionRequest::UserId)
                            .uuid()
                            .not_null()
                            .primary_key(),
                    )
                    .col(uuid_null(DeletionRequest::RequestedByUserId))
                    .col(timestamp_with_time_zone(DeletionRequest::CreatedAt))
                    .foreign_key(
                        ForeignKey::create()
                            .name("deletion_requests_user_fk")
                            .from(DeletionRequest::Table, DeletionRequest::UserId)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("deletion_requests_requested_by_fk")
                            .from(DeletionRequest::Table, DeletionRequest::RequestedByUserId)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::SetNull),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("deletion_requests_requested_by_idx")
                    .table(DeletionRequest::Table)
                    .col(DeletionRequest::RequestedByUserId)
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(GameAccount::Table)
                    .col(pk_uuid(GameAccount::Id))
                    .col(uuid(GameAccount::HoyolabAccountId))
                    .col(string_len(GameAccount::GameKind, 16))
                    .col(string_len(GameAccount::GameUid, 20))
                    .col(string_len(GameAccount::Region, 64))
                    .col(boolean(GameAccount::Visible).default(false))
                    .col(boolean(GameAccount::Detected).default(true))
                    .col(timestamp_with_time_zone(GameAccount::LastDetectedAt))
                    .foreign_key(
                        ForeignKey::create()
                            .name("game_accounts_hoyolab_account_fk")
                            .from(GameAccount::Table, GameAccount::HoyolabAccountId)
                            .to(HoyolabAccount::Table, HoyolabAccount::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("game_accounts_link_kind_uid_region_key")
                    .table(GameAccount::Table)
                    .col(GameAccount::HoyolabAccountId)
                    .col(GameAccount::GameKind)
                    .col(GameAccount::GameUid)
                    .col(GameAccount::Region)
                    .unique()
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(RewardSetting::Table)
                    .col(pk_uuid(RewardSetting::Id))
                    .col(uuid(RewardSetting::HoyolabAccountId))
                    .col(string_len(RewardSetting::GameKind, 16))
                    .col(boolean(RewardSetting::Enabled).default(false))
                    .col(string_len(RewardSetting::LocalTime, 5).default("06:00"))
                    .col(string_len(RewardSetting::AuthStatus, 32).default("ready"))
                    .col(
                        ColumnDef::new(RewardSetting::NextRunAt)
                            .timestamp_with_time_zone()
                            .null(),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("reward_settings_hoyolab_account_fk")
                            .from(RewardSetting::Table, RewardSetting::HoyolabAccountId)
                            .to(HoyolabAccount::Table, HoyolabAccount::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("reward_settings_link_kind_key")
                    .table(RewardSetting::Table)
                    .col(RewardSetting::HoyolabAccountId)
                    .col(RewardSetting::GameKind)
                    .unique()
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(RewardRun::Table)
                    .col(pk_uuid(RewardRun::Id))
                    .col(uuid(RewardRun::SettingId))
                    .col(string_len(RewardRun::Trigger, 16))
                    .col(string_len(RewardRun::State, 32))
                    .col(timestamp_with_time_zone(RewardRun::CreatedAt))
                    .col(timestamp_with_time_zone(RewardRun::UpdatedAt))
                    .col(
                        ColumnDef::new(RewardRun::EndedAt)
                            .timestamp_with_time_zone()
                            .null(),
                    )
                    .col(integer(RewardRun::AttemptCount).default(0))
                    .col(big_integer(RewardRun::CredentialVersion))
                    .col(string_len(RewardRun::SendState, 32).default("not_sent"))
                    .col(
                        ColumnDef::new(RewardRun::NextRetryAt)
                            .timestamp_with_time_zone()
                            .null(),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("reward_runs_setting_fk")
                            .from(RewardRun::Table, RewardRun::SettingId)
                            .to(RewardSetting::Table, RewardSetting::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("reward_runs_setting_idx")
                    .table(RewardRun::Table)
                    .col(RewardRun::SettingId)
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(RewardAttempt::Table)
                    .col(pk_uuid(RewardAttempt::Id))
                    .col(uuid(RewardAttempt::RunId))
                    .col(integer(RewardAttempt::Number))
                    .col(timestamp_with_time_zone(RewardAttempt::StartedAt))
                    .col(
                        ColumnDef::new(RewardAttempt::FinishedAt)
                            .timestamp_with_time_zone()
                            .null(),
                    )
                    .col(string_len(RewardAttempt::Outcome, 32))
                    .col(string_len_null(RewardAttempt::ErrorCode, 64))
                    .col(string_len(RewardAttempt::SendState, 32).default("not_sent"))
                    .foreign_key(
                        ForeignKey::create()
                            .name("reward_attempts_run_fk")
                            .from(RewardAttempt::Table, RewardAttempt::RunId)
                            .to(RewardRun::Table, RewardRun::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("reward_attempts_run_number_key")
                    .table(RewardAttempt::Table)
                    .col(RewardAttempt::RunId)
                    .col(RewardAttempt::Number)
                    .unique()
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(RewardGuard::Table)
                    .col(
                        ColumnDef::new(RewardGuard::SettingId)
                            .uuid()
                            .not_null()
                            .primary_key(),
                    )
                    .col(boolean(RewardGuard::PostMayHaveHappened).default(false))
                    .col(timestamp_with_time_zone(RewardGuard::UpdatedAt))
                    .foreign_key(
                        ForeignKey::create()
                            .name("reward_guards_setting_fk")
                            .from(RewardGuard::Table, RewardGuard::SettingId)
                            .to(RewardSetting::Table, RewardSetting::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(RewardStatusCache::Table)
                    .col(
                        ColumnDef::new(RewardStatusCache::SettingId)
                            .uuid()
                            .not_null()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(RewardStatusCache::FetchedAt)
                            .timestamp_with_time_zone()
                            .null(),
                    )
                    .col(timestamp_with_time_zone(RewardStatusCache::UpdatedAt))
                    .col(
                        ColumnDef::new(RewardStatusCache::Payload)
                            .json_binary()
                            .null(),
                    )
                    .col(integer(RewardStatusCache::PayloadVersion).default(1))
                    .col(string_len_null(RewardStatusCache::ErrorCode, 64))
                    .col(
                        ColumnDef::new(RewardStatusCache::ErrorAt)
                            .timestamp_with_time_zone()
                            .null(),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("reward_status_cache_setting_fk")
                            .from(RewardStatusCache::Table, RewardStatusCache::SettingId)
                            .to(RewardSetting::Table, RewardSetting::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(GameStatusCache::Table)
                    .col(
                        ColumnDef::new(GameStatusCache::GameAccountId)
                            .uuid()
                            .not_null()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(GameStatusCache::FetchedAt)
                            .timestamp_with_time_zone()
                            .null(),
                    )
                    .col(timestamp_with_time_zone(GameStatusCache::UpdatedAt))
                    .col(
                        ColumnDef::new(GameStatusCache::Payload)
                            .json_binary()
                            .null(),
                    )
                    .col(integer(GameStatusCache::PayloadVersion).default(1))
                    .col(string_len_null(GameStatusCache::ErrorCode, 64))
                    .col(
                        ColumnDef::new(GameStatusCache::ErrorAt)
                            .timestamp_with_time_zone()
                            .null(),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("game_status_cache_game_account_fk")
                            .from(GameStatusCache::Table, GameStatusCache::GameAccountId)
                            .to(GameAccount::Table, GameAccount::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        let db = manager.get_connection();
        db.execute_unprepared(
            "ALTER TABLE game_accounts \
             ADD CONSTRAINT game_accounts_kind_check CHECK (game_kind IN ('genshin', 'starrail')), \
             ADD CONSTRAINT game_accounts_uid_check CHECK (game_uid ~ '^[1-9][0-9]{0,19}$')",
        )
        .await?;
        db.execute_unprepared(
            "ALTER TABLE reward_settings \
             ADD CONSTRAINT reward_settings_kind_check CHECK (game_kind IN ('genshin', 'starrail')), \
             ADD CONSTRAINT reward_settings_local_time_check CHECK (local_time ~ '^([01][0-9]|2[0-3]):[0-5][0-9]$'), \
             ADD CONSTRAINT reward_settings_auth_status_check CHECK (auth_status IN ('ready', 'action_required'))",
        )
        .await?;
        db.execute_unprepared(
            "ALTER TABLE reward_runs \
             ADD CONSTRAINT reward_runs_trigger_check CHECK (trigger IN ('manual', 'scheduled', 'catchup')), \
             ADD CONSTRAINT reward_runs_state_check CHECK (state IN ('pending', 'running', 'retry_wait', 'unknown', 'action_required', 'failed', 'stopped', 'success', 'already_claimed')), \
             ADD CONSTRAINT reward_runs_attempt_count_check CHECK (attempt_count BETWEEN 0 AND 4), \
             ADD CONSTRAINT reward_runs_credential_version_check CHECK (credential_version > 0), \
             ADD CONSTRAINT reward_runs_send_state_check CHECK (send_state IN ('not_sent', 'may_have_sent', 'confirmed'))",
        )
        .await?;
        db.execute_unprepared(
            "ALTER TABLE reward_attempts \
             ADD CONSTRAINT reward_attempts_number_check CHECK (number BETWEEN 1 AND 4), \
             ADD CONSTRAINT reward_attempts_outcome_check CHECK (outcome IN ('pending', 'success', 'already_claimed', 'retry_wait', 'unknown', 'action_required', 'failed', 'stopped')), \
             ADD CONSTRAINT reward_attempts_error_code_check CHECK (error_code IS NULL OR error_code ~ '^[A-Za-z0-9_.:-]{1,64}$'), \
             ADD CONSTRAINT reward_attempts_send_state_check CHECK (send_state IN ('not_sent', 'may_have_sent', 'confirmed'))",
        )
        .await?;
        db.execute_unprepared(
            "ALTER TABLE reward_status_cache \
             ADD CONSTRAINT reward_status_cache_payload_version_check CHECK (payload_version > 0), \
             ADD CONSTRAINT reward_status_cache_error_code_check CHECK (error_code IS NULL OR error_code ~ '^[A-Za-z0-9_.:-]{1,64}$')",
        )
        .await?;
        db.execute_unprepared(
            "ALTER TABLE game_status_cache \
             ADD CONSTRAINT game_status_cache_payload_version_check CHECK (payload_version > 0), \
             ADD CONSTRAINT game_status_cache_error_code_check CHECK (error_code IS NULL OR error_code ~ '^[A-Za-z0-9_.:-]{1,64}$')",
        )
        .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        db.execute_unprepared(
            "ALTER TABLE hoyolab_accounts DROP CONSTRAINT hoyolab_accounts_state_check",
        )
        .await?;
        db.execute_unprepared(
            "UPDATE hoyolab_accounts SET auth_status = 'verified' WHERE auth_status = 'cookie_invalid'",
        )
        .await?;
        db.execute_unprepared(
            "ALTER TABLE hoyolab_accounts ADD CONSTRAINT hoyolab_accounts_state_check CHECK \
             ((auth_status = 'pending_verification' AND hoyolab_uid IS NULL) OR \
              (auth_status = 'verified' AND hoyolab_uid IS NOT NULL))",
        )
        .await?;

        for table in [
            GameStatusCache::Table.into_iden(),
            RewardStatusCache::Table.into_iden(),
            RewardGuard::Table.into_iden(),
            RewardAttempt::Table.into_iden(),
            RewardRun::Table.into_iden(),
            RewardSetting::Table.into_iden(),
            GameAccount::Table.into_iden(),
            DeletionRequest::Table.into_iden(),
        ] {
            manager
                .drop_table(Table::drop().table(table).to_owned())
                .await?;
        }
        Ok(())
    }
}

#[derive(DeriveIden)]
enum DeletionRequest {
    #[sea_orm(iden = "deletion_requests")]
    Table,
    UserId,
    RequestedByUserId,
    CreatedAt,
}

#[derive(DeriveIden)]
enum GameAccount {
    #[sea_orm(iden = "game_accounts")]
    Table,
    Id,
    HoyolabAccountId,
    GameKind,
    GameUid,
    Region,
    Visible,
    Detected,
    LastDetectedAt,
}

#[derive(DeriveIden)]
enum RewardSetting {
    #[sea_orm(iden = "reward_settings")]
    Table,
    Id,
    HoyolabAccountId,
    GameKind,
    Enabled,
    LocalTime,
    AuthStatus,
    NextRunAt,
}

#[derive(DeriveIden)]
enum RewardRun {
    #[sea_orm(iden = "reward_runs")]
    Table,
    Id,
    SettingId,
    Trigger,
    State,
    CreatedAt,
    UpdatedAt,
    EndedAt,
    AttemptCount,
    CredentialVersion,
    SendState,
    NextRetryAt,
}

#[derive(DeriveIden)]
enum RewardAttempt {
    #[sea_orm(iden = "reward_attempts")]
    Table,
    Id,
    RunId,
    Number,
    StartedAt,
    FinishedAt,
    Outcome,
    ErrorCode,
    SendState,
}

#[derive(DeriveIden)]
enum RewardGuard {
    #[sea_orm(iden = "reward_guards")]
    Table,
    SettingId,
    PostMayHaveHappened,
    UpdatedAt,
}

#[derive(DeriveIden)]
enum GameStatusCache {
    #[sea_orm(iden = "game_status_cache")]
    Table,
    GameAccountId,
    FetchedAt,
    UpdatedAt,
    Payload,
    PayloadVersion,
    ErrorCode,
    ErrorAt,
}

#[derive(DeriveIden)]
enum RewardStatusCache {
    #[sea_orm(iden = "reward_status_cache")]
    Table,
    SettingId,
    FetchedAt,
    UpdatedAt,
    Payload,
    PayloadVersion,
    ErrorCode,
    ErrorAt,
}

#[derive(DeriveIden)]
enum HoyolabAccount {
    #[sea_orm(iden = "hoyolab_accounts")]
    Table,
    Id,
}

#[derive(DeriveIden)]
enum Users {
    Table,
    Id,
}
