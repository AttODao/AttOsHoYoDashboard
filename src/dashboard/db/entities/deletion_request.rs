//! Durable marker for queued Dashboard user deletion.
use sea_orm::entity::prelude::*;

#[derive(Clone, PartialEq, DeriveEntityModel, derive_more::Debug)]
#[debug(
    "DeletionRequest {{ user_id: {user_id:?}, requested_by_user_id: {requested_by_user_id:?}, .. }}"
)]
#[sea_orm(table_name = "deletion_requests")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub user_id: Uuid,
    pub requested_by_user_id: Option<Uuid>,
    pub created_at: DateTimeUtc,
}

// Generated ActiveModel diagnostics are not a logging boundary: never log/instrument it.
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}
impl ActiveModelBehavior for ActiveModel {}
