//! Individual login reward attempts; diagnostics omit sanitized error classes too.
use sea_orm::entity::prelude::*;

#[derive(Clone, PartialEq, DeriveEntityModel)]
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
impl std::fmt::Debug for Model {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RewardAttempt")
            .field("id", &self.id)
            .field("run_id", &self.run_id)
            .field("number", &self.number)
            .finish_non_exhaustive()
    }
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}
impl ActiveModelBehavior for ActiveModel {}
