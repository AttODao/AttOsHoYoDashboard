//! Endgame challenge summaries for one owned game account, cached per account.
//! The same rules as the daily-note status apply: a verified link and a ready
//! account, visible detected UIDs only, sanitized error codes, and a re-check of the
//! link before a result is stored.
use super::{
    challenge_summary::{self, ModeSummary},
    service::{Games, Requester, find_owned_game},
};
use crate::dashboard::{
    auth::sessions::Identity,
    config::Tuning,
    db::entities::{game_account, game_challenge_cache},
    error::{Error, Result},
    hoyolab::gateway::{self, Gateway},
};
use attoshoyodashboard::{challenge::Challenge, hoyolab::RecordAccount};
use chrono::{DateTime, Duration, Utc};
use sea_orm::{EntityTrait, Set, sea_query::OnConflict};
use uuid::Uuid;

const CACHE_VERSION: i32 = 1;
const PRIVATE_RECORD_CODE: &str = "upstream:10102";

pub(in crate::dashboard) struct GameChallenges {
    pub(in crate::dashboard) game_kind: String,
    pub(in crate::dashboard) game_uid: String,
    pub(in crate::dashboard) region: String,
    pub(in crate::dashboard) visible: bool,
    pub(in crate::dashboard) detected: bool,
    pub(in crate::dashboard) modes: Option<Vec<ModeSummary>>,
    pub(in crate::dashboard) fetched_at: Option<DateTime<Utc>>,
    pub(in crate::dashboard) error_code: Option<String>,
    pub(in crate::dashboard) error_at: Option<DateTime<Utc>>,
    pub(in crate::dashboard) stale: bool,
}

impl std::fmt::Debug for GameChallenges {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GameChallenges")
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
    /// The current period of each endgame mode of the game account. Served from the
    /// cache while it is younger than `CHALLENGE_CACHE_TTL_MINUTES` and no period
    /// has ended since the fetch; otherwise every mode of the game is fetched again.
    pub(in crate::dashboard) async fn challenges(
        &self,
        actor: &Identity,
        id: Uuid,
    ) -> Result<GameChallenges> {
        let who = Requester::Session(actor);
        let _operation = self.store.operations.acquire(actor.user.id).await;
        let now = Utc::now();
        let (link, cookie) = self.store.ready_link(actor).await?;
        let account = find_owned_game(&self.store.db, link.id, id)
            .await?
            .ok_or(Error::NotFound)?;
        let cache = game_challenge_cache::Entity::find_by_id(account.id)
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
        if let Some(cache) = supported.filter(|cache| private_recently(cache, now, tuning)) {
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
        let mut modes = Vec::new();
        let mut first_error = None;
        let mut fetched_any = false;
        let mut cookie_invalid = false;
        for &challenge in Challenge::for_game(game) {
            match self
                .store
                .gateway
                .challenge(&cookie, challenge, &record)
                .await
            {
                Ok(raw) => {
                    fetched_any = true;
                    modes.push(challenge_summary::summarize(
                        challenge,
                        &account.region,
                        &raw,
                    ));
                }
                Err(error) => {
                    modes.push(challenge_summary::failed(
                        challenge,
                        gateway::error_code(&error),
                    ));
                    // An invalid Cookie fails every other mode too: stop asking, and
                    // report that reason rather than an earlier mode's failure.
                    if gateway::cookie_invalid(&error) {
                        cookie_invalid = true;
                        first_error = Some(error);
                        break;
                    }
                    first_error.get_or_insert(error);
                }
            }
        }

        let failure = first_error.filter(|_| cookie_invalid || !fetched_any);
        let Some(error) = failure else {
            // Some modes may carry their own error code; the rest are current.
            let fetched_at = Utc::now();
            let payload = serde_json::to_value(&modes).map_err(|_| Error::Internal)?;
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
            return Ok(GameChallenges {
                game_kind: account.game_kind,
                game_uid: account.game_uid,
                region: account.region,
                visible: account.visible,
                detected: account.detected,
                modes: Some(modes),
                fetched_at: Some(fetched_at),
                error_code: None,
                error_at: None,
                stale: false,
            });
        };

        // Every mode failed: keep the last summary and record why.
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
        if cookie_invalid {
            self.store.mark_cookie_invalid(who.owner()).await?;
        }
        Ok(GameChallenges {
            game_kind: account.game_kind,
            game_uid: account.game_uid,
            region: account.region,
            visible: account.visible,
            detected: account.detected,
            modes: payload.as_ref().and_then(modes_of),
            fetched_at,
            error_code: Some(error_code),
            error_at: Some(error_at),
            stale: true,
        })
    }
}

fn modes_of(payload: &serde_json::Value) -> Option<Vec<ModeSummary>> {
    serde_json::from_value(payload.clone()).ok()
}

/// A complete summary keeps for the challenge TTL; one with a failed mode is retried
/// after the shorter status TTL. A period that ended after the fetch is refetched
/// at once, so the next period appears without waiting for the TTL.
fn fresh(cache: &game_challenge_cache::Model, now: DateTime<Utc>, tuning: Tuning) -> bool {
    let (Some(fetched_at), Some(modes)) =
        (cache.fetched_at, cache.payload.as_ref().and_then(modes_of))
    else {
        return false;
    };
    if cache.error_code.is_some() {
        return false;
    }
    let ttl = if modes.iter().any(|mode| mode.error_code.is_some()) {
        tuning.status_ttl
    } else {
        tuning.challenge_ttl
    };
    let age = now.signed_duration_since(fetched_at);
    let ended_since_fetch = modes
        .iter()
        .filter_map(|mode| mode.ends_at)
        .any(|end| fetched_at < end && end <= now);
    age >= Duration::zero() && age <= ttl && !ended_since_fetch
}

/// Backs off after a private-record retcode, like the daily-note status.
fn private_recently(
    cache: &game_challenge_cache::Model,
    now: DateTime<Utc>,
    tuning: Tuning,
) -> bool {
    cache.error_code.as_deref() == Some(PRIVATE_RECORD_CODE)
        && cache.error_at.is_some_and(|error_at| {
            let age = now.signed_duration_since(error_at);
            age >= Duration::zero() && age <= tuning.status_ttl
        })
}

fn from_cache(
    account: game_account::Model,
    cache: Option<&game_challenge_cache::Model>,
    stale: bool,
) -> GameChallenges {
    GameChallenges {
        game_kind: account.game_kind,
        game_uid: account.game_uid,
        region: account.region,
        visible: account.visible,
        detected: account.detected,
        modes: cache
            .and_then(|cache| cache.payload.as_ref())
            .and_then(modes_of),
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
    use game_challenge_cache::Column;
    game_challenge_cache::Entity::insert(game_challenge_cache::ActiveModel {
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
