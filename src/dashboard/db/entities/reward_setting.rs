//! Per-game login reward configuration for one HoYoLAB account.
use sea_orm::entity::prelude::*;

#[derive(Clone, PartialEq, DeriveEntityModel, derive_more::Debug)]
#[debug("RewardSetting {{ id: {id:?}, hoyolab_account_id: {hoyolab_account_id:?}, .. }}")]
#[sea_orm(table_name = "reward_settings")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub hoyolab_account_id: Uuid,
    pub game_kind: String,
    pub enabled: bool,
    pub local_time: String,
    pub auth_status: String,
    pub next_run_at: Option<DateTimeUtc>,
}

// Generated ActiveModel diagnostics are not a logging boundary: never log/instrument it.
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}
impl ActiveModelBehavior for ActiveModel {}
