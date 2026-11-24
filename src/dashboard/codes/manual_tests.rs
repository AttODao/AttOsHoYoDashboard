use super::*;
use crate::dashboard::{
    codes::test_support::{game, link, ready_user, redeem_link},
    db::entities::{redemption_code, redemption_code_rejection, redemption_code_source},
    test_support::TestDatabase,
};
use sea_orm::{ActiveModelTrait, PaginatorTrait};

#[test]
fn budgets_are_per_user_and_operation_and_reset_after_their_window() {
    let throttle = Throttle::default();
    let start = Instant::now();
    let user = Uuid::new_v4();
    for _ in 0..3 {
        throttle.admit(user, Operation::RedeemNow, start).unwrap();
    }
    assert!(matches!(
        throttle.admit(user, Operation::RedeemNow, start),
        Err(Error::Throttled)
    ));
    // Another operation and another user have their own budgets.
    throttle.admit(user, Operation::EnterCode, start).unwrap();
    throttle
        .admit(Uuid::new_v4(), Operation::RedeemNow, start)
        .unwrap();
    // The window passes.
    throttle
        .admit(
            user,
            Operation::RedeemNow,
            start + Duration::from_secs(10 * 60),
        )
        .unwrap();
    for _ in 1..10 {
        throttle.admit(user, Operation::EnterCode, start).unwrap();
    }
    assert!(matches!(
        throttle.admit(user, Operation::EnterCode, start),
        Err(Error::Throttled)
    ));
}

#[tokio::test]
async fn entered_codes_are_validated_private_and_queued_for_each_uid_of_the_game() {
    let fixture = TestDatabase::new().await;
    let db = &fixture.db;
    let actor = ready_user(db, "enterer").await;
    // No link yet, then a link without redemption credentials.
    assert!(matches!(
        enter_code(db, &actor, "genshin", "GENSHINGIFT", None).await,
        Err(Error::InitialSetupRequired)
    ));
    let plain_owner = ready_user(db, "plain").await;
    let plain = link(db, plain_owner.user.id, "100000009", "verified").await;
    game(db, plain, "genshin", "800000009").await;
    assert!(matches!(
        enter_code(db, &plain_owner, "genshin", "GENSHINGIFT", None).await,
        Err(Error::Invalid(NEEDS_REDEMPTION_COOKIE))
    ));
    let owner_link = redeem_link(db, actor.user.id, "100000001").await;
    let first = game(db, owner_link, "genshin", "800000001").await;
    let second = game(db, owner_link, "genshin", "800000002").await;
    let rail = game(db, owner_link, "starrail", "900000001").await;
    for (game_kind, code) in [
        ("nap", "GENSHINGIFT"),
        ("genshin", "abc"),
        ("genshin", "has space"),
        ("genshin", "TOOLONGTOOLONGTOOLONGTOOLONGTOOLONG"),
        ("genshin", "記号"),
    ] {
        assert!(
            matches!(
                enter_code(db, &actor, game_kind, code, None).await,
                Err(Error::Invalid(_))
            ),
            "{game_kind} {code}"
        );
    }
    // Lower case is normalized as catalogue rows are; every Genshin UID gets it.
    // One chosen UID gets it alone; a UID of another game or of nobody is not found.
    let one = enter_code(db, &actor, "genshin", "ONLYSECOND", Some(second))
        .await
        .unwrap();
    assert_eq!(
        one.iter()
            .map(|entry| (entry.game_account_id, entry.state.as_str()))
            .collect::<Vec<_>>(),
        [(second, "pending")]
    );
    assert!(
        redemption_ledger::Entity::find_by_id((first, "ONLYSECOND".to_owned()))
            .one(db)
            .await
            .unwrap()
            .is_none()
    );
    for other in [rail, Uuid::from_u128(77)] {
        assert!(matches!(
            enter_code(db, &actor, "genshin", "ONLYSECOND", Some(other)).await,
            Err(Error::NotFound)
        ));
    }
    redemption_ledger::Entity::delete_many()
        .filter(redemption_ledger::Column::Code.eq("ONLYSECOND"))
        .exec(db)
        .await
        .unwrap();
    let entered = enter_code(db, &actor, "genshin", "  genshingift ", None)
        .await
        .unwrap();
    let states: Vec<_> = entered
        .iter()
        .map(|entry| {
            (
                entry.game_account_id,
                entry.code.as_str(),
                entry.state.as_str(),
            )
        })
        .collect();
    assert_eq!(
        states,
        [
            (first, "GENSHINGIFT", "pending"),
            (second, "GENSHINGIFT", "pending")
        ]
    );
    let rows = redemption_ledger::Entity::find().all(db).await.unwrap();
    assert!(
        rows.iter()
            .all(|row| row.source == MANUAL && row.posts == 0)
    );
    assert_eq!(
        redemption_code::Entity::find().count(db).await.unwrap(),
        0,
        "never added to the shared catalogue"
    );
    // Entering it again changes nothing; a dead code is settled without a request.
    enter_code(db, &actor, "genshin", "GENSHINGIFT", None)
        .await
        .unwrap();
    assert_eq!(
        redemption_ledger::Entity::find().count(db).await.unwrap(),
        2
    );
    redemption_code_rejection::ActiveModel {
        game_kind: Set("starrail".to_owned()),
        code: Set("DEADRAIL".to_owned()),
        reason: Set("expired".to_owned()),
        rejected_at: Set(Utc::now()),
    }
    .insert(db)
    .await
    .unwrap();
    let dead = enter_code(db, &actor, "starrail", "DEADRAIL", None)
        .await
        .unwrap();
    assert_eq!(dead[0].state, "expired");
    // A rank-too-low answer can be retried by entering the code once its day passed.
    redemption_ledger::Entity::update_many()
        .col_expr(
            redemption_ledger::Column::State,
            Expr::value("rank_too_low"),
        )
        .col_expr(redemption_ledger::Column::Posts, Expr::value(1))
        .col_expr(
            redemption_ledger::Column::LastSentAt,
            Expr::value(Some(Utc::now())),
        )
        .col_expr(
            redemption_ledger::Column::NextAttemptAt,
            Expr::value(Some(Utc::now() - chrono::Duration::minutes(1))),
        )
        .filter(redemption_ledger::Column::GameAccountId.eq(first))
        .exec(db)
        .await
        .unwrap();
    let again = enter_code(db, &actor, "genshin", "GENSHINGIFT", None)
        .await
        .unwrap();
    assert_eq!(again[0].state, "pending");
    // A user without a UID of that game gets not found; a stopped feature is unavailable.
    let rail_only = ready_user(db, "rail-only").await;
    let rail_link = redeem_link(db, rail_only.user.id, "100000002").await;
    game(db, rail_link, "starrail", "900000002").await;
    assert!(matches!(
        enter_code(db, &rail_only, "genshin", "GENSHINGIFT", None).await,
        Err(Error::NotFound)
    ));
    redemption_code_source::Entity::update_many()
        .col_expr(redemption_code_source::Column::Enabled, Expr::value(false))
        .exec(db)
        .await
        .unwrap();
    assert!(matches!(
        enter_code(db, &actor, "genshin", "OTHERGIFT", None).await,
        Err(Error::Unavailable(_))
    ));
    assert!(matches!(
        redeem_now(db, &actor).await,
        Err(Error::Unavailable(_))
    ));
}

#[tokio::test]
async fn the_view_reports_state_without_secrets_and_redeem_now_ends_a_pause() {
    let fixture = TestDatabase::new().await;
    let db = &fixture.db;
    let actor = ready_user(db, "viewer").await;
    let empty = serde_json::to_value(view(db, &actor, true, false).await.unwrap()).unwrap();
    assert_eq!(
        empty,
        serde_json::json!({
            "available": true,
            "redemption_credentials": false,
            "action_required": null,
            "action_at": null,
            "action_retcode": null,
            "cookie_verified_at": null,
            "action_after_cookie_update": false,
            "running": false,
            "pending": 0,
        })
    );
    let owner_link = redeem_link(db, actor.user.id, "100000001").await;
    let account = game(db, owner_link, "genshin", "800000001").await;
    enter_code(db, &actor, "genshin", "GENSHINGIFT", None)
        .await
        .unwrap();
    redemption_ledger::Entity::update_many()
        .col_expr(
            redemption_ledger::Column::State,
            Expr::value("cookie_invalid"),
        )
        .col_expr(redemption_ledger::Column::Posts, Expr::value(1))
        .col_expr(redemption_ledger::Column::Retcode, Expr::value(Some(-1071)))
        .col_expr(
            redemption_ledger::Column::LastSentAt,
            Expr::value(Some(Utc::now())),
        )
        .filter(redemption_ledger::Column::GameAccountId.eq(account))
        .exec(db)
        .await
        .unwrap();
    hoyolab_account::Entity::update_many()
        .col_expr(
            hoyolab_account::Column::RedemptionAction,
            Expr::value(Some("cookie_invalid")),
        )
        .col_expr(
            hoyolab_account::Column::RedemptionActionAt,
            Expr::value(Some(Utc::now())),
        )
        .filter(hoyolab_account::Column::Id.eq(owner_link))
        .exec(db)
        .await
        .unwrap();
    let paused = serde_json::to_value(view(db, &actor, true, true).await.unwrap()).unwrap();
    assert_eq!(paused["redemption_credentials"], true);
    assert_eq!(paused["action_required"], "cookie_invalid");
    assert!(paused["action_at"].is_string());
    // The number of the answer that paused, and that it came right after the Cookie was
    // verified (the fixture verified it just now).
    assert_eq!(paused["action_retcode"], -1071);
    assert!(paused["cookie_verified_at"].is_string());
    assert_eq!(paused["action_after_cookie_update"], true);
    assert_eq!(paused["running"], true);
    assert_eq!(paused["pending"], 0);
    let text = paused.to_string();
    assert!(!text.contains("syntheticcookietoken"), "{text}");
    redeem_now(db, &actor).await.unwrap();
    let resumed = serde_json::to_value(view(db, &actor, false, false).await.unwrap()).unwrap();
    assert_eq!(resumed["action_required"], serde_json::Value::Null);
    assert_eq!(resumed["pending"], 1);
    assert_eq!(
        resumed["available"], false,
        "no Cookie storage, no redemption"
    );
}

#[test]
fn a_pause_soon_after_registering_the_cookie_reads_as_the_new_cookie_refused() {
    let registered = Utc::now() - chrono::Duration::days(3);
    let at = |minutes: i64| Some(registered + chrono::Duration::minutes(minutes));
    assert!(after_cookie_update(at(1), Some(registered)));
    assert!(after_cookie_update(at(60), Some(registered)));
    assert!(!after_cookie_update(at(61), Some(registered)));
    // A pause older than the Cookie (cleared by registering) or without either time.
    assert!(!after_cookie_update(at(-1), Some(registered)));
    assert!(!after_cookie_update(None, Some(registered)));
    assert!(!after_cookie_update(at(1), None));
}
