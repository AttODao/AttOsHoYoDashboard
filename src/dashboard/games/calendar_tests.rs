use super::service_tests::{MockGateway, game, key, ready_identity, service, verified_link};
use crate::dashboard::{
    db::entities::{game_calendar_cache, hoyolab_account, reward_setting},
    error::Error,
    test_support::TestDatabase,
};
use attoshoyodashboard::{
    game::{Game, Genshin, Starrail},
    hoyolab::Error as UpstreamError,
};
use chrono::{DateTime, Datelike, Duration, FixedOffset, SubsecRound, Timelike, Utc};
use sea_orm::{ActiveModelTrait, EntityTrait, Set};
use serde_json::{Value, json};
use uuid::Uuid;

/// A Star Rail calendar with one warp and one event ending at `end`.
fn starrail(end: DateTime<Utc>, progress: i64) -> Value {
    let time = json!({
        "start_ts": (end - Duration::days(21)).timestamp().to_string(),
        "end_ts": end.timestamp().to_string(),
    });
    json!({
        "avatar_card_pool_list": [{
            "id": 1, "name": "Warp", "version": "3.7", "time_info": time,
            "avatar_list": [{"item_name": "Synthetic", "rarity": "5",
                "icon_url": "https://act-webstatic.hoyoverse.com/x.png"}]
        }],
        "act_list": [{
            "id": 7, "name": "Event", "time_info": time, "act_time_type": "ActTimeTypeDefault",
            "act_status": "OtherActStatusUnFinish", "all_finished": false,
            "current_progress": progress, "total_progress": 5,
            "panel_desc": "private description", "reward_list": [{"item_id": 1}]
        }],
        "challenge_list": [{"group_id": 1, "name_mi18n": "Memory of Chaos"}]
    })
}

async fn stored(db: &sea_orm::DatabaseConnection, id: Uuid) -> game_calendar_cache::Model {
    game_calendar_cache::Entity::find_by_id(id)
        .one(db)
        .await
        .unwrap()
        .unwrap()
}

async fn age_cache(
    db: &sea_orm::DatabaseConnection,
    id: Uuid,
    fetched_at: DateTime<Utc>,
    payload: Option<Value>,
) {
    let mut row: game_calendar_cache::ActiveModel = stored(db, id).await.into();
    row.fetched_at = Set(Some(fetched_at));
    if let Some(payload) = payload {
        row.payload = Set(Some(payload));
    }
    row.update(db).await.unwrap();
}

#[tokio::test]
async fn the_calendar_is_cached_refetched_at_boundaries_and_kept_when_a_refresh_fails() {
    let fixture = TestDatabase::new().await;
    let actor = ready_identity(&fixture.db, "owner").await;
    let other = ready_identity(&fixture.db, "other").await;
    let key = key();
    let link = verified_link(&fixture.db, &key, actor.user.id, "123").await;
    let other_link = verified_link(&fixture.db, &key, other.user.id, "456").await;
    let account = game(&fixture.db, link.id, "starrail", "900000001", true, true).await;
    let hidden = game(&fixture.db, link.id, "starrail", "900000002", false, true).await;
    let foreign = game(
        &fixture.db,
        other_link.id,
        "starrail",
        "900000003",
        true,
        true,
    )
    .await;
    let end = Utc::now() + Duration::days(3);
    let gateway = MockGateway::with_calendars([
        Ok(starrail(end, 2)),
        Ok(starrail(end + Duration::days(21), 0)),
        Ok(starrail(end, 4)),
        Err(UpstreamError::Network),
        Err(UpstreamError::Upstream { code: -100 }),
    ]);
    let games = service(fixture.db.clone(), key, gateway.clone());

    // Hidden and foreign accounts never reach upstream.
    let hidden_result = games.calendar(&actor, hidden.id).await.unwrap();
    assert!(hidden_result.items.is_none() && !hidden_result.visible);
    assert!(matches!(
        games.calendar(&actor, foreign.id).await,
        Err(Error::NotFound)
    ));
    assert!(gateway.calendar_calls().is_empty());

    let result = games.calendar(&actor, account.id).await.unwrap();
    assert_eq!(gateway.calendar_calls(), [Starrail::KIND]);
    assert!(!result.stale && result.error_code.is_none());
    let items = result.items.clone().unwrap();
    // The endgame list is left to the endgame summary.
    assert_eq!(
        items
            .iter()
            .map(|item| item.kind.as_str())
            .collect::<Vec<_>>(),
        ["character_banner", "event"]
    );
    assert_eq!(items[1].progress, Some(2));
    assert_eq!(
        items[0].ends_at.map(|at| at.timestamp()),
        Some(end.timestamp())
    );
    let row = stored(&fixture.db, account.id).await;
    assert_eq!(row.payload_version, 2);
    let stored_text = row.payload.as_ref().unwrap().to_string();
    assert!(!stored_text.contains("private description") && !stored_text.contains("item_id"));
    assert!(!format!("{result:?}").contains("Synthetic"));

    // Within the TTL the cache is served.
    let cached = games.calendar(&actor, account.id).await.unwrap();
    assert_eq!(cached.items, result.items);
    assert_eq!(gateway.calendar_calls().len(), 1);

    // An item that ended after the fetch triggers a refetch before the TTL runs out.
    let mut payload = row.payload.clone().unwrap();
    payload[0]["ends_at"] = json!((Utc::now() - Duration::minutes(1)).to_rfc3339());
    let ten_minutes_ago = Utc::now() - Duration::minutes(10);
    age_cache(&fixture.db, account.id, ten_minutes_ago, Some(payload)).await;
    let next = games.calendar(&actor, account.id).await.unwrap();
    assert_eq!(gateway.calendar_calls().len(), 2);
    assert_eq!(next.items.as_ref().unwrap()[1].progress, Some(0));

    // So does an announced item that started after the fetch.
    let mut payload = stored(&fixture.db, account.id).await.payload.unwrap();
    payload[0]["starts_at"] = json!((Utc::now() - Duration::minutes(1)).to_rfc3339());
    age_cache(&fixture.db, account.id, ten_minutes_ago, Some(payload)).await;
    games.calendar(&actor, account.id).await.unwrap();
    assert_eq!(gateway.calendar_calls().len(), 3);

    // After the TTL a failed refresh keeps the last calendar and records why.
    age_cache(
        &fixture.db,
        account.id,
        Utc::now() - Duration::minutes(61),
        None,
    )
    .await;
    let failed = games.calendar(&actor, account.id).await.unwrap();
    assert_eq!(gateway.calendar_calls().len(), 4);
    assert!(failed.stale);
    assert_eq!(failed.error_code.as_deref(), Some("network"));
    assert_eq!(failed.items.unwrap()[1].progress, Some(4));
    assert!(failed.fetched_at.is_some());

    // A failed refresh backs off for the status TTL instead of asking on every poll.
    let again = games.calendar(&actor, account.id).await.unwrap();
    assert_eq!(gateway.calendar_calls().len(), 4);
    assert!(again.stale && again.items.is_some());
    assert_eq!(again.error_code.as_deref(), Some("network"));

    // After it, the next attempt runs; an invalid Cookie marks the link.
    let mut row: game_calendar_cache::ActiveModel = stored(&fixture.db, account.id).await.into();
    row.error_at = Set(Some(Utc::now() - Duration::minutes(6)));
    row.update(&fixture.db).await.unwrap();
    let rejected = games.calendar(&actor, account.id).await.unwrap();
    assert_eq!(gateway.calendar_calls().len(), 5);
    assert_eq!(rejected.error_code.as_deref(), Some("upstream:-100"));
    assert!(rejected.items.is_some());
    let link = hoyolab_account::Entity::find_by_id(link.id)
        .one(&fixture.db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(link.auth_status, "cookie_invalid");
}

#[tokio::test]
async fn private_records_back_off_and_action_required_games_are_not_fetched() {
    let fixture = TestDatabase::new().await;
    let actor = ready_identity(&fixture.db, "owner").await;
    let key = key();
    let link = verified_link(&fixture.db, &key, actor.user.id, "123").await;
    let genshin = game(&fixture.db, link.id, "genshin", "800000001", true, true).await;
    let starrail = game(&fixture.db, link.id, "starrail", "900000001", true, true).await;
    reward_setting::ActiveModel {
        id: Set(Uuid::new_v4()),
        hoyolab_account_id: Set(link.id),
        game_kind: Set("starrail".to_owned()),
        enabled: Set(true),
        local_time: Set("06:00".to_owned()),
        auth_status: Set("action_required".to_owned()),
        next_run_at: Set(None),
    }
    .insert(&fixture.db)
    .await
    .unwrap();
    let gateway = MockGateway::with_calendars([Err(UpstreamError::Upstream { code: 10102 })]);
    let games = service(fixture.db.clone(), key, gateway.clone());

    let first = games.calendar(&actor, genshin.id).await.unwrap();
    assert_eq!(gateway.calendar_calls(), [Genshin::KIND]);
    assert!(first.stale && first.items.is_none() && first.fetched_at.is_none());
    assert_eq!(first.error_code.as_deref(), Some("upstream:10102"));
    let again = games.calendar(&actor, genshin.id).await.unwrap();
    assert_eq!(gateway.calendar_calls().len(), 1);
    assert_eq!(again.error_code.as_deref(), Some("upstream:10102"));

    let blocked = games.calendar(&actor, starrail.id).await.unwrap();
    assert_eq!(gateway.calendar_calls().len(), 1);
    assert!(blocked.stale && blocked.items.is_none());
    assert_eq!(blocked.error_code.as_deref(), Some("action_required"));
    // A private record only concerns this game; the link stays verified.
    let link = hoyolab_account::Entity::find_by_id(link.id)
        .one(&fixture.db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(link.auth_status, "verified");
}

/// A Genshin date mapping (genshin.py's `DateTime`) in the server time of `os_usa`, the
/// region of the test accounts.
fn usa_mapping(at: DateTime<Utc>) -> Value {
    let local = at.with_timezone(&FixedOffset::west_opt(5 * 3600).unwrap());
    json!({"year": local.year(), "month": local.month(), "day": local.day(),
           "hour": local.hour(), "minute": local.minute(), "second": local.second()})
}

#[tokio::test]
async fn calendars_summarized_by_an_older_version_are_fetched_again_within_the_ttl() {
    let fixture = TestDatabase::new().await;
    let actor = ready_identity(&fixture.db, "owner").await;
    let key = key();
    let link = verified_link(&fixture.db, &key, actor.user.id, "123").await;
    let account = game(&fixture.db, link.id, "genshin", "800000001", true, true).await;
    let start = (Utc::now() - Duration::days(3)).trunc_subsecs(0);
    let end = start + Duration::days(10);
    let gateway = MockGateway::with_calendars([Ok(json!({
        "act_list": [{"id": 9, "name": "Synthetic Record", "start_timestamp": "0",
                      "end_timestamp": "0", "start_time": usa_mapping(start),
                      "end_time": usa_mapping(end), "countdown_seconds": 0,
                      "is_finished": false}]
    }))]);
    // A fresh calendar stored by 2.1.0 (version 1), whose summary lost the event's period.
    let fetched_at = Utc::now() - Duration::minutes(1);
    game_calendar_cache::ActiveModel {
        game_account_id: Set(account.id),
        fetched_at: Set(Some(fetched_at)),
        updated_at: Set(fetched_at),
        payload: Set(Some(
            json!([{"kind": "event", "id": 9, "name": "Synthetic Record",
            "version": null, "starts_at": null, "ends_at": null, "finished": false,
            "locked": null, "progress": null, "progress_total": null, "featured": []}]),
        )),
        payload_version: Set(1),
        error_code: Set(None),
        error_at: Set(None),
    }
    .insert(&fixture.db)
    .await
    .unwrap();
    let games = service(fixture.db.clone(), key, gateway.clone());

    let result = games.calendar(&actor, account.id).await.unwrap();
    assert_eq!(gateway.calendar_calls(), [Genshin::KIND]);
    let items = result.items.unwrap();
    assert_eq!(
        (items[0].starts_at, items[0].ends_at),
        (Some(start), Some(end))
    );
    assert_eq!(stored(&fixture.db, account.id).await.payload_version, 2);
    // The refreshed calendar is served from the cache again.
    games.calendar(&actor, account.id).await.unwrap();
    assert_eq!(gateway.calendar_calls().len(), 1);
}
