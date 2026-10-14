use super::{
    super::{
        Rewards,
        cache::{CacheWrite, upsert_cache},
    },
    state::{finish_run_tx, upsert_guard},
};
use crate::dashboard::{
    db::entities::{
        hoyolab_account, reward_guard, reward_run, reward_setting, reward_status_cache,
    },
    error::{Error, Result},
    hoyolab::gateway::{self, Gateway},
};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, TransactionTrait};
use uuid::Uuid;

impl<G: Gateway> Rewards<G> {
    pub(in crate::dashboard) async fn confirm_owner_for_deletion(&self, owner: Uuid) -> Result<()> {
        let Some(link) = hoyolab_account::Entity::find()
            .filter(hoyolab_account::Column::OwnerUserId.eq(owner))
            .one(&self.store.db)
            .await?
        else {
            return Ok(());
        };
        let settings = reward_setting::Entity::find()
            .filter(reward_setting::Column::HoyolabAccountId.eq(link.id))
            .all(&self.store.db)
            .await?;
        for setting in settings {
            let guard = reward_guard::Entity::find_by_id(setting.id)
                .one(&self.store.db)
                .await?;
            let may_have_guard = guard
                .as_ref()
                .is_some_and(|guard| guard.post_may_have_happened);
            let runs: Vec<_> = reward_run::Entity::find()
                .filter(reward_run::Column::SettingId.eq(setting.id))
                .filter(reward_run::Column::SendState.eq("may_have_sent"))
                .all(&self.store.db)
                .await?;
            if !may_have_guard && runs.is_empty() {
                continue;
            }
            // A guard outlives five-day history. It still requires a bounded GET before deletion.
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
                finish_run_tx(&tx, run.id, None, state, "may_have_sent", None, true).await?;
            }
            crate::dashboard::auth::accounts::record(&tx, None, owner, "reward_delete_checked")
                .await?;
            tx.commit().await?;
        }
        Ok(())
    }
}
