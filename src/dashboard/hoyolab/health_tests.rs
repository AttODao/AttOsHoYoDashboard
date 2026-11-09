use super::*;
use crate::dashboard::test_support::{TEMP_PASSWORD, TestDatabase};
use chrono::Duration;
use sea_orm::{ActiveModelTrait, Set};

fn health(status: &str, games: Vec<GameHealth>) -> LinkHealth {
    LinkHealth {
        status: status.to_owned(),
        verified_at: None,
        status_fetched_at: None,
        games,
    }
}

fn game(setting_status: &str, run: Option<&str>) -> GameHealth {
    GameHealth {
        game_kind: "genshin".to_owned(),
        automatic: true,
        setting_status: setting_status.to_owned(),
        last_run: run.map(|state| LastRun {
            state: state.to_owned(),
            ended_at: Utc::now(),
            error_code: None,
        }),
    }
}

#[test]
fn attention_follows_cookie_setting_and_final_run_states() {
    assert!(!LinkHealth::not_configured().needs_attention());
    assert_eq!(LinkHealth::not_configured().status, NOT_CONFIGURED);
    for status in ["pending_verification", "verified", "removed"] {
        assert!(!health(status, vec![]).needs_attention(), "{status}");
    }
    assert!(health("cookie_invalid", vec![]).needs_attention());
    for state in ["success", "already_claimed", "stopped"] {
        assert!(
            !health("verified", vec![game("ready", Some(state))]).needs_attention(),
            "{state}"
        );
    }
    for state in ATTENTION_RUN_STATES {
        assert!(
            health("verified", vec![game("ready", Some(state))]).needs_attention(),
            "{state}"
        );
    }
    assert!(!health("verified", vec![game("ready", None)]).needs_attention());
    // Kept history of a removed or not yet verified link is not a call for help.
    for status in ["removed", "pending_verification", NOT_CONFIGURED] {
        assert!(
            !health(status, vec![game("action_required", Some("failed"))]).needs_attention(),
            "{status}"
        );
    }
    assert!(health("verified", vec![game("action_required", None)]).needs_attention());
    let mut off = game("ready", Some("success"));
    off.automatic = false;
    let both = health("verified", vec![off, game("ready", None)]);
    assert_eq!(both.automatic_claims(), 1);
}

async fn link(db: &TestDatabase, owner: Uuid, status: &str, uid: &str) -> hoyolab_account::Model {
    hoyolab_account::ActiveModel {
        id: Set(Uuid::new_v4()),
        owner_user_id: Set(owner),
        hoyolab_uid: Set(Some(uid.to_owned())),
        encrypted_credentials: Set(Some(vec![0; 32])),
        credential_version: Set(1),
        auth_status: Set(status.to_owned()),
        created_at: Set(Utc::now()),
        updated_at: Set(Utc::now()),
        verified_at: Set(Some(Utc::now() - Duration::days(3))),
    }
    .insert(&db.db)
    .await
    .unwrap()
}

async fn status_cache(
    db: &TestDatabase,
    link: Uuid,
    uid: &str,
    fetched_at: Option<DateTime<Utc>>,
    error_code: Option<&str>,
) {
    let account = game_account::ActiveModel {
        id: Set(Uuid::new_v4()),
        hoyolab_account_id: Set(link),
        game_kind: Set("genshin".to_owned()),
        game_uid: Set(uid.to_owned()),
        region: Set("os_asia".to_owned()),
        visible: Set(true),
        detected: Set(true),
        last_detected_at: Set(Utc::now()),
    }
    .insert(&db.db)
    .await
    .unwrap();
    game_status_cache::ActiveModel {
        game_account_id: Set(account.id),
        fetched_at: Set(fetched_at),
        updated_at: Set(Utc::now()),
        payload: Set(None),
        payload_version: Set(1),
        error_code: Set(error_code.map(str::to_owned)),
        error_at: Set(error_code.map(|_| Utc::now())),
    }
    .insert(&db.db)
    .await
    .unwrap();
}

async fn setting(db: &TestDatabase, link: Uuid, game: &str, enabled: bool, status: &str) -> Uuid {
    reward_setting::ActiveModel {
        id: Set(Uuid::new_v4()),
        hoyolab_account_id: Set(link),
        game_kind: Set(game.to_owned()),
        enabled: Set(enabled),
        local_time: Set("06:00".to_owned()),
        auth_status: Set(status.to_owned()),
        next_run_at: Set(None),
    }
    .insert(&db.db)
    .await
    .unwrap()
    .id
}

/// A run with one attempt per entry of `codes`, in order.
async fn run(
    db: &TestDatabase,
    setting: Uuid,
    state: &str,
    ended_at: Option<DateTime<Utc>>,
    codes: &[Option<&str>],
) {
    let created = ended_at.unwrap_or_else(Utc::now) - Duration::minutes(1);
    let run = reward_run::ActiveModel {
        id: Set(Uuid::new_v4()),
        setting_id: Set(setting),
        trigger: Set("scheduled".to_owned()),
        state: Set(state.to_owned()),
        created_at: Set(created),
        updated_at: Set(created),
        ended_at: Set(ended_at),
        attempt_count: Set(codes.len() as i32),
        credential_version: Set(1),
        send_state: Set("not_sent".to_owned()),
        next_retry_at: Set(None),
    }
    .insert(&db.db)
    .await
    .unwrap();
    for (index, code) in codes.iter().enumerate() {
        reward_attempt::ActiveModel {
            id: Set(Uuid::new_v4()),
            run_id: Set(run.id),
            number: Set(index as i32 + 1),
            started_at: Set(created),
            finished_at: Set(ended_at),
            outcome: Set(if index + 1 == codes.len() {
                state
            } else {
                "retry_wait"
            }
            .to_owned()),
            error_code: Set(code.map(str::to_owned)),
            send_state: Set("not_sent".to_owned()),
        }
        .insert(&db.db)
        .await
        .unwrap();
    }
}

#[tokio::test]
async fn by_owner_reports_newest_fetch_and_last_ended_run_per_game() {
    let test = TestDatabase::new().await;
    let (accounts, admin) = test.ready_admin().await;
    let member = accounts
        .create_user(&admin, "member".to_owned(), TEMP_PASSWORD.to_owned())
        .await
        .unwrap();
    let expired = accounts
        .create_user(&admin, "expired".to_owned(), TEMP_PASSWORD.to_owned())
        .await
        .unwrap();
    let now = Utc::now();

    let verified = link(&test, member.id, "verified", "101").await;
    let newest = now - Duration::minutes(5);
    status_cache(
        &test,
        verified.id,
        "800000001",
        Some(now - Duration::hours(2)),
        None,
    )
    .await;
    status_cache(&test, verified.id, "800000002", Some(newest), None).await;
    // A failed fetch leaves no success time and is not reported as one.
    status_cache(&test, verified.id, "800000003", None, Some("network")).await;

    let starrail = setting(&test, verified.id, "starrail", false, "ready").await;
    let genshin = setting(&test, verified.id, "genshin", true, "ready").await;
    let failed_at = now - Duration::hours(1);
    run(
        &test,
        genshin,
        "success",
        Some(now - Duration::days(1)),
        &[None],
    )
    .await;
    run(
        &test,
        genshin,
        "failed",
        Some(failed_at),
        &[Some("network"), Some("upstream:-5003")],
    )
    .await;
    // Runs still in flight do not replace the last outcome.
    run(&test, genshin, "running", None, &[]).await;
    run(
        &test,
        starrail,
        "success",
        Some(now - Duration::hours(3)),
        &[],
    )
    .await;

    link(&test, expired.id, "cookie_invalid", "202").await;

    let rows = by_owner(&test.db).await.unwrap();
    assert!(!rows.contains_key(&admin.user.id));
    let member_health = &rows[&member.id];
    assert_eq!(member_health.status, "verified");
    assert!(member_health.verified_at.is_some());
    let fetched = member_health.status_fetched_at.unwrap();
    assert!((fetched - newest).num_milliseconds().abs() < 1);
    assert_eq!(
        member_health
            .games
            .iter()
            .map(|game| (game.game_kind.as_str(), game.automatic))
            .collect::<Vec<_>>(),
        [("genshin", true), ("starrail", false)]
    );
    let last = member_health.games[0].last_run.as_ref().unwrap();
    assert_eq!(last.state, "failed");
    assert!((last.ended_at - failed_at).num_milliseconds().abs() < 1);
    assert_eq!(last.error_code.as_deref(), Some("upstream:-5003"));
    let other = member_health.games[1].last_run.as_ref().unwrap();
    assert_eq!(other.state, "success");
    assert_eq!(other.error_code, None);
    assert_eq!(member_health.automatic_claims(), 1);
    assert!(member_health.needs_attention());

    let expired_health = &rows[&expired.id];
    assert_eq!(expired_health.status, "cookie_invalid");
    assert_eq!(expired_health.status_fetched_at, None);
    assert!(expired_health.games.is_empty());
    assert!(expired_health.needs_attention());
}
