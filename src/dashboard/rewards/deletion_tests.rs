//! Without a cookie key, deletion waits for both unresolved guards and `may_have_sent` runs.
use super::confirm_owner;
use crate::dashboard::{
    db::entities::{hoyolab_account, reward_guard, reward_run, reward_setting, user},
    error::Error,
    hoyolab::{
        credentials::{CookieKey, Credentials},
        store::CookieStore,
    },
    test_support::TestDatabase,
};
use chrono::{DateTime, Duration, Utc};
use sea_orm::{ActiveModelTrait, DatabaseConnection, EntityTrait, Set};
use std::sync::Arc;
use uuid::Uuid;

async fn owner_with_setting(db: &DatabaseConnection) -> (Uuid, Uuid) {
    let now = Utc::now();
    let owner = user::ActiveModel {
        id: Set(Uuid::new_v4()),
        username: Set("deleted-owner".to_owned()),
        password_hash: Set("synthetic-private-hash".to_owned()),
        is_admin: Set(false),
        active: Set(false),
        must_change_password: Set(false),
        created_at: Set(now),
    }
    .insert(db)
    .await
    .unwrap();
    let link = hoyolab_account::ActiveModel {
        id: Set(Uuid::new_v4()),
        owner_user_id: Set(owner.id),
        hoyolab_uid: Set(Some("123".to_owned())),
        // Sealed with a synthetic key the server under test does not have.
        encrypted_credentials: Set(CookieKey::from_hex(&"11".repeat(32))
            .unwrap()
            .seal(
                owner.id,
                Some("123"),
                &Credentials::parse("ltoken_v2=synthetictoken123; ltuid_v2=123").unwrap(),
            )
            .unwrap()),
        credential_version: Set(1),
        auth_status: Set("verified".to_owned()),
        created_at: Set(now),
        updated_at: Set(now),
    }
    .insert(db)
    .await
    .unwrap();
    let setting = reward_setting::ActiveModel {
        id: Set(Uuid::new_v4()),
        hoyolab_account_id: Set(link.id),
        game_kind: Set("genshin".to_owned()),
        enabled: Set(false),
        local_time: Set("06:00".to_owned()),
        auth_status: Set("ready".to_owned()),
        next_run_at: Set(None),
    }
    .insert(db)
    .await
    .unwrap();
    (owner.id, setting.id)
}

async fn confirm_without_key(
    db: &DatabaseConnection,
    owner: Uuid,
) -> crate::dashboard::error::Result<()> {
    confirm_owner(db, None::<&Arc<CookieStore>>, owner).await
}

async fn set_guard(db: &DatabaseConnection, setting: Uuid, at: DateTime<Utc>) {
    reward_guard::Entity::delete_by_id(setting)
        .exec(db)
        .await
        .unwrap();
    reward_guard::ActiveModel {
        setting_id: Set(setting),
        post_may_have_happened: Set(true),
        updated_at: Set(at),
    }
    .insert(db)
    .await
    .unwrap();
}

#[tokio::test]
async fn keyless_deletion_waits_for_unresolved_guard_or_may_have_sent_run() {
    let fixture = TestDatabase::new().await;
    let db = &fixture.db;
    confirm_without_key(db, Uuid::new_v4()).await.unwrap();
    let (owner, setting) = owner_with_setting(db).await;
    confirm_without_key(db, owner).await.unwrap();

    // An ended run older than the guard's reward day still blocks deletion.
    let old = Utc::now() - Duration::days(6);
    let run = reward_run::ActiveModel {
        id: Set(Uuid::new_v4()),
        setting_id: Set(setting),
        trigger: Set("manual".to_owned()),
        state: Set("unknown".to_owned()),
        created_at: Set(old),
        updated_at: Set(old),
        ended_at: Set(Some(old)),
        attempt_count: Set(1),
        credential_version: Set(1),
        send_state: Set("may_have_sent".to_owned()),
        next_retry_at: Set(None),
    }
    .insert(db)
    .await
    .unwrap();
    assert!(matches!(
        confirm_without_key(db, owner).await,
        Err(Error::Unavailable(_))
    ));
    reward_run::Entity::delete_by_id(run.id)
        .exec(db)
        .await
        .unwrap();

    set_guard(db, setting, Utc::now()).await;
    assert!(matches!(
        confirm_without_key(db, owner).await,
        Err(Error::Unavailable(_))
    ));
    // A guard from an earlier reward day no longer blocks.
    set_guard(db, setting, Utc::now() - Duration::days(2)).await;
    confirm_without_key(db, owner).await.unwrap();
}
