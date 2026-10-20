use crate::dashboard::{
    auth::sessions::Sessions,
    db::entities::{hoyolab_account, reward_setting},
    error::Error,
    test_support::{TEMP_PASSWORD, TestDatabase},
};
use chrono::Utc;
use sea_orm::{ActiveModelTrait, Set};
use uuid::Uuid;

#[tokio::test]
async fn overview_reports_link_state_and_automatic_claims_for_admins_only() {
    let test = TestDatabase::new().await;
    let (accounts, admin) = test.ready_admin().await;
    let member = accounts
        .create_user(&admin, "member".to_owned(), TEMP_PASSWORD.to_owned())
        .await
        .unwrap();
    let link = hoyolab_account::ActiveModel {
        id: Set(Uuid::new_v4()),
        owner_user_id: Set(member.id),
        hoyolab_uid: Set(Some("42".to_owned())),
        encrypted_credentials: Set(None),
        credential_version: Set(2),
        auth_status: Set("removed".to_owned()),
        created_at: Set(Utc::now()),
        updated_at: Set(Utc::now()),
        verified_at: Set(None),
    }
    .insert(&test.db)
    .await
    .unwrap();
    for (game, enabled) in [("genshin", true), ("starrail", false)] {
        reward_setting::ActiveModel {
            id: Set(Uuid::new_v4()),
            hoyolab_account_id: Set(link.id),
            game_kind: Set(game.to_owned()),
            enabled: Set(enabled),
            local_time: Set("06:00".to_owned()),
            auth_status: Set("ready".to_owned()),
            next_run_at: Set(None),
        }
        .insert(&test.db)
        .await
        .unwrap();
    }
    let rows = accounts.user_overview(&admin).await.unwrap();
    assert_eq!(rows.len(), 2);
    let admin_row = rows
        .iter()
        .find(|row| row.user.id == admin.user.id)
        .unwrap();
    assert_eq!(admin_row.link_status, "not_configured");
    assert_eq!(admin_row.automatic_claims, 0);
    let member_row = rows.iter().find(|row| row.user.id == member.id).unwrap();
    assert_eq!(member_row.link_status, "removed");
    assert_eq!(member_row.automatic_claims, 1);
}

#[tokio::test]
async fn audit_log_names_users_pages_backwards_and_requires_an_admin() {
    let test = TestDatabase::new().await;
    let (accounts, admin) = test.ready_admin().await;
    let member = accounts
        .create_user(&admin, "member".to_owned(), TEMP_PASSWORD.to_owned())
        .await
        .unwrap();
    accounts.set_active(&admin, member.id, false).await.unwrap();
    let entries = accounts.audit_log(&admin, 50, None).await.unwrap();
    assert!(entries.len() >= 2);
    assert_eq!(entries[0].action, "user_disabled");
    assert_eq!(entries[0].actor.as_deref(), Some("admin"));
    assert_eq!(entries[0].target.as_deref(), Some("member"));
    assert!(
        entries
            .windows(2)
            .all(|pair| pair[0].created_at >= pair[1].created_at)
    );
    let first_page = accounts.audit_log(&admin, 1, None).await.unwrap();
    assert_eq!(first_page.len(), 1);
    let older = accounts
        .audit_log(&admin, 50, Some(first_page[0].created_at))
        .await
        .unwrap();
    assert!(
        older
            .iter()
            .all(|entry| entry.created_at < first_page[0].created_at)
    );
    assert!(older.iter().any(|entry| entry.action == "user_created"));

    // A non-admin session cannot read the log.
    accounts.set_active(&admin, member.id, true).await.unwrap();
    let issued =
        Sessions::issue_until(&test.db, member.id, Utc::now() + chrono::Duration::hours(1))
            .await
            .unwrap();
    let member_identity = Sessions::authenticate(
        &test.db,
        &crate::dashboard::auth::password::token_hash(&issued.token).unwrap(),
    )
    .await
    .unwrap();
    assert!(
        accounts
            .audit_log(&member_identity, 50, None)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn administrators_can_grant_and_revoke_others_but_not_their_own_role() {
    let test = TestDatabase::new().await;
    let (accounts, admin) = test.ready_admin().await;
    let member = accounts
        .create_user(&admin, "member".to_owned(), TEMP_PASSWORD.to_owned())
        .await
        .unwrap();
    assert!(matches!(
        accounts.set_admin(&admin, admin.user.id, false).await,
        Err(Error::Invalid(_))
    ));
    accounts.set_admin(&admin, member.id, true).await.unwrap();
    let promoted = accounts.user_overview(&admin).await.unwrap();
    assert!(
        promoted
            .iter()
            .any(|row| row.user.id == member.id && row.user.is_admin)
    );
    accounts.set_admin(&admin, member.id, false).await.unwrap();
    let entries = accounts.audit_log(&admin, 10, None).await.unwrap();
    assert_eq!(entries[0].action, "admin_revoked");
    assert_eq!(entries[1].action, "admin_granted");
    // Repeating the current role changes nothing and records nothing.
    accounts.set_admin(&admin, member.id, false).await.unwrap();
    assert_eq!(
        accounts.audit_log(&admin, 10, None).await.unwrap()[0].id,
        entries[0].id
    );
    assert!(matches!(
        accounts.set_admin(&admin, Uuid::new_v4(), true).await,
        Err(Error::NotFound)
    ));
}
