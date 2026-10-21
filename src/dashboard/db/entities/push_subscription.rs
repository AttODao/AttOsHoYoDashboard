//! A browser's Web Push endpoint and its message-encryption keys.
use sea_orm::entity::prelude::*;

#[derive(Clone, PartialEq, DeriveEntityModel, derive_more::Debug)]
#[debug("PushSubscription {{ id: {id:?}, .. }}")]
#[sea_orm(table_name = "push_subscriptions")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub user_id: Uuid,
    #[sea_orm(unique)]
    pub endpoint: String,
    pub p256dh: String,
    pub auth: String,
    pub created_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}
impl ActiveModelBehavior for ActiveModel {}
