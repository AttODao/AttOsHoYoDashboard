//! Independent, bounded reward and deletion work; neither depends on Dashboard visits.
use super::App;
use crate::dashboard::{
    auth::sessions::Sessions,
    error::Result,
    rewards::{deletion, service::Rewards},
};
use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicI64, Ordering},
    },
    time::Instant,
};
use tokio::{
    sync::watch,
    task::JoinSet,
    time::{Duration, MissedTickBehavior, interval},
};
use uuid::Uuid;

const DELETION_TASKS: usize = 32;
const MAINTENANCE_INTERVAL: Duration = Duration::from_secs(60 * 60);
/// A reward worker silent for longer than this reports the service as degraded.
pub(super) const STALLED_AFTER_SECS: i64 = 120;

/// Last time the reward worker completed a tick, for `/health`.
#[derive(Clone)]
pub(super) struct Heartbeat(Arc<AtomicI64>);

impl Heartbeat {
    pub(super) fn new() -> Self {
        Self(Arc::new(AtomicI64::new(chrono::Utc::now().timestamp())))
    }
    fn beat(&self) {
        self.0
            .store(chrono::Utc::now().timestamp(), Ordering::Relaxed);
    }
    #[cfg(test)]
    pub(super) fn set_age_secs(&self, age: i64) {
        self.0
            .store(chrono::Utc::now().timestamp() - age, Ordering::Relaxed);
    }
    pub(super) fn age_secs(&self) -> i64 {
        chrono::Utc::now().timestamp() - self.0.load(Ordering::Relaxed)
    }
}

pub(super) async fn run(app: App, stop: watch::Receiver<bool>) {
    tokio::join!(
        rewards(app.clone(), stop.clone()),
        deletions(app.clone(), stop.clone()),
        maintenance(app, stop)
    );
}

/// Hourly housekeeping that no request path depends on.
async fn maintenance(app: App, mut stop: watch::Receiver<bool>) {
    let mut timer = interval(MAINTENANCE_INTERVAL);
    timer.set_missed_tick_behavior(MissedTickBehavior::Skip);
    loop {
        tokio::select! {_ = stop.changed() => break, _ = timer.tick() => {}}
        if let Err(error) = Sessions::purge_expired(&app.accounts.db).await {
            tracing::error!(error = %error, "expired session cleanup failed");
        }
    }
}
async fn rewards(app: App, mut stop: watch::Receiver<bool>) {
    let Some(cookies) = &app.cookies else {
        return;
    };
    let rewards = std::sync::Arc::new(Rewards::new(cookies.clone()));
    let mut timer = interval(Duration::from_secs(1));
    timer.set_missed_tick_behavior(MissedTickBehavior::Skip);
    let mut maintenance = interval(MAINTENANCE_INTERVAL);
    maintenance.set_missed_tick_behavior(MissedTickBehavior::Skip);
    loop {
        tokio::select! {
            _ = stop.changed() => break,
            _ = maintenance.tick() => {
                if let Err(error) = rewards.cleanup_history().await {
                    tracing::error!(error = %error, "reward history cleanup failed");
                }
                continue;
            }
            _ = timer.tick() => {}
        }
        if let Err(error) = rewards.tick().await {
            tracing::error!(error=%error,"reward worker failed");
        }
        app.heartbeat.beat();
        if *stop.borrow() {
            break;
        }
    }
}
async fn deletions(app: App, mut stop: watch::Receiver<bool>) {
    let mut timer = interval(Duration::from_secs(1));
    timer.set_missed_tick_behavior(MissedTickBehavior::Skip);
    let mut tasks = JoinSet::new();
    let mut owners = HashMap::new();
    loop {
        tokio::select! {
            _ = stop.changed() => break,
            Some(result) = tasks.join_next_with_id(), if !tasks.is_empty() => {
                complete(&app, &mut owners, result);
            }
            _ = timer.tick() => {
                let active: Vec<Uuid> = owners.values().copied().collect();
                let pending = match app.accounts.pending_deletions(&active, Instant::now()).await {
                    Ok(ids) => ids,
                    Err(error) => {
                        tracing::error!(error = %error, "deletion queue unavailable");
                        continue;
                    }
                };
                for owner in pending.into_iter().take(DELETION_TASKS.saturating_sub(owners.len())) {
                    let app = app.clone();
                    let handle = tasks.spawn(async move { finish_owner(app, owner).await });
                    owners.insert(handle.id(), owner);
                }
            }
        }
    }
    while let Some(result) = tasks.join_next_with_id().await {
        complete(&app, &mut owners, result);
    }
}
fn complete(
    app: &App,
    owners: &mut HashMap<tokio::task::Id, Uuid>,
    result: std::result::Result<(tokio::task::Id, Result<()>), tokio::task::JoinError>,
) {
    let (id, error) = match result {
        Ok((id, Ok(()))) => {
            owners.remove(&id);
            return;
        }
        Ok((id, Err(error))) => (id, error.to_string()),
        Err(error) => (error.id(), "deletion task failed".to_owned()),
    };
    let Some(owner) = owners.remove(&id) else {
        return;
    };
    // A failing owner backs off so it neither spams retries nor starves later owners.
    match app.accounts.postpone_deletion(owner, Instant::now()) {
        Ok(delay) => tracing::warn!(
            error = %error,
            retry_in_secs = delay.as_secs(),
            "deletion completion postponed"
        ),
        Err(_) => tracing::error!(error = %error, "deletion completion postponed"),
    }
}
async fn finish_owner(app: App, owner: Uuid) -> Result<()> {
    let _operation = app.operations.acquire(owner).await;
    deletion::confirm_owner(&app.accounts.db, app.cookies.as_ref(), owner).await?;
    app.accounts.finish_deletion(owner).await
}
