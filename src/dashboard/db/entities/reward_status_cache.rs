//! Latest login reward status snapshot; diagnostics omit payload and error details.
use sea_orm::entity::prelude::*;

#[derive(Clone, PartialEq, DeriveEntityModel, derive_more::Debug)]
#[debug(
    "RewardStatusCache {{ setting_id: {setting_id:?}, fetched_at: {fetched_at:?}, updated_at: {updated_at:?}, .. }}"
)]
#[sea_orm(table_name = "reward_status_cache")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub setting_id: Uuid,
    pub fetched_at: Option<DateTimeUtc>,
    pub updated_at: DateTimeUtc,
    pub payload: Option<Json>,
    pub payload_version: i32,
    pub error_code: Option<String>,
    pub error_at: Option<DateTimeUtc>,
}

// Generated ActiveModel diagnostics include payload and error_code: never log/instrument it.
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}
impl ActiveModelBehavior for ActiveModel {}
