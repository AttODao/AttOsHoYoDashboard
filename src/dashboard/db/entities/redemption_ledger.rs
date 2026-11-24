//! One code for one game UID: whether it may have been sent and what HoYoLAB answered.
//! Keyed by the code text, not the catalogue row, so pruning the catalogue never reopens it.
use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "redemption_ledger")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub game_account_id: Uuid,
    #[sea_orm(primary_key, auto_increment = false)]
    pub code: String,
    /// `catalogue` (the shared list) or `manual` (entered by the owner, private).
    pub source: String,
    /// `pending`, `sending` (a POST may have happened), or an outcome.
    pub state: String,
    /// Redemption POSTs made for this row.
    pub posts: i32,
    /// The numeric HoYoLAB retcode of the last answer; never a message or body.
    pub retcode: Option<i32>,
    /// Not sent again before this time (cooldown or the daily rank retry).
    pub next_attempt_at: Option<DateTimeUtc>,
    pub last_sent_at: Option<DateTimeUtc>,
    pub created_at: DateTimeUtc,
    pub updated_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}
impl ActiveModelBehavior for ActiveModel {}
