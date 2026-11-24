//! Real isolated PostgreSQL checks for schema ordering, constraints, and stored model privacy.
use super::*;
use crate::dashboard::{
    db::entities::{audit, passkey, session, user},
    test_support::TestDatabase,
};
use chrono::{Duration, Utc};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DbBackend, EntityTrait, PaginatorTrait,
    QueryFilter, Set, SqlErr, Statement,
};
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

#[tokio::test]
async fn migrations_are_ordered_idempotent_and_reversible() {
    let fixture = TestDatabase::new().await;
    migrate(&fixture.db).await.unwrap();
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
        .map(|r| r.try_get("", "version").unwrap())
        .collect();
    assert_eq!(
        names,
        [
            "m20261012_000001_accounts",
            "m20261012_000002_passkeys",
            "m20261013_000003_hoyolab_accounts",
            "m20261013_000004_dashboard_games",
            "m20261020_000005_hoyolab_cookie_removal",
            "m20261020_000006_account_activity",
            "m20261020_000007_notifications",
            "m20261027_000008_api_tokens",
            "m20261028_000009_app_notifications",
            "m20261028_000010_remove_web_push",
            "m20261028_000011_app_notification_events",
            "m20261109_000012_game_challenge_cache",
            "m20261109_000013_app_device_push",
            "m20261109_000014_game_calendar_cache",
            "m20261109_000015_resource_history",
            "m20261123_000016_redemption_codes",
            "m20261124_000017_code_redemption"
        ]
    );
    migrations::Migrator::down(&fixture.db, None).await.unwrap();
    let row = fixture
        .db
        .query_one_raw(Statement::from_string(
            DbBackend::Postgres,
            "SELECT to_regclass('public.users')::text AS name",
        ))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.try_get::<Option<String>>("", "name").unwrap(), None);
    migrate(&fixture.db).await.unwrap();
    assert_eq!(user::Entity::find().count(&fixture.db).await.unwrap(), 0);
}

#[tokio::test]
async fn user_and_credential_uniqueness_foreign_keys_and_cascade_are_enforced() {
    let fixture = TestDatabase::new().await;
    let owner = insert_user(&fixture.db, "owner").await;
    let duplicate = user::ActiveModel {
        id: Set(Uuid::new_v4()),
        username: Set("owner".to_owned()),
        password_hash: Set("dummy".to_owned()),
        is_admin: Set(false),
        active: Set(true),
        must_change_password: Set(true),
        created_at: Set(Utc::now()),
        last_login_at: Set(None),
    }
    .insert(&fixture.db)
    .await
    .unwrap_err();
    assert!(matches!(
        duplicate.sql_err(),
        Some(SqlErr::UniqueConstraintViolation(_))
    ));
    let key = passkey::ActiveModel {
        id: Set(Uuid::new_v4()),
        user_id: Set(owner.id),
        credential_id: Set(vec![1, 2, 3]),
        label: Set("local".to_owned()),
        credential: Set(serde_json::json!({"synthetic-private-key-metadata": true})),
        created_at: Set(Utc::now()),
        last_used_at: Set(None),
    };
    key.clone().insert(&fixture.db).await.unwrap();
    let mut duplicate = key;
    duplicate.id = Set(Uuid::new_v4());
    assert!(matches!(
        duplicate.insert(&fixture.db).await.unwrap_err().sql_err(),
        Some(SqlErr::UniqueConstraintViolation(_))
    ));
    let expires = Utc::now() + Duration::days(1);
    session::ActiveModel {
        token_hash: Set("a".repeat(64)),
        user_id: Set(owner.id),
        expires_at: Set(expires),
        id: Set(uuid::Uuid::new_v4()),
        created_at: Set(chrono::Utc::now()),
        last_seen_at: Set(None),
        user_agent: Set(None),
    }
    .insert(&fixture.db)
    .await
    .unwrap();
    let missing = session::ActiveModel {
        token_hash: Set("b".repeat(64)),
        user_id: Set(Uuid::new_v4()),
        expires_at: Set(expires),
        id: Set(uuid::Uuid::new_v4()),
        created_at: Set(chrono::Utc::now()),
        last_seen_at: Set(None),
        user_agent: Set(None),
    }
    .insert(&fixture.db)
    .await;
    assert!(missing.is_err(), "session must reference an existing user");
    user::Entity::delete_by_id(owner.id)
        .exec(&fixture.db)
        .await
        .unwrap();
    assert_eq!(passkey::Entity::find().count(&fixture.db).await.unwrap(), 0);
    assert_eq!(session::Entity::find().count(&fixture.db).await.unwrap(), 0);
}

#[tokio::test]
async fn audit_requires_existing_subject_and_diagnostics_redact_credentials() {
    let fixture = TestDatabase::new().await;
    let owner = insert_user(&fixture.db, "owner").await;
    let row = audit::ActiveModel {
        id: Set(Uuid::new_v4()),
        actor_user_id: Set(None),
        target_user_id: Set(owner.id),
        action: Set("synthetic_action".to_owned()),
        created_at: Set(Utc::now()),
    }
    .insert(&fixture.db)
    .await
    .unwrap();
    assert_eq!(row.actor_user_id, None);
    assert!(
        audit::Entity::find()
            .filter(audit::Column::TargetUserId.eq(owner.id))
            .one(&fixture.db)
            .await
            .unwrap()
            .is_some()
    );
    assert!(
        user::Entity::delete_by_id(owner.id)
            .exec(&fixture.db)
            .await
            .is_err(),
        "audit deletion must be handled explicitly by future account deletion"
    );
    for diagnostic in [format!("{owner:?}"), format!("{owner:#?}")] {
        assert_eq!(diagnostic, format!("User {{ id: {:?}, .. }}", owner.id));
    }
    let key = passkey::Model {
        id: Uuid::new_v4(),
        user_id: owner.id,
        credential_id: vec![1],
        label: "synthetic-private-label".to_owned(),
        credential: serde_json::json!({"synthetic-private-metadata":true}),
        created_at: Utc::now(),
        last_used_at: None,
    };
    for diagnostic in [format!("{key:?}"), format!("{key:#?}")] {
        assert_eq!(diagnostic, format!("Passkey {{ id: {:?}, .. }}", key.id));
    }
}

#[tokio::test]
async fn app_event_choices_start_as_the_webhook_choices_and_revert_cleanly() {
    let fixture = TestDatabase::new().await;
    let db = &fixture.db;
    migrations::Migrator::down(db, Some(7)).await.unwrap();
    let owner = insert_user(db, "owner").await;
    db.execute_unprepared(&format!(
        "INSERT INTO notification_settings (user_id, webhook_format, language, events) \
         VALUES ('{}', 'discord', 'ja', '{{\"reward_claimed\": true, \"resource_full\": false}}')",
        owner.id
    ))
    .await
    .unwrap();
    migrations::Migrator::up(db, None).await.unwrap();
    let copied = db
        .query_one_raw(Statement::from_string(
            DbBackend::Postgres,
            format!(
                "SELECT app_events = events AS same, app_events->>'resource_full' AS full \
                 FROM notification_settings WHERE user_id = '{}'",
                owner.id
            ),
        ))
        .await
        .unwrap()
        .unwrap();
    assert!(copied.try_get::<bool>("", "same").unwrap());
    assert_eq!(copied.try_get::<String>("", "full").unwrap(), "false");
    // New rows without explicit app choices get an empty object (every kind at its default).
    let other = insert_user(db, "other").await;
    db.execute_unprepared(&format!(
        "INSERT INTO notification_settings (user_id, webhook_format, language, events) \
         VALUES ('{}', 'json', 'en', '{{}}')",
        other.id
    ))
    .await
    .unwrap();
    let empty = db
        .query_one_raw(Statement::from_string(
            DbBackend::Postgres,
            format!(
                "SELECT app_events::text AS app FROM notification_settings WHERE user_id = '{}'",
                other.id
            ),
        ))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(empty.try_get::<String>("", "app").unwrap(), "{}");
    assert!(
        db.execute_unprepared(&format!(
            "UPDATE notification_settings SET app_events = '[]' WHERE user_id = '{}'",
            other.id
        ))
        .await
        .is_err()
    );
    migrations::Migrator::down(db, Some(7)).await.unwrap();
    let column = db
        .query_one_raw(Statement::from_string(
            DbBackend::Postgres,
            "SELECT count(*) AS n FROM information_schema.columns \
             WHERE table_name = 'notification_settings' AND column_name = 'app_events'",
        ))
        .await
        .unwrap()
        .unwrap()
        .try_get::<i64>("", "n")
        .unwrap();
    assert_eq!(column, 0);
}

#[tokio::test]
async fn redemption_code_schema_checks_values_and_reverts_cleanly() {
    let fixture = TestDatabase::new().await;
    let db = &fixture.db;
    let owner = insert_user(db, "owner").await;
    let link = Uuid::new_v4();
    let account = Uuid::new_v4();
    db.execute_unprepared(&format!(
        "INSERT INTO hoyolab_accounts (id, owner_user_id, hoyolab_uid, encrypted_credentials, \
         credential_version, auth_status, created_at, updated_at) \
         VALUES ('{link}', '{}', '1', NULL, 1, 'removed', now(), now()); \
         INSERT INTO game_accounts (id, hoyolab_account_id, game_kind, game_uid, region, visible, \
         detected, last_detected_at) VALUES ('{account}', '{link}', 'genshin', '800000001', \
         'os_asia', true, true, now())",
        owner.id
    ))
    .await
    .unwrap();
    // The single source row exists with the documented defaults.
    let source = db
        .query_one_raw(Statement::from_string(
            DbBackend::Postgres,
            "SELECT enabled, interval_minutes, consecutive_failures FROM redemption_code_source",
        ))
        .await
        .unwrap()
        .unwrap();
    assert!(source.try_get::<bool>("", "enabled").unwrap());
    assert_eq!(source.try_get::<i32>("", "interval_minutes").unwrap(), 180);
    assert_eq!(
        source.try_get::<i32>("", "consecutive_failures").unwrap(),
        0
    );
    for bad in [
        "INSERT INTO redemption_code_source (id) VALUES (2)".to_owned(),
        "UPDATE redemption_code_source SET interval_minutes = 59".to_owned(),
        "UPDATE redemption_code_source SET interval_minutes = 1441".to_owned(),
        "UPDATE redemption_code_source SET last_error = 'synthetic upstream body'".to_owned(),
        "INSERT INTO redemption_codes (id, game_kind, code, first_seen_at, last_seen_at) \
         VALUES (gen_random_uuid(), 'nap', 'GOODCODE', now(), now())"
            .to_owned(),
        "INSERT INTO redemption_codes (id, game_kind, code, first_seen_at, last_seen_at) \
         VALUES (gen_random_uuid(), 'genshin', 'lower', now(), now())"
            .to_owned(),
        "INSERT INTO redemption_codes (id, game_kind, code, first_seen_at, last_seen_at) \
         VALUES (gen_random_uuid(), 'genshin', 'GOODCODE', now(), now() - interval '1 day')"
            .to_owned(),
        format!(
            "INSERT INTO redemption_code_settings (game_account_id, mode, enabled_at, updated_at) \
             VALUES ('{account}', 'notify', NULL, now())"
        ),
        format!(
            "INSERT INTO redemption_code_settings (game_account_id, mode, enabled_at, updated_at) \
             VALUES ('{account}', 'always', now(), now())"
        ),
    ] {
        assert!(db.execute_unprepared(&bad).await.is_err(), "{bad}");
    }
    let code = Uuid::new_v4();
    db.execute_unprepared(&format!(
        "INSERT INTO redemption_codes (id, game_kind, code, first_seen_at, last_seen_at) \
         VALUES ('{code}', 'genshin', 'GOODCODE', now(), now()); \
         INSERT INTO redemption_code_settings (game_account_id, mode, enabled_at, updated_at) \
         VALUES ('{account}', 'auto', now(), now())"
    ))
    .await
    .unwrap();
    for state in [
        "announced",
        "pending",
        "redeemed",
        "already_used",
        "expired",
        "invalid",
        "rank_too_low",
        "cookie_invalid",
        "captcha_required",
        "unknown",
    ] {
        db.execute_unprepared(&format!(
            "INSERT INTO redemption_code_results (game_account_id, code_id, state, created_at, \
             updated_at) VALUES ('{account}', '{code}', '{state}', now(), now()) \
             ON CONFLICT (game_account_id, code_id) DO UPDATE SET state = EXCLUDED.state"
        ))
        .await
        .unwrap();
    }
    assert!(
        db.execute_unprepared(&format!(
            "UPDATE redemption_code_results SET state = 'available' WHERE code_id = '{code}'"
        ))
        .await
        .is_err(),
        "`available` is derived, never stored"
    );
    // The code-redemption migration is undone first.
    migrations::Migrator::down(db, Some(2)).await.unwrap();
    let left = db
        .query_one_raw(Statement::from_string(
            DbBackend::Postgres,
            "SELECT to_regclass('public.redemption_codes')::text AS name",
        ))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(left.try_get::<Option<String>>("", "name").unwrap(), None);
    migrations::Migrator::up(db, None).await.unwrap();
}

#[tokio::test]
async fn code_redemption_schema_checks_values_and_reverts_cleanly() {
    let fixture = TestDatabase::new().await;
    let db = &fixture.db;
    let owner = insert_user(db, "owner").await;
    let link = Uuid::new_v4();
    let account = Uuid::new_v4();
    db.execute_unprepared(&format!(
        "INSERT INTO hoyolab_accounts (id, owner_user_id, hoyolab_uid, encrypted_credentials, \
         credential_version, auth_status, created_at, updated_at) \
         VALUES ('{link}', '{}', '1', NULL, 1, 'removed', now(), now()); \
         INSERT INTO game_accounts (id, hoyolab_account_id, game_kind, game_uid, region, visible, \
         detected, last_detected_at) VALUES ('{account}', '{link}', 'genshin', '800000001', \
         'os_asia', true, true, now())",
        owner.id
    ))
    .await
    .unwrap();
    // Existing links start without redemption credentials or a pending action.
    let row = db
        .query_one_raw(Statement::from_string(
            DbBackend::Postgres,
            format!(
                "SELECT redemption_credentials, redemption_action IS NULL AS clear \
                 FROM hoyolab_accounts WHERE id = '{link}'"
            ),
        ))
        .await
        .unwrap()
        .unwrap();
    assert!(!row.try_get::<bool>("", "redemption_credentials").unwrap());
    assert!(row.try_get::<bool>("", "clear").unwrap());
    let ledger = |code: &str, state: &str, posts: i32, sent: &str| {
        format!(
            "INSERT INTO redemption_ledger (game_account_id, code, source, state, posts, \
             last_sent_at, created_at, updated_at) \
             VALUES ('{account}', '{code}', 'catalogue', '{state}', {posts}, {sent}, now(), now())"
        )
    };
    for bad in [
        format!(
            "UPDATE hoyolab_accounts SET redemption_action = 'cookie_invalid' WHERE id = '{link}'"
        ),
        format!(
            "UPDATE hoyolab_accounts SET redemption_action = 'banned', \
             redemption_action_at = now() WHERE id = '{link}'"
        ),
        ledger("lower", "pending", 0, "NULL"),
        ledger("GOODCODE", "available", 0, "NULL"),
        ledger("GOODCODE", "pending", 1, "NULL"),
        ledger("GOODCODE", "redeemed", 0, "now()"),
        ledger("GOODCODE", "pending", -1, "NULL"),
        format!(
            "INSERT INTO redemption_ledger (game_account_id, code, source, state, created_at, \
             updated_at) VALUES ('{account}', 'GOODCODE', 'shared', 'pending', now(), now())"
        ),
        "INSERT INTO redemption_code_rejections (game_kind, code, reason, rejected_at) \
         VALUES ('genshin', 'GOODCODE', 'already_used', now())"
            .to_owned(),
        "INSERT INTO redemption_code_rejections (game_kind, code, reason, rejected_at) \
         VALUES ('nap', 'GOODCODE', 'expired', now())"
            .to_owned(),
    ] {
        assert!(db.execute_unprepared(&bad).await.is_err(), "{bad}");
    }
    db.execute_unprepared(&format!(
        "UPDATE hoyolab_accounts SET redemption_credentials = true, \
         redemption_action = 'captcha_required', redemption_action_at = now() WHERE id = '{link}'"
    ))
    .await
    .unwrap();
    for (index, state) in [
        "pending",
        "sending",
        "redeemed",
        "already_used",
        "expired",
        "invalid",
        "rank_too_low",
        "cookie_invalid",
        "captcha_required",
        "unknown",
    ]
    .into_iter()
    .enumerate()
    {
        db.execute_unprepared(&ledger(&format!("CODE{index:04}"), state, 1, "now()"))
            .await
            .unwrap();
    }
    db.execute_unprepared(
        "INSERT INTO redemption_code_rejections (game_kind, code, reason, rejected_at) \
         VALUES ('genshin', 'GOODCODE', 'expired', now())",
    )
    .await
    .unwrap();
    // Deleting the user removes the ledger with the UID; rejections are not per user.
    user::Entity::delete_by_id(owner.id).exec(db).await.unwrap();
    let count = |sql: &'static str| async move {
        db.query_one_raw(Statement::from_string(DbBackend::Postgres, sql))
            .await
            .unwrap()
            .unwrap()
            .try_get::<i64>("", "n")
            .unwrap()
    };
    assert_eq!(
        count("SELECT count(*) AS n FROM redemption_ledger").await,
        0
    );
    assert_eq!(
        count("SELECT count(*) AS n FROM redemption_code_rejections").await,
        1
    );
    migrations::Migrator::down(db, Some(1)).await.unwrap();
    assert_eq!(
        count(
            "SELECT count(*) AS n FROM information_schema.columns \
             WHERE table_name = 'hoyolab_accounts' AND column_name LIKE 'redemption%'"
        )
        .await,
        0
    );
    let left = db
        .query_one_raw(Statement::from_string(
            DbBackend::Postgres,
            "SELECT to_regclass('public.redemption_ledger')::text AS name",
        ))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(left.try_get::<Option<String>>("", "name").unwrap(), None);
    migrations::Migrator::up(db, None).await.unwrap();
}
