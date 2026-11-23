//! Administrator controls (server-wide switch, fetch interval) and the source status.
use super::{
    catalog::GAMES,
    worker::{INTERVAL_MINUTES, next_fetch, source_state},
};
use crate::dashboard::{
    auth::{
        accounts::{lock_user, require_admin},
        sessions::{Identity, Sessions},
    },
    db::entities::{redemption_code, redemption_code_source},
    error::{Error, Result},
};
use chrono::{DateTime, Utc};
use sea_orm::{
    ColumnTrait, ConnectionTrait, DatabaseConnection, DbBackend, EntityTrait, PaginatorTrait,
    QueryFilter, Set, Statement, TransactionTrait, sea_query::OnConflict,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::dashboard) struct AdminInput {
    enabled: bool,
    interval_minutes: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub(in crate::dashboard) struct CatalogueCount {
    listed: u64,
    unlisted: u64,
}

/// Who uses redemption, for the administrator; counts only.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize)]
pub(in crate::dashboard) struct RedemptionCounts {
    /// Users with at least one detected UID in `auto`.
    pub(in crate::dashboard) auto_users: i64,
    /// UIDs in `auto`.
    pub(in crate::dashboard) auto_uids: i64,
    /// Users whose redemption waits for their action (CAPTCHA or Cookie).
    pub(in crate::dashboard) action_required_users: i64,
    /// Ledger rows of unknown outcome (never sent again automatically).
    pub(in crate::dashboard) unknown: i64,
}

/// Fixed counting SQL; no user data leaves it.
const REDEMPTION_COUNTS: &str = "SELECT \
    (SELECT count(DISTINCT h.owner_user_id) FROM redemption_code_settings s \
       JOIN game_accounts g ON g.id = s.game_account_id AND g.detected \
       JOIN hoyolab_accounts h ON h.id = g.hoyolab_account_id WHERE s.mode = 'auto') AS auto_users, \
    (SELECT count(*) FROM redemption_code_settings s \
       JOIN game_accounts g ON g.id = s.game_account_id AND g.detected WHERE s.mode = 'auto') AS auto_uids, \
    (SELECT count(*) FROM hoyolab_accounts WHERE redemption_action IS NOT NULL) AS action_required_users, \
    (SELECT count(*) FROM redemption_ledger WHERE state = 'unknown') AS unknown";

pub(in crate::dashboard) async fn redemption_counts(
    db: &impl ConnectionTrait,
) -> Result<RedemptionCounts> {
    let Some(row) = db
        .query_one_raw(Statement::from_string(
            DbBackend::Postgres,
            REDEMPTION_COUNTS,
        ))
        .await?
    else {
        return Ok(RedemptionCounts::default());
    };
    Ok(RedemptionCounts {
        auto_users: row.try_get("", "auto_users")?,
        auto_uids: row.try_get("", "auto_uids")?,
        action_required_users: row.try_get("", "action_required_users")?,
        unknown: row.try_get("", "unknown")?,
    })
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub(in crate::dashboard) struct AdminView {
    enabled: bool,
    interval_minutes: i32,
    interval_minutes_min: i32,
    interval_minutes_max: i32,
    /// The configured source's host (`CODES_SOURCE_URL`).
    source_host: String,
    last_attempt_at: Option<DateTime<Utc>>,
    last_success_at: Option<DateTime<Utc>>,
    consecutive_failures: i32,
    /// Error class of the latest failed run (`timeout`, `connect`, `http_status`,
    /// `redirect`, `too_large`, `invalid_payload`, `storage`); never an upstream body.
    last_error: Option<String>,
    /// When the next run is due; null while the feature is stopped.
    next_fetch_at: Option<DateTime<Utc>>,
    catalogue: BTreeMap<&'static str, CatalogueCount>,
    redemption: RedemptionCounts,
}

async fn administrator(db: &impl ConnectionTrait, actor: &Identity) -> Result<()> {
    // Session only: never an API token.
    let fresh = Sessions::authenticate(db, &actor.token_hash).await?;
    if fresh.user.id != actor.user.id {
        return Err(Error::Unauthorized);
    }
    require_admin(&fresh.user)
}

pub(in crate::dashboard) async fn view(
    db: &DatabaseConnection,
    actor: &Identity,
    source_host: &str,
) -> Result<AdminView> {
    administrator(db, actor).await?;
    let state = source_state(db).await?;
    let mut catalogue = BTreeMap::new();
    for game in GAMES {
        let count = |listed: bool| {
            redemption_code::Entity::find()
                .filter(redemption_code::Column::GameKind.eq(game))
                .filter(redemption_code::Column::Listed.eq(listed))
                .count(db)
        };
        catalogue.insert(
            game,
            CatalogueCount {
                listed: count(true).await?,
                unlisted: count(false).await?,
            },
        );
    }
    let now = Utc::now();
    Ok(AdminView {
        next_fetch_at: next_fetch(&state, now).map(|at| at.max(now)),
        enabled: state.enabled,
        interval_minutes: state.interval_minutes,
        interval_minutes_min: *INTERVAL_MINUTES.start(),
        interval_minutes_max: *INTERVAL_MINUTES.end(),
        source_host: source_host.to_owned(),
        last_attempt_at: state.last_attempt_at,
        last_success_at: state.last_success_at,
        consecutive_failures: state.consecutive_failures,
        last_error: state.last_error,
        catalogue,
        redemption: redemption_counts(db).await?,
    })
}

/// Stores the switch and interval; the worker reads them before each due check, so a
/// change applies within a minute without a restart.
pub(in crate::dashboard) async fn update(
    db: &DatabaseConnection,
    actor: &Identity,
    input: AdminInput,
) -> Result<()> {
    if !INTERVAL_MINUTES.contains(&input.interval_minutes) {
        return Err(Error::Invalid("interval_minutes must be 60 to 1440"));
    }
    let tx = db.begin().await?;
    lock_user(&tx, actor.user.id).await?;
    administrator(&tx, actor).await?;
    redemption_code_source::Entity::insert(redemption_code_source::ActiveModel {
        id: Set(1),
        enabled: Set(input.enabled),
        interval_minutes: Set(input.interval_minutes),
        updated_at: Set(Utc::now()),
        ..Default::default()
    })
    .on_conflict(
        OnConflict::column(redemption_code_source::Column::Id)
            .update_columns([
                redemption_code_source::Column::Enabled,
                redemption_code_source::Column::IntervalMinutes,
                redemption_code_source::Column::UpdatedAt,
            ])
            .to_owned(),
    )
    .exec_without_returning(&tx)
    .await?;
    tx.commit().await?;
    Ok(())
}
