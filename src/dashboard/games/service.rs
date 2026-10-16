use super::summary::summarize;
use crate::dashboard::{
    auth::sessions::Identity,
    db::entities::{game_account, game_status_cache, reward_setting},
    error::{Error, Result},
    hoyolab::{
        gateway::{self, Gateway, LiveGateway},
        store::CookieStore,
    },
};
use attosgameapi::hoyolab::RecordAccount;
use chrono::{DateTime, Duration, Utc};
use sea_orm::{
    ColumnTrait, EntityTrait, QueryFilter, QueryOrder, Set, TransactionTrait, sea_query::OnConflict,
};
use serde_json::Value;
use std::sync::Arc;
use uuid::Uuid;

const CACHE_VERSION: i32 = 2;
const FRESH_TTL: Duration = Duration::minutes(5);

pub(in crate::dashboard) struct Games<G: Gateway = LiveGateway> {
    store: Arc<CookieStore<G>>,
}

pub(in crate::dashboard) struct GameStatus {
    pub(in crate::dashboard) game_kind: String,
    pub(in crate::dashboard) game_uid: String,
    pub(in crate::dashboard) region: String,
    pub(in crate::dashboard) visible: bool,
    pub(in crate::dashboard) detected: bool,
    pub(in crate::dashboard) payload: Option<Value>,
    pub(in crate::dashboard) fetched_at: Option<DateTime<Utc>>,
    pub(in crate::dashboard) error_code: Option<String>,
    pub(in crate::dashboard) error_at: Option<DateTime<Utc>>,
    pub(in crate::dashboard) stale: bool,
}

impl std::fmt::Debug for GameStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GameStatus")
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
    pub(in crate::dashboard) fn new(store: Arc<CookieStore<G>>) -> Self {
        Self { store }
    }

    pub(in crate::dashboard) async fn list(
        &self,
        actor: &Identity,
    ) -> Result<Vec<game_account::Model>> {
        let _operation = self.store.operations.acquire(actor.user.id).await;
        let (link, _) = self.store.ready_link(actor).await?;
        Ok(game_account::Entity::find()
            .filter(game_account::Column::HoyolabAccountId.eq(link.id))
            .order_by_asc(game_account::Column::GameKind)
            .order_by_asc(game_account::Column::GameUid)
            .order_by_asc(game_account::Column::Region)
            .all(&self.store.db)
            .await?)
    }

    pub(in crate::dashboard) async fn set_visible(
        &self,
        actor: &Identity,
        id: Uuid,
        visible: bool,
    ) -> Result<()> {
        let _operation = self.store.operations.acquire(actor.user.id).await;
        let (link, _) = self.store.ready_link(actor).await?;
        let updated = game_account::Entity::update_many()
            .col_expr(
                game_account::Column::Visible,
                sea_orm::sea_query::Expr::value(visible),
            )
            .filter(game_account::Column::Id.eq(id))
            .filter(game_account::Column::HoyolabAccountId.eq(link.id))
            .exec(&self.store.db)
            .await?;
        if updated.rows_affected == 1 {
            Ok(())
        } else {
            Err(Error::NotFound)
        }
    }

    pub(in crate::dashboard) async fn status(
        &self,
        actor: &Identity,
        id: Uuid,
    ) -> Result<GameStatus> {
        let _operation = self.store.operations.acquire(actor.user.id).await;
        let now = Utc::now();
        let (link, cookie) = self.store.ready_link(actor).await?;
        let account = find_owned_game(&self.store.db, link.id, id)
            .await?
            .ok_or(Error::NotFound)?;
        let cache = game_status_cache::Entity::find_by_id(account.id)
            .one(&self.store.db)
            .await?;

        if !account.visible || !account.detected {
            return Ok(status_from_cache(account, cache.as_ref(), None, false));
        }
        let setting = reward_setting::Entity::find()
            .filter(reward_setting::Column::HoyolabAccountId.eq(link.id))
            .filter(reward_setting::Column::GameKind.eq(&account.game_kind))
            .one(&self.store.db)
            .await?;
        if setting.is_some_and(|setting| setting.auth_status == "action_required") {
            let payload = cache
                .as_ref()
                .filter(|cache| cache.payload_version == CACHE_VERSION)
                .and_then(|cache| cache.payload.clone());
            let mut status = status_from_cache(account, cache.as_ref(), payload, true);
            status.error_code = Some("action_required".into());
            return Ok(status);
        }
        if let Some(cache) = cache.as_ref().filter(|cache| fresh(cache, now)) {
            return Ok(status_from_cache(
                account,
                Some(cache),
                cache.payload.clone(),
                false,
            ));
        }

        let game = gateway::game(&account.game_kind)?;
        let record = RecordAccount {
            uid: account.game_uid.clone(),
            server: account.region.clone(),
        };
        match self.store.gateway.note(&cookie, game, &record).await {
            Ok(raw) => {
                let fetched_at = Utc::now();
                let payload = summarize(game, fetched_at, &raw);
                self.recheck_before_store(actor, link.id, link.credential_version, account.id)
                    .await?;
                let row = CacheWrite {
                    fetched_at: Some(fetched_at),
                    updated_at: fetched_at,
                    payload: payload.clone(),
                    error_code: None,
                    error_at: None,
                };
                upsert_cache(&self.store.db, account.id, row).await?;
                Ok(GameStatus {
                    game_kind: account.game_kind,
                    game_uid: account.game_uid,
                    region: account.region,
                    visible: account.visible,
                    detected: account.detected,
                    payload,
                    fetched_at: Some(fetched_at),
                    error_code: None,
                    error_at: None,
                    stale: false,
                })
            }
            Err(error) => {
                let error_code = gateway::error_code(&error);
                let error_at = Utc::now();
                let prior_supported = cache
                    .as_ref()
                    .filter(|cache| cache.payload_version == CACHE_VERSION);
                let payload = prior_supported.and_then(|cache| cache.payload.clone());
                let fetched_at = prior_supported.and_then(|cache| cache.fetched_at);
                self.recheck_before_store(actor, link.id, link.credential_version, account.id)
                    .await?;
                let row = CacheWrite {
                    fetched_at,
                    updated_at: error_at,
                    payload: payload.clone(),
                    error_code: Some(error_code.clone()),
                    error_at: Some(error_at),
                };
                upsert_cache(&self.store.db, account.id, row).await?;
                if gateway::cookie_invalid(&error) {
                    self.store.mark_cookie_invalid(actor.user.id).await?;
                } else if matches!(
                    error,
                    attosgameapi::hoyolab::Error::Upstream { code: 10102 }
                ) {
                    let tx = self.store.db.begin().await?;
                    self.store.actor_locked(&tx, actor).await?;
                    reward_setting::Entity::update_many()
                        .col_expr(
                            reward_setting::Column::AuthStatus,
                            sea_orm::sea_query::Expr::value("action_required"),
                        )
                        .col_expr(
                            reward_setting::Column::NextRunAt,
                            sea_orm::sea_query::Expr::value(Option::<DateTime<Utc>>::None),
                        )
                        .filter(reward_setting::Column::HoyolabAccountId.eq(link.id))
                        .filter(reward_setting::Column::GameKind.eq(&account.game_kind))
                        .exec(&tx)
                        .await?;
                    tx.commit().await?;
                }
                Ok(GameStatus {
                    game_kind: account.game_kind,
                    game_uid: account.game_uid,
                    region: account.region,
                    visible: account.visible,
                    detected: account.detected,
                    payload,
                    fetched_at,
                    error_code: Some(error_code),
                    error_at: Some(error_at),
                    stale: true,
                })
            }
        }
    }

    async fn recheck_before_store(
        &self,
        actor: &Identity,
        link_id: Uuid,
        credential_version: i64,
        game_id: Uuid,
    ) -> Result<()> {
        let tx = self.store.db.begin().await?;
        let fresh_actor = self.store.actor_locked(&tx, actor).await?;
        let (fresh_link, _) = self.store.owner_link_in(&tx, fresh_actor.user.id).await?;
        if fresh_link.id != link_id || fresh_link.credential_version != credential_version {
            return Err(Error::Forbidden);
        }
        let account = find_owned_game(&tx, fresh_link.id, game_id)
            .await?
            .ok_or(Error::NotFound)?;
        if !account.visible || !account.detected {
            return Err(Error::Forbidden);
        }
        tx.commit().await?;
        Ok(())
    }
}

async fn find_owned_game<C>(db: &C, link_id: Uuid, id: Uuid) -> Result<Option<game_account::Model>>
where
    C: sea_orm::ConnectionTrait,
{
    Ok(game_account::Entity::find()
        .filter(game_account::Column::Id.eq(id))
        .filter(game_account::Column::HoyolabAccountId.eq(link_id))
        .one(db)
        .await?)
}

fn fresh(cache: &game_status_cache::Model, now: DateTime<Utc>) -> bool {
    cache.payload_version == CACHE_VERSION
        && cache.payload.is_some()
        && cache.fetched_at.is_some_and(|fetched_at| {
            let age = now.signed_duration_since(fetched_at);
            age >= Duration::zero() && age <= FRESH_TTL
        })
}

fn status_from_cache(
    account: game_account::Model,
    cache: Option<&game_status_cache::Model>,
    payload: Option<Value>,
    stale: bool,
) -> GameStatus {
    let supported = cache.filter(|cache| cache.payload_version == CACHE_VERSION);
    GameStatus {
        game_kind: account.game_kind,
        game_uid: account.game_uid,
        region: account.region,
        visible: account.visible,
        detected: account.detected,
        payload,
        fetched_at: supported.and_then(|cache| cache.fetched_at),
        error_code: supported.and_then(|cache| cache.error_code.clone()),
        error_at: supported.and_then(|cache| cache.error_at),
        stale,
    }
}

struct CacheWrite {
    fetched_at: Option<DateTime<Utc>>,
    updated_at: DateTime<Utc>,
    payload: Option<Value>,
    error_code: Option<String>,
    error_at: Option<DateTime<Utc>>,
}

async fn upsert_cache(
    db: &impl sea_orm::ConnectionTrait,
    game_account_id: Uuid,
    write: CacheWrite,
) -> Result<()> {
    use game_status_cache::Column;
    game_status_cache::Entity::insert(game_status_cache::ActiveModel {
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
