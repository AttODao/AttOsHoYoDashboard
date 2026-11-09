//! Resin and Trailblaze Power history: one snapshot per successful status fetch (no
//! upstream request of its own), the time at cap over the last 7 days, and retention.
//! Snapshots of hidden or no longer detected UIDs are deleted; the foreign key removes
//! them with the game account, the HoYoLAB link and the user.
use super::{
    cap_time::{self, CapTime, Rules, Sample},
    service::{Games, find_owned_game},
};
use crate::dashboard::{
    auth::sessions::Identity,
    db::entities::{game_account, resource_history},
    error::{Error, Result},
    hoyolab::gateway::Gateway,
};
use chrono::{DateTime, Duration, Utc};
use sea_orm::{
    ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, QueryOrder, Set, sea_query::OnConflict,
    sea_query::Query,
};
use serde_json::Value;
use uuid::Uuid;

/// The period the dashboard summarizes; `RESOURCE_HISTORY_DAYS` keeps at least this.
pub(in crate::dashboard) const WINDOW: Duration = Duration::days(7);
/// The longest interval between snapshots that is trusted. The notification worker
/// refreshes every 15 minutes and the dashboard after `STATUS_CACHE_TTL_MINUTES`
/// (at most 60); longer gaps are reported as unknown after the projected cap.
pub(in crate::dashboard) const MAX_GAP: Duration = Duration::minutes(90);

/// The primary capped resource of a game and its regeneration time per unit.
struct Resource {
    name: &'static str,
    current: &'static str,
    maximum: &'static str,
    full_at: &'static str,
    unit: Duration,
}

fn resource(game_kind: &str) -> Option<Resource> {
    match game_kind {
        "genshin" => Some(Resource {
            name: "resin",
            current: "current_resin",
            maximum: "max_resin",
            full_at: "resin_full_at",
            unit: Duration::minutes(8),
        }),
        "starrail" => Some(Resource {
            name: "trailblaze_power",
            current: "current_stamina",
            maximum: "max_stamina",
            full_at: "stamina_full_at",
            unit: Duration::minutes(6),
        }),
        _ => None,
    }
}

/// The snapshot a status summary carries, if it has a usable count and cap.
pub(super) fn sample_from_summary(
    game_kind: &str,
    fetched_at: DateTime<Utc>,
    summary: &Value,
) -> Option<Sample> {
    let resource = resource(game_kind)?;
    let count = |key: &str| {
        summary
            .get(key)
            .and_then(Value::as_i64)
            .filter(|value| (0..=100_000).contains(value))
    };
    let current = count(resource.current)?;
    let maximum = count(resource.maximum).filter(|maximum| *maximum > 0)?;
    let full_at = summary
        .get(resource.full_at)
        .and_then(Value::as_str)
        .and_then(|text| DateTime::parse_from_rfc3339(text).ok())
        .map(|at| at.with_timezone(&Utc));
    Some(Sample {
        at: fetched_at,
        current,
        maximum,
        full_at,
    })
}

pub(super) async fn record(
    db: &impl ConnectionTrait,
    game_account_id: Uuid,
    sample: Sample,
) -> Result<()> {
    // Counts were bounded to 0..=100000 by `sample_from_summary`.
    resource_history::Entity::insert(resource_history::ActiveModel {
        game_account_id: Set(game_account_id),
        fetched_at: Set(sample.at),
        current: Set(sample.current as i32),
        maximum: Set(sample.maximum as i32),
        full_at: Set(sample.full_at),
    })
    .on_conflict(
        OnConflict::columns([
            resource_history::Column::GameAccountId,
            resource_history::Column::FetchedAt,
        ])
        .do_nothing()
        .to_owned(),
    )
    .exec_without_returning(db)
    .await?;
    Ok(())
}

/// Deletes every snapshot of one game account (hidden by its owner).
pub(super) async fn forget(db: &impl ConnectionTrait, game_account_id: Uuid) -> Result<()> {
    resource_history::Entity::delete_many()
        .filter(resource_history::Column::GameAccountId.eq(game_account_id))
        .exec(db)
        .await?;
    Ok(())
}

pub(in crate::dashboard) struct ResourceHistory {
    pub(in crate::dashboard) game_kind: String,
    pub(in crate::dashboard) game_uid: String,
    pub(in crate::dashboard) region: String,
    pub(in crate::dashboard) visible: bool,
    pub(in crate::dashboard) detected: bool,
    pub(in crate::dashboard) resource: &'static str,
    pub(in crate::dashboard) unit: Duration,
    pub(in crate::dashboard) window_start: DateTime<Utc>,
    pub(in crate::dashboard) window_end: DateTime<Utc>,
    pub(in crate::dashboard) sample_count: usize,
    pub(in crate::dashboard) cap_time: CapTime,
}

impl std::fmt::Debug for ResourceHistory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ResourceHistory")
            .field("game_kind", &self.game_kind)
            .field("region", &self.region)
            .field("visible", &self.visible)
            .field("detected", &self.detected)
            .field("window_end", &self.window_end)
            .field("sample_count", &self.sample_count)
            .finish_non_exhaustive()
    }
}

impl<G: Gateway> Games<G> {
    /// The time the primary resource of an owned game account spent at its cap in the
    /// last 7 days, from stored snapshots only. Hidden or undetected accounts have no
    /// snapshots, so their whole window is unknown.
    pub(in crate::dashboard) async fn resource_history(
        &self,
        actor: &Identity,
        id: Uuid,
    ) -> Result<ResourceHistory> {
        let _operation = self.store.operations.acquire(actor.user.id).await;
        let (link, _) = self.store.ready_link(actor).await?;
        let account = find_owned_game(&self.store.db, link.id, id)
            .await?
            .ok_or(Error::NotFound)?;
        let resource = resource(&account.game_kind).ok_or(Error::NotFound)?;
        let window_end = Utc::now();
        let window_start = window_end - WINDOW;
        let rows = if account.visible && account.detected {
            // One earlier snapshot within the gap can still anchor the window start.
            resource_history::Entity::find()
                .filter(resource_history::Column::GameAccountId.eq(account.id))
                .filter(resource_history::Column::FetchedAt.gte(window_start - MAX_GAP))
                .filter(resource_history::Column::FetchedAt.lte(window_end))
                .order_by_asc(resource_history::Column::FetchedAt)
                .all(&self.store.db)
                .await?
        } else {
            Vec::new()
        };
        let samples: Vec<Sample> = rows
            .iter()
            .map(|row| Sample {
                at: row.fetched_at,
                current: row.current.into(),
                maximum: row.maximum.into(),
                full_at: row.full_at,
            })
            .collect();
        let cap_time = cap_time::time_at_cap(
            &samples,
            window_start,
            window_end,
            Rules {
                unit: resource.unit,
                max_gap: MAX_GAP,
            },
        );
        Ok(ResourceHistory {
            game_kind: account.game_kind,
            game_uid: account.game_uid,
            region: account.region,
            visible: account.visible,
            detected: account.detected,
            resource: resource.name,
            unit: resource.unit,
            window_start,
            window_end,
            sample_count: samples
                .iter()
                .filter(|sample| sample.at >= window_start)
                .count(),
            cap_time,
        })
    }

    /// Hourly retention: snapshots older than `RESOURCE_HISTORY_DAYS` (plus `MAX_GAP`,
    /// so the snapshot just before the 7-day window can still anchor its start), and
    /// every snapshot of a hidden or no longer detected UID.
    pub(in crate::dashboard) async fn cleanup_resource_history(&self) -> Result<()> {
        let cutoff = Utc::now() - Duration::days(self.store.tuning.resource_history_days) - MAX_GAP;
        resource_history::Entity::delete_many()
            .filter(resource_history::Column::FetchedAt.lt(cutoff))
            .exec(&self.store.db)
            .await?;
        resource_history::Entity::delete_many()
            .filter(
                resource_history::Column::GameAccountId.in_subquery(
                    Query::select()
                        .column(game_account::Column::Id)
                        .from(game_account::Entity)
                        .cond_where(
                            sea_orm::Condition::any()
                                .add(game_account::Column::Visible.eq(false))
                                .add(game_account::Column::Detected.eq(false)),
                        )
                        .to_owned(),
                ),
            )
            .exec(&self.store.db)
            .await?;
        Ok(())
    }
}
