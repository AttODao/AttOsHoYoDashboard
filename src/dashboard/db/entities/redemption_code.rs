//! One redemption code in the shared catalogue, as the code source last listed it.
use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "redemption_codes")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub game_kind: String,
    pub code: String,
    /// Sanitized reward text from the source; may be empty.
    pub rewards: String,
    pub first_seen_at: DateTimeUtc,
    pub last_seen_at: DateTimeUtc,
    /// Whether the latest successful fetch of its game still listed it.
    pub listed: bool,
    /// Already listed by the first successful fetch of its game: never announced.
    pub seeded: bool,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}
impl ActiveModelBehavior for ActiveModel {}
