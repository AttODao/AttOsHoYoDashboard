use super::test_support::{
    MockGateway, identity_with, inserted_run, key, ready_identity, service, setting, verified_link,
};
use super::*;
use crate::dashboard::{
    hoyolab::gateway::{DailyState, SignState},
    test_support::TestDatabase,
};
use attoshoyodashboard::hoyolab::Result as UpstreamResult;
use chrono::Utc;
use sea_orm::EntityTrait;

#[tokio::test]
async fn manual_run_works_when_off_and_duplicate_enqueue_coalesces() {
    let fixture = TestDatabase::new().await;
    let actor = ready_identity(&fixture.db, "owner").await;
    let key = key();
    let link = verified_link(&fixture.db, &key, actor.user.id, "123").await;
    let setting = setting(&fixture.db, link.id, "genshin", false).await;
    let gateway = MockGateway::new(
        [
            Ok(DailyState {
                is_sign: false,
                total_sign_day: 1,
            }),
            Ok(DailyState {
                is_sign: true,
                total_sign_day: 2,
            }),
        ],
        [Ok(SignState {
            requires_verification: None,
        })],
    );
    let rewards = service(fixture.db.clone(), key, gateway.clone());

    let first = rewards.enqueue(&actor, setting.id).await.unwrap();
    let second = rewards.enqueue(&actor, setting.id).await.unwrap();
    assert_eq!(first.id, second.id);
    assert_eq!(gateway.sign_calls(), 0);

    rewards.tick().await.unwrap();
    let run = reward_run::Entity::find_by_id(first.id)
        .one(&fixture.db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(run.state, "success");
    assert_eq!(run.send_state, "confirmed");
    assert_eq!(gateway.daily_calls(), 2);
    assert_eq!(gateway.sign_calls(), 1);
}

#[tokio::test]
async fn enabling_schedules_strict_future_without_immediate_run() {
    let fixture = TestDatabase::new().await;
    let actor = ready_identity(&fixture.db, "owner").await;
    let key = key();
    let link = verified_link(&fixture.db, &key, actor.user.id, "123").await;
    let setting = setting(&fixture.db, link.id, "genshin", false).await;
    let gateway = MockGateway::new(
        Vec::<UpstreamResult<DailyState>>::new(),
        Vec::<UpstreamResult<SignState>>::new(),
    );
    let rewards = service(fixture.db.clone(), key, gateway.clone());

    rewards
        .update(&actor, setting.id, true, "06:00".to_owned())
        .await
        .unwrap();
    rewards.tick().await.unwrap();

    let setting = reward_setting::Entity::find_by_id(setting.id)
        .one(&fixture.db)
        .await
        .unwrap()
        .unwrap();
    assert!(setting.enabled);
    assert!(setting.next_run_at.unwrap() > Utc::now());
    assert_eq!(
        reward_run::Entity::find()
            .all(&fixture.db)
            .await
            .unwrap()
            .len(),
        0
    );
}

#[tokio::test]
async fn owner_scoping_prevents_admin_peeking_or_cross_setting_enqueue() {
    let fixture = TestDatabase::new().await;
    let owner = ready_identity(&fixture.db, "owner").await;
    let admin = identity_with(&fixture.db, "admin", true, false).await;
    let key = key();
    let owner_link = verified_link(&fixture.db, &key, owner.user.id, "123").await;
    let admin_link = verified_link(&fixture.db, &key, admin.user.id, "456").await;
    let owner_setting = setting(&fixture.db, owner_link.id, "genshin", true).await;
    let admin_setting = setting(&fixture.db, admin_link.id, "starrail", true).await;
    inserted_run(
        &fixture.db,
        owner_setting.id,
        "success",
        "confirmed",
        Utc::now(),
    )
    .await;
    inserted_run(
        &fixture.db,
        admin_setting.id,
        "failed",
        "not_sent",
        Utc::now(),
    )
    .await;
    let gateway = MockGateway::new(
        Vec::<UpstreamResult<DailyState>>::new(),
        Vec::<UpstreamResult<SignState>>::new(),
    );
    let rewards = service(fixture.db.clone(), key, gateway);

    assert!(matches!(
        rewards.enqueue(&admin, owner_setting.id).await,
        Err(Error::NotFound)
    ));
    let history = rewards.history(&admin, 100).await.unwrap();
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].run.setting_id, admin_setting.id);
}

#[tokio::test]
async fn initial_password_and_missing_verified_link_reject_work() {
    let fixture = TestDatabase::new().await;
    let must_change = identity_with(&fixture.db, "must-change", false, true).await;
    let ready = ready_identity(&fixture.db, "ready").await;
    let key = key();
    let link = verified_link(&fixture.db, &key, must_change.user.id, "123").await;
    let setting = setting(&fixture.db, link.id, "genshin", true).await;
    let gateway = MockGateway::new(
        Vec::<UpstreamResult<DailyState>>::new(),
        Vec::<UpstreamResult<SignState>>::new(),
    );
    let rewards = service(fixture.db.clone(), key.clone(), gateway.clone());

    assert!(matches!(
        rewards.enqueue(&must_change, setting.id).await,
        Err(Error::PasswordChangeRequired)
    ));
    assert_eq!(gateway.daily_calls(), 0);
    assert_eq!(gateway.sign_calls(), 0);

    let production = Rewards::new(Arc::new(CookieStore::new(fixture.db.clone(), key)));
    assert!(matches!(
        production.enqueue(&ready, setting.id).await,
        Err(Error::InitialSetupRequired)
    ));
    production.tick().await.unwrap();
}
