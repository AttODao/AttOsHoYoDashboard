//! Regression checks for durable send admission and deletion after history expiry.
use super::super::test_support::{
    MockGateway, inserted_run, key, ready_identity, service, setting, verified_link,
};
use super::state::upsert_guard;
use crate::dashboard::{
    db::entities::{
        hoyolab_account, reward_attempt, reward_guard, reward_run, reward_setting,
        reward_status_cache,
    },
    hoyolab::gateway::{self, DailyState},
    test_support::TestDatabase,
};
use attoshoyodashboard::hoyolab::Error as UpstreamError;
use chrono::{Duration, Utc};
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, IntoActiveModel, QueryFilter, Set};

#[tokio::test]
async fn deletion_confirms_guard_only_and_ended_unknown_without_posting() {
    let fixture = TestDatabase::new().await;
    let actor = ready_identity(&fixture.db, "owner").await;
    let key = key();
    let link = verified_link(&fixture.db, &key, actor.user.id, "123").await;
    let first = setting(&fixture.db, link.id, "genshin", false).await;
    let second = setting(&fixture.db, link.id, "starrail", false).await;
    upsert_guard(&fixture.db, first.id, true).await.unwrap();
    upsert_guard(&fixture.db, second.id, true).await.unwrap();
    let ended = inserted_run(
        &fixture.db,
        second.id,
        "unknown",
        "may_have_sent",
        Utc::now() - Duration::days(6),
    )
    .await;
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
        [],
    );
    let rewards = service(fixture.db.clone(), key, gateway.clone());
    let _operation = rewards.store.operations.acquire(actor.user.id).await;
    rewards
        .confirm_owner_for_deletion(actor.user.id)
        .await
        .unwrap();
    assert_eq!(gateway.daily_calls(), 2);
    assert_eq!(gateway.sign_calls(), 0);
    let audits = crate::dashboard::db::entities::audit::Entity::find()
        .filter(crate::dashboard::db::entities::audit::Column::Action.eq("reward_delete_checked"))
        .all(&fixture.db)
        .await
        .unwrap();
    assert_eq!(audits.len(), 2);
    for id in [first.id, second.id] {
        let cache = reward_status_cache::Entity::find_by_id(id)
            .one(&fixture.db)
            .await
            .unwrap()
            .unwrap();
        assert!(cache.fetched_at.is_some());
        assert!(
            reward_guard::Entity::find_by_id(id)
                .one(&fixture.db)
                .await
                .unwrap()
                .unwrap()
                .post_may_have_happened
        );
    }
    let ended = reward_run::Entity::find_by_id(ended.id)
        .one(&fixture.db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(ended.send_state, "may_have_sent");
    assert!(ended.ended_at.is_some());
}
#[tokio::test]
async fn cookie_invalid_after_post_keeps_may_have_sent_ledger_and_guard() {
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
            Err(UpstreamError::Upstream { code: -100 }),
        ],
        [Err(UpstreamError::Upstream { code: -100 })],
    );
    let rewards = service(fixture.db.clone(), key, gateway.clone());
    let run = rewards.enqueue(&actor, setting.id).await.unwrap();
    rewards.tick().await.unwrap();
    let run = reward_run::Entity::find_by_id(run.id)
        .one(&fixture.db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(run.state, "stopped");
    assert_eq!(run.send_state, "may_have_sent");
    assert_eq!(gateway.sign_calls(), 1);
    let attempt = reward_attempt::Entity::find()
        .filter(reward_attempt::Column::RunId.eq(run.id))
        .one(&fixture.db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(attempt.send_state, "may_have_sent");
    assert!(
        reward_guard::Entity::find_by_id(setting.id)
            .one(&fixture.db)
            .await
            .unwrap()
            .unwrap()
            .post_may_have_happened
    );
    assert_eq!(
        hoyolab_account::Entity::find_by_id(link.id)
            .one(&fixture.db)
            .await
            .unwrap()
            .unwrap()
            .auth_status,
        "cookie_invalid"
    );
}
#[tokio::test]
async fn one_uncertain_or_disabled_schedule_does_not_starve_another_owner() {
    let fixture = TestDatabase::new().await;
    let key = key();
    let uncertain = ready_identity(&fixture.db, "uncertain").await;
    let disabled = ready_identity(&fixture.db, "disabled").await;
    let ready = ready_identity(&fixture.db, "ready").await;
    let mut settings = vec![];
    for (actor, uid) in [(&uncertain, "123"), (&disabled, "124"), (&ready, "125")] {
        let link = verified_link(&fixture.db, &key, actor.user.id, uid).await;
        let setting = setting(&fixture.db, link.id, "genshin", true).await;
        let mut row = setting.into_active_model();
        row.next_run_at = Set(Some(Utc::now() - Duration::hours(1)));
        settings.push(row.update(&fixture.db).await.unwrap());
    }
    upsert_guard(&fixture.db, settings[0].id, true)
        .await
        .unwrap();
    crate::dashboard::db::entities::user::ActiveModel {
        id: Set(disabled.user.id),
        active: Set(false),
        ..Default::default()
    }
    .update(&fixture.db)
    .await
    .unwrap();
    let gateway = MockGateway::new(
        [Ok(DailyState {
            is_sign: true,
            total_sign_day: 1,
        })],
        [],
    );
    let rewards = service(fixture.db.clone(), key, gateway.clone());
    rewards.tick().await.unwrap();
    assert_eq!(gateway.daily_calls(), 1);
    assert_eq!(gateway.sign_calls(), 0);
    for setting in &settings[..2] {
        assert!(
            reward_run::Entity::find()
                .filter(reward_run::Column::SettingId.eq(setting.id))
                .all(&fixture.db)
                .await
                .unwrap()
                .is_empty()
        );
    }
    assert!(
        reward_setting::Entity::find_by_id(settings[1].id)
            .one(&fixture.db)
            .await
            .unwrap()
            .unwrap()
            .next_run_at
            .is_none()
    );
    assert_eq!(
        reward_run::Entity::find()
            .filter(reward_run::Column::SettingId.eq(settings[2].id))
            .one(&fixture.db)
            .await
            .unwrap()
            .unwrap()
            .state,
        "already_claimed"
    );
}
#[tokio::test]
async fn interrupted_old_work_is_confirmed_before_history_cleanup() {
    let fixture = TestDatabase::new().await;
    let actor = ready_identity(&fixture.db, "owner").await;
    let key = key();
    let link = verified_link(&fixture.db, &key, actor.user.id, "123").await;
    let setting = setting(&fixture.db, link.id, "genshin", false).await;
    upsert_guard(&fixture.db, setting.id, true).await.unwrap();
    let run = inserted_run(
        &fixture.db,
        setting.id,
        "running",
        "may_have_sent",
        Utc::now() - Duration::days(6),
    )
    .await;
    let gateway = MockGateway::new(
        [Ok(DailyState {
            is_sign: false,
            total_sign_day: 1,
        })],
        [],
    );
    let rewards = service(fixture.db.clone(), key, gateway.clone());
    rewards.tick().await.unwrap();
    assert_eq!(
        reward_run::Entity::find_by_id(run.id)
            .one(&fixture.db)
            .await
            .unwrap()
            .unwrap()
            .state,
        "unknown"
    );
    rewards.tick().await.unwrap();
    assert!(
        reward_run::Entity::find_by_id(run.id)
            .one(&fixture.db)
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(gateway.daily_calls(), 1);
    assert_eq!(gateway.sign_calls(), 0);
    assert!(
        reward_guard::Entity::find_by_id(setting.id)
            .one(&fixture.db)
            .await
            .unwrap()
            .unwrap()
            .post_may_have_happened
    );
}
#[tokio::test]
async fn schedule_time_update_stops_unsent_automatic_retry_until_next_tick() {
    let fixture = TestDatabase::new().await;
    let actor = ready_identity(&fixture.db, "owner").await;
    let key = key();
    let link = verified_link(&fixture.db, &key, actor.user.id, "123").await;
    let setting = setting(&fixture.db, link.id, "genshin", true).await;
    let run = inserted_run(
        &fixture.db,
        setting.id,
        "retry_wait",
        "not_sent",
        Utc::now(),
    )
    .await;
    let mut row = run.into_active_model();
    row.trigger = Set("catchup".into());
    let run = row.update(&fixture.db).await.unwrap();
    let gateway = MockGateway::new([], []);
    let rewards = service(fixture.db.clone(), key, gateway.clone());
    rewards
        .update(&actor, setting.id, true, "07:00".into())
        .await
        .unwrap();
    rewards.tick().await.unwrap();
    assert_eq!(
        reward_run::Entity::find_by_id(run.id)
            .one(&fixture.db)
            .await
            .unwrap()
            .unwrap()
            .state,
        "stopped"
    );
    assert_eq!(gateway.daily_calls(), 0);
    assert_eq!(gateway.sign_calls(), 0);
    assert!(
        reward_setting::Entity::find_by_id(setting.id)
            .one(&fixture.db)
            .await
            .unwrap()
            .unwrap()
            .next_run_at
            .unwrap()
            > Utc::now()
    );
}

#[tokio::test]
async fn shutdown_between_durable_mark_and_send_aborts_known_unsent_reservation() {
    let fixture = TestDatabase::new().await;
    let actor = ready_identity(&fixture.db, "owner").await;
    let key = key();
    let link = verified_link(&fixture.db, &key, actor.user.id, "123").await;
    let setting = setting(&fixture.db, link.id, "genshin", false).await;
    let gateway = MockGateway::new([], []);
    let rewards = service(fixture.db.clone(), key, gateway.clone());
    let run = rewards.enqueue(&actor, setting.id).await.unwrap();
    let _operation = rewards.store.operations.acquire(actor.user.id).await;
    let prepared = rewards.prepare_attempt(run.id).await.unwrap().unwrap();
    assert!(rewards.mark_may_have_sent(&prepared).await.unwrap());
    rewards.store.stop_new_operations();
    rewards
        .send_after_admission(prepared, gateway::game("genshin").unwrap())
        .await
        .unwrap();
    assert_eq!(gateway.sign_calls(), 0);
    assert_eq!(gateway.daily_calls(), 0);
    assert!(
        !reward_guard::Entity::find_by_id(setting.id)
            .one(&fixture.db)
            .await
            .unwrap()
            .unwrap()
            .post_may_have_happened
    );
    let run = reward_run::Entity::find_by_id(run.id)
        .one(&fixture.db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(run.state, "stopped");
    assert_eq!(run.send_state, "not_sent");
}
