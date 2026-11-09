use super::resource_history::{MAX_GAP, WINDOW, sample_from_summary};
use super::service_tests::{MockGateway, game, key, ready_identity, service, verified_link};
use crate::dashboard::{
    db::entities::{game_account, game_status_cache, resource_history},
    error::Error,
    test_support::TestDatabase,
};
use attoshoyodashboard::hoyolab::Error as UpstreamError;
use chrono::{DateTime, Duration, Utc};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, PaginatorTrait, QueryFilter,
    Set, sea_query::Expr,
};
use serde_json::json;
use uuid::Uuid;

async fn snapshot(db: &DatabaseConnection, id: Uuid, at: DateTime<Utc>, current: i32) {
    resource_history::ActiveModel {
        game_account_id: Set(id),
        fetched_at: Set(at),
        current: Set(current),
        maximum: Set(200),
        full_at: Set(None),
    }
    .insert(db)
    .await
    .unwrap();
}

async fn count(db: &DatabaseConnection, id: Uuid) -> u64 {
    resource_history::Entity::find()
        .filter(resource_history::Column::GameAccountId.eq(id))
        .count(db)
        .await
        .unwrap()
}

#[test]
fn snapshots_take_the_primary_resource_of_each_game_from_the_summary() {
    let at = Utc::now();
    let full = (at + Duration::minutes(80)).to_rfc3339();
    let genshin = sample_from_summary(
        "genshin",
        at,
        &json!({"current_resin": 190, "max_resin": 200, "resin_full_at": full,
                "current_home_coin": 10, "max_home_coin": 2400}),
    )
    .unwrap();
    assert_eq!((genshin.current, genshin.maximum), (190, 200));
    assert_eq!(
        genshin.full_at.map(|full_at| full_at.timestamp()),
        Some((at + Duration::minutes(80)).timestamp())
    );
    let starrail = sample_from_summary(
        "starrail",
        at,
        &json!({"current_stamina": 300, "max_stamina": 300, "stamina_full_at": "not a time"}),
    )
    .unwrap();
    assert_eq!((starrail.current, starrail.maximum), (300, 300));
    assert_eq!(starrail.full_at, None);
    for (kind, summary) in [
        ("genshin", json!({"max_resin": 200})),
        ("genshin", json!({"current_resin": -1, "max_resin": 200})),
        ("genshin", json!({"current_resin": 10, "max_resin": 0})),
        ("genshin", json!({"current_resin": "10", "max_resin": 200})),
        ("starrail", json!({"current_resin": 10, "max_resin": 200})),
        ("zzz", json!({"current_resin": 10, "max_resin": 200})),
    ] {
        assert!(
            sample_from_summary(kind, at, &summary).is_none(),
            "{summary}"
        );
    }
}

#[tokio::test]
async fn only_successful_status_fetches_record_snapshots() {
    let fixture = TestDatabase::new().await;
    let actor = ready_identity(&fixture.db, "owner").await;
    let key = key();
    let link = verified_link(&fixture.db, &key, actor.user.id, "123").await;
    let account = game(&fixture.db, link.id, "genshin", "800000001", true, true).await;
    let gateway = MockGateway::new([
        Ok(json!({"current_resin": 150, "max_resin": 200, "resin_recovery_time": "24000"})),
        Err(UpstreamError::Network),
    ]);
    let games = service(fixture.db.clone(), key, gateway.clone());

    games.status(&actor, account.id).await.unwrap();
    // A cached answer is not a new observation.
    games.status(&actor, account.id).await.unwrap();
    assert_eq!(gateway.calls(), 1);
    let rows = resource_history::Entity::find()
        .all(&fixture.db)
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!((rows[0].current, rows[0].maximum), (150, 200));
    assert!(rows[0].full_at.is_some());

    // A failed refresh keeps the old payload but records nothing.
    game_status_cache::Entity::update_many()
        .col_expr(
            game_status_cache::Column::FetchedAt,
            Expr::value(Utc::now() - Duration::hours(1)),
        )
        .exec(&fixture.db)
        .await
        .unwrap();
    let stale = games.status(&actor, account.id).await.unwrap();
    assert!(stale.stale);
    assert_eq!(gateway.calls(), 2);
    assert_eq!(count(&fixture.db, account.id).await, 1);
}

#[tokio::test]
async fn the_history_reports_time_at_cap_and_is_owner_scoped() {
    let fixture = TestDatabase::new().await;
    let actor = ready_identity(&fixture.db, "owner").await;
    let other = ready_identity(&fixture.db, "other").await;
    let key = key();
    let link = verified_link(&fixture.db, &key, actor.user.id, "123").await;
    let other_link = verified_link(&fixture.db, &key, other.user.id, "456").await;
    let account = game(&fixture.db, link.id, "genshin", "800000001", true, true).await;
    let foreign = game(
        &fixture.db,
        other_link.id,
        "genshin",
        "800000009",
        true,
        true,
    )
    .await;
    let now = Utc::now();
    // Capped for the last hour, checked every 30 minutes.
    for minutes in [60, 30, 0] {
        snapshot(
            &fixture.db,
            account.id,
            now - Duration::minutes(minutes),
            200,
        )
        .await;
    }
    // Older than the window plus the gap: never read.
    snapshot(
        &fixture.db,
        account.id,
        now - WINDOW - MAX_GAP - Duration::hours(1),
        0,
    )
    .await;
    snapshot(&fixture.db, foreign.id, now, 200).await;
    let gateway = MockGateway::empty();
    let games = service(fixture.db.clone(), key, gateway.clone());

    let history = games.resource_history(&actor, account.id).await.unwrap();
    assert_eq!(gateway.calls(), 0);
    assert_eq!(history.resource, "resin");
    assert_eq!(history.unit, Duration::minutes(8));
    assert_eq!(history.sample_count, 3);
    assert_eq!(history.window_end - history.window_start, WINDOW);
    let at_cap = history.cap_time.at_cap.num_seconds();
    assert!((3_600..=3_610).contains(&at_cap), "{at_cap}");
    // The rest of the week before the first snapshot is unknown, not below the cap.
    assert_eq!(
        history.cap_time.unknown + history.cap_time.at_cap,
        history.window_end - history.window_start
    );
    assert!(!format!("{history:?}").contains("800000001"));
    assert!(matches!(
        games.resource_history(&actor, foreign.id).await,
        Err(Error::NotFound)
    ));
    assert!(matches!(
        games.resource_history(&actor, Uuid::new_v4()).await,
        Err(Error::NotFound)
    ));
}

#[tokio::test]
async fn hiding_a_uid_forgets_its_history() {
    let fixture = TestDatabase::new().await;
    let actor = ready_identity(&fixture.db, "owner").await;
    let key = key();
    let link = verified_link(&fixture.db, &key, actor.user.id, "123").await;
    let account = game(&fixture.db, link.id, "starrail", "900000001", true, true).await;
    let kept = game(&fixture.db, link.id, "starrail", "900000002", true, true).await;
    snapshot(&fixture.db, account.id, Utc::now(), 100).await;
    snapshot(&fixture.db, kept.id, Utc::now(), 100).await;
    let games = service(fixture.db.clone(), key, MockGateway::empty());

    games.set_visible(&actor, account.id, false).await.unwrap();
    assert_eq!(count(&fixture.db, account.id).await, 0);
    assert_eq!(count(&fixture.db, kept.id).await, 1);
    let history = games.resource_history(&actor, account.id).await.unwrap();
    assert_eq!(history.resource, "trailblaze_power");
    assert!(!history.visible && history.sample_count == 0);
    assert_eq!(history.cap_time.unknown, WINDOW);
    // Showing it again starts from nothing.
    games.set_visible(&actor, account.id, true).await.unwrap();
    assert_eq!(count(&fixture.db, account.id).await, 0);
}

#[tokio::test]
async fn cleanup_applies_retention_and_drops_hidden_or_undetected_uids() {
    let fixture = TestDatabase::new().await;
    let actor = ready_identity(&fixture.db, "owner").await;
    let key = key();
    let link = verified_link(&fixture.db, &key, actor.user.id, "123").await;
    let shown = game(&fixture.db, link.id, "genshin", "800000001", true, true).await;
    let hidden = game(&fixture.db, link.id, "genshin", "800000002", true, true).await;
    let gone = game(&fixture.db, link.id, "genshin", "800000003", true, true).await;
    let now = Utc::now();
    // The default retention is 7 days.
    snapshot(&fixture.db, shown.id, now - Duration::days(8), 10).await;
    snapshot(&fixture.db, shown.id, now - Duration::days(6), 10).await;
    snapshot(&fixture.db, hidden.id, now, 10).await;
    snapshot(&fixture.db, gone.id, now, 10).await;
    // A fetch that raced the hide, and a UID HoYoLAB no longer lists.
    game_account::Entity::update_many()
        .col_expr(game_account::Column::Visible, Expr::value(false))
        .filter(game_account::Column::Id.eq(hidden.id))
        .exec(&fixture.db)
        .await
        .unwrap();
    game_account::Entity::update_many()
        .col_expr(game_account::Column::Detected, Expr::value(false))
        .filter(game_account::Column::Id.eq(gone.id))
        .exec(&fixture.db)
        .await
        .unwrap();
    let games = service(fixture.db.clone(), key, MockGateway::empty());

    games.cleanup_resource_history().await.unwrap();
    let rows = resource_history::Entity::find()
        .all(&fixture.db)
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].game_account_id, shown.id);
    assert!(rows[0].fetched_at > now - Duration::days(7));
}
