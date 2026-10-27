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
            "m20261028_000009_app_notifications"
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
