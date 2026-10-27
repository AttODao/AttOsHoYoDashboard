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
            "m20261028_000011_app_notification_events"
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
    migrations::Migrator::down(db, Some(1)).await.unwrap();
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
    migrations::Migrator::down(db, Some(1)).await.unwrap();
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
