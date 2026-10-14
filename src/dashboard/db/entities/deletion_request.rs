//! Durable marker for queued Dashboard user deletion.
use sea_orm::entity::prelude::*;

#[derive(Clone, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "deletion_requests")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub user_id: Uuid,
    pub requested_by_user_id: Option<Uuid>,
    pub created_at: DateTimeUtc,
}

// Generated ActiveModel diagnostics are not a logging boundary: never log/instrument it.
impl std::fmt::Debug for Model {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DeletionRequest")
            .field("user_id", &self.user_id)
            .field("requested_by_user_id", &self.requested_by_user_id)
            .finish_non_exhaustive()
    }
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}
impl ActiveModelBehavior for ActiveModel {}
