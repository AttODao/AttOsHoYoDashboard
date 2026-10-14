//! Detected game UIDs for one HoYoLAB account; diagnostics omit the UID.
use sea_orm::entity::prelude::*;

#[derive(Clone, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "game_accounts")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub hoyolab_account_id: Uuid,
    pub game_kind: String,
    pub game_uid: String,
    pub region: String,
    pub visible: bool,
    pub detected: bool,
    pub last_detected_at: DateTimeUtc,
}

// Generated ActiveModel diagnostics include the game UID: never log/instrument it.
impl std::fmt::Debug for Model {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GameAccount")
            .field("id", &self.id)
            .field("hoyolab_account_id", &self.hoyolab_account_id)
            .finish_non_exhaustive()
    }
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}
impl ActiveModelBehavior for ActiveModel {}
