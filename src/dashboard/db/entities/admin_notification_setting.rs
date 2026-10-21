//! The single administrator notification channel; the webhook URL is sealed.
use sea_orm::entity::prelude::*;

#[derive(Clone, PartialEq, DeriveEntityModel, derive_more::Debug)]
#[debug("AdminNotificationSetting {{ .. }}")]
#[sea_orm(table_name = "admin_notification_settings")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: i16,
    pub webhook_url: Option<Vec<u8>>,
    pub webhook_format: String,
    pub language: String,
    pub events: Json,
    pub last_delivery_at: Option<DateTimeUtc>,
    pub last_delivery_ok: Option<bool>,
    pub updated_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}
impl ActiveModelBehavior for ActiveModel {}
