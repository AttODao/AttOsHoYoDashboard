//! Focused schema checks for Dashboard game, reward, cache, and deletion state.
use super::*;
use crate::dashboard::{
    db::entities::{
        deletion_request, game_account, game_status_cache, hoyolab_account, reward_attempt,
        reward_guard, reward_run, reward_setting, reward_status_cache, user,
    },
    test_support::TestDatabase,
};
use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ConnectionTrait, DatabaseConnection, DbBackend, EntityTrait, PaginatorTrait,
    Set, Statement,
};
use sea_orm_migration::MigratorTrait;
use uuid::Uuid;

async fn insert_user(db: &DatabaseConnection, username: &str) -> user::Model {
    user::ActiveModel {
        id: Set(Uuid::new_v4()),
        username: Set(username.to_owned()),
        password_hash: Set("synthetic-private-hash".to_owned()),
        is_admin: Set(false),
        active: Set(true),
        must_change_password: Set(false),
        created_at: Set(Utc::now()),
        last_login_at: Set(None),
    }
    .insert(db)
    .await
    .unwrap()
}

async fn insert_hoyolab(db: &DatabaseConnection, owner_user_id: Uuid) -> hoyolab_account::Model {
    let model = hoyolab_account::ActiveModel {
        id: Set(Uuid::new_v4()),
        owner_user_id: Set(owner_user_id),
        hoyolab_uid: Set(Some("123456789".to_owned())),
        encrypted_credentials: Set(Some(vec![7; 30])),
        credential_version: Set(1),
        auth_status: Set("verified".to_owned()),
        created_at: Set(Utc::now()),
        updated_at: Set(Utc::now()),
        verified_at: Set(None),
    }
    .insert(db)
    .await
    .unwrap();
    assert_debug_allowlist(&model, "HoyolabAccount { .. }");
    model
}

async fn insert_game_account(
    db: &DatabaseConnection,
    hoyolab_account_id: Uuid,
) -> game_account::Model {
    game_account::ActiveModel {
        id: Set(Uuid::new_v4()),
        hoyolab_account_id: Set(hoyolab_account_id),
        game_kind: Set("genshin".to_owned()),
        game_uid: Set("800000001".to_owned()),
        region: Set("os_usa".to_owned()),
        last_detected_at: Set(Utc::now()),
        ..Default::default()
    }
    .insert(db)
    .await
    .unwrap()
}

async fn insert_reward_setting(
    db: &DatabaseConnection,
    hoyolab_account_id: Uuid,
) -> reward_setting::Model {
    reward_setting::ActiveModel {
        id: Set(Uuid::new_v4()),
        hoyolab_account_id: Set(hoyolab_account_id),
        game_kind: Set("starrail".to_owned()),
        ..Default::default()
    }
    .insert(db)
    .await
    .unwrap()
}

async fn insert_reward_run(db: &DatabaseConnection, setting_id: Uuid) -> reward_run::Model {
    reward_run::ActiveModel {
        id: Set(Uuid::new_v4()),
        setting_id: Set(setting_id),
        trigger: Set("manual".to_owned()),
        state: Set("pending".to_owned()),
        created_at: Set(Utc::now()),
        updated_at: Set(Utc::now()),
        attempt_count: Set(0),
        credential_version: Set(1),
        send_state: Set("not_sent".to_owned()),
        ..Default::default()
    }
    .insert(db)
    .await
    .unwrap()
}

fn assert_debug_allowlist(model: &impl std::fmt::Debug, expected: &str) {
    for diagnostic in [format!("{model:?}"), format!("{model:#?}")] {
        assert_eq!(diagnostic, expected);
    }
}

async fn exec_err(db: &DatabaseConnection, sql: String) {
    assert!(
        db.execute_unprepared(&sql).await.is_err(),
        "statement unexpectedly succeeded: {sql}"
    );
}

#[tokio::test]
async fn dashboard_games_migration_is_ordered_reversible_and_updates_hoyolab_state_check() {
    let fixture = TestDatabase::new().await;
    let rows = fixture
        .db
        .query_all_raw(Statement::from_string(
            DbBackend::Postgres,
            "SELECT version FROM seaql_migrations ORDER BY version",
        ))
        .await
        .unwrap();
    let names: Vec<String> = rows
        .iter()
        .map(|row| row.try_get("", "version").unwrap())
        .collect();
    assert_eq!(
        names.iter().rev().nth(5).map(String::as_str),
        Some("m20261013_000004_dashboard_games")
    );

    let owner = insert_user(&fixture.db, "owner").await;
    let account = insert_hoyolab(&fixture.db, owner.id).await;
    fixture
        .db
        .execute_unprepared(&format!(
            "UPDATE hoyolab_accounts SET auth_status = 'cookie_invalid' WHERE id = '{}'",
            account.id
        ))
        .await
        .unwrap();

    // The later Cookie-removal, activity, notification, API token and app notification migrations are undone first.
    migrations::Migrator::down(&fixture.db, Some(6))
        .await
        .unwrap();
    let row = fixture
        .db
        .query_one_raw(Statement::from_string(
            DbBackend::Postgres,
            "SELECT to_regclass('public.game_accounts')::text AS name",
        ))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.try_get::<Option<String>>("", "name").unwrap(), None);
    let row = fixture
        .db
        .query_one_raw(Statement::from_string(
            DbBackend::Postgres,
            format!(
                "SELECT auth_status FROM hoyolab_accounts WHERE id = '{}'",
                account.id
            ),
        ))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        row.try_get::<String>("", "auth_status").unwrap(),
        "verified"
    );

    migrations::Migrator::up(&fixture.db, None).await.unwrap();
    let row = fixture
        .db
        .query_one_raw(Statement::from_string(
            DbBackend::Postgres,
            "SELECT to_regclass('public.game_accounts')::text AS name",
        ))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        row.try_get::<Option<String>>("", "name").unwrap(),
        Some("game_accounts".to_owned())
    );
}

#[tokio::test]
async fn game_accounts_enforce_identity_defaults_foreign_keys_and_cascade() {
    let fixture = TestDatabase::new().await;
    let owner = insert_user(&fixture.db, "owner").await;
    let account = insert_hoyolab(&fixture.db, owner.id).await;
    let game = insert_game_account(&fixture.db, account.id).await;
    assert!(!game.visible);
    assert!(game.detected);
    assert_debug_allowlist(
        &game,
        &format!(
            "GameAccount {{ id: {:?}, hoyolab_account_id: {:?}, .. }}",
            game.id, game.hoyolab_account_id
        ),
    );

    let duplicate = game_account::ActiveModel {
        id: Set(Uuid::new_v4()),
        hoyolab_account_id: Set(account.id),
        game_kind: Set("genshin".to_owned()),
        game_uid: Set("800000001".to_owned()),
        region: Set("os_usa".to_owned()),
        last_detected_at: Set(Utc::now()),
        ..Default::default()
    }
    .insert(&fixture.db)
    .await;
    assert!(
        duplicate.is_err(),
        "same account/kind/uid/region must be unique"
    );

    exec_err(
        &fixture.db,
        format!(
            "INSERT INTO game_accounts \
             (id, hoyolab_account_id, game_kind, game_uid, region, last_detected_at) \
             VALUES ('{}', '{}', 'zzz', '800000002', 'os_usa', NOW())",
            Uuid::new_v4(),
            account.id
        ),
    )
    .await;
    exec_err(
        &fixture.db,
        format!(
            "INSERT INTO game_accounts \
             (id, hoyolab_account_id, game_kind, game_uid, region, last_detected_at) \
             VALUES ('{}', '{}', 'genshin', '0001', 'os_usa', NOW())",
            Uuid::new_v4(),
            account.id
        ),
    )
    .await;
    exec_err(
        &fixture.db,
        format!(
            "INSERT INTO game_accounts \
             (id, hoyolab_account_id, game_kind, game_uid, region, last_detected_at) \
             VALUES ('{}', '{}', 'genshin', '800000003', 'os_usa', NOW())",
            Uuid::new_v4(),
            Uuid::new_v4()
        ),
    )
    .await;

    hoyolab_account::Entity::delete_by_id(account.id)
        .exec(&fixture.db)
        .await
        .unwrap();
    assert_eq!(
        game_account::Entity::find()
            .count(&fixture.db)
            .await
            .unwrap(),
        0
    );
}

#[tokio::test]
async fn reward_settings_enforce_per_game_defaults_and_time_state_checks() {
    let fixture = TestDatabase::new().await;
    let owner = insert_user(&fixture.db, "owner").await;
    let account = insert_hoyolab(&fixture.db, owner.id).await;
    let setting = insert_reward_setting(&fixture.db, account.id).await;
    assert!(!setting.enabled);
    assert_eq!(setting.local_time, "06:00");
    assert_eq!(setting.auth_status, "ready");
    assert_debug_allowlist(
        &setting,
        &format!(
            "RewardSetting {{ id: {:?}, hoyolab_account_id: {:?}, .. }}",
            setting.id, setting.hoyolab_account_id
        ),
    );

    let duplicate = reward_setting::ActiveModel {
        id: Set(Uuid::new_v4()),
        hoyolab_account_id: Set(account.id),
        game_kind: Set("starrail".to_owned()),
        ..Default::default()
    }
    .insert(&fixture.db)
    .await;
    assert!(
        duplicate.is_err(),
        "same account/game setting must be unique"
    );

    for (column, value) in [
        ("game_kind", "zzz"),
        ("local_time", "24:00"),
        ("local_time", "6:00"),
        ("auth_status", "cookie_invalid"),
    ] {
        let id = Uuid::new_v4();
        let sql = match column {
            "game_kind" => format!(
                "INSERT INTO reward_settings (id, hoyolab_account_id, game_kind) \
                 VALUES ('{id}', '{}', '{value}')",
                account.id
            ),
            "local_time" => format!(
                "INSERT INTO reward_settings (id, hoyolab_account_id, game_kind, local_time) \
                 VALUES ('{id}', '{}', 'genshin', '{value}')",
                account.id
            ),
            _ => format!(
                "INSERT INTO reward_settings (id, hoyolab_account_id, game_kind, auth_status) \
                 VALUES ('{id}', '{}', 'genshin', '{value}')",
                account.id
            ),
        };
        exec_err(&fixture.db, sql).await;
    }

    hoyolab_account::Entity::delete_by_id(account.id)
        .exec(&fixture.db)
        .await
        .unwrap();
    assert_eq!(
        reward_setting::Entity::find()
            .count(&fixture.db)
            .await
            .unwrap(),
        0
    );
}

#[tokio::test]
async fn reward_runs_attempts_and_guards_enforce_state_checks_and_cascade() {
    let fixture = TestDatabase::new().await;
    let owner = insert_user(&fixture.db, "owner").await;
    let account = insert_hoyolab(&fixture.db, owner.id).await;
    let setting = insert_reward_setting(&fixture.db, account.id).await;
    let run = insert_reward_run(&fixture.db, setting.id).await;
    assert_eq!(run.attempt_count, 0);
    assert_eq!(run.send_state, "not_sent");
    assert_debug_allowlist(
        &run,
        &format!(
            "RewardRun {{ id: {:?}, setting_id: {:?}, .. }}",
            run.id, run.setting_id
        ),
    );

    for sql in [
        format!(
            "INSERT INTO reward_runs \
             (id, setting_id, trigger, state, created_at, updated_at, attempt_count, credential_version, send_state) \
             VALUES ('{}', '{}', 'bad', 'pending', NOW(), NOW(), 0, 1, 'not_sent')",
            Uuid::new_v4(),
            setting.id
        ),
        format!(
            "INSERT INTO reward_runs \
             (id, setting_id, trigger, state, created_at, updated_at, attempt_count, credential_version, send_state) \
             VALUES ('{}', '{}', 'manual', 'bad', NOW(), NOW(), 0, 1, 'not_sent')",
            Uuid::new_v4(),
            setting.id
        ),
        format!(
            "INSERT INTO reward_runs \
             (id, setting_id, trigger, state, created_at, updated_at, attempt_count, credential_version, send_state) \
             VALUES ('{}', '{}', 'manual', 'pending', NOW(), NOW(), 5, 1, 'not_sent')",
            Uuid::new_v4(),
            setting.id
        ),
        format!(
            "INSERT INTO reward_runs \
             (id, setting_id, trigger, state, created_at, updated_at, attempt_count, credential_version, send_state) \
             VALUES ('{}', '{}', 'manual', 'pending', NOW(), NOW(), 0, 0, 'not_sent')",
            Uuid::new_v4(),
            setting.id
        ),
        format!(
            "INSERT INTO reward_runs \
             (id, setting_id, trigger, state, created_at, updated_at, attempt_count, credential_version, send_state) \
             VALUES ('{}', '{}', 'manual', 'pending', NOW(), NOW(), 0, 1, 'lost')",
            Uuid::new_v4(),
            setting.id
        ),
    ] {
        exec_err(&fixture.db, sql).await;
    }

    let attempt = reward_attempt::ActiveModel {
        id: Set(Uuid::new_v4()),
        run_id: Set(run.id),
        number: Set(1),
        started_at: Set(Utc::now()),
        outcome: Set("unknown".to_owned()),
        error_code: Set(Some("retcode:-1".to_owned())),
        send_state: Set("not_sent".to_owned()),
        ..Default::default()
    }
    .insert(&fixture.db)
    .await
    .unwrap();
    assert_eq!(attempt.send_state, "not_sent");
    assert_debug_allowlist(
        &attempt,
        &format!(
            "RewardAttempt {{ id: {:?}, run_id: {:?}, number: {:?}, .. }}",
            attempt.id, attempt.run_id, attempt.number
        ),
    );

    let duplicate = reward_attempt::ActiveModel {
        id: Set(Uuid::new_v4()),
        run_id: Set(run.id),
        number: Set(1),
        started_at: Set(Utc::now()),
        outcome: Set("pending".to_owned()),
        send_state: Set("not_sent".to_owned()),
        ..Default::default()
    }
    .insert(&fixture.db)
    .await;
    assert!(duplicate.is_err(), "same run/attempt number must be unique");

    for sql in [
        format!(
            "INSERT INTO reward_attempts (id, run_id, number, started_at, outcome, send_state) \
             VALUES ('{}', '{}', 0, NOW(), 'pending', 'not_sent')",
            Uuid::new_v4(),
            run.id
        ),
        format!(
            "INSERT INTO reward_attempts (id, run_id, number, started_at, outcome, send_state) \
             VALUES ('{}', '{}', 2, NOW(), 'bad', 'not_sent')",
            Uuid::new_v4(),
            run.id
        ),
        format!(
            "INSERT INTO reward_attempts (id, run_id, number, started_at, outcome, error_code, send_state) \
             VALUES ('{}', '{}', 2, NOW(), 'failed', 'message with spaces', 'not_sent')",
            Uuid::new_v4(),
            run.id
        ),
    ] {
        exec_err(&fixture.db, sql).await;
    }

    let guard = reward_guard::ActiveModel {
        setting_id: Set(setting.id),
        updated_at: Set(Utc::now()),
        ..Default::default()
    }
    .insert(&fixture.db)
    .await
    .unwrap();
    assert!(!guard.post_may_have_happened);
    assert_debug_allowlist(
        &guard,
        &format!("RewardGuard {{ setting_id: {:?}, .. }}", guard.setting_id),
    );

    reward_setting::Entity::delete_by_id(setting.id)
        .exec(&fixture.db)
        .await
        .unwrap();
    assert_eq!(
        reward_run::Entity::find().count(&fixture.db).await.unwrap(),
        0
    );
    assert_eq!(
        reward_attempt::Entity::find()
            .count(&fixture.db)
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        reward_guard::Entity::find()
            .count(&fixture.db)
            .await
            .unwrap(),
        0
    );
}

#[tokio::test]
async fn game_status_cache_allows_initial_failure_and_redacts_payload_and_errors() {
    let fixture = TestDatabase::new().await;
    let owner = insert_user(&fixture.db, "owner").await;
    let account = insert_hoyolab(&fixture.db, owner.id).await;
    let game = insert_game_account(&fixture.db, account.id).await;

    let cache = game_status_cache::ActiveModel {
        game_account_id: Set(game.id),
        updated_at: Set(Utc::now()),
        payload: Set(Some(serde_json::json!({"private_note": "do-not-log"}))),
        payload_version: Set(1),
        error_code: Set(Some("retcode:-1".to_owned())),
        error_at: Set(Some(Utc::now())),
        ..Default::default()
    }
    .insert(&fixture.db)
    .await
    .unwrap();
    assert_eq!(cache.fetched_at, None);
    assert_eq!(cache.payload_version, 1);
    assert_debug_allowlist(
        &cache,
        &format!(
            "GameStatusCache {{ game_account_id: {:?}, fetched_at: {:?}, updated_at: {:?}, .. }}",
            cache.game_account_id, cache.fetched_at, cache.updated_at
        ),
    );

    for sql in [
        format!(
            "INSERT INTO game_status_cache \
             (game_account_id, updated_at, payload_version) \
             VALUES ('{}', NOW(), 0)",
            Uuid::new_v4()
        ),
        format!(
            "UPDATE game_status_cache SET payload_version = 0 WHERE game_account_id = '{}'",
            game.id
        ),
        format!(
            "UPDATE game_status_cache SET error_code = 'message with spaces' \
             WHERE game_account_id = '{}'",
            game.id
        ),
    ] {
        exec_err(&fixture.db, sql).await;
    }

    game_account::Entity::delete_by_id(game.id)
        .exec(&fixture.db)
        .await
        .unwrap();
    assert_eq!(
        game_status_cache::Entity::find()
            .count(&fixture.db)
            .await
            .unwrap(),
        0
    );
}

#[tokio::test]
async fn reward_status_cache_allows_initial_failure_default_version_and_cascade() {
    let fixture = TestDatabase::new().await;
    let owner = insert_user(&fixture.db, "owner").await;
    let account = insert_hoyolab(&fixture.db, owner.id).await;
    let setting = insert_reward_setting(&fixture.db, account.id).await;

    let cache = reward_status_cache::ActiveModel {
        setting_id: Set(setting.id),
        updated_at: Set(Utc::now()),
        payload: Set(Some(serde_json::json!({
            "is_sign": false,
            "total_sign_day": 3,
            "private_note": "do-not-log"
        }))),
        error_code: Set(Some("retcode:-1".to_owned())),
        error_at: Set(Some(Utc::now())),
        ..Default::default()
    }
    .insert(&fixture.db)
    .await
    .unwrap();
    assert_eq!(cache.fetched_at, None);
    assert_eq!(cache.payload_version, 1);
    assert_debug_allowlist(
        &cache,
        &format!(
            "RewardStatusCache {{ setting_id: {:?}, fetched_at: {:?}, updated_at: {:?}, .. }}",
            cache.setting_id, cache.fetched_at, cache.updated_at
        ),
    );

    for sql in [
        format!(
            "INSERT INTO reward_status_cache \
             (setting_id, updated_at, payload_version) \
             VALUES ('{}', NOW(), 1)",
            Uuid::new_v4()
        ),
        format!(
            "UPDATE reward_status_cache SET payload_version = 0 WHERE setting_id = '{}'",
            setting.id
        ),
        format!(
            "UPDATE reward_status_cache SET error_code = 'message with spaces' \
             WHERE setting_id = '{}'",
            setting.id
        ),
    ] {
        exec_err(&fixture.db, sql).await;
    }

    reward_setting::Entity::delete_by_id(setting.id)
        .exec(&fixture.db)
        .await
        .unwrap();
    assert_eq!(
        reward_status_cache::Entity::find()
            .count(&fixture.db)
            .await
            .unwrap(),
        0
    );
}

#[tokio::test]
async fn deletion_requests_cascade_with_target_and_null_deleted_requester() {
    let fixture = TestDatabase::new().await;
    let target = insert_user(&fixture.db, "target").await;
    let requester = insert_user(&fixture.db, "requester").await;
    let row = deletion_request::ActiveModel {
        user_id: Set(target.id),
        requested_by_user_id: Set(Some(requester.id)),
        created_at: Set(Utc::now()),
    }
    .insert(&fixture.db)
    .await
    .unwrap();
    assert_eq!(row.requested_by_user_id, Some(requester.id));
    assert_debug_allowlist(
        &row,
        &format!(
            "DeletionRequest {{ user_id: {:?}, requested_by_user_id: {:?}, .. }}",
            row.user_id, row.requested_by_user_id
        ),
    );

    exec_err(
        &fixture.db,
        format!(
            "INSERT INTO deletion_requests (user_id, requested_by_user_id, created_at) \
             VALUES ('{}', NULL, NOW())",
            Uuid::new_v4()
        ),
    )
    .await;

    user::Entity::delete_by_id(requester.id)
        .exec(&fixture.db)
        .await
        .unwrap();
    let row = deletion_request::Entity::find_by_id(target.id)
        .one(&fixture.db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.requested_by_user_id, None);

    user::Entity::delete_by_id(target.id)
        .exec(&fixture.db)
        .await
        .unwrap();
    assert_eq!(
        deletion_request::Entity::find()
            .count(&fixture.db)
            .await
            .unwrap(),
        0
    );
}

#[tokio::test]
async fn removed_cookie_state_requires_no_ciphertext_and_reverts_by_deleting_the_link() {
    let fixture = TestDatabase::new().await;
    let owner = insert_user(&fixture.db, "owner").await;
    let account = insert_hoyolab(&fixture.db, owner.id).await;
    let db = &fixture.db;
    // Removal must drop the ciphertext, and only removal may.
    exec_err(
        db,
        format!(
            "UPDATE hoyolab_accounts SET auth_status = 'removed' WHERE id = '{}'",
            account.id
        ),
    )
    .await;
    exec_err(
        db,
        format!(
            "UPDATE hoyolab_accounts SET encrypted_credentials = NULL WHERE id = '{}'",
            account.id
        ),
    )
    .await;
    db.execute_unprepared(&format!(
        "UPDATE hoyolab_accounts SET auth_status = 'removed', encrypted_credentials = NULL \
         WHERE id = '{}'",
        account.id
    ))
    .await
    .unwrap();
    exec_err(
        db,
        format!(
            "UPDATE hoyolab_accounts SET hoyolab_uid = NULL WHERE id = '{}'",
            account.id
        ),
    )
    .await;

    migrations::Migrator::down(db, Some(5)).await.unwrap();
    // The entity now names a column the older schema lacks: count with raw SQL.
    let remaining = db
        .query_one_raw(Statement::from_string(
            DbBackend::Postgres,
            "SELECT count(*) AS n FROM hoyolab_accounts",
        ))
        .await
        .unwrap()
        .unwrap()
        .try_get::<i64>("", "n")
        .unwrap();
    assert_eq!(
        remaining, 0,
        "a removed link cannot satisfy the older non-null schema"
    );
    migrations::Migrator::up(db, None).await.unwrap();
}
