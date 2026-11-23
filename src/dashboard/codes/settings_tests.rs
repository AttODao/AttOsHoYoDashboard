use super::*;
use crate::dashboard::{
    auth::sessions::Credential,
    codes::test_support::{game, link, ready_user},
    db::entities::user,
    test_support::TestDatabase,
};
use chrono::Duration;
use sea_orm::{ActiveModelTrait, PaginatorTrait};

#[test]
fn modes_and_redemption_links() {
    assert!(validate_mode("off").is_ok());
    assert!(validate_mode("notify").is_ok());
    // Stored and validated, but refused until automatic redemption exists.
    assert!(MODES.contains(&"auto"));
    assert!(matches!(validate_mode("auto"), Err(Error::Invalid(_))));
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

    assert!(matches!(
        service.update(&actor, rail, "auto").await,
        Err(Error::Invalid(_))
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
