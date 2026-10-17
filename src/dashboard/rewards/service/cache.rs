use super::Rewards;
use crate::dashboard::{
    auth::sessions::Identity,
    db::entities::{hoyolab_account, reward_status_cache},
    error::Result,
    hoyolab::{
        credentials::Credentials,
        gateway::{self, DailyState, Gateway},
    },
};
use attoshoyodashboard::game::GameKind;
use chrono::{DateTime, Duration, Utc};
use sea_orm::{ConnectionTrait, EntityTrait, Set, sea_query::OnConflict};
use serde_json::{Value, json};
use uuid::Uuid;

#[cfg(test)]
#[path = "cache_tests.rs"]
mod tests;

pub(super) const CACHE_VERSION: i32 = 1;
const FRESH_TTL: Duration = Duration::minutes(5);

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
                self.recheck_actor_link(actor, link.id, link.credential_version, setting_id)
                    .await?;
                upsert_cache(&self.store.db, setting_id, CacheWrite::success(state)).await
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
    write: CacheWrite,
) -> Result<reward_status_cache::Model> {
    use reward_status_cache::Column;
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

pub(super) fn cache_fresh(cache: &reward_status_cache::Model, now: DateTime<Utc>) -> bool {
    cache.payload_version == CACHE_VERSION
        && cache.payload.is_some()
        && cache.fetched_at.is_some_and(|at| {
            let age = now.signed_duration_since(at);
            age >= Duration::zero() && age <= FRESH_TTL
        })
}
