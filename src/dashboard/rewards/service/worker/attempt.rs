use super::{
    super::{MAX_ATTEMPTS, Rewards},
    state::{Finish, PreparedAttempt, finish_run_tx},
};
use crate::dashboard::{
    db::entities::{hoyolab_account, reward_attempt, reward_guard, reward_run, reward_setting},
    error::{Error, Result},
    hoyolab::gateway::{self, Gateway, SignState},
};
use attoshoyodashboard::game::GameKind;
use chrono::Utc;
use sea_orm::{ActiveModelTrait, EntityTrait, IntoActiveModel, QuerySelect, Set, TransactionTrait};
use uuid::Uuid;

impl<G: Gateway> Rewards<G> {
    pub(super) async fn process_run(&self, run_id: Uuid) -> Result<()> {
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

    pub(super) async fn run_owner(&self, run_id: Uuid) -> Result<Uuid> {
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

    pub(super) async fn prepare_attempt(&self, run_id: Uuid) -> Result<Option<PreparedAttempt>> {
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

    pub(super) async fn send_after_admission(
        &self,
        prepared: PreparedAttempt,
        game: GameKind,
    ) -> Result<()> {
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

    async fn finish_get_error(
        &self,
        prepared: PreparedAttempt,
        error: attoshoyodashboard::hoyolab::Error,
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
        error: attoshoyodashboard::hoyolab::Error,
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
}
