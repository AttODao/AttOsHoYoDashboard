//! Webhook and browser-push notification settings, delivery log and scan cursor.
use sea_orm_migration::prelude::*;

pub(super) struct Notifications;
impl MigrationName for Notifications {
    fn name(&self) -> &str {
        "m20261020_000007_notifications"
    }
}

/// Columns shared by personal and administrator channels.
const CHANNEL: &str = "webhook_url bytea NULL CHECK (webhook_url IS NULL OR octet_length(webhook_url) BETWEEN 29 AND 1100), \
     webhook_format varchar(16) NOT NULL DEFAULT 'discord' CHECK (webhook_format IN ('discord', 'json')), \
     language varchar(8) NOT NULL DEFAULT 'ja' CHECK (language IN ('ja', 'en')), \
     events jsonb NOT NULL DEFAULT '{}'::jsonb CHECK (jsonb_typeof(events) = 'object'), \
     last_delivery_at timestamptz NULL, \
     last_delivery_ok boolean NULL, \
     updated_at timestamptz NOT NULL DEFAULT now()";

#[async_trait::async_trait]
impl MigrationTrait for Notifications {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        db.execute_unprepared(&format!(
            "CREATE TABLE notification_settings ( \
             user_id uuid PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE, \
             {CHANNEL}, \
             resin_warn_hours integer NOT NULL DEFAULT 2 CHECK (resin_warn_hours BETWEEN 1 AND 24), \
             daily_reminder_time varchar(5) NOT NULL DEFAULT '21:00' CHECK (daily_reminder_time ~ '^([01][0-9]|2[0-3]):[0-5][0-9]$'), \
             weekly_reminder_time varchar(5) NOT NULL DEFAULT '15:00' CHECK (weekly_reminder_time ~ '^([01][0-9]|2[0-3]):[0-5][0-9]$'))"
        ))
        .await?;
        db.execute_unprepared(&format!(
            "CREATE TABLE admin_notification_settings ( \
             id smallint PRIMARY KEY CHECK (id = 1), {CHANNEL})"
        ))
        .await?;
        // Scope is a user id, or the nil UUID for the administrator channel. Keys
        // are deduplication handles such as "<account>:resin_full" or "<run>:failed".
        db.execute_unprepared(
            "CREATE TABLE notification_log ( \
             scope uuid NOT NULL, \
             event_key varchar(200) NOT NULL, \
             sent_at timestamptz NOT NULL DEFAULT now(), \
             PRIMARY KEY (scope, event_key))",
        )
        .await?;
        db.execute_unprepared(
            "CREATE INDEX notification_log_sent_at_idx ON notification_log (sent_at)",
        )
        .await?;
        db.execute_unprepared(
            "CREATE TABLE notification_cursor ( \
             id smallint PRIMARY KEY CHECK (id = 1), \
             scanned_until timestamptz NOT NULL)",
        )
        .await?;
        db.execute_unprepared(
            "CREATE TABLE push_subscriptions ( \
             id uuid PRIMARY KEY, \
             user_id uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE, \
             endpoint varchar(1024) NOT NULL UNIQUE CHECK (endpoint ~ '^https://'), \
             p256dh varchar(128) NOT NULL, \
             auth varchar(64) NOT NULL, \
             created_at timestamptz NOT NULL DEFAULT now())",
        )
        .await?;
        db.execute_unprepared(
            "CREATE INDEX push_subscriptions_user_idx ON push_subscriptions (user_id)",
        )
        .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        for table in [
            "push_subscriptions",
            "notification_cursor",
            "notification_log",
            "admin_notification_settings",
            "notification_settings",
        ] {
            db.execute_unprepared(&format!("DROP TABLE {table}"))
                .await?;
        }
        Ok(())
    }
}
