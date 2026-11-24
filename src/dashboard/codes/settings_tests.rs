use super::*;
use crate::dashboard::{
    auth::sessions::Credential,
    codes::test_support::{game, link, ready_user, redeem_link},
    db::entities::user,
    test_support::TestDatabase,
};
use chrono::Duration;
use sea_orm::{ActiveModelTrait, PaginatorTrait};

#[test]
fn modes_and_redemption_links() {
    assert!(validate_mode("off").is_ok());
    assert!(validate_mode("notify").is_ok());
    // Valid as a value; whether a UID may take it depends on its link (`can_redeem`).
    assert!(MODES.contains(&"auto"));
    assert!(validate_mode("auto").is_ok());
    assert!(matches!(validate_mode("always"), Err(Error::Invalid(_))));
    assert_eq!(
        redeem_url("genshin", "GENSHINGIFT").as_deref(),
        Some("https://genshin.hoyoverse.com/en/gift?code=GENSHINGIFT")
    );
    assert_eq!(
        redeem_url("starrail", "STARRAILGIFT").as_deref(),
        Some("https://hsr.hoyoverse.com/gift?code=STARRAILGIFT")
    );
    assert_eq!(redeem_url("nap", "ZZZ"), None);
}

async fn code(db: &DatabaseConnection, game: &str, code: &str, listed: bool) -> Uuid {
    let now = Utc::now();
    redemption_code::ActiveModel {
        id: Set(Uuid::new_v4()),
        game_kind: Set(game.to_owned()),
        code: Set(code.to_owned()),
        rewards: Set("Primogem*60".to_owned()),
        first_seen_at: Set(now),
        last_seen_at: Set(now),
        listed: Set(listed),
        seeded: Set(false),
    }
    .insert(db)
    .await
    .unwrap()
    .id
}

#[tokio::test]
async fn modes_belong_to_the_verified_link_and_change_only_over_a_session() {
    let fixture = TestDatabase::new().await;
    let db = &fixture.db;
    let service = CodeSettings::new(db.clone());
    let actor = ready_user(db, "codes-owner").await;
    // No verified link yet: like reward settings, the UIDs are not there to configure.
    assert!(matches!(
        service.list(&actor).await,
        Err(Error::InitialSetupRequired)
    ));
    let owner_link = link(db, actor.user.id, "100000001", "verified").await;
    let genshin = game(db, owner_link, "genshin", "800000001").await;
    let rail = game(db, owner_link, "starrail", "900000001").await;
    let listed = service.list(&actor).await.unwrap();
    assert_eq!(listed.len(), 2);
    assert!(
        listed
            .iter()
            .all(|row| row.mode == "off" && row.enabled_at.is_none())
    );

    service.update(&actor, genshin, "notify").await.unwrap();
    let first = redemption_code_setting::Entity::find_by_id(genshin)
        .one(db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(first.mode, "notify");
    let since = first.enabled_at.unwrap();
    // Staying on keeps the start; going off clears it; on again starts anew.
    service.update(&actor, genshin, "notify").await.unwrap();
    let again = redemption_code_setting::Entity::find_by_id(genshin)
        .one(db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(again.enabled_at, Some(since));
    service.update(&actor, genshin, "off").await.unwrap();
    assert_eq!(
        redemption_code_setting::Entity::find_by_id(genshin)
            .one(db)
            .await
            .unwrap()
            .unwrap()
            .enabled_at,
        None
    );
    service.update(&actor, genshin, "notify").await.unwrap();

    // The link's Cookie has no cookie_token_v2: `auto` cannot run, so it is refused.
    assert!(matches!(
        service.update(&actor, rail, "auto").await,
        Err(Error::Invalid(NEEDS_REDEMPTION_COOKIE))
    ));
    assert!(matches!(
        service.update(&actor, rail, "sometimes").await,
        Err(Error::Invalid(_))
    ));
    // Another user's UID, or an unknown one, is not found.
    let other = ready_user(db, "codes-other").await;
    let other_link = link(db, other.user.id, "100000002", "verified").await;
    let foreign = game(db, other_link, "genshin", "800000009").await;
    for id in [foreign, Uuid::new_v4()] {
        assert!(matches!(
            service.update(&actor, id, "notify").await,
            Err(Error::NotFound)
        ));
    }
    let modes: Vec<_> = service
        .list(&actor)
        .await
        .unwrap()
        .into_iter()
        .map(|row| (row.game_uid, row.mode))
        .collect();
    assert_eq!(
        modes,
        [
            ("800000001".to_owned(), "notify".to_owned()),
            ("900000001".to_owned(), "off".to_owned())
        ]
    );

    // An API token may read but never change a mode, whatever the route allows.
    let token = crate::dashboard::auth::api_tokens::ApiTokens::issue(
        db,
        &actor,
        "codes tool",
        crate::dashboard::auth::api_tokens::Scope::Run,
    )
    .await
    .unwrap();
    let bearer = crate::dashboard::auth::api_tokens::ApiTokens::authenticate(
        db,
        &crate::dashboard::auth::api_tokens::hash(&token.token).unwrap(),
    )
    .await
    .unwrap();
    assert!(matches!(bearer.credential, Credential::ApiToken { .. }));
    assert_eq!(service.list(&bearer).await.unwrap().len(), 2);
    assert!(matches!(
        service.update(&bearer, rail, "notify").await,
        Err(Error::Unauthorized)
    ));
    // A signed-out session can do neither.
    Sessions::revoke_all(db, actor.user.id).await.unwrap();
    assert!(matches!(
        service.list(&actor).await,
        Err(Error::Unauthorized)
    ));
    assert!(matches!(
        service.update(&actor, rail, "notify").await,
        Err(Error::Unauthorized)
    ));
}

#[tokio::test]
async fn catalogue_and_history_show_results_and_available_codes() {
    let fixture = TestDatabase::new().await;
    let db = &fixture.db;
    let service = CodeSettings::new(db.clone());
    let actor = ready_user(db, "history-owner").await;
    let owner_link = link(db, actor.user.id, "100000001", "verified").await;
    let genshin = game(db, owner_link, "genshin", "800000001").await;
    code(db, "genshin", "FRESHCODE1", true).await;
    let announced = code(db, "genshin", "TOLDCODE1", true).await;
    let expired_told = code(db, "genshin", "GONECODE1", false).await;
    code(db, "genshin", "GONECODE2", false).await;
    code(db, "starrail", "RAILCODE1", true).await;
    for (id, state) in [(announced, "announced"), (expired_told, "expired")] {
        redemption_code_result::ActiveModel {
            game_account_id: Set(genshin),
            code_id: Set(id),
            state: Set(state.to_owned()),
            created_at: Set(Utc::now()),
            updated_at: Set(Utc::now()),
        }
        .insert(db)
        .await
        .unwrap();
    }

    let catalogue = serde_json::to_value(service.catalogue(&actor).await.unwrap()).unwrap();
    assert_eq!(catalogue["enabled"], true);
    // No fetch has run yet: none failed, and the first one is due now.
    assert_eq!(catalogue["consecutive_failures"], 0);
    assert!(catalogue["next_fetch_at"].is_string());
    assert_eq!(catalogue["codes"].as_array().unwrap().len(), 5);
    let rail = catalogue["codes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["code"] == "RAILCODE1")
        .unwrap();
    assert_eq!(
        rail["redeem_url"],
        "https://hsr.hoyoverse.com/gift?code=RAILCODE1"
    );
    assert_eq!(rail["rewards"], "Primogem*60");

    let history: Vec<(String, String)> = service
        .history(&actor)
        .await
        .unwrap()
        .into_iter()
        .map(|entry| {
            let value = serde_json::to_value(entry).unwrap();
            (
                value["code"].as_str().unwrap().to_owned(),
                value["state"].as_str().unwrap().to_owned(),
            )
        })
        .collect();
    let mut sorted = history.clone();
    sorted.sort();
    assert_eq!(
        sorted,
        [
            ("FRESHCODE1".to_owned(), "available".to_owned()),
            ("GONECODE1".to_owned(), "expired".to_owned()),
            ("TOLDCODE1".to_owned(), "announced".to_owned()),
        ],
        "Star Rail codes need a Star Rail UID; unlisted codes appear only with a result"
    );
}

#[tokio::test]
async fn deleting_a_user_removes_its_modes_and_results_but_not_the_catalogue() {
    let fixture = TestDatabase::new().await;
    let db = &fixture.db;
    let actor = ready_user(db, "deleted-owner").await;
    let owner_link = link(db, actor.user.id, "100000001", "verified").await;
    let genshin = game(db, owner_link, "genshin", "800000001").await;
    let told = code(db, "genshin", "TOLDCODE1", true).await;
    let tx = db.begin().await.unwrap();
    set_mode_in(&tx, &[genshin], "notify", Utc::now() - Duration::hours(1))
        .await
        .unwrap();
    tx.commit().await.unwrap();
    redemption_code_result::ActiveModel {
        game_account_id: Set(genshin),
        code_id: Set(told),
        state: Set("announced".to_owned()),
        created_at: Set(Utc::now()),
        updated_at: Set(Utc::now()),
    }
    .insert(db)
    .await
    .unwrap();
    // `Accounts::finish_deletion` ends with deleting the user row; the rest cascades.
    user::Entity::delete_by_id(actor.user.id)
        .exec(db)
        .await
        .unwrap();
    assert_eq!(
        redemption_code_setting::Entity::find()
            .count(db)
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        redemption_code_result::Entity::find()
            .count(db)
            .await
            .unwrap(),
        0
    );
    // The shared catalogue is never per user.
    assert_eq!(redemption_code::Entity::find().count(db).await.unwrap(), 1);
}

#[tokio::test]
async fn auto_needs_redemption_credentials_and_follows_the_cookie() {
    let fixture = TestDatabase::new().await;
    let db = &fixture.db;
    let service = CodeSettings::new(db.clone());
    let actor = ready_user(db, "auto-owner").await;
    let owner_link = redeem_link(db, actor.user.id, "100000001").await;
    let genshin = game(db, owner_link, "genshin", "800000001").await;
    let rail = game(db, owner_link, "starrail", "900000001").await;
    service.update(&actor, genshin, "auto").await.unwrap();
    service.update(&actor, rail, "notify").await.unwrap();
    let since = redemption_code_setting::Entity::find_by_id(genshin)
        .one(db)
        .await
        .unwrap()
        .unwrap()
        .enabled_at;
    assert!(since.is_some());

    // A Cookie update without redemption credentials keeps the UID announced (notify).
    let tx = db.begin().await.unwrap();
    auto_to_notify_in(&tx, owner_link).await.unwrap();
    tx.commit().await.unwrap();
    let modes: Vec<_> = service
        .list(&actor)
        .await
        .unwrap()
        .into_iter()
        .map(|row| row.mode)
        .collect();
    assert_eq!(modes, ["notify", "notify"]);
    assert_eq!(
        redemption_code_setting::Entity::find_by_id(genshin)
            .one(db)
            .await
            .unwrap()
            .unwrap()
            .enabled_at,
        since,
        "notify keeps when the UID started hearing about codes"
    );

    // Without the credentials `auto` is refused again; with them it is accepted.
    hoyolab_account::Entity::update_many()
        .col_expr(
            hoyolab_account::Column::RedemptionCredentials,
            sea_orm::sea_query::Expr::value(false),
        )
        .filter(hoyolab_account::Column::Id.eq(owner_link))
        .exec(db)
        .await
        .unwrap();
    assert!(matches!(
        service.update(&actor, genshin, "auto").await,
        Err(Error::Invalid(NEEDS_REDEMPTION_COOKIE))
    ));
    // Cookie removal turns everything off and keeps the ledger.
    let tx = db.begin().await.unwrap();
    turn_off_link(&tx, owner_link).await.unwrap();
    tx.commit().await.unwrap();
    assert!(
        service
            .list(&actor)
            .await
            .unwrap()
            .iter()
            .all(|row| row.mode == "off")
    );
}

#[tokio::test]
async fn history_keeps_redemptions_after_pruning_and_shows_entered_codes() {
    use crate::dashboard::codes::ledger::{CATALOGUE, MANUAL};
    use crate::dashboard::db::entities::{redemption_code_rejection, redemption_ledger};
    let fixture = TestDatabase::new().await;
    let db = &fixture.db;
    let service = CodeSettings::new(db.clone());
    let actor = ready_user(db, "ledger-history").await;
    let owner_link = redeem_link(db, actor.user.id, "100000001").await;
    let genshin = game(db, owner_link, "genshin", "800000001").await;
    code(db, "genshin", "LISTEDCODE", true).await;
    code(db, "genshin", "DEADLISTED", true).await;
    let now = Utc::now();
    for (code, source, state, retcode) in [
        ("LISTEDCODE", CATALOGUE, "redeemed", Some(0)),
        ("PRUNEDCODE", CATALOGUE, "already_used", Some(-2017)),
        ("MYOWNCODE", MANUAL, "pending", None),
    ] {
        redemption_ledger::ActiveModel {
            game_account_id: Set(genshin),
            code: Set(code.to_owned()),
            source: Set(source.to_owned()),
            state: Set(state.to_owned()),
            posts: Set(i32::from(state != "pending")),
            retcode: Set(retcode),
            next_attempt_at: Set(None),
            last_sent_at: Set((state != "pending").then_some(now)),
            created_at: Set(now),
            updated_at: Set(now),
        }
        .insert(db)
        .await
        .unwrap();
    }
    redemption_code_rejection::ActiveModel {
        game_kind: Set("genshin".to_owned()),
        code: Set("DEADLISTED".to_owned()),
        reason: Set("expired".to_owned()),
        rejected_at: Set(now),
    }
    .insert(db)
    .await
    .unwrap();
    let entries: Vec<serde_json::Value> = service
        .history(&actor)
        .await
        .unwrap()
        .into_iter()
        .map(|entry| serde_json::to_value(entry).unwrap())
        .collect();
    // HoYoLAB's number of each answer (null without one, and for global rejections).
    let retcodes: std::collections::BTreeMap<&str, &serde_json::Value> = entries
        .iter()
        .map(|value| (value["code"].as_str().unwrap(), &value["retcode"]))
        .collect();
    assert_eq!(retcodes["LISTEDCODE"], &serde_json::json!(0));
    assert_eq!(retcodes["PRUNEDCODE"], &serde_json::json!(-2017));
    assert!(retcodes["MYOWNCODE"].is_null() && retcodes["DEADLISTED"].is_null());
    let mut history: Vec<(String, String, String, bool)> = entries
        .iter()
        .map(|value| {
            (
                value["code"].as_str().unwrap().to_owned(),
                value["state"].as_str().unwrap().to_owned(),
                value["source"].as_str().unwrap().to_owned(),
                value["code_id"].is_null(),
            )
        })
        .collect();
    history.sort();
    let row = |code: &str, state: &str, source: &str, no_id: bool| {
        (code.to_owned(), state.to_owned(), source.to_owned(), no_id)
    };
    assert_eq!(
        history,
        [
            row("DEADLISTED", "expired", "catalogue", false),
            row("LISTEDCODE", "redeemed", "catalogue", false),
            row("MYOWNCODE", "pending", "manual", true),
            row("PRUNEDCODE", "already_used", "catalogue", true),
        ]
    );
    // Entered codes never join the shared catalogue.
    assert_eq!(redemption_code::Entity::find().count(db).await.unwrap(), 2);
}

#[tokio::test]
async fn catalogue_reports_failing_and_stopped_sources() {
    let fixture = TestDatabase::new().await;
    let db = &fixture.db;
    let service = CodeSettings::new(db.clone());
    let actor = ready_user(db, "source-reader").await;
    let attempt = Utc::now() - Duration::minutes(30);
    // The migration seeds the single source row; record three failed fetches on it.
    let mut failing: redemption_code_source::ActiveModel =
        redemption_code_source::Entity::find_by_id(1_i16)
            .one(db)
            .await
            .unwrap()
            .unwrap()
            .into();
    failing.enabled = Set(true);
    failing.interval_minutes = Set(180);
    failing.last_attempt_at = Set(Some(attempt));
    failing.last_success_at = Set(Some(attempt - Duration::hours(12)));
    failing.consecutive_failures = Set(3);
    failing.last_error = Set(Some("timeout".to_owned()));
    let source = failing.update(db).await.unwrap();
    let catalogue = serde_json::to_value(service.catalogue(&actor).await.unwrap()).unwrap();
    assert_eq!(catalogue["consecutive_failures"], 3);
    // After the third failure the worker retries an hour after the attempt, sooner than
    // the three-hour interval.
    let next: DateTime<Utc> = catalogue["next_fetch_at"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    assert!((next - (attempt + Duration::hours(1))).num_seconds().abs() <= 1);
    let mut stopped: redemption_code_source::ActiveModel = source.into();
    stopped.enabled = Set(false);
    stopped.update(db).await.unwrap();
    let catalogue = serde_json::to_value(service.catalogue(&actor).await.unwrap()).unwrap();
    assert_eq!(catalogue["enabled"], false);
    assert_eq!(catalogue["next_fetch_at"], serde_json::Value::Null);
}
