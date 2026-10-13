//! Persisted WebAuthn credentials with redacted model diagnostics.
use sea_orm::entity::prelude::*;

#[derive(Clone, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "passkeys")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub user_id: Uuid,
    #[sea_orm(unique)]
    pub credential_id: Vec<u8>,
    pub label: String,
    pub credential: Json,
    pub created_at: DateTimeUtc,
}

// Never expose the stored WebAuthn credential, even in error diagnostics.
// SeaORM's generated ActiveModel has unredacted Debug: never log/instrument it.
impl std::fmt::Debug for Model {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Passkey")
            .field("id", &self.id)
            .finish_non_exhaustive()
    }
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}
impl ActiveModelBehavior for ActiveModel {}
