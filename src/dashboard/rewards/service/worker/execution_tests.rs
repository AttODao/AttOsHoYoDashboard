use super::super::test_support::{
    MockGateway, inserted_run, key, ready_identity, service, setting, verified_link,
};
use crate::dashboard::{
    db::entities::{reward_guard, reward_run, reward_setting, reward_status_cache},
    hoyolab::gateway::{DailyState, SignState},
    test_support::TestDatabase,
};
use attoshoyodashboard::hoyolab::{Error as UpstreamError, Result as UpstreamResult};
use chrono::{Duration, Utc};
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, IntoActiveModel, QueryFilter, Set};
use serde_json::json;
use uuid::Uuid;

async fn cached_daily(db: &sea_orm::DatabaseConnection, setting_id: Uuid) -> serde_json::Value {
    let cache = reward_status_cache::Entity::find_by_id(setting_id)
        .one(db)
        .await
        .unwrap()
        .unwrap();
    assert!(cache.fetched_at.is_some());
    assert!(cache.error_code.is_none());
    cache.payload.unwrap()
}

async fn guard_set(db: &sea_orm::DatabaseConnection, setting_id: Uuid) -> bool {
    reward_guard::Entity::find_by_id(setting_id)
        .one(db)
        .await
        .unwrap()
        .unwrap()
        .post_may_have_happened
}

#[tokio::test]
async fn already_signed_never_posts() {
    let fixture = TestDatabase::new().await;
    let actor = ready_identity(&fixture.db, "owner").await;
    let key = key();
    let link = verified_link(&fixture.db, &key, actor.user.id, "123").await;
    let setting = setting(&fixture.db, link.id, "starrail", true).await;
    let gateway = MockGateway::new(
        [Ok(DailyState {
            is_sign: true,
            total_sign_day: 7,
        })],
        Vec::<UpstreamResult<SignState>>::new(),
    );
    let rewards = service(fixture.db.clone(), key, gateway.clone());
    let run = rewards.enqueue(&actor, setting.id).await.unwrap();

    rewards.tick().await.unwrap();

    let run = reward_run::Entity::find_by_id(run.id)
        .one(&fixture.db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(run.state, "already_claimed");
    assert_eq!(gateway.sign_calls(), 0);
}

#[tokio::test]
async fn ambiguous_post_error_is_not_resent_on_later_ticks() {
    let fixture = TestDatabase::new().await;
    let actor = ready_identity(&fixture.db, "owner").await;
    let key = key();
    let link = verified_link(&fixture.db, &key, actor.user.id, "123").await;
    let setting = setting(&fixture.db, link.id, "genshin", true).await;
    let gateway = MockGateway::new(
        [
            Ok(DailyState {
                is_sign: false,
                total_sign_day: 1,
            }),
            Ok(DailyState {
                is_sign: false,
                total_sign_day: 1,
            }),
        ],
        [Err(UpstreamError::Network)],
    );
    let rewards = service(fixture.db.clone(), key, gateway.clone());
    let run = rewards.enqueue(&actor, setting.id).await.unwrap();

    rewards.tick().await.unwrap();
    rewards.tick().await.unwrap();

    let run = reward_run::Entity::find_by_id(run.id)
        .one(&fixture.db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(run.state, "unknown");
    assert_eq!(run.send_state, "may_have_sent");
    assert_eq!(gateway.sign_calls(), 1);
}

#[tokio::test]
async fn requires_verification_stops_only_that_game_before_success() {
    let fixture = TestDatabase::new().await;
    let actor = ready_identity(&fixture.db, "owner").await;
    let key = key();
    let link = verified_link(&fixture.db, &key, actor.user.id, "123").await;
    let setting = setting(&fixture.db, link.id, "genshin", true).await;
    let gateway = MockGateway::new(
        [Ok(DailyState {
            is_sign: false,
            total_sign_day: 1,
        })],
        [Ok(SignState {
            requires_verification: Some(true),
        })],
    );
    let rewards = service(fixture.db.clone(), key, gateway.clone());
    let run = rewards.enqueue(&actor, setting.id).await.unwrap();

    rewards.tick().await.unwrap();

    let run = reward_run::Entity::find_by_id(run.id)
        .one(&fixture.db)
        .await
        .unwrap()
        .unwrap();
    let setting = reward_setting::Entity::find_by_id(setting.id)
        .one(&fixture.db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(run.state, "action_required");
    assert_eq!(setting.auth_status, "action_required");
    assert!(!setting.enabled);
    assert_eq!(gateway.sign_calls(), 1);
}

#[tokio::test]
async fn initial_get_retries_at_most_four_attempts_and_never_posts() {
    let fixture = TestDatabase::new().await;
    let actor = ready_identity(&fixture.db, "owner").await;
    let key = key();
    let link = verified_link(&fixture.db, &key, actor.user.id, "123").await;
    let setting = setting(&fixture.db, link.id, "genshin", true).await;
    let gateway = MockGateway::new(
        [
            Err(UpstreamError::Network),
            Err(UpstreamError::Network),
            Err(UpstreamError::Network),
            Err(UpstreamError::Network),
        ],
        Vec::<UpstreamResult<SignState>>::new(),
    );
    let rewards = service(fixture.db.clone(), key, gateway.clone());
    let run = rewards.enqueue(&actor, setting.id).await.unwrap();

    for _ in 0..4 {
        rewards.tick().await.unwrap();
        if let Some(run) = reward_run::Entity::find_by_id(run.id)
            .one(&fixture.db)
            .await
            .unwrap()
            && run.state == "retry_wait"
        {
            let mut row = run.into_active_model();
            row.next_retry_at = Set(Some(Utc::now() - Duration::seconds(1)));
            row.update(&fixture.db).await.unwrap();
        }
    }

    let run = reward_run::Entity::find_by_id(run.id)
        .one(&fixture.db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(run.state, "failed");
    assert_eq!(run.attempt_count, 4);
    assert_eq!(gateway.daily_calls(), 4);
    assert_eq!(gateway.sign_calls(), 0);
}

#[tokio::test]
async fn stale_credential_version_stops_before_post() {
    let fixture = TestDatabase::new().await;
    let actor = ready_identity(&fixture.db, "owner").await;
    let key = key();
    let link = verified_link(&fixture.db, &key, actor.user.id, "123").await;
    let setting = setting(&fixture.db, link.id, "genshin", true).await;
    let gateway = MockGateway::new(
        Vec::<UpstreamResult<DailyState>>::new(),
        Vec::<UpstreamResult<SignState>>::new(),
    );
    let rewards = service(fixture.db.clone(), key, gateway.clone());
    let run = rewards.enqueue(&actor, setting.id).await.unwrap();
    let mut link_row = link.into_active_model();
    link_row.credential_version = Set(2);
    link_row.update(&fixture.db).await.unwrap();

    rewards.tick().await.unwrap();

    let run = reward_run::Entity::find_by_id(run.id)
        .one(&fixture.db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(run.state, "stopped");
    assert_eq!(gateway.daily_calls(), 0);
    assert_eq!(gateway.sign_calls(), 0);
}

#[tokio::test]
async fn confirmed_post_clears_durable_guard() {
    let fixture = TestDatabase::new().await;
    let actor = ready_identity(&fixture.db, "owner").await;
    let key = key();
    let link = verified_link(&fixture.db, &key, actor.user.id, "123").await;
    let setting = setting(&fixture.db, link.id, "genshin", true).await;
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
            requires_verification: Some(false),
        })],
    );
    let rewards = service(fixture.db.clone(), key, gateway.clone());
    let run = rewards.enqueue(&actor, setting.id).await.unwrap();

    rewards.tick().await.unwrap();

    let run = reward_run::Entity::find_by_id(run.id)
        .one(&fixture.db)
        .await
        .unwrap()
        .unwrap();
    let guard = reward_guard::Entity::find_by_id(setting.id)
        .one(&fixture.db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(run.state, "success");
    assert_eq!(run.send_state, "confirmed");
    assert!(!guard.post_may_have_happened);
    assert_eq!(gateway.sign_calls(), 1);
    assert_eq!(
        cached_daily(&fixture.db, setting.id).await,
        json!({"is_sign": true, "total_sign_day": 2})
    );
}

#[tokio::test]
async fn confirm_only_signed_get_clears_guard_and_reenables_posting() {
    let fixture = TestDatabase::new().await;
    let actor = ready_identity(&fixture.db, "owner").await;
    let key = key();
    let link = verified_link(&fixture.db, &key, actor.user.id, "123").await;
    let setting = setting(&fixture.db, link.id, "genshin", true).await;
    reward_guard::ActiveModel {
        setting_id: Set(setting.id),
        post_may_have_happened: Set(true),
        updated_at: Set(Utc::now()),
    }
    .insert(&fixture.db)
    .await
    .unwrap();
    let gateway = MockGateway::new(
        [
            Ok(DailyState {
                is_sign: true,
                total_sign_day: 3,
            }),
            Ok(DailyState {
                is_sign: false,
                total_sign_day: 3,
            }),
            Ok(DailyState {
                is_sign: true,
                total_sign_day: 4,
            }),
        ],
        [Ok(SignState {
            requires_verification: Some(false),
        })],
    );
    let rewards = service(fixture.db.clone(), key, gateway.clone());
    let first = rewards.enqueue(&actor, setting.id).await.unwrap();

    rewards.tick().await.unwrap();

    let first = reward_run::Entity::find_by_id(first.id)
        .one(&fixture.db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(first.state, "success");
    assert_eq!(gateway.sign_calls(), 0);
    assert!(!guard_set(&fixture.db, setting.id).await);
    assert_eq!(
        cached_daily(&fixture.db, setting.id).await,
        json!({"is_sign": true, "total_sign_day": 3})
    );

    // A later run is a normal attempt again: it checks the state and may POST.
    let second = rewards.enqueue(&actor, setting.id).await.unwrap();
    rewards.tick().await.unwrap();
    let second = reward_run::Entity::find_by_id(second.id)
        .one(&fixture.db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(second.state, "success");
    assert_eq!(second.send_state, "confirmed");
    assert_eq!(gateway.sign_calls(), 1);
}

#[tokio::test]
async fn post_error_then_signed_get_clears_guard() {
    let fixture = TestDatabase::new().await;
    let actor = ready_identity(&fixture.db, "owner").await;
    let key = key();
    let link = verified_link(&fixture.db, &key, actor.user.id, "123").await;
    let setting = setting(&fixture.db, link.id, "genshin", true).await;
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
        [Err(UpstreamError::Network)],
    );
    let rewards = service(fixture.db.clone(), key, gateway.clone());
    let run = rewards.enqueue(&actor, setting.id).await.unwrap();

    rewards.tick().await.unwrap();

    let run = reward_run::Entity::find_by_id(run.id)
        .one(&fixture.db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(run.state, "success");
    assert_eq!(run.send_state, "may_have_sent");
    assert!(!guard_set(&fixture.db, setting.id).await);
    assert_eq!(
        cached_daily(&fixture.db, setting.id).await,
        json!({"is_sign": true, "total_sign_day": 2})
    );
}

#[tokio::test]
async fn guard_from_previous_reward_day_does_not_block_schedule_or_post() {
    let fixture = TestDatabase::new().await;
    let actor = ready_identity(&fixture.db, "owner").await;
    let key = key();
    let link = verified_link(&fixture.db, &key, actor.user.id, "123").await;
    let setting = setting(&fixture.db, link.id, "genshin", true).await;
    let mut row = setting.clone().into_active_model();
    row.next_run_at = Set(Some(Utc::now() - Duration::minutes(1)));
    row.update(&fixture.db).await.unwrap();
    reward_guard::ActiveModel {
        setting_id: Set(setting.id),
        post_may_have_happened: Set(true),
        updated_at: Set(Utc::now() - Duration::days(2)),
    }
    .insert(&fixture.db)
    .await
    .unwrap();
    let gateway = MockGateway::new(
        [
            Ok(DailyState {
                is_sign: false,
                total_sign_day: 5,
            }),
            Ok(DailyState {
                is_sign: true,
                total_sign_day: 6,
            }),
        ],
        [Ok(SignState {
            requires_verification: Some(false),
        })],
    );
    let rewards = service(fixture.db.clone(), key, gateway.clone());

    rewards.tick().await.unwrap();

    let run = reward_run::Entity::find()
        .filter(reward_run::Column::SettingId.eq(setting.id))
        .one(&fixture.db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(run.trigger, "scheduled");
    assert_eq!(run.state, "success");
    assert_eq!(run.send_state, "confirmed");
    assert_eq!(gateway.sign_calls(), 1);
    assert!(!guard_set(&fixture.db, setting.id).await);
}

#[tokio::test]
async fn restart_guard_never_posts_and_survives_history_cleanup() {
    let fixture = TestDatabase::new().await;
    let actor = ready_identity(&fixture.db, "owner").await;
    let key = key();
    let link = verified_link(&fixture.db, &key, actor.user.id, "123").await;
    let setting = setting(&fixture.db, link.id, "genshin", true).await;
    reward_guard::ActiveModel {
        setting_id: Set(setting.id),
        post_may_have_happened: Set(true),
        updated_at: Set(Utc::now()),
    }
    .insert(&fixture.db)
    .await
    .unwrap();
    let active = inserted_run(&fixture.db, setting.id, "pending", "not_sent", Utc::now()).await;
    let old = inserted_run(
        &fixture.db,
        setting.id,
        "unknown",
        "may_have_sent",
        Utc::now() - Duration::days(6),
    )
    .await;
    let gateway = MockGateway::new(
        [Ok(DailyState {
            is_sign: false,
            total_sign_day: 1,
        })],
        Vec::<UpstreamResult<SignState>>::new(),
    );
    let rewards = service(fixture.db.clone(), key, gateway.clone());

    rewards.tick().await.unwrap();
    rewards.cleanup_history().await.unwrap();

    let active = reward_run::Entity::find_by_id(active.id)
        .one(&fixture.db)
        .await
        .unwrap()
        .unwrap();
    assert!(
        reward_run::Entity::find_by_id(old.id)
            .one(&fixture.db)
            .await
            .unwrap()
            .is_none()
    );
    let guard = reward_guard::Entity::find_by_id(setting.id)
        .one(&fixture.db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(active.state, "unknown");
    assert!(guard.post_may_have_happened);
    assert_eq!(gateway.daily_calls(), 1);
    assert_eq!(gateway.sign_calls(), 0);
}

#[tokio::test]
async fn disabled_user_stops_before_post() {
    let fixture = TestDatabase::new().await;
    let actor = ready_identity(&fixture.db, "owner").await;
    let key = key();
    let link = verified_link(&fixture.db, &key, actor.user.id, "123").await;
    let setting = setting(&fixture.db, link.id, "genshin", true).await;
    let gateway = MockGateway::new(
        Vec::<UpstreamResult<DailyState>>::new(),
        Vec::<UpstreamResult<SignState>>::new(),
    );
    let rewards = service(fixture.db.clone(), key, gateway.clone());
    let run = rewards.enqueue(&actor, setting.id).await.unwrap();
    let mut user = actor.user.clone().into_active_model();
    user.active = Set(false);
    user.update(&fixture.db).await.unwrap();

    rewards.tick().await.unwrap();

    let run = reward_run::Entity::find_by_id(run.id)
        .one(&fixture.db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(run.state, "stopped");
    assert_eq!(gateway.daily_calls(), 0);
    assert_eq!(gateway.sign_calls(), 0);
}
