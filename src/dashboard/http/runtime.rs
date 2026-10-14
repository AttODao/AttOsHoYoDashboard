//! Independent, bounded reward and deletion work; neither depends on Dashboard visits.
use super::App;
use crate::dashboard::{
    db::entities::{hoyolab_account, reward_guard, reward_setting},
    error::{Error, Result},
    rewards::service::Rewards,
};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
use std::collections::HashMap;
use tokio::{
    sync::watch,
    task::JoinSet,
    time::{Duration, MissedTickBehavior, interval},
};
use uuid::Uuid;
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
         _=stop.changed()=>break,
         result=tasks.join_next_with_id(),if !tasks.is_empty()=>{if let Some(result)=result{complete(&mut owners,result);}},
         _=timer.tick()=>{
          let pending=match app.accounts.pending_deletions().await {Ok(ids)=>ids,Err(error)=>{tracing::error!(error=%error,"deletion queue unavailable");continue;}};
          for owner in pending {
           if owners.len()>=32 {break;}
           // ponytail: scan at most 32 active deletions; index by owner if the limit grows.
           if owners.values().any(|active|*active==owner){continue;}
           let app=app.clone();let handle=tasks.spawn(async move{finish_owner(app,owner).await});owners.insert(handle.id(),owner);
          }
         }
        }
    }
    while let Some(result) = tasks.join_next_with_id().await {
        complete(&mut owners, result);
    }
}
fn complete(
    owners: &mut HashMap<tokio::task::Id, Uuid>,
    result: std::result::Result<(tokio::task::Id, Result<()>), tokio::task::JoinError>,
) {
    match result {
        Ok((id, result)) => {
            owners.remove(&id);
            if let Err(error) = result {
                tracing::error!(error=%error,"deletion completion postponed");
            }
        }
        Err(error) => {
            owners.remove(&error.id());
            tracing::error!("deletion worker failed");
        }
    }
}
async fn finish_owner(app: App, owner: Uuid) -> Result<()> {
    let _operation = app.operations.acquire(owner).await;
    if let Some(cookies) = &app.cookies {
        Rewards::new(cookies.clone())
            .confirm_owner_for_deletion(owner)
            .await?;
    } else if let Some(link) = hoyolab_account::Entity::find()
        .filter(hoyolab_account::Column::OwnerUserId.eq(owner))
        .one(&app.accounts.db)
        .await?
    {
        let ids = reward_setting::Entity::find()
            .filter(reward_setting::Column::HoyolabAccountId.eq(link.id))
            .all(&app.accounts.db)
            .await?
            .into_iter()
            .map(|row| row.id)
            .collect::<Vec<_>>();
        if !ids.is_empty()
            && reward_guard::Entity::find()
                .filter(reward_guard::Column::SettingId.is_in(ids))
                .filter(reward_guard::Column::PostMayHaveHappened.eq(true))
                .one(&app.accounts.db)
                .await?
                .is_some()
        {
            return Err(Error::Unavailable(
                "Cookie key is required to confirm an earlier operation",
            ));
        }
    }
    app.accounts.finish_deletion(owner).await
}
