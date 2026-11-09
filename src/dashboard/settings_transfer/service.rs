//! Export and import of one user's own settings. Every read and write re-checks the
//! session (never an API token) and the user's readiness; an import is planned and applied
//! in one transaction, so its report always describes what was written.
use super::document::{self, Document, FieldError, GameEntry, NotificationEntry, RewardEntry};
use crate::dashboard::{
    auth::{
        accounts::{lock_user, require_ready},
        sessions::{Identity, Sessions},
    },
    db::entities::{game_account, hoyolab_account, notification_setting, reward_setting},
    error::{Error, Result},
    hoyolab::operations::Operations,
    notifications::channel::PersonalPreferences,
    rewards::{
        schedule::{self, ScheduleZone},
        service::reschedule_in,
    },
};
use chrono::Utc;
use sea_orm::{
    ColumnTrait, DatabaseConnection, DatabaseTransaction, EntityTrait, QueryFilter, QueryOrder,
    QuerySelect, TransactionTrait, sea_query::Expr,
};
use serde::Serialize;
use serde_json::{Value, json};
use uuid::Uuid;

pub(in crate::dashboard) struct SettingsTransfer {
    db: DatabaseConnection,
    timezone: ScheduleZone,
    operations: Operations,
    /// The server is shutting down: like other reward changes, imports are refused.
    closing: bool,
}

/// One setting an import changes.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub(in crate::dashboard) struct Change {
    /// `rewards`, `game_accounts` or `notifications`.
    section: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    game_kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    game_uid: Option<String>,
    /// e.g. `enabled`, `local_time`, `visible`, `events.reward_claimed`, `language`.
    setting: String,
    from: Value,
    to: Value,
}

/// A file entry an import leaves alone, and why.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub(in crate::dashboard) struct Skipped {
    section: &'static str,
    game_kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    game_uid: Option<String>,
    /// `not_linked` (no verified HoYoLAB account), `game_not_found` (no reward for that
    /// game), `uid_not_found` (no such UID on this account; it is never created) or
    /// `reward_not_ready` (the reward needs HoYoLAB action before it can be turned on).
    reason: &'static str,
}

/// What an import changes (preview) or changed (apply).
#[derive(Clone, Debug, PartialEq, Serialize)]
pub(in crate::dashboard) struct Report {
    changes: Vec<Change>,
    skipped: Vec<Skipped>,
    webhook: WebhookNote,
}

/// The Webhook URL never travels in the file; this tells whether it must be set again.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub(in crate::dashboard) struct WebhookNote {
    /// The exporting account had a Webhook.
    in_file: bool,
    /// This account has one now.
    configured: bool,
}

/// An import that could not be read: every problem, nothing applied.
#[derive(Debug, PartialEq)]
pub(in crate::dashboard) enum Outcome {
    Invalid(Vec<FieldError>),
    Planned(Report),
}

/// A planned write, kept beside its report so apply writes exactly what preview showed.
enum Write {
    Reward {
        setting: reward_setting::Model,
        entry: RewardEntry,
    },
    /// Every row of one game UID on the link (the UID may exist in more than one region).
    Visibility {
        link: Uuid,
        game_kind: String,
        game_uid: String,
        visible: bool,
    },
    Notifications(PersonalPreferences),
}

impl SettingsTransfer {
    pub(in crate::dashboard) fn new(
        db: DatabaseConnection,
        timezone: ScheduleZone,
        operations: Operations,
        closing: bool,
    ) -> Self {
        Self {
            db,
            timezone,
            operations,
            closing,
        }
    }

    /// Locks the user and re-checks the session inside `tx`. Only browser sessions pass:
    /// `Sessions::authenticate` never accepts an API token hash.
    async fn session_locked(&self, tx: &DatabaseTransaction, actor: &Identity) -> Result<Uuid> {
        lock_user(tx, actor.user.id).await?;
        let fresh = Sessions::authenticate(tx, &actor.token_hash).await?;
        if fresh.user.id != actor.user.id {
            return Err(Error::Unauthorized);
        }
        require_ready(&fresh.user)?;
        Ok(fresh.user.id)
    }

    /// The user's settings as an `ahd-settings` file.
    pub(in crate::dashboard) async fn export(&self, actor: &Identity) -> Result<Value> {
        let tx = self.db.begin().await?;
        let user = self.session_locked(&tx, actor).await?;
        let link = hoyolab_account::Entity::find()
            .filter(hoyolab_account::Column::OwnerUserId.eq(user))
            .one(&tx)
            .await?;
        let (rewards, game_accounts) = match &link {
            Some(link) => (
                reward_rows(&tx, link.id, false).await?,
                game_rows(&tx, link.id, false).await?,
            ),
            None => (Vec::new(), Vec::new()),
        };
        let stored = notification_setting::Entity::find_by_id(user)
            .one(&tx)
            .await?;
        tx.commit().await?;
        let preferences = PersonalPreferences::from_row(stored.as_ref());
        let document = Document {
            rewards: rewards
                .into_iter()
                .map(|row| RewardEntry {
                    game_kind: row.game_kind,
                    enabled: row.enabled,
                    local_time: row.local_time,
                })
                .collect(),
            game_accounts: game_accounts
                .into_iter()
                .map(|row| GameEntry {
                    game_kind: row.game_kind,
                    game_uid: row.game_uid,
                    visible: row.visible,
                })
                .collect(),
            notifications: NotificationEntry {
                webhook_configured: stored.as_ref().is_some_and(|row| row.webhook_url.is_some()),
                webhook_format: preferences.webhook_format,
                language: preferences.language,
                events: preferences.events,
                app_events: preferences.app_events,
                resin_warn_hours: preferences.resin_warn_hours,
                daily_reminder_time: preferences.daily_reminder_time,
                weekly_reminder_time: preferences.weekly_reminder_time,
            },
        };
        Ok(document::render(&document, Utc::now()))
    }

    /// What importing `file` would change; nothing is written.
    pub(in crate::dashboard) async fn preview(
        &self,
        actor: &Identity,
        file: &Value,
    ) -> Result<Outcome> {
        let tx = self.db.begin().await?;
        let user = self.session_locked(&tx, actor).await?;
        let document = match document::parse(file) {
            Ok(document) => document,
            Err(errors) => return Ok(Outcome::Invalid(errors)),
        };
        let (report, _) = self.plan(&tx, user, &document).await?;
        tx.rollback().await?;
        Ok(Outcome::Planned(report))
    }

    /// Applies `file` in one transaction and reports what changed.
    pub(in crate::dashboard) async fn import(
        &self,
        actor: &Identity,
        file: &Value,
    ) -> Result<Outcome> {
        let document = match document::parse(file) {
            Ok(document) => document,
            Err(errors) => {
                // Still reject a stale session before saying anything about the file.
                let tx = self.db.begin().await?;
                self.session_locked(&tx, actor).await?;
                tx.rollback().await?;
                return Ok(Outcome::Invalid(errors));
            }
        };
        if self.closing {
            return Err(Error::Unavailable("server is shutting down"));
        }
        // Serialize with the reward worker like a schedule change from the settings page.
        let _operation = self.operations.acquire(actor.user.id).await;
        let tx = self.db.begin().await?;
        let user = self.session_locked(&tx, actor).await?;
        let (report, writes) = self.plan(&tx, user, &document).await?;
        let now = Utc::now();
        for write in writes {
            match write {
                Write::Reward { setting, entry } => {
                    let next = schedule::next_due(now, &entry.local_time, self.timezone)?;
                    reschedule_in(&tx, setting, entry.enabled, entry.local_time, next).await?;
                }
                Write::Visibility {
                    link,
                    game_kind,
                    game_uid,
                    visible,
                } => {
                    let updated = game_account::Entity::update_many()
                        .col_expr(game_account::Column::Visible, Expr::value(visible))
                        .filter(game_account::Column::HoyolabAccountId.eq(link))
                        .filter(game_account::Column::GameKind.eq(game_kind))
                        .filter(game_account::Column::GameUid.eq(game_uid))
                        .exec(&tx)
                        .await?;
                    // The rows were read in this transaction under the owner's operation lock;
                    // a vanished row means the plan no longer holds, so nothing is applied.
                    if updated.rows_affected == 0 {
                        return Err(Error::Conflict);
                    }
                }
                Write::Notifications(preferences) => preferences.store(&tx, user).await?,
            }
        }
        tx.commit().await?;
        Ok(Outcome::Planned(report))
    }

    /// Compares the file with the stored settings under row locks held by `tx`.
    async fn plan(
        &self,
        tx: &DatabaseTransaction,
        user: Uuid,
        document: &Document,
    ) -> Result<(Report, Vec<Write>)> {
        let mut changes = Vec::new();
        let mut skipped = Vec::new();
        let mut writes = Vec::new();
        // Rewards and UIDs belong to the verified HoYoLAB account, as on their settings pages.
        let link = hoyolab_account::Entity::find()
            .filter(hoyolab_account::Column::OwnerUserId.eq(user))
            .one(tx)
            .await?
            .filter(|link| link.auth_status == "verified" && link.hoyolab_uid.is_some());
        let (settings, games) = match &link {
            Some(link) => (
                reward_rows(tx, link.id, true).await?,
                game_rows(tx, link.id, true).await?,
            ),
            None => (Vec::new(), Vec::new()),
        };
        for entry in &document.rewards {
            let reason = if link.is_none() {
                Some("not_linked")
            } else {
                match settings.iter().find(|row| row.game_kind == entry.game_kind) {
                    None => Some("game_not_found"),
                    Some(row)
                        if row.enabled == entry.enabled && row.local_time == entry.local_time =>
                    {
                        None
                    }
                    Some(row) if entry.enabled && row.auth_status != "ready" => {
                        Some("reward_not_ready")
                    }
                    Some(row) => {
                        for (setting, from, to) in [
                            ("enabled", json!(row.enabled), json!(entry.enabled)),
                            ("local_time", json!(row.local_time), json!(entry.local_time)),
                        ] {
                            if from != to {
                                changes.push(Change {
                                    section: "rewards",
                                    game_kind: Some(entry.game_kind.clone()),
                                    game_uid: None,
                                    setting: setting.to_owned(),
                                    from,
                                    to,
                                });
                            }
                        }
                        writes.push(Write::Reward {
                            setting: row.clone(),
                            entry: entry.clone(),
                        });
                        None
                    }
                }
            };
            if let Some(reason) = reason {
                skipped.push(Skipped {
                    section: "rewards",
                    game_kind: entry.game_kind.clone(),
                    game_uid: None,
                    reason,
                });
            }
        }
        for entry in &document.game_accounts {
            let rows: Vec<_> = games
                .iter()
                .filter(|row| row.game_kind == entry.game_kind && row.game_uid == entry.game_uid)
                .collect();
            match rows
                .iter()
                .find(|row| row.visible != entry.visible)
                .or(rows.first())
            {
                None => skipped.push(Skipped {
                    section: "game_accounts",
                    game_kind: entry.game_kind.clone(),
                    game_uid: Some(entry.game_uid.clone()),
                    reason: if link.is_none() {
                        "not_linked"
                    } else {
                        "uid_not_found"
                    },
                }),
                Some(row) if row.visible == entry.visible => {}
                Some(row) => {
                    changes.push(Change {
                        section: "game_accounts",
                        game_kind: Some(entry.game_kind.clone()),
                        game_uid: Some(entry.game_uid.clone()),
                        setting: "visible".to_owned(),
                        from: json!(row.visible),
                        to: json!(entry.visible),
                    });
                    writes.push(Write::Visibility {
                        link: row.hoyolab_account_id,
                        game_kind: row.game_kind.clone(),
                        game_uid: row.game_uid.clone(),
                        visible: entry.visible,
                    });
                }
            }
        }
        let stored = notification_setting::Entity::find_by_id(user)
            .lock_exclusive()
            .one(tx)
            .await?;
        let current = PersonalPreferences::from_row(stored.as_ref());
        let wanted = imported_preferences(&current, &document.notifications);
        let before = changes.len();
        notification_changes(&current, &wanted, &mut changes);
        if changes.len() > before {
            writes.push(Write::Notifications(wanted));
        }
        let report = Report {
            changes,
            skipped,
            webhook: WebhookNote {
                in_file: document.notifications.webhook_configured,
                configured: stored.as_ref().is_some_and(|row| row.webhook_url.is_some()),
            },
        };
        Ok((report, writes))
    }
}

async fn reward_rows(
    tx: &DatabaseTransaction,
    link: Uuid,
    lock: bool,
) -> Result<Vec<reward_setting::Model>> {
    let query = reward_setting::Entity::find()
        .filter(reward_setting::Column::HoyolabAccountId.eq(link))
        .order_by_asc(reward_setting::Column::GameKind);
    Ok(if lock {
        query.lock_exclusive().all(tx).await?
    } else {
        query.all(tx).await?
    })
}

async fn game_rows(
    tx: &DatabaseTransaction,
    link: Uuid,
    lock: bool,
) -> Result<Vec<game_account::Model>> {
    let query = game_account::Entity::find()
        .filter(game_account::Column::HoyolabAccountId.eq(link))
        .order_by_asc(game_account::Column::GameKind)
        .order_by_asc(game_account::Column::GameUid)
        .order_by_asc(game_account::Column::Region);
    Ok(if lock {
        query.lock_exclusive().all(tx).await?
    } else {
        query.all(tx).await?
    })
}

/// The stored choices with the file's applied; event kinds the file leaves out are kept.
fn imported_preferences(
    current: &PersonalPreferences,
    file: &NotificationEntry,
) -> PersonalPreferences {
    let mut wanted = current.clone();
    wanted.webhook_format = file.webhook_format.clone();
    wanted.language = file.language.clone();
    wanted.events.extend(file.events.clone());
    wanted.app_events.extend(file.app_events.clone());
    wanted.resin_warn_hours = file.resin_warn_hours;
    wanted.daily_reminder_time = file.daily_reminder_time.clone();
    wanted.weekly_reminder_time = file.weekly_reminder_time.clone();
    wanted
}

fn notification_changes(
    current: &PersonalPreferences,
    wanted: &PersonalPreferences,
    changes: &mut Vec<Change>,
) {
    let mut push = |setting: String, from: Value, to: Value| {
        if from != to {
            changes.push(Change {
                section: "notifications",
                game_kind: None,
                game_uid: None,
                setting,
                from,
                to,
            });
        }
    };
    push(
        "webhook_format".into(),
        json!(current.webhook_format),
        json!(wanted.webhook_format),
    );
    push(
        "language".into(),
        json!(current.language),
        json!(wanted.language),
    );
    for (prefix, from, to) in [
        ("events", &current.events, &wanted.events),
        ("app_events", &current.app_events, &wanted.app_events),
    ] {
        for (kind, on) in to {
            push(
                format!("{prefix}.{kind}"),
                json!(from.get(kind).copied().unwrap_or(false)),
                json!(on),
            );
        }
    }
    push(
        "resin_warn_hours".into(),
        json!(current.resin_warn_hours),
        json!(wanted.resin_warn_hours),
    );
    push(
        "daily_reminder_time".into(),
        json!(current.daily_reminder_time),
        json!(wanted.daily_reminder_time),
    );
    push(
        "weekly_reminder_time".into(),
        json!(current.weekly_reminder_time),
        json!(wanted.weekly_reminder_time),
    );
}
