//! Reward-side precondition for completing a requested Dashboard user deletion.
#[cfg(test)]
#[path = "deletion_tests.rs"]
mod tests;

use super::{guard, service::Rewards};
use crate::dashboard::{
    db::entities::{hoyolab_account, reward_guard, reward_run, reward_setting},
    error::{Error, Result},
    hoyolab::{gateway::Gateway, store::CookieStore},
};
use chrono::Utc;
use sea_orm::{ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter};
use std::sync::Arc;
use uuid::Uuid;

/// A setting whose earlier POST may have happened, with its unresolved `may_have_sent` runs.
pub(super) struct Unresolved {
    pub(super) setting: reward_setting::Model,
    pub(super) runs: Vec<reward_run::Model>,
}

/// Settings that still need a bounded GET before their owner can be deleted.
pub(super) async fn unresolved_settings(
    db: &impl ConnectionTrait,
    owner: Uuid,
) -> Result<Vec<Unresolved>> {
    let Some(link) = hoyolab_account::Entity::find()
        .filter(hoyolab_account::Column::OwnerUserId.eq(owner))
        .one(db)
        .await?
    else {
        return Ok(Vec::new());
    };
    let settings = reward_setting::Entity::find()
        .filter(reward_setting::Column::HoyolabAccountId.eq(link.id))
        .all(db)
        .await?;
    let now = Utc::now();
    let mut unresolved = Vec::new();
    for setting in settings {
        let guard_row = reward_guard::Entity::find_by_id(setting.id).one(db).await?;
        let runs = reward_run::Entity::find()
            .filter(reward_run::Column::SettingId.eq(setting.id))
            .filter(reward_run::Column::SendState.eq("may_have_sent"))
            .all(db)
            .await?;
        // A guard outlives five-day history, and an ended run may predate the guard's day.
        if guard::post_unresolved(guard_row.as_ref(), now) || !runs.is_empty() {
            unresolved.push(Unresolved { setting, runs });
        }
    }
    Ok(unresolved)
}

/// Run under the owner's operation lock before `Accounts::finish_deletion`.
/// Without a cookie key nothing can be confirmed, so unresolved POSTs postpone deletion.
pub(in crate::dashboard) async fn confirm_owner<G: Gateway>(
    db: &impl ConnectionTrait,
    cookies: Option<&Arc<CookieStore<G>>>,
    owner: Uuid,
) -> Result<()> {
    match cookies {
        Some(store) => {
            Rewards::new(Arc::clone(store))
                .confirm_owner_for_deletion(owner)
                .await
        }
        None if unresolved_settings(db, owner).await?.is_empty() => Ok(()),
        None => Err(Error::Unavailable(
            "Cookie key is required to confirm an earlier operation",
        )),
    }
}
