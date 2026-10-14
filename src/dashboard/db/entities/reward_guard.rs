//! Durable guard for reward POSTs that may already have happened.
use sea_orm::entity::prelude::*;

#[derive(Clone, PartialEq, DeriveEntityModel, derive_more::Debug)]
#[debug("RewardGuard {{ setting_id: {setting_id:?}, .. }}")]
#[sea_orm(table_name = "reward_guards")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub setting_id: Uuid,
    pub post_may_have_happened: bool,
    pub updated_at: DateTimeUtc,
}

// Generated ActiveModel diagnostics are not a logging boundary: never log/instrument it.
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}
impl ActiveModelBehavior for ActiveModel {}
