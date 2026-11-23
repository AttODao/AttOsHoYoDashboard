//! The background fetch: schedule and backoff, both games per run, source status, and the
//! counters `/metrics` reports. Independent of page views.
use super::{
    announce, catalog,
    source::{FetchError, Source},
};
use crate::dashboard::{
    db::entities::redemption_code_source, error::Result, notifications::channel::Notifications,
};
use chrono::{DateTime, Duration, Utc};
use sea_orm::{
    ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, Set,
    sea_query::{Expr, OnConflict},
};
use std::{
    collections::BTreeMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
};

/// Fetch intervals an administrator may choose, in minutes.
pub(in crate::dashboard) const INTERVAL_MINUTES: std::ops::RangeInclusive<i32> = 60..=1440;
/// The first retry after a failed run; each further failure doubles it, up to the interval.
const FIRST_RETRY_MINUTES: i64 = 15;
/// How often the worker checks whether a run is due.
pub(in crate::dashboard) const TICK: std::time::Duration = std::time::Duration::from_secs(60);

/// The wait after the last attempt: the interval after a success, a doubling backoff
/// (15, 30, 60 … minutes, never longer than the interval) after failures.
pub(super) fn delay(interval_minutes: i32, failures: i32) -> Duration {
    let interval = Duration::minutes(interval_minutes.into());
    if failures <= 0 {
        return interval;
    }
    let factor = 1_i64 << (failures - 1).min(16);
    Duration::minutes(FIRST_RETRY_MINUTES.saturating_mul(factor)).min(interval)
}

/// When the next run is due; None when the feature is stopped.
pub(super) fn next_fetch(
    state: &redemption_code_source::Model,
    now: DateTime<Utc>,
) -> Option<DateTime<Utc>> {
    if !state.enabled {
        return None;
    }
    Some(state.last_attempt_at.map_or(now, |at| {
        at + delay(state.interval_minutes, state.consecutive_failures)
    }))
}

/// The source row, or its defaults if it is missing.
pub(super) async fn source_state(db: &DatabaseConnection) -> Result<redemption_code_source::Model> {
    Ok(redemption_code_source::Entity::find_by_id(1_i16)
        .one(db)
        .await?
        .unwrap_or_else(|| redemption_code_source::Model {
            id: 1,
            enabled: true,
            interval_minutes: 180,
            last_attempt_at: None,
            last_success_at: None,
            consecutive_failures: 0,
            last_error: None,
            updated_at: Utc::now(),
        }))
}

/// Outcomes since start, by game; reported with every label so none is ever absent.
#[derive(Default)]
pub(in crate::dashboard) struct Stats {
    fetches: Mutex<BTreeMap<(&'static str, &'static str), u64>>,
    rejected: Mutex<BTreeMap<&'static str, u64>>,
    announcements: AtomicU64,
}

impl Stats {
    /// Fetch outcome labels: `ok`, a `FetchError` class, or `storage`.
    pub(in crate::dashboard) const OUTCOMES: [&'static str; 8] = [
        "ok",
        "timeout",
        "connect",
        "http_status",
        "redirect",
        "too_large",
        "invalid_payload",
        "storage",
    ];

    fn fetched(&self, game: &'static str, outcome: &'static str) {
        *self
            .fetches
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .entry((game, outcome))
            .or_default() += 1;
    }

    fn reject(&self, game: &'static str, rows: u64) {
        *self
            .rejected
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .entry(game)
            .or_default() += rows;
    }

    /// `(game, outcome, count)` for every game and outcome.
    pub(in crate::dashboard) fn fetches(&self) -> Vec<(&'static str, &'static str, u64)> {
        let counts = self
            .fetches
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        catalog::GAMES
            .iter()
            .flat_map(|game| {
                Self::OUTCOMES.iter().map(|outcome| {
                    (
                        *game,
                        *outcome,
                        counts.get(&(*game, *outcome)).copied().unwrap_or(0),
                    )
                })
            })
            .collect()
    }

    /// `(game, rejected rows)` for every game.
    pub(in crate::dashboard) fn rejected(&self) -> Vec<(&'static str, u64)> {
        let counts = self
            .rejected
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        catalog::GAMES
            .iter()
            .map(|game| (*game, counts.get(game).copied().unwrap_or(0)))
            .collect()
    }

    /// Summary notices sent since start.
    pub(in crate::dashboard) fn announcements(&self) -> u64 {
        self.announcements.load(Ordering::Relaxed)
    }
}

pub(in crate::dashboard) struct CodeWorker {
    db: DatabaseConnection,
    source: Source,
    notifications: Arc<Notifications>,
    pub(in crate::dashboard) stats: Stats,
}

impl CodeWorker {
    pub(in crate::dashboard) fn new(
        db: DatabaseConnection,
        source: Source,
        notifications: Arc<Notifications>,
    ) -> Self {
        Self {
            db,
            source,
            notifications,
            stats: Stats::default(),
        }
    }

    pub(in crate::dashboard) fn source_host(&self) -> &str {
        self.source.host()
    }

    /// Runs a fetch when one is due: both games, then the source status, pruning and the
    /// notify-only announcements. True when a run happened.
    pub(in crate::dashboard) async fn tick(&self, now: DateTime<Utc>) -> Result<bool> {
        let state = source_state(&self.db).await?;
        if next_fetch(&state, now).is_none_or(|due| due > now) {
            return Ok(false);
        }
        // The migration creates the row; recreate it if it was removed, so the stamps below
        // land and a missing row cannot turn into a fetch every minute.
        redemption_code_source::Entity::insert(redemption_code_source::ActiveModel {
            id: Set(1),
            updated_at: Set(now),
            ..Default::default()
        })
        .on_conflict(
            OnConflict::column(redemption_code_source::Column::Id)
                .do_nothing()
                .to_owned(),
        )
        .exec_without_returning(&self.db)
        .await?;
        // Recorded first, so a crash mid-run still waits for the next due time.
        redemption_code_source::Entity::update_many()
            .col_expr(
                redemption_code_source::Column::LastAttemptAt,
                Expr::value(Some(now)),
            )
            .filter(redemption_code_source::Column::Id.eq(1_i16))
            .exec(&self.db)
            .await?;
        let mut failure: Option<&'static str> = None;
        for game in catalog::GAMES {
            let outcome = match self.source.fetch(game).await {
                Ok(listing) => {
                    self.stats.reject(game, listing.rejected);
                    let first_run = state.last_success_at.is_none();
                    match catalog::store(&self.db, game, &listing, now, first_run).await {
                        Ok(_) => "ok",
                        Err(error) => {
                            tracing::error!(game, error = %error, "storing redemption codes failed");
                            "storage"
                        }
                    }
                }
                Err(error) => {
                    tracing::warn!(game, error = error.as_str(), "redemption code fetch failed");
                    error.as_str()
                }
            };
            self.stats.fetched(game, outcome);
            if outcome != "ok" {
                failure.get_or_insert(outcome);
            }
        }
        let mut update = redemption_code_source::Entity::update_many()
            .col_expr(redemption_code_source::Column::UpdatedAt, Expr::value(now));
        update = match failure {
            None => update
                .col_expr(
                    redemption_code_source::Column::LastSuccessAt,
                    Expr::value(Some(now)),
                )
                .col_expr(
                    redemption_code_source::Column::ConsecutiveFailures,
                    Expr::value(0),
                )
                .col_expr(
                    redemption_code_source::Column::LastError,
                    Expr::value(Option::<String>::None),
                ),
            Some(class) => update
                .col_expr(
                    redemption_code_source::Column::ConsecutiveFailures,
                    Expr::cust("consecutive_failures + 1"),
                )
                .col_expr(
                    redemption_code_source::Column::LastError,
                    Expr::value(Some(class)),
                ),
        };
        update
            .filter(redemption_code_source::Column::Id.eq(1_i16))
            .exec(&self.db)
            .await?;
        catalog::prune(&self.db, now).await?;
        let sent = announce::announce(&self.db, &self.notifications, now).await?;
        self.stats.announcements.fetch_add(sent, Ordering::Relaxed);
        Ok(true)
    }
}

// Every fetch error class has a status and metrics label.
const _: () = assert!(FetchError::ALL.len() + 2 == Stats::OUTCOMES.len());

#[cfg(test)]
#[path = "worker_tests.rs"]
mod tests;
