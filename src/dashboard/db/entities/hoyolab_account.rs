//! One owner's encrypted credentials; a nullable UID is never ownership proof.
use sea_orm::entity::prelude::*;

#[derive(Clone, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "hoyolab_accounts")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    #[sea_orm(unique)]
    pub owner_user_id: Uuid,
    #[sea_orm(unique)]
    pub hoyolab_uid: Option<String>,
    pub encrypted_credentials: Vec<u8>,
    pub credential_version: i64,
    pub auth_status: String,
    pub created_at: DateTimeUtc,
    pub updated_at: DateTimeUtc,
}

// Generated ActiveModel diagnostics are also unsuitable for logging.
impl std::fmt::Debug for Model {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HoyolabAccount").finish_non_exhaustive()
    }
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}
impl ActiveModelBehavior for ActiveModel {}
