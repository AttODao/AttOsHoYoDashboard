//! Independent, bounded reward and deletion work; neither depends on Dashboard visits.
use super::App;
use crate::dashboard::{
    error::Result,
    rewards::{deletion, service::Rewards},
};
use std::{collections::HashMap, time::Instant};
use tokio::{
    sync::watch,
    task::JoinSet,
    time::{Duration, MissedTickBehavior, interval},
};
use uuid::Uuid;

const DELETION_TASKS: usize = 32;

pub(super) async fn run(app: App, stop: watch::Receiver<bool>) {
    tokio::join!(rewards(app.clone(), stop.clone()), deletions(app, stop));
}
async fn rewards(app: App, mut stop: watch::Receiver<bool>) {
    let Some(cookies) = &app.cookies else {
        return;
    };
    let rewards = std::sync::Arc::new(Rewards::new(cookies.clone()));
    let mut timer = interval(Duration::from_secs(1));
    timer.set_missed_tick_behavior(MissedTickBehavior::Skip);
    loop {
        tokio::select! {_=stop.changed()=>break,_=timer.tick()=>{}}
        if let Err(error) = rewards.tick().await {
            tracing::error!(error=%error,"reward worker failed");
        }
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
