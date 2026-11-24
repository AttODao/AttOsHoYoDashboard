//! What the owner does with redemption: read its state, enter a private code, and redeem
//! now (which is also the explicit retry after a CAPTCHA or Cookie answer). Changes need a
//! session, never an API token, and are rate-limited per user; queuing the listed codes
//! for UIDs outside `auto` (`codes::bulk`) shares the session rules and the budgets.
use super::{
    bulk::{self, Candidate},
    catalog::GAMES,
    ledger::{MANUAL, NO_GAME_ACCOUNT, PENDING, rejection},
    plan::switched_on,
    settings::{NEEDS_REDEMPTION_COOKIE, can_redeem},
    source,
};
use crate::dashboard::{
    auth::{
        accounts::{lock_user, require_ready},
        sessions::{Identity, Sessions, reauthenticate},
    },
    db::entities::{game_account, hoyolab_account, redemption_ledger},
    error::{Error, Result},
};
use chrono::{DateTime, Utc};
use sea_orm::{
    ColumnTrait, ConnectionTrait, DatabaseConnection, DatabaseTransaction, EntityTrait,
    QueryFilter, QueryOrder, Set, TransactionTrait,
    sea_query::{Expr, OnConflict},
};
use serde::Serialize;
use std::{
    collections::HashMap,
    sync::Mutex,
    time::{Duration, Instant},
};
use uuid::Uuid;

/// Requests per user and window for each operation.
const LIMITS: [(Operation, u32, Duration); 3] = [
    (Operation::EnterCode, 10, Duration::from_secs(10 * 60)),
    (Operation::RedeemNow, 3, Duration::from_secs(10 * 60)),
    (Operation::RedeemListed, 2, Duration::from_secs(10 * 60)),
];
const MAX_BUCKETS: usize = 4096;
/// A pause this soon after the Cookie was verified (registering one verifies it) means a
/// freshly checked Cookie was refused, not an old one that expired, so the settings tab says
/// registering the same Cookie again will not help.
const FRESH_COOKIE: chrono::Duration = chrono::Duration::hours(1);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(in crate::dashboard) enum Operation {
    EnterCode,
    RedeemNow,
    /// Queuing the listed codes for UIDs outside `auto` (`codes::bulk`).
    RedeemListed,
}

/// Fixed-window budgets per user and operation, kept in memory.
#[derive(Default)]
pub(in crate::dashboard) struct Throttle(Mutex<HashMap<(Uuid, Operation), (Instant, u32)>>);

impl Throttle {
    pub(in crate::dashboard) fn admit(
        &self,
        user: Uuid,
        operation: Operation,
        now: Instant,
    ) -> Result<()> {
        let (_, budget, window) = LIMITS
            .iter()
            .find(|(known, _, _)| *known == operation)
            .copied()
            .ok_or(Error::Internal)?;
        let mut buckets = self.0.lock().map_err(|_| Error::Internal)?;
        if buckets.len() >= MAX_BUCKETS {
            buckets.retain(|(_, op), (started, _)| {
                let window = LIMITS
                    .iter()
                    .find(|(known, _, _)| known == op)
                    .map_or(Duration::ZERO, |(_, _, window)| *window);
                now.duration_since(*started) < window
            });
        }
        let bucket = buckets.entry((user, operation)).or_insert((now, 0));
        if now.duration_since(bucket.0) >= window {
            *bucket = (now, 0);
        }
        if bucket.1 >= budget {
            return Err(Error::Throttled);
        }
        bucket.1 += 1;
        Ok(())
    }
}

/// The owner's redemption state, for the settings screen.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub(in crate::dashboard) struct RedemptionView {
    /// The server can redeem (Cookie storage configured) and the administrator's switch is on.
    available: bool,
    /// The registered Cookie carries the redemption credentials (`cookie_token_v2`).
    redemption_credentials: bool,
    /// Why redemption waits for the owner (`cookie_invalid`, `captcha_required`), or null.
    action_required: Option<String>,
    action_at: Option<DateTime<Utc>>,
    /// HoYoLAB's numeric retcode for the answer that paused redemption (never a message).
    action_retcode: Option<i32>,
    /// When the current Cookie was last verified (registering a Cookie verifies it).
    cookie_verified_at: Option<DateTime<Utc>>,
    /// The pause came within `FRESH_COOKIE` of verifying the current Cookie.
    action_after_cookie_update: bool,
    /// A run for this owner is going on.
    running: bool,
    /// Codes waiting to be sent for the owner's UIDs, including those held for a UID
    /// HoYoLAB did not find (`no_game_account`), which "redeem now" sends again.
    pending: u64,
    /// The detected UIDs outside `auto`, each with how many listed codes "redeem the
    /// listed codes" would queue for it now (`codes::bulk`).
    bulk: Vec<Candidate>,
}

/// One UID a code was entered for, and its state.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub(in crate::dashboard) struct EnteredCode {
    game_account_id: Uuid,
    game_uid: String,
    code: String,
    state: String,
}

/// The session owner, locked, ready, with a link that can redeem.
pub(super) async fn session_owner(
    tx: &DatabaseTransaction,
    actor: &Identity,
) -> Result<hoyolab_account::Model> {
    lock_user(tx, actor.user.id).await?;
    // Session only: `Sessions::authenticate` never accepts an API token.
    let fresh = Sessions::authenticate(tx, &actor.token_hash).await?;
    if fresh.user.id != actor.user.id {
        return Err(Error::Unauthorized);
    }
    require_ready(&fresh.user)?;
    let link = hoyolab_account::Entity::find()
        .filter(hoyolab_account::Column::OwnerUserId.eq(fresh.user.id))
        .one(tx)
        .await?
        .filter(|link| link.auth_status == "verified" && link.hoyolab_uid.is_some())
        .ok_or(Error::InitialSetupRequired)?;
    if !can_redeem(&link) {
        return Err(Error::Invalid(NEEDS_REDEMPTION_COOKIE));
    }
    if !switched_on(tx).await? {
        return Err(Error::Unavailable(
            "redemption codes are stopped by the administrator",
        ));
    }
    Ok(link)
}

/// The link's detected UIDs.
pub(super) async fn detected(
    db: &impl ConnectionTrait,
    link: Uuid,
    game_kind: Option<&str>,
) -> Result<Vec<game_account::Model>> {
    let mut query = game_account::Entity::find()
        .filter(game_account::Column::HoyolabAccountId.eq(link))
        .filter(game_account::Column::Detected.eq(true));
    if let Some(game_kind) = game_kind {
        query = query.filter(game_account::Column::GameKind.eq(game_kind));
    }
    Ok(query
        .order_by_asc(game_account::Column::GameUid)
        .order_by_asc(game_account::Column::Region)
        .all(db)
        .await?)
}

/// The owner's redemption state. API tokens may read it (`auth::api_tokens::permits`).
pub(in crate::dashboard) async fn view(
    db: &DatabaseConnection,
    actor: &Identity,
    server_can_redeem: bool,
    running: bool,
) -> Result<RedemptionView> {
    let tx = db.begin().await?;
    let fresh = reauthenticate(&tx, actor).await?;
    if fresh.user.id != actor.user.id {
        return Err(Error::Unauthorized);
    }
    require_ready(&fresh.user)?;
    let link = hoyolab_account::Entity::find()
        .filter(hoyolab_account::Column::OwnerUserId.eq(fresh.user.id))
        .one(&tx)
        .await?;
    let (pending, action_retcode, candidates) = match &link {
        Some(link) => {
            let ids: Vec<Uuid> = detected(&tx, link.id, None)
                .await?
                .into_iter()
                .map(|row| row.id)
                .collect();
            let pending = sea_orm::PaginatorTrait::count(
                redemption_ledger::Entity::find()
                    .filter(redemption_ledger::Column::GameAccountId.is_in(ids.clone()))
                    .filter(redemption_ledger::Column::State.is_in([PENDING, NO_GAME_ACCOUNT])),
                &tx,
            )
            .await?;
            // The row whose answer paused the link keeps that answer's state and number.
            let retcode = match &link.redemption_action {
                Some(action) => redemption_ledger::Entity::find()
                    .filter(redemption_ledger::Column::GameAccountId.is_in(ids))
                    .filter(redemption_ledger::Column::State.eq(action.as_str()))
                    .order_by_desc(redemption_ledger::Column::UpdatedAt)
                    .order_by_asc(redemption_ledger::Column::Code)
                    .one(&tx)
                    .await?
                    .and_then(|row| row.retcode),
                None => None,
            };
            let candidates = if link.auth_status == "verified" {
                bulk::candidates(&tx, link.id).await?
            } else {
                Vec::new()
            };
            (pending, retcode, candidates)
        }
        None => (0, None, Vec::new()),
    };
    let available = server_can_redeem && switched_on(&tx).await?;
    tx.commit().await?;
    let action_at = link.as_ref().and_then(|row| row.redemption_action_at);
    let cookie_verified_at = link
        .as_ref()
        .filter(|row| row.auth_status != "removed")
        .and_then(|row| row.verified_at);
    Ok(RedemptionView {
        available,
        redemption_credentials: link.as_ref().is_some_and(can_redeem),
        action_required: link.as_ref().and_then(|row| row.redemption_action.clone()),
        action_at,
        action_retcode,
        cookie_verified_at,
        action_after_cookie_update: after_cookie_update(action_at, cookie_verified_at),
        running,
        pending,
        bulk: candidates,
    })
}

/// Whether a pause at `action_at` came right after the Cookie verified at `registered_at`.
fn after_cookie_update(
    action_at: Option<DateTime<Utc>>,
    registered_at: Option<DateTime<Utc>>,
) -> bool {
    match (action_at, registered_at) {
        (Some(action_at), Some(registered_at)) => {
            action_at >= registered_at && action_at - registered_at <= FRESH_COOKIE
        }
        _ => false,
    }
}

/// Queues `raw` for every detected UID of `game_kind` (or only for `only`, one of
/// them), private to the owner (it never enters the shared catalogue). A code HoYoLAB already rejected for everyone takes that
/// state without a request; one the UID already has keeps its state (a `rank_too_low`
/// whose day has passed is pending again). The caller starts the owner's run.
pub(in crate::dashboard) async fn enter_code(
    db: &DatabaseConnection,
    actor: &Identity,
    game_kind: &str,
    raw: &str,
    only: Option<Uuid>,
) -> Result<Vec<EnteredCode>> {
    if !GAMES.contains(&game_kind) {
        return Err(Error::Invalid("game_kind must be genshin or starrail"));
    }
    let code = source::code(raw).ok_or(Error::Invalid("code must be 4 to 32 letters or digits"))?;
    let now = Utc::now();
    let tx = db.begin().await?;
    let link = session_owner(&tx, actor).await?;
    let mut accounts = detected(&tx, link.id, Some(game_kind)).await?;
    if let Some(only) = only {
        accounts.retain(|account| account.id == only);
    }
    if accounts.is_empty() {
        return Err(Error::NotFound);
    }
    let dead = rejection(&tx, game_kind, &code).await?;
    let mut entered = Vec::new();
    for account in &accounts {
        redemption_ledger::Entity::insert(redemption_ledger::ActiveModel {
            game_account_id: Set(account.id),
            code: Set(code.clone()),
            source: Set(MANUAL.to_owned()),
            state: Set(dead.clone().unwrap_or_else(|| PENDING.to_owned())),
            posts: Set(0),
            retcode: Set(None),
            next_attempt_at: Set(None),
            last_sent_at: Set(None),
            created_at: Set(now),
            updated_at: Set(now),
        })
        .on_conflict(
            OnConflict::columns([
                redemption_ledger::Column::GameAccountId,
                redemption_ledger::Column::Code,
            ])
            .do_nothing()
            .to_owned(),
        )
        .exec_without_returning(&tx)
        .await?;
        // An earlier rank-too-low answer may be retried by entering the code again once
        // its day has passed.
        redemption_ledger::Entity::update_many()
            .col_expr(redemption_ledger::Column::State, Expr::value(PENDING))
            .col_expr(
                redemption_ledger::Column::NextAttemptAt,
                Expr::value(Option::<DateTime<Utc>>::None),
            )
            // Entered by the owner now: it is planned whatever the UID's mode.
            .col_expr(redemption_ledger::Column::Source, Expr::value(MANUAL))
            .col_expr(redemption_ledger::Column::UpdatedAt, Expr::value(now))
            .filter(redemption_ledger::Column::GameAccountId.eq(account.id))
            .filter(redemption_ledger::Column::Code.eq(&code))
            .filter(redemption_ledger::Column::State.eq("rank_too_low"))
            .filter(redemption_ledger::Column::NextAttemptAt.lte(now))
            .exec(&tx)
            .await?;
        let row = redemption_ledger::Entity::find_by_id((account.id, code.clone()))
            .one(&tx)
            .await?
            .ok_or(Error::Internal)?;
        entered.push(EnteredCode {
            game_account_id: account.id,
            game_uid: account.game_uid.clone(),
            code: code.clone(),
            state: row.state,
        });
    }
    tx.commit().await?;
    Ok(entered)
}

/// The explicit retry: ends a pause for the owner's action and makes the codes it stopped
/// pending again. The caller starts the owner's run.
pub(in crate::dashboard) async fn redeem_now(
    db: &DatabaseConnection,
    actor: &Identity,
) -> Result<()> {
    let tx = db.begin().await?;
    let link = session_owner(&tx, actor).await?;
    super::ledger::reopen_in(&tx, link.id, Utc::now()).await?;
    tx.commit().await?;
    Ok(())
}

#[cfg(test)]
#[path = "manual_tests.rs"]
mod tests;
