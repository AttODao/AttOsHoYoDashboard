//! Event calendar for one owned game account, cached per account. The same rules as
//! the daily-note status apply: a verified link and a ready account, visible detected
//! UIDs only, sanitized error codes, and a re-check of the link before a result is
//! stored. The upstream calendar is one read-only request per refresh.
use super::{
    calendar_summary::{self, CalendarItem},
    service::{Games, Requester, find_owned_game},
};
use crate::dashboard::{
    auth::sessions::Identity,
    config::Tuning,
    db::entities::{game_account, game_calendar_cache},
    error::{Error, Result},
    hoyolab::gateway::{self, Gateway},
};
use attoshoyodashboard::hoyolab::RecordAccount;
use chrono::{DateTime, Duration, Utc};
use sea_orm::{EntityTrait, Set, sea_query::OnConflict};
use uuid::Uuid;

/// Bumped whenever the summary reads upstream differently, so calendars summarized by
/// an older version are fetched again instead of being served until the TTL runs out
/// (2: Genshin and Star Rail periods given as date mappings or text).
const CACHE_VERSION: i32 = 2;

pub(in crate::dashboard) struct GameCalendar {
    pub(in crate::dashboard) game_kind: String,
    pub(in crate::dashboard) game_uid: String,
    pub(in crate::dashboard) region: String,
    pub(in crate::dashboard) visible: bool,
    pub(in crate::dashboard) detected: bool,
    pub(in crate::dashboard) items: Option<Vec<CalendarItem>>,
    pub(in crate::dashboard) fetched_at: Option<DateTime<Utc>>,
    pub(in crate::dashboard) error_code: Option<String>,
    pub(in crate::dashboard) error_at: Option<DateTime<Utc>>,
    pub(in crate::dashboard) stale: bool,
}

impl std::fmt::Debug for GameCalendar {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GameCalendar")
            .field("game_kind", &self.game_kind)
            .field("region", &self.region)
            .field("visible", &self.visible)
            .field("detected", &self.detected)
            .field("fetched_at", &self.fetched_at)
            .field("error_code", &self.error_code)
            .field("error_at", &self.error_at)
            .field("stale", &self.stale)
            .finish_non_exhaustive()
    }
}

impl<G: Gateway> Games<G> {
    /// The running and announced banners and events of the game account. Served from
    /// the cache while it is younger than `CALENDAR_CACHE_TTL_MINUTES` and no item
    /// started or ended since the fetch; otherwise the calendar is fetched again.
    pub(in crate::dashboard) async fn calendar(
        &self,
        actor: &Identity,
        id: Uuid,
    ) -> Result<GameCalendar> {
        let who = Requester::Session(actor);
        let _operation = self.store.operations.acquire(actor.user.id).await;
        let now = Utc::now();
        let (link, cookie) = self.store.ready_link(actor).await?;
        let account = find_owned_game(&self.store.db, link.id, id)
            .await?
            .ok_or(Error::NotFound)?;
        let cache = game_calendar_cache::Entity::find_by_id(account.id)
            .one(&self.store.db)
            .await?;
        let supported = cache
            .as_ref()
            .filter(|cache| cache.payload_version == CACHE_VERSION);

        if !account.visible || !account.detected {
            return Ok(from_cache(account, None, false));
        }
        if self.action_required(link.id, &account.game_kind).await? {
            let mut result = from_cache(account, supported, true);
            result.error_code = Some("action_required".into());
            return Ok(result);
        }
        let tuning = self.store.tuning;
        if let Some(cache) = supported.filter(|cache| failed_recently(cache, now, tuning)) {
            return Ok(from_cache(account, Some(cache), true));
        }
        if let Some(cache) = supported.filter(|cache| fresh(cache, now, tuning)) {
            return Ok(from_cache(account, Some(cache), false));
        }

        let game = gateway::game(&account.game_kind)?;
        let record = RecordAccount {
            uid: account.game_uid.clone(),
            server: account.region.clone(),
        };
        let fetched = self.store.gateway.calendar(&cookie, game, &record).await;
        match fetched {
            Ok(raw) => {
                let fetched_at = Utc::now();
                let items = calendar_summary::summarize(game, &account.region, &raw, fetched_at);
                let payload = serde_json::to_value(&items).map_err(|_| Error::Internal)?;
                self.recheck_before_store(who, link.id, link.credential_version, account.id)
                    .await?;
                upsert_cache(
                    &self.store.db,
                    account.id,
                    CacheWrite {
                        fetched_at: Some(fetched_at),
                        updated_at: fetched_at,
                        payload: Some(payload),
                        error_code: None,
                        error_at: None,
                    },
                )
                .await?;
                Ok(GameCalendar {
                    game_kind: account.game_kind,
                    game_uid: account.game_uid,
                    region: account.region,
                    visible: account.visible,
                    detected: account.detected,
                    items: Some(items),
                    fetched_at: Some(fetched_at),
                    error_code: None,
                    error_at: None,
                    stale: false,
                })
            }
            Err(error) => {
                // Keep the last calendar and record why it could not be refreshed.
                let error_code = gateway::error_code(&error);
                let error_at = Utc::now();
                let payload = supported.and_then(|cache| cache.payload.clone());
                let fetched_at = supported.and_then(|cache| cache.fetched_at);
                self.recheck_before_store(who, link.id, link.credential_version, account.id)
                    .await?;
                upsert_cache(
                    &self.store.db,
                    account.id,
                    CacheWrite {
                        fetched_at,
                        updated_at: error_at,
                        payload: payload.clone(),
                        error_code: Some(error_code.clone()),
                        error_at: Some(error_at),
                    },
                )
                .await?;
                if gateway::cookie_invalid(&error) {
                    self.store.mark_cookie_invalid(who.owner()).await?;
                }
                Ok(GameCalendar {
                    game_kind: account.game_kind,
                    game_uid: account.game_uid,
                    region: account.region,
                    visible: account.visible,
                    detected: account.detected,
                    items: payload.as_ref().and_then(items_of),
                    fetched_at,
                    error_code: Some(error_code),
                    error_at: Some(error_at),
                    stale: true,
                })
            }
        }
    }
}

fn items_of(payload: &serde_json::Value) -> Option<Vec<CalendarItem>> {
    serde_json::from_value(payload.clone()).ok()
}

/// A calendar keeps for the calendar TTL. When an item ended or an announced item
/// started after the fetch, it is fetched again at once so the next banner (and the
/// player's progress in it) appears without waiting for the TTL.
fn fresh(cache: &game_calendar_cache::Model, now: DateTime<Utc>, tuning: Tuning) -> bool {
    let (Some(fetched_at), Some(items)) =
        (cache.fetched_at, cache.payload.as_ref().and_then(items_of))
    else {
        return false;
    };
    if cache.error_code.is_some() {
        return false;
    }
    let age = now.signed_duration_since(fetched_at);
    let boundary_since_fetch = items
        .iter()
        .flat_map(|item| [item.starts_at, item.ends_at])
        .flatten()
        .any(|at| fetched_at < at && at <= now);
    age >= Duration::zero() && age <= tuning.calendar_ttl && !boundary_since_fetch
}

/// Backs off after any failed refresh (a private record, an outage) for the status
/// TTL: the dashboard polls every minute, and the calendar is not urgent enough to
/// ask upstream again on each poll.
fn failed_recently(cache: &game_calendar_cache::Model, now: DateTime<Utc>, tuning: Tuning) -> bool {
    cache.error_code.is_some()
        && cache.error_at.is_some_and(|error_at| {
            let age = now.signed_duration_since(error_at);
            age >= Duration::zero() && age <= tuning.status_ttl
        })
}

fn from_cache(
    account: game_account::Model,
    cache: Option<&game_calendar_cache::Model>,
    stale: bool,
) -> GameCalendar {
    GameCalendar {
        game_kind: account.game_kind,
        game_uid: account.game_uid,
        region: account.region,
        visible: account.visible,
        detected: account.detected,
        items: cache
            .and_then(|cache| cache.payload.as_ref())
            .and_then(items_of),
        fetched_at: cache.and_then(|cache| cache.fetched_at),
        error_code: cache.and_then(|cache| cache.error_code.clone()),
        error_at: cache.and_then(|cache| cache.error_at),
        stale,
    }
}

struct CacheWrite {
    fetched_at: Option<DateTime<Utc>>,
    updated_at: DateTime<Utc>,
    payload: Option<serde_json::Value>,
    error_code: Option<String>,
    error_at: Option<DateTime<Utc>>,
}

async fn upsert_cache(
    db: &impl sea_orm::ConnectionTrait,
    game_account_id: Uuid,
    write: CacheWrite,
) -> Result<()> {
    use game_calendar_cache::Column;
    game_calendar_cache::Entity::insert(game_calendar_cache::ActiveModel {
        game_account_id: Set(game_account_id),
        fetched_at: Set(write.fetched_at),
        updated_at: Set(write.updated_at),
        payload: Set(write.payload),
        payload_version: Set(CACHE_VERSION),
        error_code: Set(write.error_code),
        error_at: Set(write.error_at),
    })
    .on_conflict(
        OnConflict::column(Column::GameAccountId)
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
    .exec_without_returning(db)
    .await?;
    Ok(())
}
