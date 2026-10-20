use super::Rewards;
use crate::dashboard::{
    auth::sessions::Identity,
    db::entities::{hoyolab_account, reward_status_cache},
    error::Result,
    hoyolab::{
        credentials::Credentials,
        gateway::{self, DailyState, Gateway},
    },
    rewards::guard::reward_day,
};
use attoshoyodashboard::game::GameKind;
use chrono::{DateTime, Duration, Utc};
use sea_orm::{ConnectionTrait, EntityTrait, Set, sea_query::OnConflict};
use serde_json::{Map, Value, json};
use uuid::Uuid;

#[cfg(test)]
#[path = "cache_tests.rs"]
mod tests;

pub(super) const CACHE_VERSION: i32 = 1;

impl<G: Gateway> Rewards<G> {
    pub(super) async fn refresh_cache(
        &self,
        actor: &Identity,
        link: &hoyolab_account::Model,
        cookie: &Credentials,
        setting_id: Uuid,
        game: GameKind,
        prior: Option<&reward_status_cache::Model>,
    ) -> Result<reward_status_cache::Model> {
        match self.store.gateway.daily(cookie, game).await {
            Ok(state) => {
                let awards = self.month_awards(cookie, game, prior).await;
                self.recheck_actor_link(actor, link.id, link.credential_version, setting_id)
                    .await?;
                let write = CacheWrite::success(state).with_awards(awards);
                upsert_cache(&self.store.db, setting_id, write).await
            }
            Err(error) => {
                let write = CacheWrite::failure(gateway::error_code(&error), prior);
                self.recheck_actor_link(actor, link.id, link.credential_version, setting_id)
                    .await?;
                let row = upsert_cache(&self.store.db, setting_id, write).await?;
                if gateway::cookie_invalid(&error) {
                    self.store.mark_cookie_invalid(actor.user.id).await?;
                }
                Ok(row)
            }
        }
    }
}

impl<G: Gateway> Rewards<G> {
    /// The month's reward list changes monthly; fetch it once per reward month, best effort.
    async fn month_awards(
        &self,
        cookie: &Credentials,
        game: GameKind,
        prior: Option<&reward_status_cache::Model>,
    ) -> Option<Map<String, Value>> {
        let month = award_month(Utc::now());
        let cached = prior
            .filter(|row| row.payload_version == CACHE_VERSION)
            .and_then(|row| row.payload.as_ref())
            .and_then(|payload| month_awards_of(payload, &month));
        if cached.is_some() {
            return cached;
        }
        let awards = self.store.gateway.awards(cookie, game).await.ok()?;
        let items: Vec<Value> = awards
            .into_iter()
            .map(|award| json!({"name": award.name, "count": award.count, "icon": award.icon}))
            .collect();
        (!items.is_empty()).then(|| {
            Map::from_iter([
                ("awards_month".to_owned(), Value::String(month)),
                ("awards".to_owned(), Value::Array(items)),
            ])
        })
    }
}

fn award_month(now: DateTime<Utc>) -> String {
    reward_day(now).format("%Y-%m").to_string()
}

fn month_awards_of(payload: &Value, month: &str) -> Option<Map<String, Value>> {
    (payload.get("awards_month")?.as_str()? == month && payload.get("awards")?.is_array()).then(
        || {
            ["awards_month", "awards"]
                .into_iter()
                .filter_map(|key| Some((key.to_owned(), payload.get(key)?.clone())))
                .collect()
        },
    )
}

/// The reward for today's claim: the next one if unclaimed, else the one just claimed.
pub(in crate::dashboard::rewards) fn today_award(
    payload: &Value,
    now: DateTime<Utc>,
) -> Option<Value> {
    let awards = month_awards_of(payload, &award_month(now))?;
    let claimed = payload.get("total_sign_day")?.as_u64()?;
    let index = if payload.get("is_sign")?.as_bool()? {
        claimed.checked_sub(1)?
    } else {
        claimed
    };
    awards["awards"].get(usize::try_from(index).ok()?).cloned()
}

pub(super) struct CacheWrite {
    fetched_at: Option<DateTime<Utc>>,
    payload: Option<Value>,
    error_code: Option<String>,
    error_at: Option<DateTime<Utc>>,
}

impl CacheWrite {
    pub(super) fn success(state: DailyState) -> Self {
        Self {
            fetched_at: Some(Utc::now()),
            payload: Some(
                json!({"is_sign": state.is_sign, "total_sign_day": state.total_sign_day}),
            ),
            error_code: None,
            error_at: None,
        }
    }
    fn with_awards(mut self, awards: Option<Map<String, Value>>) -> Self {
        if let (Some(Value::Object(payload)), Some(awards)) = (self.payload.as_mut(), awards) {
            payload.extend(awards);
        }
        self
    }
    pub(super) fn failure(code: String, prior: Option<&reward_status_cache::Model>) -> Self {
        let supported = prior.filter(|row| row.payload_version == CACHE_VERSION);
        Self {
            fetched_at: supported.and_then(|row| row.fetched_at),
            payload: supported.and_then(|row| row.payload.clone()),
            error_code: Some(code),
            error_at: Some(Utc::now()),
        }
    }
}

pub(super) async fn upsert_cache(
    db: &impl ConnectionTrait,
    setting_id: Uuid,
    mut write: CacheWrite,
) -> Result<reward_status_cache::Model> {
    use reward_status_cache::Column;
    // Status writes from the worker carry no award list; keep the month's list already stored.
    if let Some(Value::Object(payload)) = write.payload.as_mut()
        && !payload.contains_key("awards")
        && let Some(prior) = reward_status_cache::Entity::find_by_id(setting_id)
            .one(db)
            .await?
            .filter(|row| row.payload_version == CACHE_VERSION)
        && let Some(awards) = prior
            .payload
            .as_ref()
            .and_then(|prior| month_awards_of(prior, &award_month(Utc::now())))
    {
        payload.extend(awards);
    }
    Ok(
        reward_status_cache::Entity::insert(reward_status_cache::ActiveModel {
            setting_id: Set(setting_id),
            fetched_at: Set(write.fetched_at),
            updated_at: Set(Utc::now()),
            payload: Set(write.payload),
            payload_version: Set(CACHE_VERSION),
            error_code: Set(write.error_code),
            error_at: Set(write.error_at),
        })
        .on_conflict(
            OnConflict::column(Column::SettingId)
                .update_columns([
                    Column::FetchedAt,
                    Column::UpdatedAt,
                    Column::Payload,
                    Column::PayloadVersion,
                    Column::ErrorCode,
                    Column::ErrorAt,
                ])
                .to_owned(),
        )
        .exec_with_returning(db)
        .await?,
    )
}

pub(super) fn cache_fresh(
    cache: &reward_status_cache::Model,
    now: DateTime<Utc>,
    ttl: Duration,
) -> bool {
    cache.payload_version == CACHE_VERSION
        && cache.payload.is_some()
        && cache.fetched_at.is_some_and(|at| {
            let age = now.signed_duration_since(at);
            age >= Duration::zero() && age <= ttl
        })
}
