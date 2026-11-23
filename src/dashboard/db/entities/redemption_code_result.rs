//! What happened with one catalogue code for one game UID.
use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "redemption_code_results")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub game_account_id: Uuid,
    #[sea_orm(primary_key, auto_increment = false)]
    pub code_id: Uuid,
    pub state: String,
    pub created_at: DateTimeUtc,
    pub updated_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}
impl ActiveModelBehavior for ActiveModel {}
