//! Durable guard for reward POSTs that may already have happened.
use sea_orm::entity::prelude::*;

#[derive(Clone, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "reward_guards")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub setting_id: Uuid,
    pub post_may_have_happened: bool,
    pub updated_at: DateTimeUtc,
}

// Generated ActiveModel diagnostics are not a logging boundary: never log/instrument it.
impl std::fmt::Debug for Model {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RewardGuard")
            .field("setting_id", &self.setting_id)
            .finish_non_exhaustive()
    }
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}
impl ActiveModelBehavior for ActiveModel {}
