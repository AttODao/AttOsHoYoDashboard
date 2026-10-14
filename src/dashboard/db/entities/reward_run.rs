//! Durable login reward execution records.
use sea_orm::entity::prelude::*;

#[derive(Clone, PartialEq, DeriveEntityModel, derive_more::Debug)]
#[debug("RewardRun {{ id: {id:?}, setting_id: {setting_id:?}, .. }}")]
#[sea_orm(table_name = "reward_runs")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub setting_id: Uuid,
    pub trigger: String,
    pub state: String,
    pub created_at: DateTimeUtc,
    pub updated_at: DateTimeUtc,
    pub ended_at: Option<DateTimeUtc>,
    pub attempt_count: i32,
    pub credential_version: i64,
    pub send_state: String,
    pub next_retry_at: Option<DateTimeUtc>,
}

// Generated ActiveModel diagnostics are not a logging boundary: never log/instrument it.
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}
impl ActiveModelBehavior for ActiveModel {}
