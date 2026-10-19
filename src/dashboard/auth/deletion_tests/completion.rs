use super::{ready_user, stub_passkey};
use crate::dashboard::{
    db::entities::{
        audit, deletion_request, game_account, game_status_cache, hoyolab_account, passkey,
        reward_attempt, reward_guard, reward_run, reward_setting, reward_status_cache, session,
        user,
    },
    test_support::{PASSWORD, TestDatabase},
};
use chrono::{Duration, Utc};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, PaginatorTrait, QueryFilter,
    QueryOrder, Set,
};
use std::time::{Duration as StdDuration, Instant};
use uuid::Uuid;

async fn insert_owned_tree(db: &DatabaseConnection, owner: Uuid, uid: &str) -> (Uuid, Uuid, Uuid) {
    let now = Utc::now();
    let link = Uuid::new_v4();
    hoyolab_account::ActiveModel {
        id: Set(link),
        owner_user_id: Set(owner),
        hoyolab_uid: Set(Some(uid.to_owned())),
        encrypted_credentials: Set(vec![7; 32]),
        credential_version: Set(1),
        auth_status: Set("verified".to_owned()),
        created_at: Set(now),
        updated_at: Set(now),
    }
    .insert(db)
    .await
    .unwrap();
    let game = game_account::ActiveModel {
        id: Set(Uuid::new_v4()),
        hoyolab_account_id: Set(link),
        game_kind: Set("genshin".to_owned()),
        game_uid: Set("100000001".to_owned()),
        region: Set("os_usa".to_owned()),
        visible: Set(true),
        detected: Set(true),
        last_detected_at: Set(now),
    }
    .insert(db)
    .await
    .unwrap();
    game_status_cache::ActiveModel {
        game_account_id: Set(game.id),
        fetched_at: Set(Some(now)),
        updated_at: Set(now),
        payload: Set(Some(serde_json::json!({"synthetic": true}))),
        payload_version: Set(1),
        error_code: Set(None),
        error_at: Set(None),
    }
    .insert(db)
    .await
    .unwrap();
    let setting = reward_setting::ActiveModel {
        id: Set(Uuid::new_v4()),
        hoyolab_account_id: Set(link),
        game_kind: Set("genshin".to_owned()),
        enabled: Set(true),
        local_time: Set("06:00".to_owned()),
        auth_status: Set("ready".to_owned()),
        next_run_at: Set(Some(now + Duration::hours(1))),
    }
    .insert(db)
    .await
    .unwrap();
    let run = reward_run::ActiveModel {
        id: Set(Uuid::new_v4()),
        setting_id: Set(setting.id),
        trigger: Set("manual".to_owned()),
        state: Set("success".to_owned()),
        created_at: Set(now),
        updated_at: Set(now),
        ended_at: Set(Some(now)),
        attempt_count: Set(1),
        credential_version: Set(1),
        send_state: Set("confirmed".to_owned()),
        next_retry_at: Set(None),
    }
    .insert(db)
    .await
    .unwrap();
    reward_attempt::ActiveModel {
        id: Set(Uuid::new_v4()),
        run_id: Set(run.id),
        number: Set(1),
        started_at: Set(now),
        finished_at: Set(Some(now)),
        outcome: Set("success".to_owned()),
        error_code: Set(None),
        send_state: Set("confirmed".to_owned()),
    }
    .insert(db)
    .await
    .unwrap();
    reward_guard::ActiveModel {
        setting_id: Set(setting.id),
        post_may_have_happened: Set(false),
        updated_at: Set(now),
    }
    .insert(db)
    .await
    .unwrap();
    reward_status_cache::ActiveModel {
        setting_id: Set(setting.id),
        fetched_at: Set(Some(now)),
        updated_at: Set(now),
        payload: Set(Some(serde_json::json!({"synthetic": true}))),
        payload_version: Set(1),
        error_code: Set(None),
        error_at: Set(None),
    }
    .insert(db)
    .await
    .unwrap();
    (link, game.id, setting.id)
}

#[tokio::test]
async fn finish_deletion_cascades_owned_data_releases_uid_and_cleans_audit_references() {
    let test = TestDatabase::new().await;
    let (accounts, admin) = test.ready_admin().await;
    let member = ready_user(&accounts, &admin, "member").await;
    let (link, game, setting) = insert_owned_tree(&test.db, member.user.id, "123456").await;
    let passkey = stub_passkey(&test.db, member.user.id).await;
    audit::ActiveModel {
        id: Set(Uuid::new_v4()),
        actor_user_id: Set(Some(member.user.id)),
        target_user_id: Set(admin.user.id),
        action: Set("member_actor".to_owned()),
        created_at: Set(Utc::now()),
    }
    .insert(&test.db)
    .await
    .unwrap();

    accounts
        .request_deletion(
            &member,
            member.user.id,
            PASSWORD.to_owned(),
            "member".to_owned(),
        )
        .await
        .unwrap();
    accounts.finish_deletion(member.user.id).await.unwrap();

    assert!(
        user::Entity::find_by_id(member.user.id)
            .one(&test.db)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        hoyolab_account::Entity::find_by_id(link)
            .one(&test.db)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        game_account::Entity::find_by_id(game)
            .one(&test.db)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        game_status_cache::Entity::find_by_id(game)
            .one(&test.db)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        reward_setting::Entity::find_by_id(setting)
            .one(&test.db)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        reward_guard::Entity::find_by_id(setting)
            .one(&test.db)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        reward_status_cache::Entity::find_by_id(setting)
            .one(&test.db)
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(reward_run::Entity::find().count(&test.db).await.unwrap(), 0);
    assert_eq!(
        reward_attempt::Entity::find()
            .count(&test.db)
            .await
            .unwrap(),
        0
    );
    assert!(
        passkey::Entity::find_by_id(passkey)
            .one(&test.db)
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(
        session::Entity::find()
            .filter(session::Column::UserId.eq(member.user.id))
            .count(&test.db)
            .await
            .unwrap(),
        0
    );
    assert!(
        deletion_request::Entity::find_by_id(member.user.id)
            .one(&test.db)
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(
        audit::Entity::find()
            .filter(audit::Column::TargetUserId.eq(member.user.id))
            .count(&test.db)
            .await
            .unwrap(),
        0
    );
    let preserved = audit::Entity::find()
        .filter(audit::Column::Action.eq("member_actor"))
        .one(&test.db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(preserved.actor_user_id, None);
    assert_eq!(preserved.target_user_id, admin.user.id);

    hoyolab_account::ActiveModel {
        id: Set(Uuid::new_v4()),
        owner_user_id: Set(admin.user.id),
        hoyolab_uid: Set(Some("123456".to_owned())),
        encrypted_credentials: Set(vec![8; 32]),
        credential_version: Set(1),
        auth_status: Set("verified".to_owned()),
        created_at: Set(Utc::now()),
        updated_at: Set(Utc::now()),
    }
    .insert(&test.db)
    .await
    .unwrap();
}

#[tokio::test]
async fn pending_deletions_are_oldest_first_and_capped() {
    let test = TestDatabase::new().await;
    let (accounts, admin) = test.ready_admin().await;
    let all = queue_deletions(&test, &admin.user, 33).await;
    let expected = all[..32].to_vec();
    let pending = accounts
        .pending_deletions(&[], Instant::now())
        .await
        .unwrap();
    assert_eq!(pending.len(), 32);
    assert_eq!(pending, expected);
    // Owners already being completed are skipped, so the batch reaches later owners.
    let pending = accounts
        .pending_deletions(&expected[..1], Instant::now())
        .await
        .unwrap();
    assert_eq!(pending, all[1..].to_vec());
    let rows = deletion_request::Entity::find()
        .order_by_asc(deletion_request::Column::CreatedAt)
        .all(&test.db)
        .await
        .unwrap();
    assert_eq!(rows.len(), 33);
}

#[tokio::test]
async fn failing_owners_back_off_without_starving_later_owners() {
    let test = TestDatabase::new().await;
    let (accounts, admin) = test.ready_admin().await;
    let all = queue_deletions(&test, &admin.user, 33).await;
    let now = Instant::now();
    for owner in &all[..32] {
        assert_eq!(
            accounts.postpone_deletion(*owner, now).unwrap(),
            StdDuration::from_secs(10)
        );
    }
    // The 32 oldest keep failing; the next owner is still selected.
    assert_eq!(
        accounts.pending_deletions(&[], now).await.unwrap(),
        vec![all[32]]
    );
    let before = now + StdDuration::from_secs(9);
    assert_eq!(
        accounts.pending_deletions(&[], before).await.unwrap(),
        vec![all[32]]
    );
    let after = now + StdDuration::from_secs(10);
    assert_eq!(
        accounts.pending_deletions(&[], after).await.unwrap(),
        all[..32]
    );

    // Repeated failures double the delay up to one hour.
    let owner = all[0];
    let mut delays = Vec::new();
    for _ in 0..12 {
        delays.push(accounts.postpone_deletion(owner, after).unwrap().as_secs());
    }
    assert_eq!(&delays[..4], &[20, 40, 80, 160]);
    assert_eq!(*delays.last().unwrap(), 3600);
    assert!(
        !accounts
            .pending_deletions(&[], after + StdDuration::from_secs(3599))
            .await
            .unwrap()
            .contains(&owner)
    );

    // Successful completion forgets the backoff state.
    accounts.finish_deletion(owner).await.unwrap();
    let retries = accounts.deletion_retries.0.lock().unwrap();
    assert!(!retries.contains_key(&owner));
    assert_eq!(retries.len(), 31);
}

async fn queue_deletions(test: &TestDatabase, admin: &user::Model, count: i64) -> Vec<Uuid> {
    let base = Utc::now();
    let mut queued = Vec::new();
    for i in 0..count {
        let id = Uuid::new_v4();
        user::ActiveModel {
            id: Set(id),
            username: Set(format!("queued-{i}")),
            password_hash: Set(admin.password_hash.clone()),
            is_admin: Set(false),
            active: Set(false),
            must_change_password: Set(false),
            created_at: Set(base),
        }
        .insert(&test.db)
        .await
        .unwrap();
        deletion_request::ActiveModel {
            user_id: Set(id),
            requested_by_user_id: Set(Some(admin.id)),
            created_at: Set(base + Duration::seconds(i)),
        }
        .insert(&test.db)
        .await
        .unwrap();
        queued.push(id);
    }
    queued
}
