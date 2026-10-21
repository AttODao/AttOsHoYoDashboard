//! Sent notifications, so each condition is reported once until it clears.
use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "notification_log")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub scope: Uuid,
    #[sea_orm(primary_key, auto_increment = false)]
    pub event_key: String,
    pub sent_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}
impl ActiveModelBehavior for ActiveModel {}
