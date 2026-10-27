//! Scans for new events and periodically evaluates game status, then delivers.
use super::{
    Notice,
    channel::{ADMIN_SCOPE, Channel, Notifications},
    conditions::{self, Game, Outcome, Rules},
    messages::word,
};
use crate::dashboard::{
    config::Tuning,
    db::entities::{
        audit, deletion_request, hoyolab_account, notification_cursor, notification_log,
        notification_setting, reward_run, reward_setting, user,
    },
    error::Result,
    games::service::Games,
    rewards::schedule::ScheduleZone,
};
use chrono::{DateTime, Duration, Utc};
use sea_orm::{
    ColumnTrait, Condition, EntityTrait, QueryFilter, QueryOrder, Set, sea_query::OnConflict,
};
use std::sync::Arc;
use uuid::Uuid;

/// How often the notifier ticks and the resource-notification game-status refresh TTL.
#[derive(Clone, Copy)]
pub(in crate::dashboard) struct NotifierSchedule {
    pub interval: std::time::Duration,
    pub status_interval: Duration,
}

impl Default for NotifierSchedule {
    fn default() -> Self {
        Self {
            interval: std::time::Duration::from_secs(60),
            status_interval: Duration::minutes(15),
        }
    }
}

/// Owns one scan + per-user status pass; shared between the background job and tests.
pub(in crate::dashboard) struct Notifier<G: crate::dashboard::hoyolab::gateway::Gateway> {
    notifications: Arc<Notifications>,
    games: Arc<Games<G>>,
    schedule: NotifierSchedule,
    zone: ScheduleZone,
    tuning: Tuning,
}

impl<G: crate::dashboard::hoyolab::gateway::Gateway> Notifier<G> {
    pub(in crate::dashboard) fn new(
        notifications: Arc<Notifications>,
        games: Arc<Games<G>>,
        schedule: NotifierSchedule,
        zone: ScheduleZone,
        tuning: Tuning,
    ) -> Self {
        Self {
            notifications,
            games,
            schedule,
            zone,
            tuning,
        }
    }

    pub(in crate::dashboard) async fn tick(
        &self,
        now: DateTime<Utc>,
        worker_stall: Option<i64>,
    ) -> Result<()> {
        let previous = self.cursor().await?;
        let admin = self.notifications.admin_channel().await?;
        self.scan_audit(previous, now, &admin).await?;
        self.scan_reward_runs(previous, now, &admin).await?;
        self.scan_cookie_states(now).await?;
        self.scan_stuck_deletions(now, &admin).await?;
        if let Some(stall_minutes) = worker_stall {
            self.worker_stalled(stall_minutes, &admin).await?;
        }
        self.scan_games(now).await?;
        self.set_cursor(now).await?;
        Ok(())
    }

    async fn cursor(&self) -> Result<DateTime<Utc>> {
        let row = notification_cursor::Entity::find_by_id(1_i16)
            .one(&self.notifications.db)
            .await?;
        Ok(row
            .map(|row| row.scanned_until)
            .unwrap_or_else(|| Utc::now() - Duration::minutes(5)))
    }

    async fn set_cursor(&self, now: DateTime<Utc>) -> Result<()> {
        notification_cursor::Entity::insert(notification_cursor::ActiveModel {
            id: Set(1_i16),
            scanned_until: Set(now),
        })
        .on_conflict(
            OnConflict::column(notification_cursor::Column::Id)
                .update_column(notification_cursor::Column::ScannedUntil)
                .to_owned(),
        )
        .exec(&self.notifications.db)
        .await?;
        Ok(())
    }

    async fn scan_audit(
        &self,
        since: DateTime<Utc>,
        until: DateTime<Utc>,
        admin: &Channel,
    ) -> Result<()> {
        let rows = audit::Entity::find()
            .filter(audit::Column::CreatedAt.gt(since))
            .filter(audit::Column::CreatedAt.lte(until))
            .filter(audit::Column::Action.is_in(["signed_in", "sign_in_failed"]))
            .order_by_asc(audit::Column::CreatedAt)
            .all(&self.notifications.db)
            .await?;
        for row in rows {
            let username = user::Entity::find_by_id(row.target_user_id)
                .one(&self.notifications.db)
                .await?
                .map(|user| user.username);
            let method = word(
                if row.actor_user_id.is_some() {
                    "password"
                } else {
                    "passkey"
                },
                "ja",
            );
            let base = Notice::new(
                if row.action == "signed_in" {
                    "signed_in"
                } else {
                    "sign_in_failed"
                },
                format!("audit:{}", row.id),
            )
            .with("method", method);
            // Personal copy for the subject.
            let (user_channel, _) = self
                .notifications
                .personal_channel(row.target_user_id)
                .await?;
            self.deliver(Some(row.target_user_id), &user_channel, &base)
                .await?;
            // Admin copy, with the subject name, under its own key so the admin dedup is independent.
            let admin_notice = Notice::new(
                if row.action == "signed_in" {
                    "admin_signed_in"
                } else {
                    "admin_sign_in_failed"
                },
                format!("audit:{}:admin", row.id),
            )
            .with("username", username.unwrap_or_default())
            .with("method", method);
            self.deliver_admin(admin, &admin_notice).await?;
        }
        Ok(())
    }

    async fn scan_reward_runs(
        &self,
        since: DateTime<Utc>,
        until: DateTime<Utc>,
        admin: &Channel,
    ) -> Result<()> {
        let rows = reward_run::Entity::find()
            .filter(reward_run::Column::EndedAt.is_not_null())
            .filter(reward_run::Column::EndedAt.gt(since))
            .filter(reward_run::Column::EndedAt.lte(until))
            .order_by_asc(reward_run::Column::EndedAt)
            .all(&self.notifications.db)
            .await?;
        for row in rows {
            let setting = reward_setting::Entity::find_by_id(row.setting_id)
                .one(&self.notifications.db)
                .await?;
            let Some(setting) = setting else { continue };
            let link = hoyolab_account::Entity::find_by_id(setting.hoyolab_account_id)
                .one(&self.notifications.db)
                .await?;
            let Some(link) = link else { continue };
            let (channel, _) = self
                .notifications
                .personal_channel(link.owner_user_id)
                .await?;
            let game_name = word(&setting.game_kind, &channel.language);
            match row.state.as_str() {
                "success" | "already_claimed" => {
                    let notice = Notice::new("reward_claimed", format!("run:{}", row.id))
                        .with("game", game_name);
                    self.deliver(Some(link.owner_user_id), &channel, &notice)
                        .await?;
                }
                "failed" | "stopped" | "unknown" | "action_required" => {
                    let state = word(
                        if row.state == "action_required" {
                            "action_required"
                        } else if row.state == "unknown" {
                            "unknown"
                        } else {
                            "failed"
                        },
                        &channel.language,
                    );
                    let notice = Notice::new("reward_failed", format!("run:{}", row.id))
                        .with("game", game_name)
                        .with("state", state);
                    self.deliver(Some(link.owner_user_id), &channel, &notice)
                        .await?;
                    let admin_notice =
                        Notice::new("user_attention", format!("run:{}:admin", row.id))
                            .with(
                                "username",
                                user::Entity::find_by_id(link.owner_user_id)
                                    .one(&self.notifications.db)
                                    .await?
                                    .map(|user| user.username)
                                    .unwrap_or_default(),
                            )
                            .with("reason", word("reward_attention", &admin.language));
                    self.deliver_admin(admin, &admin_notice).await?;
                }
                _ => {}
            }
        }
        Ok(())
    }

    async fn scan_cookie_states(&self, now: DateTime<Utc>) -> Result<()> {
        let rows = hoyolab_account::Entity::find()
            .filter(hoyolab_account::Column::AuthStatus.eq("cookie_invalid"))
            .filter(hoyolab_account::Column::UpdatedAt.gt(now - Duration::days(1)))
            .all(&self.notifications.db)
            .await?;
        for row in rows {
            let notice = Notice::new(
                "cookie_invalid",
                format!("cookie:{}:{}", row.id, row.credential_version),
            );
            let (channel, _) = self
                .notifications
                .personal_channel(row.owner_user_id)
                .await?;
            self.deliver(Some(row.owner_user_id), &channel, &notice)
                .await?;
        }
        Ok(())
    }

    async fn scan_stuck_deletions(&self, now: DateTime<Utc>, admin: &Channel) -> Result<()> {
        let rows = deletion_request::Entity::find()
            .filter(deletion_request::Column::CreatedAt.lt(now - Duration::hours(1)))
            .all(&self.notifications.db)
            .await?;
        for row in rows {
            let name = user::Entity::find_by_id(row.user_id)
                .one(&self.notifications.db)
                .await?
                .map(|user| user.username)
                .unwrap_or_default();
            let notice = Notice::new("deletion_stuck", format!("deletion:{}", row.user_id))
                .with("username", name);
            self.deliver_admin(admin, &notice).await?;
        }
        Ok(())
    }

    async fn worker_stalled(&self, minutes: i64, admin: &Channel) -> Result<()> {
        let notice = Notice::new("worker_stalled", format!("worker:{}", (minutes / 15) * 15))
            .with("minutes", minutes.to_string());
        self.deliver_admin(admin, &notice).await?;
        Ok(())
    }

    /// Refreshes stale game status for every ready user, then evaluates conditions.
    async fn scan_games(&self, now: DateTime<Utc>) -> Result<()> {
        let settings = notification_setting::Entity::find()
            .all(&self.notifications.db)
            .await?;
        for setting in settings {
            let channel = self
                .notifications
                .personal_from(setting.user_id, Some(&setting));
            // Nothing to notify about: skip the HoYoLAB GET entirely.
            if channel.url.is_none() && !self.notifications.has_app_device(setting.user_id).await? {
                continue;
            }
            let link = hoyolab_account::Entity::find()
                .filter(hoyolab_account::Column::OwnerUserId.eq(setting.user_id))
                .filter(hoyolab_account::Column::AuthStatus.eq("verified"))
                .one(&self.notifications.db)
                .await?;
            let Some(link) = link else { continue };
            let games = crate::dashboard::db::entities::game_account::Entity::find()
                .filter(
                    crate::dashboard::db::entities::game_account::Column::HoyolabAccountId
                        .eq(link.id),
                )
                .filter(crate::dashboard::db::entities::game_account::Column::Visible.eq(true))
                .filter(crate::dashboard::db::entities::game_account::Column::Detected.eq(true))
                .all(&self.notifications.db)
                .await?;
            let rules = Rules {
                warn_hours: setting.resin_warn_hours.into(),
                daily: super::channel::parse_clock(&setting.daily_reminder_time)?,
                weekly: super::channel::parse_clock(&setting.weekly_reminder_time)?,
                zone: self.zone,
            };
            for account in games {
                let cache = crate::dashboard::db::entities::game_status_cache::Entity::find_by_id(
                    account.id,
                )
                .one(&self.notifications.db)
                .await?;
                let needs_refresh = cache
                    .as_ref()
                    .and_then(|row| row.fetched_at)
                    .is_none_or(|at| now - at >= self.schedule.status_interval);
                let payload = if needs_refresh {
                    match self
                        .games
                        .status_for_owner(setting.user_id, account.id)
                        .await
                    {
                        Ok(status) => status.payload,
                        Err(_) => cache.as_ref().and_then(|row| row.payload.clone()),
                    }
                } else {
                    cache.and_then(|row| row.payload)
                };
                let Some(payload) = payload else { continue };
                let game = Game {
                    account: account.id,
                    kind: &account.game_kind,
                    uid: &account.game_uid,
                    summary: &payload,
                };
                for outcome in conditions::evaluate(&game, rules, now, &channel.language) {
                    match outcome {
                        Outcome::Fire(notice) => {
                            self.deliver(Some(setting.user_id), &channel, &notice)
                                .await?
                        }
                        Outcome::Clear(key) => self.clear(setting.user_id, &key).await?,
                    }
                }
            }
        }
        let _ = self.tuning;
        Ok(())
    }

    async fn clear(&self, user: Uuid, key: &str) -> Result<()> {
        notification_log::Entity::delete_many()
            .filter(notification_log::Column::Scope.eq(user))
            .filter(notification_log::Column::EventKey.eq(key))
            .exec(&self.notifications.db)
            .await?;
        Ok(())
    }

    /// Sends a notice once per key to every route that chose its kind: the webhook by
    /// `events`, a user's app inbox by `app_events`.
    async fn deliver(&self, user: Option<Uuid>, channel: &Channel, notice: &Notice) -> Result<()> {
        let webhook = channel.wants(notice.kind);
        let app = user.filter(|_| channel.wants_app(notice.kind));
        if !webhook && app.is_none() {
            return Ok(());
        }
        let scope = user.unwrap_or(ADMIN_SCOPE);
        if self.already_sent(scope, &notice.key).await? {
            return Ok(());
        }
        if webhook {
            self.notifications
                .send_webhook(user, channel, notice)
                .await?;
        }
        if let Some(user) = app {
            self.notifications
                .send_to_app(user, notice, &channel.language)
                .await?;
        }
        self.mark_sent(scope, &notice.key).await?;
        Ok(())
    }

    async fn deliver_admin(&self, channel: &Channel, notice: &Notice) -> Result<()> {
        self.deliver(None, channel, notice).await
    }

    async fn already_sent(&self, scope: Uuid, key: &str) -> Result<bool> {
        Ok(notification_log::Entity::find()
            .filter(notification_log::Column::Scope.eq(scope))
            .filter(notification_log::Column::EventKey.eq(key))
            .one(&self.notifications.db)
            .await?
            .is_some())
    }

    async fn mark_sent(&self, scope: Uuid, key: &str) -> Result<()> {
        notification_log::Entity::insert(notification_log::ActiveModel {
            scope: Set(scope),
            event_key: Set(key.to_owned()),
            sent_at: Set(Utc::now()),
        })
        .on_conflict(
            OnConflict::columns([
                notification_log::Column::Scope,
                notification_log::Column::EventKey,
            ])
            .do_nothing()
            .to_owned(),
        )
        .exec(&self.notifications.db)
        .await?;
        Ok(())
    }

    /// Deletes log entries older than seven days whose key is time-bounded (dates, weeks, run ids).
    pub(in crate::dashboard) async fn cleanup(&self) -> Result<()> {
        self.notifications.cleanup_app(Utc::now()).await?;
        notification_log::Entity::delete_many()
            .filter(notification_log::Column::SentAt.lt(Utc::now() - Duration::days(7)))
            .filter(
                Condition::any()
                    .add(notification_log::Column::EventKey.contains(":daily:"))
                    .add(notification_log::Column::EventKey.contains(":weekly:"))
                    .add(notification_log::Column::EventKey.starts_with("audit:"))
                    .add(notification_log::Column::EventKey.starts_with("run:"))
                    .add(notification_log::Column::EventKey.starts_with("deletion:"))
                    .add(notification_log::Column::EventKey.starts_with("worker:")),
            )
            .exec(&self.notifications.db)
            .await?;
        Ok(())
    }
}
