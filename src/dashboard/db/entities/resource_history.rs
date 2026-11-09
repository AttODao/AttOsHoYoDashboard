//! One resin or Trailblaze Power snapshot of a game account, taken from a status fetch.
use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "resource_history")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub game_account_id: Uuid,
    #[sea_orm(primary_key, auto_increment = false)]
    pub fetched_at: DateTimeUtc,
    pub current: i32,
    pub maximum: i32,
    pub full_at: Option<DateTimeUtc>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}
impl ActiveModelBehavior for ActiveModel {}
