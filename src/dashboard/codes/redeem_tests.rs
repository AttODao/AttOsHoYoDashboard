use super::*;
use crate::dashboard::{
    codes::{
        manual,
        test_support::{MockRedeem, fast_tuning, game, ready_user, redeem_link, test_key},
    },
    db::entities::{
        app_device, app_notification, hoyolab_account, redemption_code, redemption_code_rejection,
        redemption_code_setting, redemption_code_source, redemption_ledger, user,
    },
    test_support::TestDatabase,
};
use attoshoyodashboard::hoyolab::Error as UpstreamError;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, PaginatorTrait, QueryFilter,
    QueryOrder, Set, sea_query::Expr,
};

struct Fixture {
    db: TestDatabase,
    gateway: MockRedeem,
    redeemer: Arc<Redeemer<MockRedeem>>,
}

fn fixture_with(db: TestDatabase, tuning: RedeemTuning) -> Fixture {
    let store = Arc::new(CookieStore::new(db.db.clone(), Arc::new(test_key())));
    let notifications = Arc::new(Notifications::new(
        db.db.clone(),
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
    Fixture {
        db,
        gateway,
        redeemer,
    }
}

async fn fixture() -> Fixture {
    fixture_with(TestDatabase::new().await, fast_tuning())
}

/// A user with an app installation (every personal notice lands in the inbox), a link
/// with redemption credentials and one Genshin UID in `auto`.
async fn auto_user(
    db: &DatabaseConnection,
    name: &str,
    hoyolab: &str,
    uid: &str,
) -> (Uuid, Uuid, Uuid) {
    let owner = ready_user(db, name).await.user.id;
    app_device::ActiveModel {
        id: Set(Uuid::new_v4()),
        user_id: Set(owner),
        created_at: Set(Utc::now()),
        last_seen_at: Set(Utc::now()),
        push_subscription: Set(None),
    }
    .insert(db)
    .await
    .unwrap();
    let link = redeem_link(db, owner, hoyolab).await;
    let account = game(db, link, "genshin", uid).await;
    set_mode(db, account, "auto").await;
    (owner, link, account)
}

async fn set_mode(db: &DatabaseConnection, account: Uuid, mode: &str) {
    redemption_code_setting::Entity::delete_by_id(account)
        .exec(db)
        .await
        .unwrap();
    redemption_code_setting::ActiveModel {
        game_account_id: Set(account),
        mode: Set(mode.to_owned()),
        enabled_at: Set((mode != "off").then(Utc::now)),
        updated_at: Set(Utc::now()),
    }
    .insert(db)
    .await
    .unwrap();
}

/// Lists `codes` for Genshin, oldest first in the given order.
async fn list(db: &DatabaseConnection, codes: &[&str]) {
    for (index, code) in codes.iter().enumerate() {
        let seen = Utc::now() - chrono::Duration::hours(100 - index as i64);
        redemption_code::ActiveModel {
            id: Set(Uuid::new_v4()),
            game_kind: Set("genshin".to_owned()),
            code: Set((*code).to_owned()),
            rewards: Set(format!("Primogem*{}", index + 1)),
            first_seen_at: Set(seen),
            last_seen_at: Set(seen),
            listed: Set(true),
            seeded: Set(false),
        }
        .insert(db)
        .await
        .unwrap();
    }
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

async fn state(db: &DatabaseConnection, account: Uuid, code: &str) -> Option<String> {
    row(db, account, code).await.map(|row| row.state)
}

/// The kinds and bodies of `owner`'s app inbox, oldest first.
async fn inbox(db: &DatabaseConnection, owner: Uuid) -> Vec<(String, String)> {
    app_notification::Entity::find()
        .filter(app_notification::Column::UserId.eq(owner))
        .order_by_asc(app_notification::Column::Id)
        .all(db)
        .await
        .unwrap()
        .into_iter()
        .map(|row| (row.kind, row.body))
        .collect()
}

#[tokio::test]
async fn each_listed_code_is_sent_once_with_only_the_redemption_cookie() {
    let fixture = fixture().await;
    let db = &fixture.db.db;
    let (owner, _, account) = auto_user(db, "redeemer", "100000001", "800000001").await;
    list(db, &["FIRSTCODE", "SECONDCODE"]).await;
    let runs = fixture.redeemer.run_every_owner().await.unwrap();
    assert_eq!(
        runs,
        [OwnerRun {
            posts: 2,
            redeemed: 2,
            action: None
        }]
    );
    let sent = fixture.gateway.sent();
    assert_eq!(fixture.gateway.codes(), ["FIRSTCODE", "SECONDCODE"]);
    assert_eq!(sent[0].game, "genshin");
    assert_eq!(sent[0].uid, "800000001");
    assert_eq!(sent[0].region, "os_asia");
    assert_eq!(
        sent[0].cookie,
        "cookie_token_v2=syntheticcookietoken100000001; account_id_v2=100000001"
    );
    for code in ["FIRSTCODE", "SECONDCODE"] {
        let row = row(db, account, code).await.unwrap();
        assert_eq!((row.state.as_str(), row.posts), ("redeemed", 1));
    }
    // Later runs, a restart's recovery and a catalogue re-listing never send them again.
    fixture.redeemer.recover_ledger().await.unwrap();
    assert_eq!(fixture.redeemer.run_every_owner().await.unwrap(), []);
    assert_eq!(fixture.gateway.sent().len(), 2);
    // One summary per run, with the reward text.
    let notices = inbox(db, owner).await;
    assert_eq!(notices.len(), 1);
    assert_eq!(notices[0].0, "code_redeemed");
    assert_eq!(
        notices[0].1,
        "原神 UID 800000001: FIRSTCODE (Primogem×1)\n原神 UID 800000001: SECONDCODE (Primogem×2)"
    );
    let answers = fixture.redeemer.stats.answers();
    assert!(answers.contains(&("genshin", "redeemed", 2)));
    assert!(answers.contains(&("starrail", "unknown", 0)));
    assert_eq!(answers.len(), 2 * Outcome::ALL.len());
}

#[tokio::test]
async fn uncertain_answers_become_unknown_and_are_never_resent() {
    let fixture = fixture().await;
    let db = &fixture.db.db;
    let (owner, link, account) = auto_user(db, "uncertain", "100000001", "800000001").await;
    list(db, &["TIMEOUTCODE", "ODDCODE", "BROKENCODE", "LASTCODE"]).await;
    fixture
        .gateway
        .script("TIMEOUTCODE", [Err(UpstreamError::Network)]);
    fixture
        .gateway
        .script("ODDCODE", [Ok(RedemptionReply::Other(-31337))]);
    fixture
        .gateway
        .script("BROKENCODE", [Err(UpstreamError::InvalidEnvelope)]);
    fixture.redeemer.run_owner(owner).await.unwrap();
    assert_eq!(
        state(db, account, "TIMEOUTCODE").await.as_deref(),
        Some("unknown")
    );
    let odd = row(db, account, "ODDCODE").await.unwrap();
    assert_eq!((odd.state.as_str(), odd.retcode), ("unknown", Some(-31337)));
    assert_eq!(
        state(db, account, "BROKENCODE").await.as_deref(),
        Some("unknown")
    );
    assert_eq!(
        state(db, account, "LASTCODE").await.as_deref(),
        Some("redeemed")
    );
    // A crash between marking and answering leaves `sending`: recovery makes it unknown.
    list(db, &["CRASHCODE"]).await;
    let now = Utc::now();
    redemption_ledger::ActiveModel {
        game_account_id: Set(account),
        code: Set("CRASHCODE".to_owned()),
        source: Set("catalogue".to_owned()),
        state: Set("sending".to_owned()),
        posts: Set(1),
        retcode: Set(None),
        next_attempt_at: Set(None),
        last_sent_at: Set(Some(now)),
        created_at: Set(now),
        updated_at: Set(now),
    }
    .insert(db)
    .await
    .unwrap();
    assert_eq!(fixture.redeemer.recover_ledger().await.unwrap(), 1);
    assert_eq!(
        state(db, account, "CRASHCODE").await.as_deref(),
        Some("unknown")
    );
    for _ in 0..2 {
        fixture.redeemer.run_owner(owner).await.unwrap();
    }
    // A Cookie update reopens only CAPTCHA/Cookie rows, never unknown ones.
    let tx = sea_orm::TransactionTrait::begin(db).await.unwrap();
    ledger::reopen_in(&tx, link, Utc::now()).await.unwrap();
    tx.commit().await.unwrap();
    fixture.redeemer.run_owner(owner).await.unwrap();
    assert_eq!(fixture.gateway.sent().len(), 4, "nothing was sent twice");
    assert!(
        fixture
            .redeemer
            .stats
            .answers()
            .contains(&("genshin", "unknown", 3))
    );
}

#[tokio::test]
async fn requests_are_spaced_capped_per_run_and_cooldowns_wait_without_failing() {
    let tuning = RedeemTuning {
        spacing: Duration::from_millis(60),
        cooldown_wait: Duration::from_millis(150),
        max_posts: 4,
        ..fast_tuning()
    };
    let fixture = fixture_with(TestDatabase::new().await, tuning);
    let db = &fixture.db.db;
    let (owner, _, account) = auto_user(db, "paced", "100000001", "800000001").await;
    list(
        db,
        &[
            "CODE0001", "COOLCODE", "CODE0003", "CODE0004", "CODE0005", "CODE0006",
        ],
    )
    .await;
    fixture.gateway.script(
        "COOLCODE",
        [Ok(RedemptionReply::Cooldown), Ok(RedemptionReply::Redeemed)],
    );
    let run = fixture.redeemer.run_owner(owner).await.unwrap();
    assert_eq!(
        run.posts, 4,
        "at most max_posts requests per account and run"
    );
    let sent = fixture.gateway.sent();
    assert_eq!(
        fixture.gateway.codes(),
        ["CODE0001", "COOLCODE", "CODE0003", "CODE0004"]
    );
    for pair in sent.windows(2) {
        assert!(
            pair[1].at.duration_since(pair[0].at) >= tuning.spacing,
            "{:?}",
            pair[1].at.duration_since(pair[0].at)
        );
    }
    assert!(
        sent[2].at.duration_since(sent[1].at) >= tuning.cooldown_wait,
        "a cooldown answer waits longer"
    );
    // The cooldown code is pending again (not a failure) and waits its turn.
    let cool = row(db, account, "COOLCODE").await.unwrap();
    assert_eq!((cool.state.as_str(), cool.posts), ("pending", 1));
    assert!(cool.next_attempt_at.is_some());
    assert!(
        fixture
            .redeemer
            .stats
            .answers()
            .contains(&("genshin", "cooldown", 1))
    );
    // The next run continues where the cap stopped, and the cooldown code is retried.
    tokio::time::sleep(tuning.cooldown_wait).await;
    let run = fixture.redeemer.run_owner(owner).await.unwrap();
    assert_eq!(run.posts, 3);
    assert_eq!(
        fixture.gateway.codes()[4..],
        ["COOLCODE", "CODE0005", "CODE0006"]
    );
    let cool = row(db, account, "COOLCODE").await.unwrap();
    assert_eq!((cool.state.as_str(), cool.posts), ("redeemed", 2));
    // The spacing holds across runs for the same HoYoLAB account.
    let sent = fixture.gateway.sent();
    assert!(sent[4].at.duration_since(sent[3].at) >= tuning.spacing);
}

#[tokio::test]
async fn a_cooldown_answer_is_retried_once_in_the_same_run() {
    let fixture = fixture().await;
    let db = &fixture.db.db;
    let (owner, _, account) = auto_user(db, "cool", "100000001", "800000001").await;
    list(db, &["COOLCODE", "NEXTCODE"]).await;
    fixture.gateway.script(
        "COOLCODE",
        [Ok(RedemptionReply::Cooldown), Ok(RedemptionReply::Redeemed)],
    );
    let run = fixture.redeemer.run_owner(owner).await.unwrap();
    assert_eq!(run.posts, 3);
    assert_eq!(
        fixture.gateway.codes(),
        ["COOLCODE", "NEXTCODE", "COOLCODE"]
    );
    assert_eq!(
        state(db, account, "COOLCODE").await.as_deref(),
        Some("redeemed")
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_administrator_switch_stops_the_next_request_and_every_later_run() {
    let fixture = fixture().await;
    let db = fixture.db.db.clone();
    let (owner, _, account) = auto_user(&db, "stopped", "100000001", "800000001").await;
    list(&db, &["FIRSTCODE", "SECONDCODE", "THIRDCODE"]).await;
    let handle = tokio::runtime::Handle::current();
    let switch = db.clone();
    // The administrator turns the feature off while the first request is in flight.
    fixture.gateway.on_request(move |_| {
        tokio::task::block_in_place(|| {
            handle.block_on(async {
                redemption_code_source::Entity::update_many()
                    .col_expr(redemption_code_source::Column::Enabled, Expr::value(false))
                    .exec(&switch)
                    .await
                    .unwrap();
            });
        });
    });
    let run = fixture.redeemer.run_owner(owner).await.unwrap();
    assert_eq!(run.posts, 1);
    assert_eq!(fixture.gateway.codes(), ["FIRSTCODE"]);
    assert_eq!(state(&db, account, "SECONDCODE").await, None);
    assert_eq!(fixture.redeemer.run_every_owner().await.unwrap(), []);
    assert_eq!(fixture.gateway.sent().len(), 1);
}

#[tokio::test]
async fn cookie_and_captcha_answers_pause_the_owner_until_an_explicit_retry() {
    let fixture = fixture().await;
    let db = &fixture.db.db;
    let (owner, link, account) = auto_user(db, "paused", "100000001", "800000001").await;
    let (other, _, other_account) = auto_user(db, "unaffected", "100000002", "800000002").await;
    list(db, &["CAPTCHACODE", "SECONDCODE"]).await;
    fixture.gateway.script(
        "CAPTCHACODE",
        [
            Ok(RedemptionReply::CaptchaRequired),
            Ok(RedemptionReply::Redeemed),
            Ok(RedemptionReply::Redeemed),
        ],
    );
    let run = fixture.redeemer.run_owner(owner).await.unwrap();
    assert_eq!(
        run,
        OwnerRun {
            posts: 1,
            redeemed: 0,
            action: Some("captcha_required")
        }
    );
    assert_eq!(
        state(db, account, "CAPTCHACODE").await.as_deref(),
        Some("captcha_required")
    );
    assert_eq!(
        state(db, account, "SECONDCODE").await,
        None,
        "stopped after the answer"
    );
    let paused = hoyolab_account::Entity::find_by_id(link)
        .one(db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        paused.redemption_action.as_deref(),
        Some("captcha_required")
    );
    assert_eq!(
        paused.auth_status, "verified",
        "the ltoken link keeps working"
    );
    let notices = inbox(db, owner).await;
    assert_eq!(notices.len(), 1);
    assert_eq!(notices[0].0, "code_action_required");
    assert!(notices[0].1.contains("CAPTCHA"), "{}", notices[0].1);
    // Other users go on; this owner gets nothing until it acts.
    fixture.redeemer.run_every_owner().await.unwrap();
    assert_eq!(
        state(db, other_account, "CAPTCHACODE").await.as_deref(),
        Some("redeemed")
    );
    assert_eq!(
        fixture.redeemer.run_owner(owner).await.unwrap(),
        OwnerRun::default()
    );
    assert!(
        fixture
            .gateway
            .sent()
            .iter()
            .all(|sent| sent.uid != "800000001" || sent.code == "CAPTCHACODE")
    );
    // "Redeem now" is the explicit retry: the stopped code is pending again and sent.
    let actor = crate::dashboard::auth::sessions::Sessions::authenticate(
        db,
        &crate::dashboard::auth::password::token_hash(
            &crate::dashboard::auth::sessions::Sessions::new(chrono::Duration::days(1))
                .issue(db, owner)
                .await
                .unwrap()
                .token,
        )
        .unwrap(),
    )
    .await
    .unwrap();
    manual::redeem_now(db, &actor).await.unwrap();
    let run = fixture.redeemer.run_owner(owner).await.unwrap();
    assert_eq!((run.posts, run.redeemed), (2, 2));
    assert_eq!(
        state(db, account, "CAPTCHACODE").await.as_deref(),
        Some("redeemed")
    );
    let _ = other;
}

#[tokio::test]
async fn an_expired_code_is_never_sent_for_anyone_again_but_account_answers_stay_private() {
    let fixture = fixture().await;
    let db = &fixture.db.db;
    let (first, _, first_account) = auto_user(db, "first", "100000001", "800000001").await;
    let (second, _, second_account) = auto_user(db, "second", "100000002", "800000002").await;
    list(db, &["DEADCODE", "USEDCODE", "LOWRANKCODE"]).await;
    fixture
        .gateway
        .script("DEADCODE", [Ok(RedemptionReply::Expired)]);
    fixture
        .gateway
        .script("USEDCODE", [Ok(RedemptionReply::AlreadyUsed)]);
    fixture
        .gateway
        .script("LOWRANKCODE", [Ok(RedemptionReply::RankTooLow)]);
    fixture.redeemer.run_owner(first).await.unwrap();
    fixture.redeemer.run_owner(second).await.unwrap();
    let second_codes: Vec<String> = fixture
        .gateway
        .sent()
        .into_iter()
        .filter(|sent| sent.uid == "800000002")
        .map(|sent| sent.code)
        .collect();
    assert_eq!(second_codes, ["USEDCODE", "LOWRANKCODE"]);
    assert_eq!(
        state(db, second_account, "DEADCODE").await,
        None,
        "never planned again"
    );
    assert_eq!(
        state(db, second_account, "USEDCODE").await.as_deref(),
        Some("redeemed")
    );
    assert_eq!(
        state(db, first_account, "LOWRANKCODE").await.as_deref(),
        Some("rank_too_low")
    );
    // The catalogue prunes the code and the source lists it again later: still dead.
    redemption_code::Entity::delete_many()
        .filter(redemption_code::Column::Code.eq("DEADCODE"))
        .exec(db)
        .await
        .unwrap();
    list(db, &["DEADCODE"]).await;
    let (third, _, _) = auto_user(db, "third", "100000003", "800000003").await;
    fixture.redeemer.run_owner(third).await.unwrap();
    fixture.redeemer.run_every_owner().await.unwrap();
    assert_eq!(
        fixture
            .gateway
            .sent()
            .iter()
            .filter(|sent| sent.code == "DEADCODE")
            .count(),
        1
    );
    assert_eq!(
        redemption_code_rejection::Entity::find()
            .count(db)
            .await
            .unwrap(),
        1
    );
}

#[tokio::test]
async fn rank_too_low_is_retried_once_a_day_while_listed() {
    let fixture = fixture().await;
    let db = &fixture.db.db;
    let (owner, _, account) = auto_user(db, "low-rank", "100000001", "800000001").await;
    list(db, &["RANKCODE"]).await;
    fixture.gateway.script(
        "RANKCODE",
        [
            Ok(RedemptionReply::RankTooLow),
            Ok(RedemptionReply::RankTooLow),
        ],
    );
    fixture.redeemer.run_owner(owner).await.unwrap();
    fixture.redeemer.run_owner(owner).await.unwrap();
    assert_eq!(fixture.gateway.sent().len(), 1, "not again the same day");
    let low = row(db, account, "RANKCODE").await.unwrap();
    assert!(low.next_attempt_at.unwrap() > Utc::now() + chrono::Duration::hours(23));
    // A day later it is tried again.
    let yesterday = |db: &DatabaseConnection| {
        let db = db.clone();
        async move {
            redemption_ledger::Entity::update_many()
                .col_expr(
                    redemption_ledger::Column::NextAttemptAt,
                    Expr::value(Some(Utc::now() - chrono::Duration::minutes(1))),
                )
                .exec(&db)
                .await
                .unwrap();
        }
    };
    yesterday(db).await;
    fixture.redeemer.run_owner(owner).await.unwrap();
    assert_eq!(fixture.gateway.sent().len(), 2);
    // Once the source no longer lists it, it waits forever.
    yesterday(db).await;
    redemption_code::Entity::update_many()
        .col_expr(redemption_code::Column::Listed, Expr::value(false))
        .exec(db)
        .await
        .unwrap();
    fixture.redeemer.run_owner(owner).await.unwrap();
    assert_eq!(fixture.gateway.sent().len(), 2);
    redemption_code::Entity::update_many()
        .col_expr(redemption_code::Column::Listed, Expr::value(true))
        .exec(db)
        .await
        .unwrap();
    fixture.redeemer.run_owner(owner).await.unwrap();
    assert_eq!(
        state(db, account, "RANKCODE").await.as_deref(),
        Some("redeemed")
    );
}

#[tokio::test]
async fn notify_and_off_uids_and_paused_or_plain_links_are_never_sent() {
    let fixture = fixture().await;
    let db = &fixture.db.db;
    let (_, _, notify) = auto_user(db, "notify-only", "100000001", "800000001").await;
    set_mode(db, notify, "notify").await;
    let (_, _, off) = auto_user(db, "off", "100000002", "800000002").await;
    set_mode(db, off, "off").await;
    let (_, link, _) = auto_user(db, "no-cookie-token", "100000003", "800000003").await;
    hoyolab_account::Entity::update_many()
        .col_expr(
            hoyolab_account::Column::RedemptionCredentials,
            Expr::value(false),
        )
        .filter(hoyolab_account::Column::Id.eq(link))
        .exec(db)
        .await
        .unwrap();
    let (deleted, _, _) = auto_user(db, "deactivated", "100000004", "800000004").await;
    user::Entity::update_many()
        .col_expr(user::Column::Active, Expr::value(false))
        .filter(user::Column::Id.eq(deleted))
        .exec(db)
        .await
        .unwrap();
    list(db, &["SOMECODE"]).await;
    assert_eq!(fixture.redeemer.run_every_owner().await.unwrap(), []);
    assert!(fixture.gateway.sent().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn owners_run_one_at_a_time_with_concurrency_one_and_never_twice_at_once() {
    let tuning = RedeemTuning {
        concurrency: 1,
        ..fast_tuning()
    };
    let fixture = fixture_with(TestDatabase::new().await, tuning);
    let db = &fixture.db.db;
    for (name, hoyolab, uid) in [
        ("one", "100000001", "800000001"),
        ("two", "100000002", "800000002"),
        ("three", "100000003", "800000003"),
    ] {
        auto_user(db, name, hoyolab, uid).await;
    }
    list(db, &["CODEA001", "CODEB002", "CODEC003"]).await;
    fixture.redeemer.run_every_owner().await.unwrap();
    let uids: Vec<String> = fixture
        .gateway
        .sent()
        .into_iter()
        .map(|sent| sent.uid)
        .collect();
    assert_eq!(uids.len(), 9);
    // With one slot an owner's requests are never interleaved with another's.
    let mut blocks = uids.clone();
    blocks.dedup();
    assert_eq!(blocks.len(), 3, "{uids:?}");
    // A request for an owner already running is folded into one more pass.
    let owner = ready_user(db, "folded").await.user.id;
    assert!(!fixture.redeemer.is_running(owner));
    let (first, second) = tokio::join!(
        fixture.redeemer.run_owner(owner),
        fixture.redeemer.run_owner(owner)
    );
    assert!(first.is_ok() && second.is_ok());
    assert!(!fixture.redeemer.is_running(owner));
}

#[tokio::test]
async fn removing_and_registering_the_cookie_again_resends_nothing_and_deletion_drops_the_ledger() {
    let fixture = fixture().await;
    let db = &fixture.db.db;
    let (owner, link, account) = auto_user(db, "lifecycle", "100000001", "800000001").await;
    list(db, &["KEPTCODE"]).await;
    fixture.redeemer.run_owner(owner).await.unwrap();
    assert_eq!(fixture.gateway.sent().len(), 1);
    // Cookie removal: every mode off, the ledger stays.
    let tx = sea_orm::TransactionTrait::begin(db).await.unwrap();
    crate::dashboard::codes::settings::turn_off_link(&tx, link)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    assert_eq!(
        state(db, account, "KEPTCODE").await.as_deref(),
        Some("redeemed")
    );
    assert_eq!(fixture.redeemer.run_every_owner().await.unwrap(), []);
    // The same account registered again and `auto` chosen again: nothing is resent.
    set_mode(db, account, "auto").await;
    hoyolab_account::Entity::update_many()
        .col_expr(
            hoyolab_account::Column::CredentialVersion,
            Expr::value(3_i64),
        )
        .filter(hoyolab_account::Column::Id.eq(link))
        .exec(db)
        .await
        .unwrap();
    fixture.redeemer.run_every_owner().await.unwrap();
    assert_eq!(fixture.gateway.sent().len(), 1);
    // User deletion removes the ledger with the UID.
    user::Entity::delete_by_id(owner).exec(db).await.unwrap();
    assert_eq!(
        redemption_ledger::Entity::find().count(db).await.unwrap(),
        0
    );
}
