//! Individual login reward attempts; diagnostics omit sanitized error classes too.
use sea_orm::entity::prelude::*;

#[derive(Clone, PartialEq, DeriveEntityModel, derive_more::Debug)]
#[debug("RewardAttempt {{ id: {id:?}, run_id: {run_id:?}, number: {number:?}, .. }}")]
#[sea_orm(table_name = "reward_attempts")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub run_id: Uuid,
    pub number: i32,
    pub started_at: DateTimeUtc,
    pub finished_at: Option<DateTimeUtc>,
    pub outcome: String,
    pub error_code: Option<String>,
    pub send_state: String,
}

// Generated ActiveModel diagnostics include error_code: never log/instrument it.
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}
impl ActiveModelBehavior for ActiveModel {}
