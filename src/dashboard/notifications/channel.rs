//! Notification channels: per-user and administrator settings, sealed URLs and test sends.
use super::{ADMIN_EVENTS, Notice, PERSONAL_EVENTS, delivery, push, secret};
use crate::dashboard::{
    auth::{
        accounts::{require_admin, require_ready},
        sessions::{Identity, Sessions},
    },
    db::entities::{admin_notification_setting, notification_setting, push_subscription},
    error::{Error, Result},
    hoyolab::credentials::CookieKey,
};
use chrono::{DateTime, NaiveTime, Utc};
use sea_orm::{
    ColumnTrait, DatabaseConnection, EntityTrait, PaginatorTrait, QueryFilter, Set,
    TransactionTrait,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::BTreeMap, sync::Arc};
use uuid::Uuid;

/// The administrator channel's scope in the delivery log and URL sealing.
pub(super) const ADMIN_SCOPE: Uuid = Uuid::nil();

/// A channel ready to send: decrypted URL (if any) and preferences.
#[derive(Clone)]
pub(super) struct Channel {
    pub(super) url: Option<String>,
    pub(super) format: String,
    pub(super) language: String,
    pub(super) events: BTreeMap<String, bool>,
}

impl Channel {
    pub(super) fn wants(&self, kind: &str) -> bool {
        self.events.get(kind).copied().unwrap_or(false)
    }
}

#[derive(Serialize)]
pub(in crate::dashboard) struct ChannelView {
    webhook_configured: bool,
    webhook_hint: Option<String>,
    webhook_format: String,
    language: String,
    events: BTreeMap<String, bool>,
    last_delivery_at: Option<DateTime<Utc>>,
    last_delivery_ok: Option<bool>,
}

#[derive(Serialize)]
pub(in crate::dashboard) struct PersonalView {
    #[serde(flatten)]
    channel: ChannelView,
    resin_warn_hours: i32,
    daily_reminder_time: String,
    weekly_reminder_time: String,
    /// VAPID public key for browser push, when the server can send push.
    push_public_key: Option<String>,
    push_subscriptions: u64,
}

/// `webhook_url`: absent keeps the stored URL, null removes it, a string replaces it.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::dashboard) struct ChannelInput {
    #[serde(default, deserialize_with = "deserialize_present")]
    webhook_url: Option<Value>,
    webhook_format: String,
    language: String,
    events: BTreeMap<String, bool>,
}

/// Serde's default drops `null` to `None`; we need it preserved so an explicit null clears.
fn deserialize_present<'de, D>(deserializer: D) -> std::result::Result<Option<Value>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::Deserialize;
    Value::deserialize(deserializer).map(Some)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::dashboard) struct PersonalInput {
    #[serde(flatten)]
    channel: ChannelInput,
    resin_warn_hours: i32,
    daily_reminder_time: String,
    weekly_reminder_time: String,
}

#[derive(Serialize)]
pub(in crate::dashboard) struct TestResult {
    /// "sent", "failed" or "not_configured".
    webhook: &'static str,
    /// Browsers that accepted a push message.
    push: usize,
    /// Whether the notice was queued for the user's registered app installations.
    app: bool,
}

pub(in crate::dashboard) struct Notifications {
    pub(super) db: DatabaseConnection,
    pub(super) key: Option<Arc<CookieKey>>,
    /// PUBLIC_ORIGIN, the VAPID contact subject.
    pub(super) origin: String,
}

fn merged(stored: &Value, known: &[(&str, bool)]) -> BTreeMap<String, bool> {
    known
        .iter()
        .map(|(kind, default)| {
            let value = stored
                .get(*kind)
                .and_then(Value::as_bool)
                .unwrap_or(*default);
            ((*kind).to_owned(), value)
        })
        .collect()
}

fn validate_choice(value: &str, allowed: &[&str], message: &'static str) -> Result<()> {
    if allowed.contains(&value) {
        Ok(())
    } else {
        Err(Error::Invalid(message))
    }
}

pub(super) fn parse_clock(value: &str) -> Result<NaiveTime> {
    if value.len() != 5 {
        return Err(Error::Invalid("reminder times must be HH:MM"));
    }
    NaiveTime::parse_from_str(value, "%H:%M")
        .map_err(|_| Error::Invalid("reminder times must be HH:MM"))
}

impl ChannelInput {
    fn validate(&self, known: &[(&str, bool)]) -> Result<()> {
        validate_choice(
            &self.webhook_format,
            &["discord", "json"],
            "webhook_format must be discord or json",
        )?;
        validate_choice(&self.language, &["ja", "en"], "language must be ja or en")?;
        if self
            .events
            .keys()
            .any(|kind| !known.iter().any(|(k, _)| k == kind))
        {
            return Err(Error::Invalid("unknown notification event"));
        }
        Ok(())
    }

    /// The sealed URL to store: Some(new value) to write, None to keep.
    fn sealed_url(&self, key: Option<&CookieKey>, scope: Uuid) -> Result<Option<Option<Vec<u8>>>> {
        match &self.webhook_url {
            None => Ok(None),
            Some(Value::Null) => Ok(Some(None)),
            Some(Value::String(raw)) if raw.trim().is_empty() => Ok(Some(None)),
            Some(Value::String(raw)) => {
                let url = delivery::validate_webhook_url(raw)?;
                let key =
                    key.ok_or(Error::Unavailable("notification storage is not configured"))?;
                Ok(Some(Some(secret::seal(key, scope, &url)?)))
            }
            Some(_) => Err(Error::Invalid("webhook_url must be a string or null")),
        }
    }

    fn events_json(&self, known: &[(&str, bool)]) -> Value {
        Value::Object(
            merged(
                &serde_json::to_value(&self.events).unwrap_or(Value::Null),
                known,
            )
            .into_iter()
            .map(|(kind, on)| (kind, Value::Bool(on)))
            .collect(),
        )
    }
}

impl Notifications {
    pub(in crate::dashboard) fn new(
        db: DatabaseConnection,
        key: Option<Arc<CookieKey>>,
        origin: String,
    ) -> Self {
        Self { db, key, origin }
    }

    async fn ready(&self, actor: &Identity) -> Result<Identity> {
        let fresh = Sessions::authenticate(&self.db, &actor.token_hash).await?;
        require_ready(&fresh.user)?;
        Ok(fresh)
    }

    async fn admin(&self, actor: &Identity) -> Result<Identity> {
        let fresh = Sessions::authenticate(&self.db, &actor.token_hash).await?;
        require_admin(&fresh.user)?;
        Ok(fresh)
    }

    fn open_url(&self, scope: Uuid, sealed: Option<&[u8]>) -> Option<String> {
        let key = self.key.as_deref()?;
        // An unreadable URL (e.g. after a key change without rekey) acts as unset.
        secret::open(key, scope, sealed?).ok()
    }

    pub(super) async fn personal_channel(
        &self,
        user: Uuid,
    ) -> Result<(Channel, Option<notification_setting::Model>)> {
        let row = notification_setting::Entity::find_by_id(user)
            .one(&self.db)
            .await?;
        let channel = Channel {
            url: row
                .as_ref()
                .and_then(|row| self.open_url(user, row.webhook_url.as_deref())),
            format: row
                .as_ref()
                .map_or_else(|| "discord".to_owned(), |row| row.webhook_format.clone()),
            language: row
                .as_ref()
                .map_or_else(|| "ja".to_owned(), |row| row.language.clone()),
            events: merged(
                row.as_ref().map_or(&Value::Null, |row| &row.events),
                &PERSONAL_EVENTS,
            ),
        };
        Ok((channel, row))
    }

    pub(super) async fn admin_channel(&self) -> Result<Channel> {
        let row = admin_notification_setting::Entity::find_by_id(1_i16)
            .one(&self.db)
            .await?;
        Ok(Channel {
            url: row
                .as_ref()
                .and_then(|row| self.open_url(ADMIN_SCOPE, row.webhook_url.as_deref())),
            format: row
                .as_ref()
                .map_or_else(|| "discord".to_owned(), |row| row.webhook_format.clone()),
            language: row
                .as_ref()
                .map_or_else(|| "ja".to_owned(), |row| row.language.clone()),
            events: merged(
                row.as_ref().map_or(&Value::Null, |row| &row.events),
                &ADMIN_EVENTS,
            ),
        })
    }

    fn view(
        channel: &Channel,
        last_at: Option<DateTime<Utc>>,
        last_ok: Option<bool>,
    ) -> ChannelView {
        ChannelView {
            webhook_configured: channel.url.is_some(),
            webhook_hint: channel.url.as_deref().map(delivery::hint),
            webhook_format: channel.format.clone(),
            language: channel.language.clone(),
            events: channel.events.clone(),
            last_delivery_at: last_at,
            last_delivery_ok: last_ok,
        }
    }

    pub(in crate::dashboard) async fn personal_view(
        &self,
        actor: &Identity,
    ) -> Result<PersonalView> {
        let fresh = self.ready(actor).await?;
        let (channel, row) = self.personal_channel(fresh.user.id).await?;
        let subscriptions = push_subscription::Entity::find()
            .filter(push_subscription::Column::UserId.eq(fresh.user.id))
            .count(&self.db)
            .await?;
        Ok(PersonalView {
            channel: Self::view(
                &channel,
                row.as_ref().and_then(|row| row.last_delivery_at),
                row.as_ref().and_then(|row| row.last_delivery_ok),
            ),
            resin_warn_hours: row.as_ref().map_or(2, |row| row.resin_warn_hours),
            daily_reminder_time: row
                .as_ref()
                .map_or_else(|| "21:00".to_owned(), |row| row.daily_reminder_time.clone()),
            weekly_reminder_time: row.as_ref().map_or_else(
                || "15:00".to_owned(),
                |row| row.weekly_reminder_time.clone(),
            ),
            push_public_key: self
                .key
                .as_deref()
                .and_then(|key| push::public_key(key).ok()),
            push_subscriptions: subscriptions,
        })
    }

    pub(in crate::dashboard) async fn update_personal(
        &self,
        actor: &Identity,
        input: PersonalInput,
    ) -> Result<()> {
        let fresh = self.ready(actor).await?;
        input.channel.validate(&PERSONAL_EVENTS)?;
        if !(1..=24).contains(&input.resin_warn_hours) {
            return Err(Error::Invalid("resin_warn_hours must be 1 to 24"));
        }
        parse_clock(&input.daily_reminder_time)?;
        parse_clock(&input.weekly_reminder_time)?;
        let user = fresh.user.id;
        let sealed = input.channel.sealed_url(self.key.as_deref(), user)?;
        let events = input.channel.events_json(&PERSONAL_EVENTS);
        let row = notification_setting::ActiveModel {
            user_id: Set(user),
            webhook_format: Set(input.channel.webhook_format),
            language: Set(input.channel.language),
            events: Set(events),
            resin_warn_hours: Set(input.resin_warn_hours),
            daily_reminder_time: Set(input.daily_reminder_time),
            weekly_reminder_time: Set(input.weekly_reminder_time),
            webhook_url: Set(sealed.clone().unwrap_or(None)),
            last_delivery_at: Set(None),
            last_delivery_ok: Set(None),
            updated_at: Set(Utc::now()),
        };
        let tx = self.db.begin().await?;
        notification_setting::Entity::insert(row)
            .on_conflict(
                sea_orm::sea_query::OnConflict::column(notification_setting::Column::UserId)
                    .update_columns([
                        notification_setting::Column::WebhookFormat,
                        notification_setting::Column::Language,
                        notification_setting::Column::Events,
                        notification_setting::Column::ResinWarnHours,
                        notification_setting::Column::DailyReminderTime,
                        notification_setting::Column::WeeklyReminderTime,
                        notification_setting::Column::UpdatedAt,
                    ])
                    .to_owned(),
            )
            .exec_without_returning(&tx)
            .await?;
        // A webhook change updates the URL and resets the delivery outcome; absence keeps both.
        if let Some(url) = sealed {
            use sea_orm::{ConnectionTrait, DbBackend, Statement, Value as SqlValue};
            let bytes = url.map(SqlValue::from).unwrap_or(SqlValue::Bytes(None));
            tx.execute_raw(Statement::from_sql_and_values(
                DbBackend::Postgres,
                "UPDATE notification_settings SET webhook_url = $1, last_delivery_at = NULL, \
                 last_delivery_ok = NULL WHERE user_id = $2",
                [bytes, SqlValue::from(user)],
            ))
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    pub(in crate::dashboard) async fn admin_view(&self, actor: &Identity) -> Result<ChannelView> {
        self.admin(actor).await?;
        let row = admin_notification_setting::Entity::find_by_id(1_i16)
            .one(&self.db)
            .await?;
        let channel = self.admin_channel().await?;
        Ok(Self::view(
            &channel,
            row.as_ref().and_then(|row| row.last_delivery_at),
            row.as_ref().and_then(|row| row.last_delivery_ok),
        ))
    }

    pub(in crate::dashboard) async fn update_admin(
        &self,
        actor: &Identity,
        input: ChannelInput,
    ) -> Result<()> {
        self.admin(actor).await?;
        input.validate(&ADMIN_EVENTS)?;
        let sealed = input.sealed_url(self.key.as_deref(), ADMIN_SCOPE)?;
        let events = input.events_json(&ADMIN_EVENTS);
        let row = admin_notification_setting::ActiveModel {
            id: Set(1_i16),
            webhook_format: Set(input.webhook_format),
            language: Set(input.language),
            events: Set(events),
            webhook_url: Set(sealed.clone().unwrap_or(None)),
            last_delivery_at: Set(None),
            last_delivery_ok: Set(None),
            updated_at: Set(Utc::now()),
        };
        let tx = self.db.begin().await?;
        admin_notification_setting::Entity::insert(row)
            .on_conflict(
                sea_orm::sea_query::OnConflict::column(admin_notification_setting::Column::Id)
                    .update_columns([
                        admin_notification_setting::Column::WebhookFormat,
                        admin_notification_setting::Column::Language,
                        admin_notification_setting::Column::Events,
                        admin_notification_setting::Column::UpdatedAt,
                    ])
                    .to_owned(),
            )
            .exec_without_returning(&tx)
            .await?;
        if let Some(url) = sealed {
            use sea_orm::{ConnectionTrait, DbBackend, Statement, Value as SqlValue};
            let bytes = url.map(SqlValue::from).unwrap_or(SqlValue::Bytes(None));
            tx.execute_raw(Statement::from_sql_and_values(
                DbBackend::Postgres,
                "UPDATE admin_notification_settings SET webhook_url = $1, \
                 last_delivery_at = NULL, last_delivery_ok = NULL WHERE id = 1",
                [bytes],
            ))
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    /// Sends a test notice through every configured route of the actor's channel.
    pub(in crate::dashboard) async fn test_personal(&self, actor: &Identity) -> Result<TestResult> {
        let fresh = self.ready(actor).await?;
        let (channel, _) = self.personal_channel(fresh.user.id).await?;
        let notice = Notice::new("test", "test");
        let webhook = self
            .send_webhook(Some(fresh.user.id), &channel, &notice)
            .await?;
        let push = push::send_to_user(self, fresh.user.id, &notice, &channel.language).await?;
        let app = self
            .send_to_app(fresh.user.id, &notice, &channel.language)
            .await?;
        Ok(TestResult { webhook, push, app })
    }

    pub(in crate::dashboard) async fn test_admin(&self, actor: &Identity) -> Result<TestResult> {
        self.admin(actor).await?;
        let channel = self.admin_channel().await?;
        let webhook = self
            .send_webhook(None, &channel, &Notice::new("test", "test"))
            .await?;
        Ok(TestResult {
            webhook,
            push: 0,
            app: false,
        })
    }

    /// Posts to the channel's webhook and records the outcome for the settings page.
    pub(super) async fn send_webhook(
        &self,
        user: Option<Uuid>,
        channel: &Channel,
        notice: &Notice,
    ) -> Result<&'static str> {
        let Some(url) = &channel.url else {
            return Ok("not_configured");
        };
        let ok = delivery::post(
            url,
            &delivery::payload(notice, &channel.format, &channel.language),
        )
        .await;
        if !ok {
            tracing::warn!(kind = notice.kind, "webhook delivery failed");
        }
        let now = Some(Utc::now());
        match user {
            Some(user) => {
                notification_setting::Entity::update_many()
                    .col_expr(
                        notification_setting::Column::LastDeliveryAt,
                        sea_orm::sea_query::Expr::value(now),
                    )
                    .col_expr(
                        notification_setting::Column::LastDeliveryOk,
                        sea_orm::sea_query::Expr::value(Some(ok)),
                    )
                    .filter(notification_setting::Column::UserId.eq(user))
                    .exec(&self.db)
                    .await?;
            }
            None => {
                admin_notification_setting::Entity::update_many()
                    .col_expr(
                        admin_notification_setting::Column::LastDeliveryAt,
                        sea_orm::sea_query::Expr::value(now),
                    )
                    .col_expr(
                        admin_notification_setting::Column::LastDeliveryOk,
                        sea_orm::sea_query::Expr::value(Some(ok)),
                    )
                    .filter(admin_notification_setting::Column::Id.eq(1_i16))
                    .exec(&self.db)
                    .await?;
            }
        }
        Ok(if ok { "sent" } else { "failed" })
    }
}
