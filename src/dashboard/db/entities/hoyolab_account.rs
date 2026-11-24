//! One owner's encrypted credentials; a nullable UID is never ownership proof.
use sea_orm::entity::prelude::*;

#[derive(Clone, PartialEq, DeriveEntityModel, derive_more::Debug)]
#[debug("HoyolabAccount {{ .. }}")]
#[sea_orm(table_name = "hoyolab_accounts")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    #[sea_orm(unique)]
    pub owner_user_id: Uuid,
    #[sea_orm(unique)]
    pub hoyolab_uid: Option<String>,
    /// Absent only after the owner removed the Cookie (`auth_status = "removed"`).
    pub encrypted_credentials: Option<Vec<u8>>,
    pub credential_version: i64,
    pub auth_status: String,
    pub created_at: DateTimeUtc,
    pub updated_at: DateTimeUtc,
    /// Last successful identity verification.
    pub verified_at: Option<DateTimeUtc>,
    /// Whether the sealed Cookie carries the redemption credentials (`cookie_token_v2`).
    pub redemption_credentials: bool,
    /// Why code redemption waits for the owner (`cookie_invalid` or `captcha_required`);
    /// cleared by a Cookie update or an explicit retry.
    pub redemption_action: Option<String>,
    pub redemption_action_at: Option<DateTimeUtc>,
}

impl Model {
    /// The sealed Cookie; a removed Cookie means setup must start again.
    pub(in crate::dashboard) fn sealed_credentials(
        &self,
    ) -> crate::dashboard::error::Result<&[u8]> {
        self.encrypted_credentials
            .as_deref()
            .ok_or(crate::dashboard::error::Error::InitialSetupRequired)
    }
}

// Generated ActiveModel diagnostics are also unsuitable for logging.
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}
impl ActiveModelBehavior for ActiveModel {}
