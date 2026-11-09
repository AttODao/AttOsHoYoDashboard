use super::service_tests::{MockGateway, game, key, ready_identity, service, verified_link};
use crate::dashboard::{
    db::entities::{game_challenge_cache, hoyolab_account},
    error::Error,
    test_support::TestDatabase,
};
use attoshoyodashboard::{challenge::Challenge, hoyolab::Error as UpstreamError};
use chrono::{Duration, Utc};
use sea_orm::{ActiveModelTrait, EntityTrait, Set};
use serde_json::{Value, json};

fn spiral(end: chrono::DateTime<Utc>) -> Value {
    json!({
        "is_unlock": true,
        "start_time": (end - Duration::days(14)).timestamp().to_string(),
        "end_time": end.timestamp().to_string(),
        "total_battle_times": 12, "max_floor": "12-3", "total_star": 36,
        "reveal_rank": [{"avatar_id": 10000002, "value": 9}]
    })
}

async fn stored(db: &sea_orm::DatabaseConnection, id: uuid::Uuid) -> game_challenge_cache::Model {
    game_challenge_cache::Entity::find_by_id(id)
        .one(db)
        .await
        .unwrap()
        .unwrap()
}

async fn age_cache(
    db: &sea_orm::DatabaseConnection,
    id: uuid::Uuid,
    fetched_at: chrono::DateTime<Utc>,
    payload: Option<Value>,
) {
    let mut row: game_challenge_cache::ActiveModel = stored(db, id).await.into();
    row.fetched_at = Set(Some(fetched_at));
    if let Some(payload) = payload {
        row.payload = Set(Some(payload));
    }
    row.update(db).await.unwrap();
}

#[tokio::test]
async fn every_mode_is_fetched_once_cached_and_refetched_after_its_period_ends() {
    let fixture = TestDatabase::new().await;
    let actor = ready_identity(&fixture.db, "owner").await;
    let other = ready_identity(&fixture.db, "other").await;
    let key = key();
    let link = verified_link(&fixture.db, &key, actor.user.id, "123").await;
    let other_link = verified_link(&fixture.db, &key, other.user.id, "456").await;
    let account = game(&fixture.db, link.id, "genshin", "800000001", true, true).await;
    let hidden = game(&fixture.db, link.id, "genshin", "800000002", false, true).await;
    let foreign = game(
        &fixture.db,
        other_link.id,
        "genshin",
        "800000003",
        true,
        true,
    )
    .await;
    let end = Utc::now() + Duration::days(3);
    let gateway = MockGateway::with_challenges([
        Ok(spiral(end)),
        Ok(json!({"is_unlock": false, "data": []})),
        Ok(json!({})),
        Ok(spiral(end + Duration::days(14))),
        Ok(json!({})),
        Ok(json!({})),
    ]);
    let games = service(fixture.db.clone(), key, gateway.clone());

    // Hidden and foreign accounts never reach upstream.
    let hidden_result = games.challenges(&actor, hidden.id).await.unwrap();
    assert!(hidden_result.modes.is_none() && !hidden_result.visible);
    assert!(matches!(
        games.challenges(&actor, foreign.id).await,
        Err(Error::NotFound)
    ));
    assert!(gateway.challenge_calls().is_empty());

    let result = games.challenges(&actor, account.id).await.unwrap();
    assert_eq!(
        gateway.challenge_calls(),
        Challenge::for_game(attoshoyodashboard::game::GameKind::Genshin(
            attoshoyodashboard::game::Genshin
        ))
        .to_vec()
    );
    assert!(!result.stale && result.error_code.is_none());
    let modes = result.modes.clone().unwrap();
    let ids: Vec<_> = modes.iter().map(|mode| mode.mode.as_str()).collect();
    assert_eq!(
        ids,
        ["spiral_abyss", "imaginarium_theater", "stygian_onslaught"]
    );
    assert_eq!(modes[0].stars, Some(36));
    assert_eq!(
        modes[0].ends_at.map(|at| at.timestamp()),
        Some(end.timestamp())
    );
    assert_eq!(modes[1].unlocked, Some(false));
    assert!(!format!("{result:?}").contains("12-3"));
    let row = stored(&fixture.db, account.id).await;
    assert_eq!(row.payload_version, 1);
    assert!(
        !row.payload
            .as_ref()
            .unwrap()
            .to_string()
            .contains("avatar_id")
    );

    // Within the TTL the cache is served.
    let cached = games.challenges(&actor, account.id).await.unwrap();
    assert_eq!(cached.modes, result.modes);
    assert_eq!(gateway.challenge_calls().len(), 3);

    // A period that ended after the fetch is refetched before the TTL runs out.
    let mut payload = row.payload.clone().unwrap();
    payload[0]["ends_at"] = json!((Utc::now() - Duration::minutes(1)).to_rfc3339());
    age_cache(
        &fixture.db,
        account.id,
        Utc::now() - Duration::minutes(10),
        Some(payload),
    )
    .await;
    let next = games.challenges(&actor, account.id).await.unwrap();
    assert_eq!(gateway.challenge_calls().len(), 6);
    assert_eq!(
        next.modes.unwrap()[0].ends_at.map(|at| at.timestamp()),
        Some((end + Duration::days(14)).timestamp())
    );
}

#[tokio::test]
async fn one_failed_mode_keeps_the_others_and_is_retried_after_the_status_ttl() {
    let fixture = TestDatabase::new().await;
    let actor = ready_identity(&fixture.db, "owner").await;
    let key = key();
    let link = verified_link(&fixture.db, &key, actor.user.id, "123").await;
    let account = game(&fixture.db, link.id, "starrail", "900000001", true, true).await;
    let floors = json!({"has_data": true, "star_num": 27, "max_floor": "Floor 10"});
    let gateway = MockGateway::with_challenges([
        Ok(floors.clone()),
        Err(UpstreamError::Network),
        Ok(floors.clone()),
        Ok(json!({"challenge_peak_records": []})),
        Ok(floors.clone()),
        Ok(floors.clone()),
        Ok(floors.clone()),
        Ok(floors.clone()),
    ]);
    let games = service(fixture.db.clone(), key, gateway.clone());

    let result = games.challenges(&actor, account.id).await.unwrap();
    assert_eq!(gateway.challenge_calls().len(), 4);
    assert!(!result.stale && result.error_code.is_none());
    let modes = result.modes.unwrap();
    assert_eq!(modes[0].stars, Some(27));
    assert_eq!(modes[1].mode, "pure_fiction");
    assert_eq!(modes[1].error_code.as_deref(), Some("network"));
    assert_eq!(modes[1].stars, None);

    games.challenges(&actor, account.id).await.unwrap();
    assert_eq!(gateway.challenge_calls().len(), 4);
    age_cache(
        &fixture.db,
        account.id,
        Utc::now() - Duration::minutes(6),
        None,
    )
    .await;
    let retried = games.challenges(&actor, account.id).await.unwrap();
    assert_eq!(gateway.challenge_calls().len(), 8);
    assert!(
        retried
            .modes
            .unwrap()
            .iter()
            .all(|mode| mode.error_code.is_none())
    );
}

#[tokio::test]
async fn an_invalid_cookie_stops_fetching_keeps_the_last_summary_and_marks_the_link() {
    let fixture = TestDatabase::new().await;
    let actor = ready_identity(&fixture.db, "owner").await;
    let key = key();
    let link = verified_link(&fixture.db, &key, actor.user.id, "123").await;
    let account = game(&fixture.db, link.id, "genshin", "800000001", true, true).await;
    let gateway = MockGateway::with_challenges([
        Ok(spiral(Utc::now() + Duration::days(3))),
        Ok(json!({})),
        Ok(json!({})),
        Err(UpstreamError::Network),
        Err(UpstreamError::Upstream { code: -100 }),
    ]);
    let games = service(fixture.db.clone(), key, gateway.clone());
    let first = games.challenges(&actor, account.id).await.unwrap();
    let fetched_at = first.fetched_at;
    age_cache(
        &fixture.db,
        account.id,
        Utc::now() - Duration::hours(2),
        None,
    )
    .await;

    let failed = games.challenges(&actor, account.id).await.unwrap();

    // The Cookie rejection stops the remaining mode and is the reported reason,
    // even though an earlier mode failed for another reason.
    assert_eq!(gateway.challenge_calls().len(), 5);
    assert!(failed.stale);
    assert_eq!(failed.error_code.as_deref(), Some("upstream:-100"));
    assert_eq!(failed.modes.unwrap()[0].stars, Some(36));
    assert!(failed.fetched_at.is_some() && failed.fetched_at < fetched_at);
    let row = stored(&fixture.db, account.id).await;
    assert_eq!(row.error_code.as_deref(), Some("upstream:-100"));
    let link = hoyolab_account::Entity::find_by_id(link.id)
        .one(&fixture.db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(link.auth_status, "cookie_invalid");
}

#[tokio::test]
async fn a_private_record_backs_off_without_a_payload() {
    let fixture = TestDatabase::new().await;
    let actor = ready_identity(&fixture.db, "owner").await;
    let key = key();
    let link = verified_link(&fixture.db, &key, actor.user.id, "123").await;
    let account = game(&fixture.db, link.id, "genshin", "800000001", true, true).await;
    let private = || Err(UpstreamError::Upstream { code: 10102 });
    let gateway = MockGateway::with_challenges([private(), private(), private()]);
    let games = service(fixture.db.clone(), key, gateway.clone());

    let first = games.challenges(&actor, account.id).await.unwrap();
    assert_eq!(gateway.challenge_calls().len(), 3);
    assert!(first.stale && first.modes.is_none() && first.fetched_at.is_none());
    assert_eq!(first.error_code.as_deref(), Some("upstream:10102"));

    let again = games.challenges(&actor, account.id).await.unwrap();
    assert_eq!(gateway.challenge_calls().len(), 3);
    assert_eq!(again.error_code.as_deref(), Some("upstream:10102"));
    // A private record only concerns this game; the link stays verified.
    let link = hoyolab_account::Entity::find_by_id(link.id)
        .one(&fixture.db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(link.auth_status, "verified");
}
