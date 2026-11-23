//! The single code-source row: administrator controls and the last fetch outcome.
use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "redemption_code_source")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: i16,
    pub enabled: bool,
    pub interval_minutes: i32,
    pub last_attempt_at: Option<DateTimeUtc>,
    pub last_success_at: Option<DateTimeUtc>,
    pub consecutive_failures: i32,
    /// Error class of the latest failed fetch, never an upstream body.
    pub last_error: Option<String>,
    pub updated_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}
impl ActiveModelBehavior for ActiveModel {}
