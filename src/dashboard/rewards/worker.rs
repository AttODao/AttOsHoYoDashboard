use super::*;
use crate::dashboard::{
    db::entities::{hoyolab_account, reward_attempt, reward_guard, reward_run, reward_setting},
    hoyolab::{credentials::Credentials, gateway::SignState},
};
use tokio::task::JoinSet;

#[cfg(test)]
#[path = "worker_tests.rs"]
mod tests;

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

    async fn process_run(&self, run_id: Uuid) -> Result<()> {
        let owner = self.run_owner(run_id).await?;
        let _operation = self.store.operations.acquire(owner).await;
        let prepared = self.prepare_attempt(run_id).await?;
        let Some(prepared) = prepared else {
            return Ok(());
        };
        let game = gateway::game(&prepared.setting.game_kind)?;
        if prepared.confirmation_only {
            return self.confirm_only(prepared, game).await;
        }
        match self.store.gateway.daily(&prepared.cookie, game).await {
            Ok(state) if state.is_sign => {
                self.finish_attempt(
                    prepared,
                    Finish::done("already_claimed", "confirmed", false),
                )
                .await
            }
            Ok(_) => self.post_and_confirm(prepared, game).await,
            Err(error) => self.finish_get_error(prepared, error).await,
        }
    }

    async fn run_owner(&self, run_id: Uuid) -> Result<Uuid> {
        let run = reward_run::Entity::find_by_id(run_id)
            .one(&self.store.db)
            .await?
            .ok_or(Error::NotFound)?;
        let setting = reward_setting::Entity::find_by_id(run.setting_id)
            .one(&self.store.db)
            .await?
            .ok_or(Error::NotFound)?;
        let link = hoyolab_account::Entity::find_by_id(setting.hoyolab_account_id)
            .one(&self.store.db)
            .await?
            .ok_or(Error::NotFound)?;
        Ok(link.owner_user_id)
    }

    async fn prepare_attempt(&self, run_id: Uuid) -> Result<Option<PreparedAttempt>> {
        let owner = self.run_owner(run_id).await?;
        let tx = self.store.db.begin().await?;
        crate::dashboard::auth::accounts::lock_user(&tx, owner).await?;
        let run = reward_run::Entity::find_by_id(run_id)
            .lock_exclusive()
            .one(&tx)
            .await?
            .ok_or(Error::NotFound)?;
        if !matches!(run.state.as_str(), "pending" | "running" | "retry_wait")
            || run.next_retry_at.is_some_and(|retry| retry > Utc::now())
        {
            tx.commit().await?;
            return Ok(None);
        }
        let setting = reward_setting::Entity::find_by_id(run.setting_id)
            .lock_exclusive()
            .one(&tx)
            .await?
            .ok_or(Error::NotFound)?;
        let link = hoyolab_account::Entity::find_by_id(setting.hoyolab_account_id)
            .one(&tx)
            .await?
            .ok_or(Error::NotFound)?;
        let guard = reward_guard::Entity::find_by_id(setting.id)
            .one(&tx)
            .await?;
        let confirmation_only = run.send_state == "may_have_sent"
            || guard.as_ref().is_some_and(|g| g.post_may_have_happened);
        if !confirmation_only
            && (run.credential_version != link.credential_version
                || (run.trigger != "manual" && !setting.enabled)
                || setting.auth_status != "ready"
                || link.auth_status != "verified"
                || run.attempt_count >= MAX_ATTEMPTS)
        {
            finish_run_tx(&tx, run.id, None, "stopped", "not_sent", None, true).await?;
            tx.commit().await?;
            return Ok(None);
        }
        let cookie = match self.store.owner_link_in(&tx, link.owner_user_id).await {
            Ok((_, cookie)) => cookie,
            Err(
                Error::Unauthorized | Error::PasswordChangeRequired | Error::InitialSetupRequired,
            ) => {
                finish_run_tx(&tx, run.id, None, "stopped", "not_sent", None, true).await?;
                tx.commit().await?;
                return Ok(None);
            }
            Err(error) => return Err(error),
        };
        let number = (run.attempt_count + 1).min(MAX_ATTEMPTS);
        let attempt = reward_attempt::ActiveModel {
            id: Set(Uuid::new_v4()),
            run_id: Set(run.id),
            number: Set(number),
            started_at: Set(Utc::now()),
            finished_at: Set(None),
            outcome: Set("pending".to_owned()),
            error_code: Set(None),
            send_state: Set(run.send_state.clone()),
        }
        .insert(&tx)
        .await?;
        let mut run_row = run.into_active_model();
        run_row.state = Set("running".to_owned());
        run_row.attempt_count = Set(number);
        run_row.updated_at = Set(Utc::now());
        run_row.next_retry_at = Set(None);
        let run = run_row.update(&tx).await?;
        tx.commit().await?;
        Ok(Some(PreparedAttempt {
            run,
            setting,
            attempt,
            cookie,
            confirmation_only,
        }))
    }

    async fn post_and_confirm(&self, prepared: PreparedAttempt, game: GameKind) -> Result<()> {
        if !self.mark_may_have_sent(&prepared).await? {
            return self.finish_attempt(prepared, Finish::stopped(None)).await;
        }
        self.send_after_admission(prepared, game).await
    }

    async fn send_after_admission(&self, prepared: PreparedAttempt, game: GameKind) -> Result<()> {
        if self.store.is_closing() {
            // This call has not invoked POST. Clear only its newly reserved, known-unsent guard.
            let mut finish = Finish::stopped(None);
            finish.clear_guard = true;
            return self.finish_attempt(prepared, finish).await;
        }
        // Final admission checkpoint: with no intervening await, this is now current work to drain.
        match self.store.gateway.sign(&prepared.cookie, game).await {
            Ok(SignState {
                requires_verification: Some(true),
            }) => {
                self.finish_attempt(prepared, Finish::action_required())
                    .await
            }
            Ok(_) => match self.store.gateway.daily(&prepared.cookie, game).await {
                Ok(state) if state.is_sign => {
                    self.finish_attempt(prepared, Finish::done("success", "confirmed", true))
                        .await
                }
                Ok(_) => self.finish_attempt(prepared, Finish::unknown()).await,
                Err(error) => self.finish_post_error(prepared, error).await,
            },
            Err(error) => match self.store.gateway.daily(&prepared.cookie, game).await {
                Ok(state) if state.is_sign => {
                    self.finish_attempt(prepared, Finish::done("success", "may_have_sent", false))
                        .await
                }
                _ => self.finish_post_error(prepared, error).await,
            },
        }
    }

    async fn confirm_only(&self, prepared: PreparedAttempt, game: GameKind) -> Result<()> {
        match self.store.gateway.daily(&prepared.cookie, game).await {
            Ok(state) if state.is_sign => {
                self.finish_attempt(prepared, Finish::done("success", "confirmed", false))
                    .await
            }
            Ok(_) => self.finish_attempt(prepared, Finish::unknown()).await,
            Err(error) => self.finish_post_error(prepared, error).await,
        }
    }

    async fn mark_may_have_sent(&self, prepared: &PreparedAttempt) -> Result<bool> {
        self.store.require_reward_contract()?;
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

    async fn finish_get_error(
        &self,
        prepared: PreparedAttempt,
        error: attosgameapi::hoyolab::Error,
    ) -> Result<()> {
        let code = gateway::error_code(&error);
        if gateway::cookie_invalid(&error) {
            let owner = self.run_owner(prepared.run.id).await?;
            self.finish_attempt(prepared, Finish::stopped(Some(code)))
                .await?;
            self.store.mark_cookie_invalid(owner).await?;
            return Ok(());
        }
        if gateway::transient(&error) && prepared.run.attempt_count < MAX_ATTEMPTS {
            self.finish_attempt(prepared, Finish::retry(code)).await
        } else {
            self.finish_attempt(prepared, Finish::failed(code)).await
        }
    }

    async fn finish_post_error(
        &self,
        prepared: PreparedAttempt,
        error: attosgameapi::hoyolab::Error,
    ) -> Result<()> {
        let code = gateway::error_code(&error);
        if gateway::cookie_invalid(&error) {
            let owner = self.run_owner(prepared.run.id).await?;
            let mut finish = Finish::stopped(Some(code));
            finish.send_state = "may_have_sent".to_owned();
            self.finish_attempt(prepared, finish).await?;
            self.store.mark_cookie_invalid(owner).await?;
            Ok(())
        } else {
            self.finish_attempt(prepared, Finish::unknown_code(code))
                .await
        }
    }

    async fn finish_attempt(&self, prepared: PreparedAttempt, finish: Finish) -> Result<()> {
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
            Some(finish.state.as_str()),
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

struct PreparedAttempt {
    run: reward_run::Model,
    setting: reward_setting::Model,
    attempt: reward_attempt::Model,
    cookie: Credentials,
    confirmation_only: bool,
}

struct Finish {
    state: String,
    send_state: String,
    error_code: Option<String>,
    next_retry_at: Option<DateTime<Utc>>,
    clear_guard: bool,
    setting_action_required: bool,
}

impl Finish {
    fn done(state: &str, send_state: &str, clear_guard: bool) -> Self {
        Self {
            state: state.to_owned(),
            send_state: send_state.to_owned(),
            error_code: None,
            next_retry_at: None,
            clear_guard,
            setting_action_required: false,
        }
    }
    fn retry(code: String) -> Self {
        Self {
            state: "retry_wait".to_owned(),
            send_state: "not_sent".to_owned(),
            error_code: Some(code),
            next_retry_at: Some(Utc::now() + Duration::seconds(10)),
            clear_guard: false,
            setting_action_required: false,
        }
    }
    fn failed(code: String) -> Self {
        Self {
            state: "failed".to_owned(),
            send_state: "not_sent".to_owned(),
            error_code: Some(code),
            next_retry_at: None,
            clear_guard: false,
            setting_action_required: false,
        }
    }
    fn stopped(code: Option<String>) -> Self {
        Self {
            state: "stopped".to_owned(),
            send_state: "not_sent".to_owned(),
            error_code: code,
            next_retry_at: None,
            clear_guard: false,
            setting_action_required: false,
        }
    }
    fn unknown() -> Self {
        Self::unknown_code("upstream_data".to_owned())
    }
    fn unknown_code(code: String) -> Self {
        Self {
            state: "unknown".to_owned(),
            send_state: "may_have_sent".to_owned(),
            error_code: Some(code),
            next_retry_at: None,
            clear_guard: false,
            setting_action_required: false,
        }
    }
    fn action_required() -> Self {
        Self {
            state: "action_required".to_owned(),
            send_state: "may_have_sent".to_owned(),
            error_code: None,
            next_retry_at: None,
            clear_guard: false,
            setting_action_required: true,
        }
    }
    fn is_retry(&self) -> bool {
        self.state == "retry_wait"
    }
}

async fn finish_run_tx(
    db: &impl ConnectionTrait,
    run_id: Uuid,
    _attempt_outcome: Option<&str>,
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

async fn upsert_guard(db: &impl ConnectionTrait, setting_id: Uuid, may_have: bool) -> Result<()> {
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
