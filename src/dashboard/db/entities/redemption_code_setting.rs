//! A game UID's redemption-code mode (`off`, `notify` or `auto`).
use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "redemption_code_settings")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub game_account_id: Uuid,
    pub mode: String,
    /// When the mode last left `off`; codes first seen before it are never announced.
    pub enabled_at: Option<DateTimeUtc>,
    pub updated_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}
impl ActiveModelBehavior for ActiveModel {}
