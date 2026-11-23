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
    ColumnTrait, ConnectionTrait, DatabaseConnection, EntityTrait, PaginatorTrait, QueryFilter,
    Set, TransactionTrait, sea_query::OnConflict,
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
