use super::{
    super::{
        Rewards,
        cache::{CacheWrite, upsert_cache},
    },
    state::{finish_run_tx, upsert_guard},
};
use crate::dashboard::{
    db::entities::reward_status_cache,
    error::{Error, Result},
    hoyolab::gateway::{self, Gateway},
    rewards::deletion::{Unresolved, unresolved_settings},
};
use sea_orm::{EntityTrait, TransactionTrait};
use uuid::Uuid;

impl<G: Gateway> Rewards<G> {
    pub(in crate::dashboard::rewards) async fn confirm_owner_for_deletion(
        &self,
        owner: Uuid,
    ) -> Result<()> {
        for Unresolved { setting, runs } in unresolved_settings(&self.store.db, owner).await? {
            // Each unresolved setting still requires a bounded GET before deletion.
            let (_, cookie) = self
                .store
                .stored_cookie(owner)
                .await?
                .ok_or(Error::Internal)?;
            let prior = reward_status_cache::Entity::find_by_id(setting.id)
                .one(&self.store.db)
                .await?;
            let (state, write) = match self
                .store
                .gateway
                .daily(&cookie, gateway::game(&setting.game_kind)?)
                .await
            {
                Ok(info) => {
                    let state = if info.is_sign {
                        "already_claimed"
                    } else {
                        "unknown"
                    };
                    (state, CacheWrite::success(info))
                }
                Err(error) => (
                    "unknown",
                    CacheWrite::failure(gateway::error_code(&error), prior.as_ref()),
                ),
            };
            let tx = self.store.db.begin().await?;
            crate::dashboard::auth::accounts::lock_user(&tx, owner).await?;
            upsert_cache(&tx, setting.id, write).await?;
            upsert_guard(&tx, setting.id, true).await?;
            for run in runs {
                finish_run_tx(&tx, run.id, state, "may_have_sent", None, true).await?;
            }
            crate::dashboard::auth::accounts::record(&tx, None, owner, "reward_delete_checked")
                .await?;
            tx.commit().await?;
        }
        Ok(())
    }
}
