use super::super::{HISTORY_DAYS, RUN_LIMIT, Rewards, enqueue_in};
use crate::dashboard::{
    db::entities::{hoyolab_account, reward_guard, reward_run, reward_setting},
    error::{Error, Result},
    hoyolab::gateway::Gateway,
    rewards::schedule,
};
use chrono::{Duration, Utc};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, EntityTrait, IntoActiveModel, QueryFilter, QueryOrder,
    QuerySelect, Set, TransactionTrait,
};
use tokio::task::JoinSet;

impl<G: Gateway> Rewards<G> {
    pub(in crate::dashboard) async fn tick(&self) -> Result<()> {
        if !self.store.reward_available() || !self.store.verification_available() {
            return Ok(());
        }
        self.cleanup_history().await?;
        self.enqueue_due_schedules().await?;
        let runs = reward_run::Entity::find()
            .filter(reward_run::Column::State.is_in(["pending", "running", "retry_wait"]))
            .filter(
                sea_orm::Condition::any()
                    .add(reward_run::Column::NextRetryAt.is_null())
                    .add(reward_run::Column::NextRetryAt.lte(Utc::now())),
            )
            .order_by_asc(reward_run::Column::CreatedAt)
            .limit(RUN_LIMIT)
            .all(&self.store.db)
            .await?;
        let mut set = JoinSet::new();
        for run in runs {
            let rewards = self.clone();
            set.spawn(async move { rewards.process_run(run.id).await });
        }
        while let Some(result) = set.join_next().await {
            result.map_err(|_| Error::Internal)??;
        }
        Ok(())
    }

    async fn enqueue_due_schedules(&self) -> Result<()> {
        let now = Utc::now();
        let due = reward_setting::Entity::find()
            .filter(reward_setting::Column::Enabled.eq(true))
            .filter(reward_setting::Column::AuthStatus.eq("ready"))
            .filter(reward_setting::Column::NextRunAt.lte(now))
            .limit(RUN_LIMIT)
            .all(&self.store.db)
            .await?;
        for setting in due {
            let Some(link_snapshot) =
                hoyolab_account::Entity::find_by_id(setting.hoyolab_account_id)
                    .one(&self.store.db)
                    .await?
            else {
                continue;
            };
            let tx = self.store.db.begin().await?;
            let ready = match self
                .store
                .owner_link_in(&tx, link_snapshot.owner_user_id)
                .await
            {
                Ok((link, _)) => Some(link),
                Err(
                    Error::Unauthorized
                    | Error::PasswordChangeRequired
                    | Error::InitialSetupRequired,
                ) => None,
                Err(error) => return Err(error),
            };
            let setting = reward_setting::Entity::find_by_id(setting.id)
                .lock_exclusive()
                .one(&tx)
                .await?
                .ok_or(Error::NotFound)?;
            if !setting.enabled
                || setting.auth_status != "ready"
                || setting.next_run_at.is_none_or(|due| due > now)
            {
                tx.commit().await?;
                continue;
            }
            let next = schedule::next_due(now, &setting.local_time, self.store.timezone)?;
            let guard = reward_guard::Entity::find_by_id(setting.id)
                .one(&tx)
                .await?;
            let unresolved = guard.is_some_and(|guard| guard.post_may_have_happened);
            let mut row = setting.clone().into_active_model();
            row.next_run_at = Set(ready.as_ref().map(|_| next));
            row.update(&tx).await?;
            if let Some(link) = ready.filter(|_| !unresolved) {
                let _ = enqueue_in(&tx, &setting, link.credential_version, "catchup").await?;
            }
            tx.commit().await?;
        }
        Ok(())
    }

    async fn cleanup_history(&self) -> Result<()> {
        let cutoff = Utc::now() - Duration::days(HISTORY_DAYS);
        // Preserve interrupted work for confirmation; the attempts FK handles completed history cleanup.
        reward_run::Entity::delete_many()
            .filter(reward_run::Column::CreatedAt.lt(cutoff))
            .filter(reward_run::Column::State.is_not_in(["pending", "running", "retry_wait"]))
            .exec(&self.store.db)
            .await?;
        Ok(())
    }
}
