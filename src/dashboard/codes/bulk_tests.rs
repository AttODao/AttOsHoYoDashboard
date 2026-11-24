use super::*;
use crate::dashboard::{
    codes::{
        announce, catalog, manual,
        plan::switched_on,
        redeem::{RedeemTuning, Redeemer},
        source::{Listing, SourceCode},
        test_support::{
            MockRedeem, answer, fast_tuning, game, link, ready_user, redeem_link, test_key,
        },
    },
    db::entities::{
        app_device, app_notification, hoyolab_account, redemption_code_setting,
        redemption_code_source,
    },
    hoyolab::store::CookieStore,
    notifications::channel::Notifications,
    test_support::TestDatabase,
};
use chrono::{DateTime, Duration};
use sea_orm::{ActiveModelTrait, PaginatorTrait, sea_query::Expr};
use std::sync::Arc;

fn listing(codes: &[&str]) -> Listing {
    Listing {
        codes: codes
            .iter()
            .map(|code| SourceCode {
                code: (*code).to_owned(),
                rewards: format!("Primogem*{}", code.len()),
            })
            .collect(),
        rejected: 0,
    }
}

/// Lists exactly `codes` for Genshin at `at` (earlier codes stay, unlisted).
async fn list(db: &DatabaseConnection, codes: &[&str], at: DateTime<Utc>) {
    catalog::store(db, "genshin", &listing(codes), at, false)
        .await
        .unwrap();
}

async fn set_mode(db: &DatabaseConnection, account: Uuid, mode: &str, since: DateTime<Utc>) {
    redemption_code_setting::Entity::delete_by_id(account)
        .exec(db)
        .await
        .unwrap();
    redemption_code_setting::ActiveModel {
        game_account_id: Set(account),
        mode: Set(mode.to_owned()),
        enabled_at: Set((mode != "off").then_some(since)),
        updated_at: Set(Utc::now()),
    }
    .insert(db)
    .await
    .unwrap();
}

async fn mode(db: &DatabaseConnection, account: Uuid) -> String {
    modes_in(db, &[account], false)
        .await
        .unwrap()
        .remove(&account)
        .unwrap()
}

async fn row(
    db: &DatabaseConnection,
    account: Uuid,
    code: &str,
) -> Option<redemption_ledger::Model> {
    redemption_ledger::Entity::find_by_id((account, code.to_owned()))
        .one(db)
        .await
        .unwrap()
}

fn redeemer(
    db: &DatabaseConnection,
    tuning: RedeemTuning,
) -> (MockRedeem, Arc<Redeemer<MockRedeem>>) {
    let store = Arc::new(CookieStore::new(db.clone(), Arc::new(test_key())));
    let notifications = Arc::new(Notifications::new(
        db.clone(),
        None,
        "https://ahd.example.test",
    ));
    let gateway = MockRedeem::new();
    let redeemer = Arc::new(Redeemer::with(
        store,
        gateway.clone(),
        notifications,
        tuning,
    ));
    (gateway, redeemer)
}

fn codes(count: usize) -> Vec<String> {
    (1..=count).map(|n| format!("CODE{n:02}")).collect()
}

fn refs(codes: &[String]) -> Vec<&str> {
    codes.iter().map(String::as_str).collect()
}

#[tokio::test]
async fn requests_queue_only_listed_codes_without_a_row_for_uids_outside_auto() {
    let fixture = TestDatabase::new().await;
    let db = &fixture.db;
    let now = Utc::now();
    let actor = ready_user(db, "bulk-owner").await;
    let owner_link = redeem_link(db, actor.user.id, "100000001").await;
    let notify = game(db, owner_link, "genshin", "800000001").await;
    let off = game(db, owner_link, "genshin", "800000002").await;
    let auto = game(db, owner_link, "genshin", "800000003").await;
    let rail = game(db, owner_link, "starrail", "900000001").await;
    set_mode(db, notify, "notify", now).await;
    set_mode(db, auto, "auto", now).await;
    let other = ready_user(db, "someone-else").await;
    let other_link = redeem_link(db, other.user.id, "100000002").await;
    let foreign = game(db, other_link, "genshin", "800000009").await;
    // Listed: four Genshin codes (one dead for everyone, one the notify UID already has)
    // and a Star Rail code; one code the source no longer lists.
    list(db, &["GONECODE"], now - Duration::hours(2)).await;
    list(
        db,
        &["DEADCODE", "HADCODE", "LISTED01", "LISTED02"],
        now - Duration::hours(1),
    )
    .await;
    catalog::store(db, "starrail", &listing(&["RAILCODE"]), now, false)
        .await
        .unwrap();
    super::super::ledger::reject_in(db, "genshin", "DEADCODE", "expired", now)
        .await
        .unwrap();
    manual::enter_code(db, &actor, "genshin", "HADCODE", Some(notify))
        .await
        .unwrap();

    // The view counts what a request would queue, per UID outside `auto`.
    let view = serde_json::to_value(manual::view(db, &actor, true, false).await.unwrap()).unwrap();
    let counts: Vec<(String, String, u64)> = view["bulk"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| {
            (
                row["game_uid"].as_str().unwrap().to_owned(),
                row["mode"].as_str().unwrap().to_owned(),
                row["codes"].as_u64().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        counts,
        [
            ("800000001".to_owned(), "notify".to_owned(), 2),
            ("800000002".to_owned(), "off".to_owned(), 3),
            ("900000001".to_owned(), "off".to_owned(), 1),
        ]
    );

    // Refused: an empty choice, an `auto` UID, a UID of another user or of nobody.
    assert!(matches!(
        enqueue(db, &actor, Some(Vec::new())).await,
        Err(Error::Invalid(_))
    ));
    assert!(matches!(
        enqueue(db, &actor, Some(vec![notify, auto])).await,
        Err(Error::Invalid(_))
    ));
    for id in [foreign, Uuid::new_v4()] {
        assert!(matches!(
            enqueue(db, &actor, Some(vec![notify, id])).await,
            Err(Error::NotFound)
        ));
    }
    assert_eq!(
        redemption_ledger::Entity::find()
            .filter(redemption_ledger::Column::Source.eq(BULK))
            .count(db)
            .await
            .unwrap(),
        0,
        "a refused request queues nothing"
    );

    // Every UID outside `auto`: listed codes without a row and not rejected.
    let summary = serde_json::to_value(enqueue(db, &actor, None).await.unwrap()).unwrap();
    let per_uid: Vec<(String, u64, u64, u64)> = summary["accounts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| {
            (
                row["game_uid"].as_str().unwrap().to_owned(),
                row["queued"].as_u64().unwrap(),
                row["already_recorded"].as_u64().unwrap(),
                row["rejected"].as_u64().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        per_uid,
        [
            ("800000001".to_owned(), 2, 1, 1),
            ("800000002".to_owned(), 3, 0, 1),
            ("900000001".to_owned(), 1, 0, 0),
        ]
    );
    assert_eq!(
        (
            summary["queued"].as_u64(),
            summary["already_recorded"].as_u64(),
            summary["rejected"].as_u64()
        ),
        (Some(6), Some(1), Some(2))
    );
    for (account, code) in [
        (notify, "LISTED01"),
        (notify, "LISTED02"),
        (off, "HADCODE"),
        (off, "LISTED01"),
        (off, "LISTED02"),
        (rail, "RAILCODE"),
    ] {
        let queued = row(db, account, code).await.unwrap();
        assert_eq!(
            (queued.source.as_str(), queued.state.as_str(), queued.posts),
            (BULK, PENDING, 0),
            "{code}"
        );
    }
    // The entered code keeps its row; nothing for `auto`, unlisted or dead codes.
    assert_eq!(row(db, notify, "HADCODE").await.unwrap().source, "manual");
    assert!(row(db, auto, "LISTED01").await.is_none());
    for account in [notify, off] {
        assert!(row(db, account, "GONECODE").await.is_none());
        assert!(row(db, account, "DEADCODE").await.is_none());
    }
    // Modes are unchanged, and the view now has nothing left to queue.
    assert_eq!(mode(db, notify).await, "notify");
    assert_eq!(mode(db, off).await, "off");
    assert_eq!(mode(db, auto).await, "auto");
    let view = serde_json::to_value(manual::view(db, &actor, true, false).await.unwrap()).unwrap();
    assert!(
        view["bulk"]
            .as_array()
            .unwrap()
            .iter()
            .all(|row| row["codes"] == 0)
    );
    assert_eq!(view["pending"], 7, "six queued and the entered code");
    // A repeated request queues nothing new.
    let again = serde_json::to_value(enqueue(db, &actor, Some(vec![off])).await.unwrap()).unwrap();
    assert_eq!(again["queued"], 0);
    assert_eq!(again["already_recorded"], 3);
}

#[tokio::test]
async fn requests_need_the_redemption_cookie_the_switch_and_no_pending_action() {
    let fixture = TestDatabase::new().await;
    let db = &fixture.db;
    let now = Utc::now();
    list(db, &["LISTED01"], now).await;
    // No link, then a link without redemption credentials.
    let actor = ready_user(db, "plain").await;
    assert!(matches!(
        enqueue(db, &actor, None).await,
        Err(Error::InitialSetupRequired)
    ));
    let plain = link(db, actor.user.id, "100000001", "verified").await;
    game(db, plain, "genshin", "800000001").await;
    assert!(matches!(
        enqueue(db, &actor, None).await,
        Err(Error::Invalid(_))
    ));
    // With them: an `auto`-only owner has nothing to choose.
    let owner = ready_user(db, "owner").await;
    let owner_link = redeem_link(db, owner.user.id, "100000002").await;
    let account = game(db, owner_link, "genshin", "800000002").await;
    set_mode(db, account, "auto", now).await;
    assert!(matches!(
        enqueue(db, &owner, None).await,
        Err(Error::NotFound)
    ));
    set_mode(db, account, "off", now).await;
    // A pause for the owner's action refuses the request.
    hoyolab_account::Entity::update_many()
        .col_expr(
            hoyolab_account::Column::RedemptionAction,
            Expr::value(Some("captcha_required")),
        )
        .col_expr(
            hoyolab_account::Column::RedemptionActionAt,
            Expr::value(Some(now)),
        )
        .filter(hoyolab_account::Column::Id.eq(owner_link))
        .exec(db)
        .await
        .unwrap();
    assert!(matches!(
        enqueue(db, &owner, None).await,
        Err(Error::Conflict)
    ));
    manual::redeem_now(db, &owner).await.unwrap();
    // The administrator's switch.
    redemption_code_source::Entity::update_many()
        .col_expr(redemption_code_source::Column::Enabled, Expr::value(false))
        .exec(db)
        .await
        .unwrap();
    assert!(!switched_on(db).await.unwrap());
    assert!(matches!(
        enqueue(db, &owner, None).await,
        Err(Error::Unavailable(_))
    ));
    redemption_code_source::Entity::update_many()
        .col_expr(redemption_code_source::Column::Enabled, Expr::value(true))
        .exec(db)
        .await
        .unwrap();
    assert_eq!(
        serde_json::to_value(enqueue(db, &owner, None).await.unwrap()).unwrap()["queued"],
        1
    );
}

#[test]
fn requests_have_their_own_budget_of_two_per_ten_minutes() {
    let throttle = manual::Throttle::default();
    let start = std::time::Instant::now();
    let user = Uuid::new_v4();
    for _ in 0..2 {
        throttle
            .admit(user, manual::Operation::RedeemListed, start)
            .unwrap();
    }
    assert!(matches!(
        throttle.admit(user, manual::Operation::RedeemListed, start),
        Err(Error::Throttled)
    ));
    throttle
        .admit(user, manual::Operation::RedeemNow, start)
        .unwrap();
    throttle
        .admit(
            user,
            manual::Operation::RedeemListed,
            start + std::time::Duration::from_secs(10 * 60),
        )
        .unwrap();
}

#[tokio::test]
async fn queued_codes_continue_over_runs_leave_with_the_list_and_are_never_sent_twice() {
    let fixture = TestDatabase::new().await;
    let db = &fixture.db;
    let now = Utc::now();
    let actor = ready_user(db, "long-list").await;
    let owner = actor.user.id;
    let owner_link = redeem_link(db, owner, "100000001").await;
    let account = game(db, owner_link, "genshin", "800000001").await;
    set_mode(db, account, "notify", now).await;
    let all = codes(13);
    list(db, &refs(&all), now - Duration::hours(1)).await;
    assert_eq!(
        serde_json::to_value(enqueue(db, &actor, None).await.unwrap()).unwrap()["queued"],
        13
    );
    let (gateway, redeemer) = redeemer(db, fast_tuning());
    // The first run stops at the cap of ten requests per account, oldest listing first.
    let run = redeemer.run_owner(owner).await.unwrap();
    assert_eq!((run.posts, run.redeemed), (10, 10));
    assert_eq!(gateway.codes(), all[..10]);
    assert_eq!(mode(db, account).await, "notify", "the mode stays");
    // CODE12 leaves the list before it is sent: its queued row goes with it.
    let mut rest = all.clone();
    rest.remove(11);
    list(db, &refs(&rest), now).await;
    assert!(row(db, account, "CODE12").await.is_none());
    // The next catalogue run continues without a new request.
    redeemer.run_every_owner().await.unwrap();
    assert_eq!(gateway.codes()[10..], ["CODE11", "CODE13"]);
    // Switching to `auto` later sends nothing twice.
    set_mode(db, account, "auto", now).await;
    assert_eq!(redeemer.run_owner(owner).await.unwrap().posts, 0);
    assert_eq!(gateway.sent().len(), 12);
    for code in &rest {
        let sent = row(db, account, code).await.unwrap();
        assert_eq!(
            (sent.source.as_str(), sent.state.as_str(), sent.posts),
            (BULK, "redeemed", 1),
            "{code}"
        );
    }
    // A code listed again is the catalogue's to send now that the UID is `auto`.
    list(db, &refs(&all), now).await;
    assert_eq!(redeemer.run_owner(owner).await.unwrap().posts, 1);
    assert_eq!(
        row(db, account, "CODE12").await.unwrap().source,
        "catalogue"
    );
}

#[tokio::test]
async fn an_invalid_cookie_pauses_the_queue_until_the_owner_retries() {
    let fixture = TestDatabase::new().await;
    let db = &fixture.db;
    let now = Utc::now();
    let actor = ready_user(db, "paused-queue").await;
    let owner = actor.user.id;
    let owner_link = redeem_link(db, owner, "100000001").await;
    let account = game(db, owner_link, "genshin", "800000001").await;
    let all = codes(4);
    list(db, &refs(&all), now).await;
    enqueue(db, &actor, None).await.unwrap();
    let (gateway, redeemer) = redeemer(db, fast_tuning());
    gateway.script("CODE02", [answer(-1071), answer(0)]);
    let run = redeemer.run_owner(owner).await.unwrap();
    assert_eq!((run.posts, run.action), (2, Some("cookie_invalid")));
    assert_eq!(
        row(db, account, "CODE02").await.unwrap().state,
        "cookie_invalid"
    );
    for code in ["CODE03", "CODE04"] {
        assert_eq!(row(db, account, code).await.unwrap().state, PENDING);
    }
    // Paused: no run sends anything, and a new request is refused.
    redeemer.run_every_owner().await.unwrap();
    assert_eq!(redeemer.run_owner(owner).await.unwrap().posts, 0);
    assert!(matches!(
        enqueue(db, &actor, None).await,
        Err(Error::Conflict)
    ));
    // "Redeem now" resumes the queue, including the code the answer stopped.
    manual::redeem_now(db, &actor).await.unwrap();
    let run = redeemer.run_owner(owner).await.unwrap();
    assert_eq!((run.posts, run.redeemed), (3, 3));
    assert_eq!(
        gateway.codes(),
        ["CODE01", "CODE02", "CODE02", "CODE03", "CODE04"]
    );
    assert_eq!(mode(db, account).await, "off");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_code_unlisted_right_before_its_request_is_dropped_not_sent() {
    let fixture = TestDatabase::new().await;
    let db = fixture.db.clone();
    let now = Utc::now();
    let actor = ready_user(&db, "last-moment").await;
    let owner_link = redeem_link(&db, actor.user.id, "100000001").await;
    let account = game(&db, owner_link, "genshin", "800000001").await;
    list(&db, &["CODE01", "CODE02"], now).await;
    enqueue(&db, &actor, None).await.unwrap();
    let (gateway, redeemer) = redeemer(&db, fast_tuning());
    let handle = tokio::runtime::Handle::current();
    let catalogue = db.clone();
    // The source drops CODE02 while CODE01 is in flight; only the flag changes (no drop).
    gateway.on_request(move |_| {
        tokio::task::block_in_place(|| {
            handle.block_on(async {
                redemption_code::Entity::update_many()
                    .col_expr(redemption_code::Column::Listed, Expr::value(false))
                    .filter(redemption_code::Column::Code.eq("CODE02"))
                    .exec(&catalogue)
                    .await
                    .unwrap();
            });
        });
    });
    let run = redeemer.run_owner(actor.user.id).await.unwrap();
    assert_eq!(run.posts, 1);
    assert_eq!(gateway.codes(), ["CODE01"]);
    assert!(row(&db, account, "CODE02").await.is_none());
}

#[tokio::test]
async fn rank_too_low_is_retried_daily_only_once_the_uid_is_auto() {
    let fixture = TestDatabase::new().await;
    let db = &fixture.db;
    let now = Utc::now();
    let actor = ready_user(db, "low-rank").await;
    let owner = actor.user.id;
    let owner_link = redeem_link(db, owner, "100000001").await;
    let account = game(db, owner_link, "genshin", "800000001").await;
    set_mode(db, account, "notify", now).await;
    list(db, &["RANKCODE"], now).await;
    enqueue(db, &actor, None).await.unwrap();
    let (gateway, redeemer) = redeemer(db, fast_tuning());
    gateway.script("RANKCODE", [answer(-2021)]);
    redeemer.run_owner(owner).await.unwrap();
    assert_eq!(
        row(db, account, "RANKCODE").await.unwrap().state,
        "rank_too_low"
    );
    redemption_ledger::Entity::update_many()
        .col_expr(
            redemption_ledger::Column::NextAttemptAt,
            Expr::value(Some(Utc::now() - Duration::minutes(1))),
        )
        .exec(db)
        .await
        .unwrap();
    // A day later: not again while the UID only announces codes; once it is `auto`, yes.
    assert_eq!(redeemer.run_owner(owner).await.unwrap().posts, 0);
    set_mode(db, account, "auto", now).await;
    assert_eq!(redeemer.run_owner(owner).await.unwrap().posts, 1);
    assert_eq!(gateway.sent().len(), 2);
    assert_eq!(
        row(db, account, "RANKCODE").await.unwrap().state,
        "redeemed"
    );
}

#[tokio::test]
async fn queued_codes_are_not_announced_again_to_notify_uids() {
    let fixture = TestDatabase::new().await;
    let db = &fixture.db;
    let now = Utc::now();
    let actor = ready_user(db, "notified").await;
    app_device::ActiveModel {
        id: Set(Uuid::new_v4()),
        user_id: Set(actor.user.id),
        created_at: Set(now),
        last_seen_at: Set(now),
        push_subscription: Set(None),
    }
    .insert(db)
    .await
    .unwrap();
    let owner_link = redeem_link(db, actor.user.id, "100000001").await;
    let account = game(db, owner_link, "genshin", "800000001").await;
    set_mode(db, account, "notify", now - Duration::hours(3)).await;
    list(db, &["QUEUED01", "QUEUED02"], now - Duration::hours(2)).await;
    enqueue(db, &actor, None).await.unwrap();
    let (_, redeemer) = redeemer(db, fast_tuning());
    // One is answered and one still waits; a code listed afterwards is news.
    redeemer.run_owner(actor.user.id).await.unwrap();
    redemption_ledger::Entity::update_many()
        .col_expr(redemption_ledger::Column::State, Expr::value(PENDING))
        .col_expr(redemption_ledger::Column::Posts, Expr::value(0))
        .col_expr(
            redemption_ledger::Column::LastSentAt,
            Expr::value(Option::<DateTime<Utc>>::None),
        )
        .filter(redemption_ledger::Column::GameAccountId.eq(account))
        .filter(redemption_ledger::Column::Code.eq("QUEUED02"))
        .exec(db)
        .await
        .unwrap();
    list(
        db,
        &["QUEUED01", "QUEUED02", "FRESHCODE"],
        now - Duration::hours(1),
    )
    .await;
    let notifications = Notifications::new(db.clone(), None, "https://ahd.example.test");
    assert_eq!(
        announce::announce(db, &notifications, now).await.unwrap(),
        1
    );
    let announced: Vec<String> = app_notification::Entity::find()
        .filter(app_notification::Column::UserId.eq(actor.user.id))
        .filter(app_notification::Column::Kind.eq("code_available"))
        .all(db)
        .await
        .unwrap()
        .into_iter()
        .map(|row| row.body)
        .collect();
    assert_eq!(announced.len(), 1, "{announced:?}");
    assert!(announced[0].contains("FRESHCODE"), "{}", announced[0]);
    assert!(!announced[0].contains("QUEUED"), "{}", announced[0]);
}
