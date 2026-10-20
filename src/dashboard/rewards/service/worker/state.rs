use super::super::{
    Rewards,
    cache::{CacheWrite, upsert_cache},
};
use crate::dashboard::{
    db::entities::{reward_attempt, reward_guard, reward_run, reward_setting},
    error::{Error, Result},
    hoyolab::{
        credentials::Credentials,
        gateway::{DailyState, Gateway},
    },
};
use chrono::{DateTime, Duration, Utc};
use sea_orm::{
    ActiveModelTrait, ConnectionTrait, EntityTrait, IntoActiveModel, QuerySelect, Set,
    TransactionTrait,
};
use uuid::Uuid;

impl<G: Gateway> Rewards<G> {
    pub(super) async fn mark_may_have_sent(&self, prepared: &PreparedAttempt) -> Result<bool> {
        let owner = self.run_owner(prepared.run.id).await?;
        let tx = self.store.db.begin().await?;
        let (link, _) = match self.store.owner_link_in(&tx, owner).await {
            Ok(value) => value,
            Err(
                Error::Unauthorized
                | Error::PasswordChangeRequired
                | Error::InitialSetupRequired
                | Error::Unavailable(_),
            ) => {
                tx.commit().await?;
                return Ok(false);
            }
            Err(error) => return Err(error),
        };
        let setting = reward_setting::Entity::find_by_id(prepared.setting.id)
            .lock_exclusive()
            .one(&tx)
            .await?
            .ok_or(Error::NotFound)?;
        let current = reward_run::Entity::find_by_id(prepared.run.id)
            .lock_exclusive()
            .one(&tx)
            .await?
            .ok_or(Error::NotFound)?;
        if link.credential_version != prepared.run.credential_version
            || setting.auth_status != "ready"
            || (prepared.run.trigger != "manual" && !setting.enabled)
            || current.state != "running"
            || current.send_state != "not_sent"
        {
            tx.commit().await?;
            return Ok(false);
        }
        upsert_guard(&tx, prepared.setting.id, true).await?;
        let mut run = reward_run::Entity::find_by_id(prepared.run.id)
            .one(&tx)
            .await?
            .ok_or(Error::NotFound)?
            .into_active_model();
        run.send_state = Set("may_have_sent".to_owned());
        run.updated_at = Set(Utc::now());
        run.update(&tx).await?;
        let mut attempt = reward_attempt::Entity::find_by_id(prepared.attempt.id)
            .one(&tx)
            .await?
            .ok_or(Error::NotFound)?
            .into_active_model();
        attempt.send_state = Set("may_have_sent".to_owned());
        attempt.update(&tx).await?;
        tx.commit().await?;
        Ok(true)
    }

    pub(super) async fn finish_attempt(
        &self,
        prepared: PreparedAttempt,
        mut finish: Finish,
    ) -> Result<()> {
        let tx = self.store.db.begin().await?;
        if finish.setting_action_required {
            let mut setting = reward_setting::Entity::find_by_id(prepared.setting.id)
                .one(&tx)
                .await?
                .ok_or(Error::NotFound)?
                .into_active_model();
            setting.auth_status = Set("action_required".to_owned());
            setting.enabled = Set(false);
            setting.next_run_at = Set(None);
            setting.update(&tx).await?;
        }
        if finish.clear_guard {
            upsert_guard(&tx, prepared.setting.id, false).await?;
        }
        if let Some(state) = finish.observed.take() {
            // Keep the dashboard status in step with the worker's confirming GET.
            upsert_cache(&tx, prepared.setting.id, CacheWrite::success(state)).await?;
        }
        let mut attempt = reward_attempt::Entity::find_by_id(prepared.attempt.id)
            .one(&tx)
            .await?
            .ok_or(Error::NotFound)?
            .into_active_model();
        attempt.finished_at = Set(Some(Utc::now()));
        attempt.outcome = Set(finish.state.clone());
        attempt.error_code = Set(finish.error_code.clone());
        attempt.send_state = Set(finish.send_state.clone());
        attempt.update(&tx).await?;
        finish_run_tx(
            &tx,
            prepared.run.id,
            &finish.state,
            &finish.send_state,
            finish.next_retry_at,
            !finish.is_retry(),
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }
}

pub(super) struct PreparedAttempt {
    pub(super) run: reward_run::Model,
    pub(super) setting: reward_setting::Model,
    pub(super) attempt: reward_attempt::Model,
    pub(super) cookie: Credentials,
    pub(super) confirmation_only: bool,
}

pub(super) struct Finish {
    state: String,
    pub(super) send_state: String,
    error_code: Option<String>,
    next_retry_at: Option<DateTime<Utc>>,
    pub(super) clear_guard: bool,
    setting_action_required: bool,
    observed: Option<DailyState>,
}

impl Finish {
    pub(super) fn done(state: &str, send_state: &str, clear_guard: bool) -> Self {
        Self {
            state: state.to_owned(),
            send_state: send_state.to_owned(),
            error_code: None,
            next_retry_at: None,
            clear_guard,
            setting_action_required: false,
            observed: None,
        }
    }
    pub(super) fn retry(code: String, delay: Duration) -> Self {
        Self {
            state: "retry_wait".to_owned(),
            send_state: "not_sent".to_owned(),
            error_code: Some(code),
            next_retry_at: Some(Utc::now() + delay),
            clear_guard: false,
            setting_action_required: false,
            observed: None,
        }
    }
    pub(super) fn failed(code: String) -> Self {
        Self {
            state: "failed".to_owned(),
            send_state: "not_sent".to_owned(),
            error_code: Some(code),
            next_retry_at: None,
            clear_guard: false,
            setting_action_required: false,
            observed: None,
        }
    }
    pub(super) fn stopped(code: Option<String>) -> Self {
        Self {
            state: "stopped".to_owned(),
            send_state: "not_sent".to_owned(),
            error_code: code,
            next_retry_at: None,
            clear_guard: false,
            setting_action_required: false,
            observed: None,
        }
    }
    pub(super) fn unknown() -> Self {
        Self::unknown_code("upstream_data".to_owned())
    }
    pub(super) fn unknown_code(code: String) -> Self {
        Self {
            state: "unknown".to_owned(),
            send_state: "may_have_sent".to_owned(),
            error_code: Some(code),
            next_retry_at: None,
            clear_guard: false,
            setting_action_required: false,
            observed: None,
        }
    }
    pub(super) fn action_required() -> Self {
        Self {
            state: "action_required".to_owned(),
            send_state: "may_have_sent".to_owned(),
            error_code: None,
            next_retry_at: None,
            clear_guard: false,
            setting_action_required: true,
            observed: None,
        }
    }
    /// Records the daily state a successful GET returned for the status cache.
    pub(super) fn observed(mut self, state: DailyState) -> Self {
        self.observed = Some(state);
        self
    }
    fn is_retry(&self) -> bool {
        self.state == "retry_wait"
    }
}

pub(super) async fn finish_run_tx(
    db: &impl ConnectionTrait,
    run_id: Uuid,
    state: &str,
    send_state: &str,
    next_retry_at: Option<DateTime<Utc>>,
    ended: bool,
) -> Result<()> {
    let mut run = reward_run::Entity::find_by_id(run_id)
        .one(db)
        .await?
        .ok_or(Error::NotFound)?
        .into_active_model();
    run.state = Set(state.to_owned());
    run.send_state = Set(send_state.to_owned());
    run.updated_at = Set(Utc::now());
    run.next_retry_at = Set(next_retry_at);
    if ended {
        run.ended_at = Set(Some(Utc::now()));
    }
    run.update(db).await?;
    Ok(())
}

pub(super) async fn upsert_guard(
    db: &impl ConnectionTrait,
    setting_id: Uuid,
    may_have: bool,
) -> Result<()> {
    if let Some(existing) = reward_guard::Entity::find_by_id(setting_id).one(db).await? {
        let mut row = existing.into_active_model();
        row.post_may_have_happened = Set(may_have);
        row.updated_at = Set(Utc::now());
        row.update(db).await?;
    } else {
        reward_guard::ActiveModel {
            setting_id: Set(setting_id),
            post_may_have_happened: Set(may_have),
            updated_at: Set(Utc::now()),
        }
        .insert(db)
        .await?;
    }
    Ok(())
}
