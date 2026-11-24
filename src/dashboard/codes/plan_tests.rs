use super::*;
use crate::dashboard::{
    codes::{
        ledger::{STATES, recover},
        test_support::{game, link, ready_user, redeem_link, test_key},
    },
    test_support::TestDatabase,
};
use chrono::Duration;
use sea_orm::{ActiveModelTrait, sea_query::Expr};
use std::sync::Arc;

async fn listed(db: &DatabaseConnection, game_kind: &str, code: &str, age_hours: i64) {
    let seen = Utc::now() - Duration::hours(age_hours);
    redemption_code::ActiveModel {
        id: Set(Uuid::new_v4()),
        game_kind: Set(game_kind.to_owned()),
        code: Set(code.to_owned()),
        rewards: Set(format!("Primogem*{age_hours}")),
        first_seen_at: Set(seen),
        last_seen_at: Set(seen),
        listed: Set(true),
        seeded: Set(true),
    }
    .insert(db)
    .await
    .unwrap();
}

async fn mode(db: &DatabaseConnection, account: Uuid, mode: &str) {
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

async fn ledger_row(
    db: &DatabaseConnection,
    account: Uuid,
    code: &str,
) -> Option<redemption_ledger::Model> {
    redemption_ledger::Entity::find_by_id((account, code.to_owned()))
        .one(db)
        .await
        .unwrap()
}

fn codes(plan: &Plan) -> Vec<(&str, &str)> {
    plan.items
        .iter()
        .map(|item| (item.code.as_str(), item.source))
        .collect()
}

#[tokio::test]
async fn plans_cover_every_listed_code_for_auto_uids_and_entered_codes_first() {
    let fixture = TestDatabase::new().await;
    let db = &fixture.db;
    let owner = ready_user(db, "planner").await.user.id;
    let owner_link = redeem_link(db, owner, "100000001").await;
    let genshin = game(db, owner_link, "genshin", "800000001").await;
    let rail = game(db, owner_link, "starrail", "900000001").await;
    // Codes listed long before `auto` was chosen are redeemed too, oldest first.
    listed(db, "genshin", "NEWERCODE", 1).await;
    listed(db, "genshin", "OLDERCODE", 48).await;
    listed(db, "starrail", "RAILCODE", 2).await;
    assert!(
        plan(db, owner, Utc::now()).await.unwrap().is_none(),
        "no auto UID"
    );
    assert!(owners(db).await.unwrap().is_empty());
    mode(db, genshin, "auto").await;
    mode(db, rail, "notify").await;
    let now = Utc::now();
    redemption_ledger::ActiveModel {
        game_account_id: Set(rail),
        code: Set("MYRAILCODE".to_owned()),
        source: Set(MANUAL.to_owned()),
        state: Set(PENDING.to_owned()),
        posts: Set(0),
        retcode: Set(None),
        next_attempt_at: Set(None),
        last_sent_at: Set(None),
        created_at: Set(now),
        updated_at: Set(now),
    }
    .insert(db)
    .await
    .unwrap();
    assert_eq!(owners(db).await.unwrap(), [owner]);
    let planned = plan(db, owner, now).await.unwrap().unwrap();
    assert_eq!(
        codes(&planned),
        [
            ("MYRAILCODE", MANUAL),
            ("OLDERCODE", CATALOGUE),
            ("NEWERCODE", CATALOGUE)
        ],
        "a notify UID gets only what its owner entered"
    );
    assert_eq!(planned.items[1].region, "os_asia");
    assert_eq!(planned.items[1].rewards, "Primogem*48");
    assert_eq!(planned.credential_version, 1);

    // Rejected codes and finished rows are left out; due waits are honoured.
    redemption_code_rejection::ActiveModel {
        game_kind: Set("genshin".to_owned()),
        code: Set("OLDERCODE".to_owned()),
        reason: Set("expired".to_owned()),
        rejected_at: Set(now),
    }
    .insert(db)
    .await
    .unwrap();
    redemption_ledger::Entity::update_many()
        .col_expr(
            redemption_ledger::Column::NextAttemptAt,
            Expr::value(Some(now + Duration::minutes(1))),
        )
        .filter(redemption_ledger::Column::Code.eq("MYRAILCODE"))
        .exec(db)
        .await
        .unwrap();
    let planned = plan(db, owner, now).await.unwrap().unwrap();
    assert_eq!(codes(&planned), [("NEWERCODE", CATALOGUE)]);
    let later = plan(db, owner, now + Duration::minutes(2))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(later.items.len(), 2);

    // Links that cannot redeem, or wait for the owner, plan nothing.
    for (column, value) in [
        (
            hoyolab_account::Column::RedemptionCredentials,
            Expr::value(false),
        ),
        (
            hoyolab_account::Column::AuthStatus,
            Expr::value("cookie_invalid"),
        ),
    ] {
        let before = hoyolab_account::Entity::find_by_id(owner_link)
            .one(db)
            .await
            .unwrap()
            .unwrap();
        hoyolab_account::Entity::update_many()
            .col_expr(column, value)
            .filter(hoyolab_account::Column::Id.eq(owner_link))
            .exec(db)
            .await
            .unwrap();
        assert!(plan(db, owner, now).await.unwrap().is_none());
        assert!(owners(db).await.unwrap().is_empty());
        sea_orm::IntoActiveModel::into_active_model(before)
            .reset_all()
            .update(db)
            .await
            .unwrap();
    }
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
    assert!(plan(db, owner, now).await.unwrap().is_none());
    // An inactive user is never planned.
    let other = ready_user(db, "inactive").await.user.id;
    let other_link = redeem_link(db, other, "100000002").await;
    let other_uid = game(db, other_link, "genshin", "800000002").await;
    mode(db, other_uid, "auto").await;
    assert_eq!(owners(db).await.unwrap(), [other]);
    user::Entity::update_many()
        .col_expr(user::Column::Active, Expr::value(false))
        .filter(user::Column::Id.eq(other))
        .exec(db)
        .await
        .unwrap();
    assert!(owners(db).await.unwrap().is_empty());
}

#[tokio::test]
async fn admission_rechecks_everything_and_marks_the_row_before_any_request() {
    let fixture = TestDatabase::new().await;
    let db = &fixture.db;
    let store = CookieStore::new(db.clone(), Arc::new(test_key()));
    let owner = ready_user(db, "admitted").await.user.id;
    let owner_link = redeem_link(db, owner, "100000001").await;
    let genshin = game(db, owner_link, "genshin", "800000001").await;
    mode(db, genshin, "auto").await;
    listed(db, "genshin", "FIRSTCODE", 3).await;
    listed(db, "genshin", "SECONDCODE", 2).await;
    listed(db, "genshin", "THIRDCODE", 1).await;
    let now = Utc::now();
    let planned = plan(db, owner, now).await.unwrap().unwrap();
    assert_eq!(planned.items.len(), 3);

    let Admission::Send(cookie) = admit(&store, &planned, &planned.items[0], now)
        .await
        .unwrap()
    else {
        panic!("first code admitted");
    };
    // Only the redemption Cookie is handed to the request.
    assert_eq!(
        cookie.redemption(),
        Some("cookie_token_v2=syntheticcookietoken100000001; account_id_v2=100000001")
    );
    let row = ledger_row(db, genshin, "FIRSTCODE").await.unwrap();
    assert_eq!((row.state.as_str(), row.posts), (SENDING, 1));
    assert!(row.last_sent_at.is_some());
    // A row that may have been sent is never admitted again, even after a restart.
    assert!(matches!(
        admit(&store, &planned, &planned.items[0], now)
            .await
            .unwrap(),
        Admission::Skip
    ));
    recover(db, None).await.unwrap();
    assert!(matches!(
        admit(&store, &planned, &planned.items[0], now)
            .await
            .unwrap(),
        Admission::Skip
    ));
    assert!(
        STATES.contains(
            &ledger_row(db, genshin, "FIRSTCODE")
                .await
                .unwrap()
                .state
                .as_str()
        )
    );
    assert!(
        plan(db, owner, now)
            .await
            .unwrap()
            .unwrap()
            .items
            .iter()
            .all(|item| item.code != "FIRSTCODE")
    );

    // A mode change, an unlisted code, a rejection: skip that item.
    redemption_code::Entity::update_many()
        .col_expr(redemption_code::Column::Listed, Expr::value(false))
        .filter(redemption_code::Column::Code.eq("SECONDCODE"))
        .exec(db)
        .await
        .unwrap();
    assert!(matches!(
        admit(&store, &planned, &planned.items[1], now)
            .await
            .unwrap(),
        Admission::Skip
    ));
    assert!(
        ledger_row(db, genshin, "SECONDCODE").await.is_none(),
        "never sent"
    );
    redemption_code_rejection::ActiveModel {
        game_kind: Set("genshin".to_owned()),
        code: Set("THIRDCODE".to_owned()),
        reason: Set("invalid".to_owned()),
        rejected_at: Set(now),
    }
    .insert(db)
    .await
    .unwrap();
    assert!(matches!(
        admit(&store, &planned, &planned.items[2], now)
            .await
            .unwrap(),
        Admission::Skip
    ));
    let third = ledger_row(db, genshin, "THIRDCODE").await.unwrap();
    assert_eq!((third.state.as_str(), third.posts), ("invalid", 0));
    mode(db, genshin, "notify").await;
    listed(db, "genshin", "FOURTHCODE", 0).await;
    let fourth = Item {
        code: "FOURTHCODE".to_owned(),
        ..planned.items[0].clone()
    };
    assert!(matches!(
        admit(&store, &planned, &fourth, now).await.unwrap(),
        Admission::Skip
    ));
    mode(db, genshin, "auto").await;

    // The administrator's switch, a new Cookie version, a pause, shutdown: stop the owner.
    redemption_code_source::Entity::update_many()
        .col_expr(redemption_code_source::Column::Enabled, Expr::value(false))
        .exec(db)
        .await
        .unwrap();
    assert!(matches!(
        admit(&store, &planned, &fourth, now).await.unwrap(),
        Admission::Stop
    ));
    redemption_code_source::Entity::update_many()
        .col_expr(redemption_code_source::Column::Enabled, Expr::value(true))
        .exec(db)
        .await
        .unwrap();
    hoyolab_account::Entity::update_many()
        .col_expr(
            hoyolab_account::Column::CredentialVersion,
            Expr::value(2_i64),
        )
        .filter(hoyolab_account::Column::Id.eq(owner_link))
        .exec(db)
        .await
        .unwrap();
    assert!(matches!(
        admit(&store, &planned, &fourth, now).await.unwrap(),
        Admission::Stop
    ));
    let replanned = plan(db, owner, now).await.unwrap().unwrap();
    store.stop_new_operations();
    assert!(matches!(
        admit(&store, &replanned, &fourth, now).await.unwrap(),
        Admission::Stop
    ));
    assert!(ledger_row(db, genshin, "FOURTHCODE").await.is_none());

    // A link without redemption credentials never sends, whatever its modes say.
    let store = CookieStore::new(db.clone(), Arc::new(test_key()));
    let plain_owner = ready_user(db, "plain").await.user.id;
    let plain_link = link(db, plain_owner, "100000003", "verified").await;
    let plain_uid = game(db, plain_link, "genshin", "800000003").await;
    mode(db, plain_uid, "auto").await;
    assert!(plan(db, plain_owner, now).await.unwrap().is_none());
    let forged = Plan {
        owner: plain_owner,
        link: plain_link,
        credential_version: 1,
        items: vec![Item {
            game_account: plain_uid,
            ..fourth.clone()
        }],
    };
    assert!(matches!(
        admit(&store, &forged, &forged.items[0], now).await.unwrap(),
        Admission::Stop
    ));
}

#[test]
fn rank_too_low_is_due_once_its_day_passed_and_only_for_catalogue_rows() {
    let now = Utc::now();
    let row = |state: &str, source: &str, next: Option<DateTime<Utc>>| redemption_ledger::Model {
        game_account_id: Uuid::nil(),
        code: "CODE".to_owned(),
        source: source.to_owned(),
        state: state.to_owned(),
        posts: 1,
        retcode: None,
        next_attempt_at: next,
        last_sent_at: Some(now),
        created_at: now,
        updated_at: now,
    };
    assert!(due(&row(PENDING, CATALOGUE, None), now));
    assert!(!due(
        &row(PENDING, CATALOGUE, Some(now + Duration::seconds(1))),
        now
    ));
    assert!(due(&row("rank_too_low", CATALOGUE, Some(now)), now));
    assert!(!due(
        &row("rank_too_low", CATALOGUE, Some(now + Duration::hours(1))),
        now
    ));
    assert!(!due(&row("rank_too_low", MANUAL, Some(now)), now));
    for state in [
        SENDING,
        "redeemed",
        "already_used",
        "expired",
        "invalid",
        "cookie_invalid",
        "captcha_required",
        "unknown",
    ] {
        assert!(!due(&row(state, CATALOGUE, None), now), "{state}");
    }
}
