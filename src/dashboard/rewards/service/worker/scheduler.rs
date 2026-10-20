use super::super::{HISTORY_DAYS, RUN_LIMIT, Rewards, enqueue_in};
use crate::dashboard::{
    db::entities::{hoyolab_account, reward_guard, reward_run, reward_setting},
    error::{Error, Result},
    hoyolab::gateway::Gateway,
    rewards::{guard, schedule},
};
use chrono::{DateTime, Duration, Utc};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, EntityTrait, IntoActiveModel, QueryFilter, QueryOrder,
    QuerySelect, Set, TransactionTrait,
};
use tokio::task::JoinSet;
use uuid::Uuid;

const SCHEDULE_RETRY: Duration = Duration::minutes(1);
/// A schedule picked up later than this (downtime, deferral) is recorded as a catch-up run.
const ON_TIME_WINDOW: Duration = Duration::minutes(5);

impl<G: Gateway> Rewards<G> {
    pub(in crate::dashboard) async fn tick(&self) -> Result<()> {
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
            set.spawn(async move { (run.id, rewards.process_run(run.id).await) });
        }
        // Drain every task: dropping the set would abort sibling runs, possibly mid-POST.
        while let Some(result) = set.join_next().await {
            match result {
                Ok((_, Ok(()))) => {}
                Ok((run_id, Err(error))) => {
                    tracing::error!(%run_id, error = %error, "reward run failed");
                }
                Err(_) => tracing::error!("reward run task failed"),
            }
        }
        Ok(())
    }

    async fn enqueue_due_schedules(&self) -> Result<()> {
        let now = Utc::now();
        let due = reward_setting::Entity::find()
            .filter(reward_setting::Column::Enabled.eq(true))
            .filter(reward_setting::Column::AuthStatus.eq("ready"))
            .filter(reward_setting::Column::NextRunAt.lte(now))
            .order_by_asc(reward_setting::Column::NextRunAt)
            .limit(RUN_LIMIT)
            .all(&self.store.db)
            .await?;
        for setting in due {
            if let Err(error) = self.enqueue_due_setting(&setting, now).await {
                tracing::error!(setting_id = %setting.id, error = %error, "reward schedule failed");
                // Back off a failing setting so it neither repeats every second nor keeps
                // filling the due batch; it is retried later within the same schedule.
                if let Err(error) = self.defer_due_setting(setting.id, now).await {
                    tracing::error!(setting_id = %setting.id, error = %error, "reward schedule deferral failed");
                }
            }
        }
        Ok(())
    }

    async fn enqueue_due_setting(
        &self,
        setting: &reward_setting::Model,
        now: DateTime<Utc>,
    ) -> Result<()> {
        let Some(link_snapshot) = hoyolab_account::Entity::find_by_id(setting.hoyolab_account_id)
            .one(&self.store.db)
            .await?
        else {
            return Ok(());
        };
        // Any early return drops the transaction, which rolls it back.
        let tx = self.store.db.begin().await?;
        let ready = match self
            .store
            .owner_link_in(&tx, link_snapshot.owner_user_id)
            .await
        {
            Ok((link, _)) => Some(link),
            Err(
                Error::Unauthorized | Error::PasswordChangeRequired | Error::InitialSetupRequired,
            ) => None,
            Err(error) => return Err(error),
        };
        let setting = reward_setting::Entity::find_by_id(setting.id)
            .lock_exclusive()
            .one(&tx)
            .await?
            .ok_or(Error::NotFound)?;
        let Some(due) = setting
            .next_run_at
            .filter(|due| setting.enabled && setting.auth_status == "ready" && *due <= now)
        else {
            tx.commit().await?;
            return Ok(());
        };
        let next = schedule::next_due(now, &setting.local_time, self.store.timezone)?;
        let guard = reward_guard::Entity::find_by_id(setting.id)
            .one(&tx)
            .await?;
        let unresolved = guard::post_unresolved(guard.as_ref(), now);
        let mut row = setting.clone().into_active_model();
        row.next_run_at = Set(ready.as_ref().map(|_| next));
        row.update(&tx).await?;
        if let Some(link) = ready.filter(|_| !unresolved) {
            let trigger = if now - due <= ON_TIME_WINDOW {
                "scheduled"
            } else {
                "catchup"
            };
            let _ = enqueue_in(&tx, &setting, link.credential_version, trigger).await?;
        }
        tx.commit().await?;
        Ok(())
    }

    async fn defer_due_setting(&self, setting_id: Uuid, now: DateTime<Utc>) -> Result<()> {
        reward_setting::Entity::update_many()
            .col_expr(
                reward_setting::Column::NextRunAt,
                sea_orm::sea_query::Expr::value(now + SCHEDULE_RETRY),
            )
            .filter(reward_setting::Column::Id.eq(setting_id))
            .filter(reward_setting::Column::NextRunAt.lte(now))
            .exec(&self.store.db)
            .await?;
        Ok(())
    }

    /// Periodic maintenance; the runtime calls it far less often than `tick`.
    pub(in crate::dashboard) async fn cleanup_history(&self) -> Result<()> {
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
