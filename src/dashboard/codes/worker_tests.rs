use super::*;
use crate::dashboard::{
    codes::test_support::{MockSource, game, link, ready_user},
    db::entities::{
        app_device, app_notification, notification_setting, redemption_code,
        redemption_code_result, redemption_code_setting,
    },
    test_support::TestDatabase,
};
use sea_orm::{ActiveModelTrait, QueryOrder, Set};
use uuid::Uuid;

#[test]
fn runs_wait_the_interval_after_success_and_back_off_after_failures() {
    assert_eq!(delay(180, 0), Duration::minutes(180));
    assert_eq!(delay(180, 1), Duration::minutes(15));
    assert_eq!(delay(180, 2), Duration::minutes(30));
    assert_eq!(delay(180, 4), Duration::minutes(120));
    assert_eq!(
        delay(180, 5),
        Duration::minutes(180),
        "never longer than the interval"
    );
    assert_eq!(delay(60, 40), Duration::minutes(60));
    let now = Utc::now();
    let mut state = redemption_code_source::Model {
        id: 1,
        enabled: true,
        interval_minutes: 180,
        last_attempt_at: None,
        last_success_at: None,
        consecutive_failures: 0,
        last_error: None,
        updated_at: now,
    };
    assert_eq!(next_fetch(&state, now), Some(now), "never fetched: due now");
    state.last_attempt_at = Some(now);
    assert_eq!(next_fetch(&state, now), Some(now + Duration::minutes(180)));
    state.consecutive_failures = 1;
    assert_eq!(next_fetch(&state, now), Some(now + Duration::minutes(15)));
    state.enabled = false;
    assert_eq!(next_fetch(&state, now), None, "stopped: never due");
}

async fn worker(db: &DatabaseConnection, source: &MockSource) -> CodeWorker {
    CodeWorker::new(
        db.clone(),
        Source::new(source.url.clone()),
        Arc::new(Notifications::new(
            db.clone(),
            None,
            "https://ahd.example.test",
        )),
    )
}

/// A user whose app installation receives every personal notice (no Webhook, no network).
async fn app_user(db: &DatabaseConnection, name: &str) -> Uuid {
    let user = ready_user(db, name).await.user.id;
    app_device::ActiveModel {
        id: Set(Uuid::new_v4()),
        user_id: Set(user),
        created_at: Set(Utc::now()),
        last_seen_at: Set(Utc::now()),
        push_subscription: Set(None),
    }
    .insert(db)
    .await
    .unwrap();
    user
}

async fn set_mode(db: &DatabaseConnection, account: Uuid, mode: &str, since: DateTime<Utc>) {
    redemption_code_setting::ActiveModel {
        game_account_id: Set(account),
        mode: Set(mode.to_owned()),
        enabled_at: Set((mode != "off").then_some(since)),
        updated_at: Set(since),
    }
    .insert(db)
    .await
    .unwrap();
}

async fn inbox(db: &DatabaseConnection, user: Uuid) -> Vec<app_notification::Model> {
    app_notification::Entity::find()
        .filter(app_notification::Column::UserId.eq(user))
        .order_by_asc(app_notification::Column::Id)
        .all(db)
        .await
        .unwrap()
}

async fn results(db: &DatabaseConnection, account: Uuid) -> Vec<String> {
    let mut codes = Vec::new();
    for row in redemption_code_result::Entity::find()
        .filter(redemption_code_result::Column::GameAccountId.eq(account))
        .all(db)
        .await
        .unwrap()
    {
        assert_eq!(row.state, "announced");
        let code = redemption_code::Entity::find_by_id(row.code_id)
            .one(db)
            .await
            .unwrap()
            .unwrap();
        codes.push(code.code);
    }
    codes.sort();
    codes
}

#[tokio::test]
async fn runs_follow_the_schedule_and_failures_keep_known_codes() {
    let fixture = TestDatabase::new().await;
    let db = &fixture.db;
    let source = MockSource::start().await;
    source.list("genshin", &[("GENSHINGIFT", "Primogem*60")]);
    source.list("hkrpg", &[("STARRAILGIFT", "50 stellar jade")]);
    let worker = worker(db, &source).await;
    let start = Utc::now();

    assert!(worker.tick(start).await.unwrap());
    assert_eq!(source.requests(), ["genshin", "hkrpg"]);
    let state = source_state(db).await.unwrap();
    assert_eq!(
        state.last_success_at.map(|at| at.timestamp()),
        Some(start.timestamp())
    );
    assert_eq!(state.consecutive_failures, 0);
    assert_eq!(
        redemption_code::Entity::find().all(db).await.unwrap().len(),
        2
    );

    // Not due again before the interval.
    assert!(!worker.tick(start + Duration::minutes(179)).await.unwrap());
    assert_eq!(source.requests().len(), 2);

    // A failed game keeps its codes and only records the error class.
    source.answer("genshin", 503, "synthetic-upstream-error-body");
    let failed_at = start + Duration::minutes(180);
    assert!(worker.tick(failed_at).await.unwrap());
    let state = source_state(db).await.unwrap();
    assert_eq!(state.consecutive_failures, 1);
    assert_eq!(state.last_error.as_deref(), Some("http_status"));
    assert_eq!(
        state.last_success_at.map(|at| at.timestamp()),
        Some(start.timestamp())
    );
    let genshin = redemption_code::Entity::find()
        .filter(redemption_code::Column::GameKind.eq("genshin"))
        .one(db)
        .await
        .unwrap()
        .unwrap();
    assert!(
        genshin.listed,
        "a failed fetch never unlists or deletes codes"
    );

    // Backoff: the retry waits 15 minutes, not the whole interval.
    assert!(
        !worker
            .tick(failed_at + Duration::minutes(14))
            .await
            .unwrap()
    );
    source.answer("genshin", 200, "not json");
    assert!(
        worker
            .tick(failed_at + Duration::minutes(15))
            .await
            .unwrap()
    );
    let state = source_state(db).await.unwrap();
    assert_eq!(state.consecutive_failures, 2);
    assert_eq!(state.last_error.as_deref(), Some("invalid_payload"));
    source.list("genshin", &[("GENSHINGIFT", "Primogem*60")]);
    assert!(
        !worker
            .tick(failed_at + Duration::minutes(44))
            .await
            .unwrap()
    );
    assert!(
        worker
            .tick(failed_at + Duration::minutes(45))
            .await
            .unwrap()
    );
    let state = source_state(db).await.unwrap();
    assert_eq!(state.consecutive_failures, 0);
    assert_eq!(state.last_error, None);

    // The administrator's switch stops runs entirely.
    redemption_code_source::Entity::update_many()
        .col_expr(redemption_code_source::Column::Enabled, Expr::value(false))
        .exec(db)
        .await
        .unwrap();
    let requests = source.requests().len();
    assert!(!worker.tick(failed_at + Duration::days(2)).await.unwrap());
    assert_eq!(source.requests().len(), requests);

    let fetches = worker.stats.fetches();
    let count = |game: &str, outcome: &str| {
        fetches
            .iter()
            .find(|(g, o, _)| *g == game && *o == outcome)
            .map(|(_, _, n)| *n)
            .unwrap()
    };
    assert_eq!(fetches.len(), 2 * Stats::OUTCOMES.len());
    assert_eq!(count("genshin", "ok"), 2);
    assert_eq!(count("genshin", "http_status"), 1);
    assert_eq!(count("genshin", "invalid_payload"), 1);
    assert_eq!(count("starrail", "ok"), 4);
    assert_eq!(count("starrail", "timeout"), 0);
}

#[tokio::test]
async fn rejected_rows_are_counted_per_game() {
    let fixture = TestDatabase::new().await;
    let db = &fixture.db;
    let source = MockSource::start().await;
    source.answer(
        "genshin",
        200,
        r#"{"codes": [{"code": "GOODCODE1"}, {"code": "bad code"}, {"code": "X"}]}"#,
    );
    source.list("hkrpg", &[]);
    let worker = worker(db, &source).await;
    assert!(worker.tick(Utc::now()).await.unwrap());
    assert_eq!(worker.stats.rejected(), [("genshin", 2), ("starrail", 0)]);
}

#[tokio::test]
async fn notify_only_sends_one_summary_per_run_and_never_announces_older_codes() {
    let fixture = TestDatabase::new().await;
    let db = &fixture.db;
    let source = MockSource::start().await;
    source.list("genshin", &[("SEEDCODE1", "Primogem*60")]);
    source.list("hkrpg", &[("SEEDRAIL1", "")]);
    let worker = worker(db, &source).await;
    let start = Utc::now();

    // Alice: notify on a Genshin and a Star Rail UID, plus a second Genshin UID.
    let alice = app_user(db, "alice").await;
    let alice_link = link(db, alice, "100000001", "verified").await;
    let alice_genshin = game(db, alice_link, "genshin", "800000001").await;
    let alice_genshin_2 = game(db, alice_link, "genshin", "800000002").await;
    let alice_rail = game(db, alice_link, "starrail", "900000001").await;
    set_mode(db, alice_genshin, "notify", start - Duration::days(1)).await;
    set_mode(db, alice_genshin_2, "notify", start - Duration::days(1)).await;
    set_mode(db, alice_rail, "notify", start - Duration::days(1)).await;
    // Bob: a UID left off. Carol: notify, but her Cookie stopped working.
    let bob = app_user(db, "bob").await;
    let bob_link = link(db, bob, "100000002", "verified").await;
    game(db, bob_link, "genshin", "800000003").await;
    let carol = app_user(db, "carol").await;
    let carol_link = link(db, carol, "100000003", "cookie_invalid").await;
    let carol_genshin = game(db, carol_link, "genshin", "800000004").await;
    set_mode(db, carol_genshin, "notify", start - Duration::days(1)).await;

    // The first run seeds the catalogue: nothing is announced.
    assert!(worker.tick(start).await.unwrap());
    assert!(inbox(db, alice).await.is_empty());
    assert_eq!(worker.stats.announcements(), 0);

    // Two new Genshin codes and one new Star Rail code: one summary for Alice.
    let second = start + Duration::minutes(180);
    source.list(
        "genshin",
        &[
            ("SEEDCODE1", "Primogem*60"),
            ("NEWCODE1", "Primogem*60"),
            ("NEWCODE2", ""),
        ],
    );
    source.list(
        "hkrpg",
        &[("SEEDRAIL1", ""), ("NEWRAIL1", "60 stellar jade")],
    );
    assert!(worker.tick(second).await.unwrap());
    let notices = inbox(db, alice).await;
    assert_eq!(notices.len(), 1, "one summary per run, not one per code");
    assert_eq!(notices[0].kind, "code_available");
    assert_eq!(notices[0].url, "/settings/codes");
    assert_eq!(notices[0].title, "新しい交換コードが3件あります");
    for expected in [
        "原神: NEWCODE1 (Primogem×60)\nhttps://genshin.hoyoverse.com/en/gift?code=NEWCODE1",
        "https://genshin.hoyoverse.com/en/gift?code=NEWCODE2",
        "崩壊：スターレイル: NEWRAIL1 (60 stellar jade)\nhttps://hsr.hoyoverse.com/gift?code=NEWRAIL1",
    ] {
        assert!(notices[0].body.contains(expected), "{}", notices[0].body);
    }
    assert!(!notices[0].body.contains("SEED"));
    assert_eq!(results(db, alice_genshin).await, ["NEWCODE1", "NEWCODE2"]);
    assert_eq!(results(db, alice_genshin_2).await, ["NEWCODE1", "NEWCODE2"]);
    assert_eq!(results(db, alice_rail).await, ["NEWRAIL1"]);
    assert!(inbox(db, bob).await.is_empty(), "mode off");
    assert!(inbox(db, carol).await.is_empty(), "only verified links");
    assert_eq!(worker.stats.announcements(), 1);

    // Nothing new: no further notice.
    let third = second + Duration::minutes(180);
    assert!(worker.tick(third).await.unwrap());
    assert_eq!(inbox(db, alice).await.len(), 1);

    // Bob switches notify on now: codes already known stay unannounced ("available"),
    // and only a code first seen afterwards reaches him.
    set_mode(db, bob_link_account(db, bob_link).await, "notify", third).await;
    source.list(
        "genshin",
        &[
            ("SEEDCODE1", ""),
            ("NEWCODE1", ""),
            ("NEWCODE2", ""),
            ("LATECODE1", ""),
        ],
    );
    assert!(worker.tick(third + Duration::minutes(180)).await.unwrap());
    let bob_notices = inbox(db, bob).await;
    assert_eq!(bob_notices.len(), 1);
    assert!(bob_notices[0].body.contains("LATECODE1"));
    assert!(!bob_notices[0].body.contains("NEWCODE1"));
    let alice_notices = inbox(db, alice).await;
    assert_eq!(alice_notices.len(), 2);
    assert_eq!(alice_notices[1].title, "新しい交換コードが1件あります");
    assert_eq!(worker.stats.announcements(), 3);

    // Choosing not to get the kind in the app sends nothing and records no result.
    notification_setting::ActiveModel {
        user_id: Set(alice),
        webhook_url: Set(None),
        webhook_format: Set("discord".to_owned()),
        language: Set("en".to_owned()),
        events: Set(serde_json::json!({})),
        last_delivery_at: Set(None),
        last_delivery_ok: Set(None),
        updated_at: Set(Utc::now()),
        resin_warn_hours: Set(2),
        daily_reminder_time: Set("21:00".to_owned()),
        weekly_reminder_time: Set("15:00".to_owned()),
        app_events: Set(serde_json::json!({"code_available": false})),
    }
    .insert(db)
    .await
    .unwrap();
    source.list("genshin", &[("LASTCODE1", "")]);
    assert!(worker.tick(third + Duration::minutes(360)).await.unwrap());
    assert_eq!(inbox(db, alice).await.len(), 2);
    assert!(
        !results(db, alice_genshin)
            .await
            .contains(&"LASTCODE1".to_owned())
    );
    assert_eq!(inbox(db, bob).await.len(), 2);
}

async fn bob_link_account(db: &DatabaseConnection, link: Uuid) -> Uuid {
    crate::dashboard::db::entities::game_account::Entity::find()
        .filter(crate::dashboard::db::entities::game_account::Column::HoyolabAccountId.eq(link))
        .one(db)
        .await
        .unwrap()
        .unwrap()
        .id
}
