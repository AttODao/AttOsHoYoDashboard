use self::cache::{CACHE_VERSION, cache_fresh};
use super::schedule;
use crate::dashboard::{
    auth::sessions::Identity,
    db::entities::{reward_attempt, reward_run, reward_setting, reward_status_cache},
    error::{Error, Result},
    hoyolab::{
        gateway::{self, Gateway, LiveGateway},
        store::CookieStore,
    },
};
use chrono::{DateTime, Duration, Utc};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, EntityTrait, IntoActiveModel, QueryFilter,
    QueryOrder, QuerySelect, Set, TransactionTrait,
};
use serde_json::Value;
use std::sync::Arc;
use uuid::Uuid;

#[cfg(test)]
mod test_support;

mod cache;
#[cfg(test)]
#[path = "service_tests.rs"]
mod tests;
mod worker;

const HISTORY_DAYS: i64 = 5;
const RUN_LIMIT: u64 = 16;
const MAX_ATTEMPTS: i32 = 4;

pub(in crate::dashboard) struct Rewards<G: Gateway = LiveGateway> {
    store: Arc<CookieStore<G>>,
}

impl<G: Gateway> Clone for Rewards<G> {
    fn clone(&self) -> Self {
        Self {
            store: Arc::clone(&self.store),
        }
    }
}

pub(in crate::dashboard) struct RewardView {
    pub(in crate::dashboard) setting: reward_setting::Model,
    pub(in crate::dashboard) last_run: Option<reward_run::Model>,
    pub(in crate::dashboard) is_sign: Option<bool>,
    pub(in crate::dashboard) last_checked_at: Option<DateTime<Utc>>,
    pub(in crate::dashboard) error_code: Option<String>,
    pub(in crate::dashboard) error_at: Option<DateTime<Utc>>,
}

pub(in crate::dashboard) struct RunView {
    pub(in crate::dashboard) run: reward_run::Model,
    pub(in crate::dashboard) game_kind: String,
    pub(in crate::dashboard) attempts: Vec<reward_attempt::Model>,
}

impl<G: Gateway> Rewards<G> {
    pub(in crate::dashboard) fn new(store: Arc<CookieStore<G>>) -> Self {
        Self { store }
    }

    pub(in crate::dashboard) async fn list(&self, actor: &Identity) -> Result<Vec<RewardView>> {
        let _operation = self.store.operations.acquire(actor.user.id).await;
        let now = Utc::now();
        let (link, cookie) = self.store.ready_link(actor).await?;
        let settings = reward_setting::Entity::find()
            .filter(reward_setting::Column::HoyolabAccountId.eq(link.id))
            .order_by_asc(reward_setting::Column::GameKind)
            .all(&self.store.db)
            .await?;
        let mut views = Vec::with_capacity(settings.len());
        for setting in settings {
            let cache = reward_status_cache::Entity::find_by_id(setting.id)
                .one(&self.store.db)
                .await?;
            let cache = if setting.auth_status == "action_required"
                || cache.as_ref().is_some_and(|cache| cache_fresh(cache, now))
            {
                cache
            } else {
                let game = gateway::game(&setting.game_kind)?;
                Some(
                    self.refresh_cache(actor, &link, &cookie, setting.id, game, cache.as_ref())
                        .await?,
                )
            };
            let last_run = reward_run::Entity::find()
                .filter(reward_run::Column::SettingId.eq(setting.id))
                .order_by_desc(reward_run::Column::CreatedAt)
                .one(&self.store.db)
                .await?;
            let cached = cache
                .as_ref()
                .filter(|row| row.payload_version == CACHE_VERSION);
            views.push(RewardView {
                setting,
                last_run,
                is_sign: cached
                    .and_then(|row| row.payload.as_ref())
                    .and_then(|payload| payload.get("is_sign"))
                    .and_then(Value::as_bool),
                last_checked_at: cached.and_then(|row| row.fetched_at),
                error_code: cached.and_then(|row| row.error_code.clone()),
                error_at: cached.and_then(|row| row.error_at),
            });
        }
        Ok(views)
    }

    pub(in crate::dashboard) async fn update(
        &self,
        actor: &Identity,
        id: Uuid,
        enabled: bool,
        time: String,
    ) -> Result<()> {
        let next = schedule::next_due(Utc::now(), &time, self.store.timezone)?;
        let tx = self.store.db.begin().await?;
        let fresh = self.store.actor_locked(&tx, actor).await?;
        let (link, _) = self.store.owner_link_in(&tx, fresh.user.id).await?;
        let setting = reward_setting::Entity::find_by_id(id)
            .filter(reward_setting::Column::HoyolabAccountId.eq(link.id))
            .lock_exclusive()
            .one(&tx)
            .await?
            .ok_or(Error::NotFound)?;
        if enabled && setting.auth_status != "ready" {
            return Err(Error::Unavailable(
                "reward requires HoYoLAB action before it can be enabled",
            ));
        }
        let mut row = setting.into_active_model();
        row.enabled = Set(enabled);
        row.local_time = Set(time);
        row.next_run_at = Set(enabled.then_some(next));
        row.update(&tx).await?;
        {
            // An ON/time update also waits for the newly selected future tick.
            reward_run::Entity::update_many()
                .col_expr(
                    reward_run::Column::State,
                    sea_orm::sea_query::Expr::value("stopped"),
                )
                .col_expr(
                    reward_run::Column::UpdatedAt,
                    sea_orm::sea_query::Expr::value(Utc::now()),
                )
                .col_expr(
                    reward_run::Column::EndedAt,
                    sea_orm::sea_query::Expr::value(Utc::now()),
                )
                .filter(reward_run::Column::SettingId.eq(id))
                .filter(reward_run::Column::Trigger.is_in(["scheduled", "catchup"]))
                .filter(reward_run::Column::State.is_in(["pending", "running", "retry_wait"]))
                .filter(reward_run::Column::SendState.eq("not_sent"))
                .exec(&tx)
                .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    pub(in crate::dashboard) async fn enqueue(
        &self,
        actor: &Identity,
        id: Uuid,
    ) -> Result<reward_run::Model> {
        let tx = self.store.db.begin().await?;
        let fresh = self.store.actor_locked(&tx, actor).await?;
        let (link, _) = self.store.owner_link_in(&tx, fresh.user.id).await?;
        let setting = reward_setting::Entity::find_by_id(id)
            .filter(reward_setting::Column::HoyolabAccountId.eq(link.id))
            .lock_exclusive()
            .one(&tx)
            .await?
            .ok_or(Error::NotFound)?;
        if setting.auth_status != "ready" {
            return Err(Error::Unavailable(
                "reward requires HoYoLAB action before it can be enqueued",
            ));
        }
        let run = enqueue_in(&tx, &setting, link.credential_version, "manual").await?;
        tx.commit().await?;
        Ok(run)
    }

    pub(in crate::dashboard) async fn history(
        &self,
        actor: &Identity,
        limit: u64,
    ) -> Result<Vec<RunView>> {
        let limit = limit.min(100);
        let tx = self.store.db.begin().await?;
        let fresh = self.store.actor_locked(&tx, actor).await?;
        let (link, _) = self.store.owner_link_in(&tx, fresh.user.id).await?;
        let settings = reward_setting::Entity::find()
            .filter(reward_setting::Column::HoyolabAccountId.eq(link.id))
            .all(&tx)
            .await?;
        let setting_ids: Vec<Uuid> = settings.iter().map(|s| s.id).collect();
        if setting_ids.is_empty() {
            tx.commit().await?;
            return Ok(Vec::new());
        }
        let runs = reward_run::Entity::find()
            .filter(reward_run::Column::SettingId.is_in(setting_ids))
            .filter(reward_run::Column::CreatedAt.gte(Utc::now() - Duration::days(HISTORY_DAYS)))
            .order_by_desc(reward_run::Column::CreatedAt)
            .limit(limit)
            .all(&tx)
            .await?;
        let mut views = Vec::with_capacity(runs.len());
        for run in runs {
            let game_kind = settings
                .iter()
                .find(|s| s.id == run.setting_id)
                .map(|s| s.game_kind.clone())
                .ok_or(Error::Internal)?;
            let attempts = reward_attempt::Entity::find()
                .filter(reward_attempt::Column::RunId.eq(run.id))
                .order_by_asc(reward_attempt::Column::Number)
                .all(&tx)
                .await?;
            views.push(RunView {
                run,
                game_kind,
                attempts,
            });
        }
        tx.commit().await?;
        Ok(views)
    }

    async fn recheck_actor_link(
        &self,
        actor: &Identity,
        link_id: Uuid,
        credential_version: i64,
        setting_id: Uuid,
    ) -> Result<()> {
        let tx = self.store.db.begin().await?;
        let fresh = self.store.actor_locked(&tx, actor).await?;
        let (fresh_link, _) = self.store.owner_link_in(&tx, fresh.user.id).await?;
        if fresh_link.id != link_id || fresh_link.credential_version != credential_version {
            return Err(Error::Forbidden);
        }
        reward_setting::Entity::find_by_id(setting_id)
            .filter(reward_setting::Column::HoyolabAccountId.eq(link_id))
            .one(&tx)
            .await?
            .ok_or(Error::NotFound)?;
        tx.commit().await?;
        Ok(())
    }
}

async fn enqueue_in(
    db: &impl ConnectionTrait,
    setting: &reward_setting::Model,
    credential_version: i64,
    trigger: &str,
) -> Result<reward_run::Model> {
    if let Some(existing) = reward_run::Entity::find()
        .filter(reward_run::Column::SettingId.eq(setting.id))
        .filter(reward_run::Column::State.is_in(["pending", "running", "retry_wait"]))
        .order_by_asc(reward_run::Column::CreatedAt)
        .one(db)
        .await?
    {
        return Ok(existing);
    }
    let now = Utc::now();
    Ok(reward_run::ActiveModel {
        id: Set(Uuid::new_v4()),
        setting_id: Set(setting.id),
        trigger: Set(trigger.to_owned()),
        state: Set("pending".to_owned()),
        created_at: Set(now),
        updated_at: Set(now),
        ended_at: Set(None),
        attempt_count: Set(0),
        credential_version: Set(credential_version),
        send_state: Set("not_sent".to_owned()),
        next_retry_at: Set(None),
    }
    .insert(db)
    .await?)
}
