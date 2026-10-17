use super::super::test_support::{
    MockGateway, key, ready_identity, service, setting, verified_link,
};
use super::*;
use crate::dashboard::{hoyolab::gateway::SignState, test_support::TestDatabase};
use attoshoyodashboard::hoyolab::{Error as UpstreamError, Result as UpstreamResult};
use sea_orm::{ActiveModelTrait, IntoActiveModel, TransactionTrait};

async fn cache(
    db: &sea_orm::DatabaseConnection,
    setting_id: Uuid,
    fetched_at: chrono::DateTime<Utc>,
    is_sign: bool,
) {
    reward_status_cache::ActiveModel {
        setting_id: Set(setting_id),
        fetched_at: Set(Some(fetched_at)),
        updated_at: Set(fetched_at),
        payload: Set(Some(json!({"is_sign": is_sign, "total_sign_day": 9}))),
        payload_version: Set(1),
        error_code: Set(None),
        error_at: Set(None),
    }
    .insert(db)
    .await
    .unwrap();
}

#[tokio::test]
async fn list_uses_fresh_cache_and_retains_old_payload_on_stale_failure() {
    let fixture = TestDatabase::new().await;
    let actor = ready_identity(&fixture.db, "owner").await;
    let key = key();
    let link = verified_link(&fixture.db, &key, actor.user.id, "123").await;
    let stale = setting(&fixture.db, link.id, "genshin", true).await;
    let fetched_at = Utc::now() - Duration::minutes(10);
    cache(&fixture.db, stale.id, fetched_at, true).await;
    let gateway = MockGateway::new(
        [
            Err(UpstreamError::Network),
            Ok(DailyState {
                is_sign: false,
                total_sign_day: 10,
            }),
        ],
        Vec::<UpstreamResult<SignState>>::new(),
    );
    let rewards = service(fixture.db.clone(), key, gateway.clone());

    let views = rewards.list(&actor).await.unwrap();

    assert_eq!(views.len(), 1);
    assert_eq!(views[0].is_sign, Some(true));
    assert_eq!(views[0].error_code.as_deref(), Some("network"));
    assert_eq!(
        views[0].last_checked_at.unwrap().timestamp_micros(),
        fetched_at.timestamp_micros()
    );
    assert!(views[0].error_at.is_some());
    assert_eq!(gateway.daily_calls(), 1);
    let stored = reward_status_cache::Entity::find_by_id(stale.id)
        .one(&fixture.db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored.fetched_at, views[0].last_checked_at);
    assert_eq!(stored.payload.as_ref().unwrap()["is_sign"], true);
    assert_eq!(stored.payload_version, 1);

    let views = rewards.list(&actor).await.unwrap();
    assert_eq!(views[0].is_sign, Some(false));
    assert!(views[0].last_checked_at.unwrap() > fetched_at);
    assert!(views[0].error_code.is_none());
    assert!(views[0].error_at.is_none());
    let stored = reward_status_cache::Entity::find_by_id(stale.id)
        .one(&fixture.db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored.fetched_at, views[0].last_checked_at);
    assert_eq!(stored.payload.as_ref().unwrap()["total_sign_day"], 10);
    assert!(stored.error_code.is_none());
    assert!(stored.error_at.is_none());
    rewards.list(&actor).await.unwrap();
    assert_eq!(gateway.daily_calls(), 2);
}

#[tokio::test]
async fn cache_upsert_returns_persisted_nullable_overwrites_in_a_transaction() {
    let fixture = TestDatabase::new().await;
    let actor = ready_identity(&fixture.db, "owner").await;
    let key = key();
    let link = verified_link(&fixture.db, &key, actor.user.id, "123").await;
    let setting = setting(&fixture.db, link.id, "genshin", false).await;
    let tx = fixture.db.begin().await.unwrap();
    let failed = upsert_cache(&tx, setting.id, CacheWrite::failure("network".into(), None))
        .await
        .unwrap();
    assert_eq!(failed.setting_id, setting.id);
    assert!(failed.payload.is_none());
    assert!(failed.fetched_at.is_none());
    assert_eq!(failed.payload_version, 1);
    assert_eq!(failed.error_code.as_deref(), Some("network"));
    assert!(failed.error_at.is_some());

    let success = upsert_cache(
        &tx,
        setting.id,
        CacheWrite::success(DailyState {
            is_sign: true,
            total_sign_day: 1,
        }),
    )
    .await
    .unwrap();
    assert!(success.payload.is_some());
    assert!(success.fetched_at.is_some());
    assert!(success.error_code.is_none());
    assert!(success.error_at.is_none());
    let mut unsupported = success.clone().into_active_model();
    unsupported.payload_version = Set(99);
    unsupported.update(&tx).await.unwrap();
    let cleared = upsert_cache(
        &tx,
        setting.id,
        CacheWrite {
            fetched_at: None,
            payload: None,
            error_code: None,
            error_at: None,
        },
    )
    .await
    .unwrap();
    assert!(cleared.payload.is_none());
    assert!(cleared.fetched_at.is_none());
    assert!(cleared.error_code.is_none());
    assert!(cleared.error_at.is_none());
    assert_eq!(cleared.payload_version, 1);
    assert!(cleared.updated_at >= success.updated_at);
    let persisted = reward_status_cache::Entity::find_by_id(setting.id)
        .one(&tx)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(cleared, persisted);
    tx.commit().await.unwrap();
}

#[tokio::test]
async fn future_cache_is_not_fresh_and_action_required_does_not_refresh() {
    let fixture = TestDatabase::new().await;
    let actor = ready_identity(&fixture.db, "owner").await;
    let key = key();
    let link = verified_link(&fixture.db, &key, actor.user.id, "123").await;
    let future = setting(&fixture.db, link.id, "genshin", true).await;
    cache(
        &fixture.db,
        future.id,
        Utc::now() + Duration::minutes(1),
        true,
    )
    .await;
    let halted = setting(&fixture.db, link.id, "starrail", true).await;
    cache(
        &fixture.db,
        halted.id,
        Utc::now() - Duration::minutes(10),
        true,
    )
    .await;
    let halted_id = halted.id;
    let mut halted_row = halted.into_active_model();
    halted_row.auth_status = Set("action_required".to_owned());
    halted_row.update(&fixture.db).await.unwrap();
    let gateway = MockGateway::new(
        [Ok(DailyState {
            is_sign: false,
            total_sign_day: 1,
        })],
        Vec::<UpstreamResult<SignState>>::new(),
    );
    let rewards = service(fixture.db.clone(), key, gateway.clone());

    let views = rewards.list(&actor).await.unwrap();

    assert_eq!(gateway.daily_calls(), 1);
    let future_view = views
        .iter()
        .find(|view| view.setting.id == future.id)
        .unwrap();
    let halted_view = views
        .iter()
        .find(|view| view.setting.id == halted_id)
        .unwrap();
    assert_eq!(future_view.is_sign, Some(false));
    assert_eq!(halted_view.is_sign, Some(true));
}
