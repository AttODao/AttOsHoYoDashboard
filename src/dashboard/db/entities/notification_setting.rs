//! A user's notification channel and conditions; the webhook URL is sealed.
use sea_orm::entity::prelude::*;

#[derive(Clone, PartialEq, DeriveEntityModel, derive_more::Debug)]
#[debug("NotificationSetting {{ user_id: {user_id:?}, .. }}")]
#[sea_orm(table_name = "notification_settings")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub user_id: Uuid,
    pub webhook_url: Option<Vec<u8>>,
    pub webhook_format: String,
    pub language: String,
    pub events: Json,
    pub last_delivery_at: Option<DateTimeUtc>,
    pub last_delivery_ok: Option<bool>,
    pub updated_at: DateTimeUtc,
    pub resin_warn_hours: i32,
    pub daily_reminder_time: String,
    pub weekly_reminder_time: String,
    /// Per-event choices for the app inbox; `events` holds the webhook's.
    pub app_events: Json,
}

// Generated ActiveModel diagnostics are not a logging boundary: never log/instrument it.
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}
impl ActiveModelBehavior for ActiveModel {}
