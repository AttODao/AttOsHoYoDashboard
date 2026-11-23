//! A code HoYoLAB answered as expired or invalid: never sent again for any user.
use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "redemption_code_rejections")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub game_kind: String,
    #[sea_orm(primary_key, auto_increment = false)]
    pub code: String,
    /// `expired` or `invalid`.
    pub reason: String,
    pub rejected_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}
impl ActiveModelBehavior for ActiveModel {}
