use super::*;
use crate::dashboard::{
    codes::test_support::{game, ready_user, redeem_link},
    test_support::TestDatabase,
};
use sea_orm::{ActiveModelTrait, Set};

#[test]
fn every_answer_has_one_state_and_only_dead_codes_are_global() {
    use attoshoyodashboard::hoyolab::Error as E;
    for (answer, outcome, state, global, action) in [
        (
            Ok(RedemptionAnswer::from_retcode(0)),
            Outcome::Redeemed,
            "redeemed",
            None,
            None,
        ),
        (
            Ok(RedemptionAnswer::from_retcode(-2017)),
            Outcome::AlreadyUsed,
            "already_used",
            None,
            None,
        ),
        (
            Ok(RedemptionAnswer::from_retcode(-2001)),
            Outcome::Expired,
            "expired",
            Some("expired"),
            None,
        ),
        (
            Ok(RedemptionAnswer::from_retcode(-2003)),
            Outcome::Invalid,
            "invalid",
            Some("invalid"),
            None,
        ),
        // Region lock / not activated: invalid for this UID, never for everyone.
        (
            Ok(RedemptionAnswer::from_retcode(-2008)),
            Outcome::NotForAccount,
            "invalid",
            None,
            None,
        ),
        (
            Ok(RedemptionAnswer::from_retcode(-2021)),
            Outcome::RankTooLow,
            "rank_too_low",
            None,
            None,
        ),
        (
            Ok(RedemptionAnswer::from_retcode(-1071)),
            Outcome::CookieInvalid,
            "cookie_invalid",
            None,
            Some("cookie_invalid"),
        ),
        (
            Ok(RedemptionAnswer::from_retcode(10035)),
            Outcome::CaptchaRequired,
            "captcha_required",
            None,
            Some("captcha_required"),
        ),
        // No game account for this UID (-1073): this UID only, never the owner's pause.
        (
            Ok(RedemptionAnswer::from_retcode(-1073)),
            Outcome::NoGameAccount,
            "no_game_account",
            None,
            None,
        ),
        (
            Ok(RedemptionAnswer::from_retcode(-2016)),
            Outcome::Cooldown,
            "pending",
            None,
            None,
        ),
        (
            Ok(RedemptionAnswer::from_retcode(-9)),
            Outcome::Unknown,
            "unknown",
            None,
            None,
        ),
        (Err(E::Network), Outcome::Unknown, "unknown", None, None),
        (
            Err(E::InvalidEnvelope),
            Outcome::Unknown,
            "unknown",
            None,
            None,
        ),
        (Err(E::Http(500)), Outcome::Unknown, "unknown", None, None),
    ] {
        assert_eq!(Outcome::of(&answer), outcome, "{answer:?}");
        assert_eq!(outcome.state(), state);
        assert_eq!(outcome.global(), global);
        assert_eq!(outcome.action(), action);
        assert!(STATES.contains(&outcome.state()));
    }
    let labels: std::collections::BTreeSet<_> =
        Outcome::ALL.iter().map(|outcome| outcome.label()).collect();
    assert_eq!(labels.len(), Outcome::ALL.len());
}

async fn row(
    db: &DatabaseConnection,
    account: Uuid,
    code: &str,
    state: &str,
) -> redemption_ledger::Model {
    let now = Utc::now();
    redemption_ledger::ActiveModel {
        game_account_id: Set(account),
        code: Set(code.to_owned()),
        source: Set(CATALOGUE.to_owned()),
        state: Set(state.to_owned()),
        posts: Set(0),
        retcode: Set(None),
        next_attempt_at: Set(None),
        last_sent_at: Set(None),
        created_at: Set(now),
        updated_at: Set(now),
    }
    .insert(db)
    .await
    .unwrap()
}

async fn state(db: &DatabaseConnection, account: Uuid, code: &str) -> String {
    stored(db, account, code).await.state
}

async fn stored(db: &DatabaseConnection, account: Uuid, code: &str) -> redemption_ledger::Model {
    redemption_ledger::Entity::find_by_id((account, code.to_owned()))
        .one(db)
        .await
        .unwrap()
        .unwrap()
}

fn trunc(at: DateTime<Utc>) -> DateTime<Utc> {
    use chrono::SubsecRound;
    at.trunc_subsecs(6)
}

#[tokio::test]
async fn dead_codes_settle_waiting_rows_of_that_game_only_and_actions_pause_the_link() {
    let fixture = TestDatabase::new().await;
    let db = &fixture.db;
    let first = ready_user(db, "first").await.user.id;
    let second = ready_user(db, "second").await.user.id;
    let first_link = redeem_link(db, first, "100000001").await;
    let second_link = redeem_link(db, second, "100000002").await;
    let mine = game(db, first_link, "genshin", "800000001").await;
    let theirs = game(db, second_link, "genshin", "800000002").await;
    let other_game = game(db, second_link, "starrail", "800000003").await;
    row(db, theirs, "DEADCODE", "pending").await;
    row(db, other_game, "DEADCODE", "pending").await;
    row(db, theirs, "OTHERCODE", "pending").await;
    let now = Utc::now();
    let answered = Answered {
        link: first_link,
        game_account: mine,
        game_kind: "genshin",
        code: "DEADCODE",
    };
    row(db, mine, "DEADCODE", SENDING).await;
    record(
        db,
        &answered,
        Outcome::Expired,
        None,
        now,
        Duration::seconds(30),
    )
    .await
    .unwrap();
    assert_eq!(state(db, mine, "DEADCODE").await, "expired");
    assert_eq!(
        state(db, theirs, "DEADCODE").await,
        "expired",
        "settled without a request"
    );
    assert_eq!(
        state(db, other_game, "DEADCODE").await,
        "pending",
        "another game's code"
    );
    assert_eq!(state(db, theirs, "OTHERCODE").await, "pending");
    assert_eq!(
        rejection(db, "genshin", "DEADCODE")
            .await
            .unwrap()
            .as_deref(),
        Some("expired")
    );
    assert_eq!(rejection(db, "starrail", "DEADCODE").await.unwrap(), None);

    // An account-specific answer never marks the code for anyone else.
    for (code, outcome) in [
        ("USEDCODE", Outcome::AlreadyUsed),
        ("LOWCODE", Outcome::RankTooLow),
        ("ELSEWHERE", Outcome::NotForAccount),
    ] {
        row(db, mine, code, SENDING).await;
        row(db, theirs, code, "pending").await;
        record(
            db,
            &Answered { code, ..answered },
            outcome,
            None,
            now,
            Duration::seconds(30),
        )
        .await
        .unwrap();
        assert_eq!(rejection(db, "genshin", code).await.unwrap(), None);
        assert_eq!(state(db, theirs, code).await, "pending");
        assert_eq!(state(db, mine, code).await, outcome.state());
    }

    // Rank too low waits a day; a cooldown waits `cooldown_wait`; an unclassified
    // answer keeps only its retcode.
    row(db, mine, "RANKCODE", SENDING).await;
    record(
        db,
        &Answered {
            code: "RANKCODE",
            ..answered
        },
        Outcome::RankTooLow,
        None,
        now,
        Duration::seconds(30),
    )
    .await
    .unwrap();
    assert_eq!(
        stored(db, mine, "RANKCODE").await.next_attempt_at,
        Some(trunc(now + RANK_RETRY))
    );
    row(db, mine, "COOLCODE", SENDING).await;
    record(
        db,
        &Answered {
            code: "COOLCODE",
            ..answered
        },
        Outcome::Cooldown,
        None,
        now,
        Duration::seconds(30),
    )
    .await
    .unwrap();
    let cool = stored(db, mine, "COOLCODE").await;
    assert_eq!(cool.state, "pending");
    assert_eq!(
        cool.next_attempt_at,
        Some(trunc(now + Duration::seconds(30)))
    );
    row(db, mine, "ODDCODE", SENDING).await;
    record(
        db,
        &Answered {
            code: "ODDCODE",
            ..answered
        },
        Outcome::Unknown,
        Some(-424242),
        now,
        Duration::seconds(30),
    )
    .await
    .unwrap();
    let odd = stored(db, mine, "ODDCODE").await;
    assert_eq!(
        (odd.state.as_str(), odd.retcode),
        ("unknown", Some(-424242))
    );

    // A CAPTCHA answer pauses the link until reopened; reopening touches only action rows.
    row(db, mine, "CAPTCHACODE", SENDING).await;
    record(
        db,
        &Answered {
            code: "CAPTCHACODE",
            ..answered
        },
        Outcome::CaptchaRequired,
        None,
        now,
        Duration::seconds(30),
    )
    .await
    .unwrap();
    let link = hoyolab_account::Entity::find_by_id(first_link)
        .one(db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(link.redemption_action.as_deref(), Some("captcha_required"));
    let tx = db.begin().await.unwrap();
    reopen_in(&tx, first_link, Utc::now()).await.unwrap();
    tx.commit().await.unwrap();
    let link = hoyolab_account::Entity::find_by_id(first_link)
        .one(db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(link.redemption_action, None);
    assert_eq!(link.redemption_action_at, None);
    assert_eq!(state(db, mine, "CAPTCHACODE").await, "pending");
    assert_eq!(
        state(db, mine, "ODDCODE").await,
        "unknown",
        "never reopened"
    );
    assert_eq!(state(db, mine, "DEADCODE").await, "expired");
    // The other user's link is untouched by either.
    let other = hoyolab_account::Entity::find_by_id(second_link)
        .one(db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(other.redemption_action, None);
}

#[tokio::test]
async fn recovery_turns_sending_rows_into_unknown() {
    let fixture = TestDatabase::new().await;
    let db = &fixture.db;
    let owner = ready_user(db, "owner").await.user.id;
    let link = redeem_link(db, owner, "100000001").await;
    let account = game(db, link, "genshin", "800000001").await;
    row(db, account, "OLDSENDING", SENDING).await;
    redemption_ledger::Entity::update_many()
        .col_expr(
            redemption_ledger::Column::UpdatedAt,
            Expr::value(Utc::now() - Duration::hours(1)),
        )
        .filter(redemption_ledger::Column::Code.eq("OLDSENDING"))
        .exec(db)
        .await
        .unwrap();
    row(db, account, "NEWSENDING", SENDING).await;
    row(db, account, "WAITING", PENDING).await;
    // During a run only stale rows are settled; a fresh one may still be in flight.
    assert_eq!(
        recover(db, Some(Utc::now() - Duration::minutes(10)))
            .await
            .unwrap(),
        1
    );
    assert_eq!(state(db, account, "OLDSENDING").await, "unknown");
    assert_eq!(state(db, account, "NEWSENDING").await, SENDING);
    // At startup nothing can be in flight.
    assert_eq!(recover(db, None).await.unwrap(), 1);
    assert_eq!(state(db, account, "NEWSENDING").await, "unknown");
    assert_eq!(state(db, account, "WAITING").await, PENDING);
}
