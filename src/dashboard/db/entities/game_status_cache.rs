//! Latest game status snapshot; diagnostics omit payload and error details.
use sea_orm::entity::prelude::*;

#[derive(Clone, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "game_status_cache")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub game_account_id: Uuid,
    pub fetched_at: Option<DateTimeUtc>,
    pub updated_at: DateTimeUtc,
    pub payload: Option<Json>,
    pub payload_version: i32,
    pub error_code: Option<String>,
    pub error_at: Option<DateTimeUtc>,
}

// Generated ActiveModel diagnostics include payload and error_code: never log/instrument it.
impl std::fmt::Debug for Model {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GameStatusCache")
            .field("game_account_id", &self.game_account_id)
            .field("fetched_at", &self.fetched_at)
            .field("updated_at", &self.updated_at)
            .finish_non_exhaustive()
    }
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}
impl ActiveModelBehavior for ActiveModel {}
