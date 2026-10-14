//! A waiting owner's deletion does not hold up another owner's completion.
use super::*;
use crate::dashboard::{
    db::entities::user,
    test_support::{PASSWORD, TEMP_PASSWORD, TestDatabase},
};
use sea_orm::EntityTrait;
#[tokio::test]
async fn waiting_owner_does_not_block_other_deletions() {
    let fixture = TestDatabase::new().await;
    let (accounts, admin) = fixture.ready_admin().await;
    let first = accounts
        .create_user(&admin, "wait-owner".into(), TEMP_PASSWORD.into())
        .await
        .unwrap();
    let second = accounts
        .create_user(&admin, "free-owner".into(), TEMP_PASSWORD.into())
        .await
        .unwrap();
    accounts
        .request_deletion(&admin, first.id, PASSWORD.into(), first.username)
        .await
        .unwrap();
    accounts
        .request_deletion(&admin, second.id, PASSWORD.into(), second.username)
        .await
        .unwrap();
    let app = app(accounts);
    let guard = app.operations.acquire(first.id).await;
    let (stop, receiver) = tokio::sync::watch::channel(false);
    let task = tokio::spawn(super::super::runtime::run(app.clone(), receiver));
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            if user::Entity::find_by_id(second.id)
                .one(&fixture.db)
                .await
                .unwrap()
                .is_none()
            {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert!(
        user::Entity::find_by_id(first.id)
            .one(&fixture.db)
            .await
            .unwrap()
            .is_some()
    );
    drop(guard);
    app.cookies.as_ref().unwrap().stop_new_operations();
    stop.send(true).unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(10), task)
        .await
        .unwrap()
        .unwrap();
    assert!(
        user::Entity::find_by_id(first.id)
            .one(&fixture.db)
            .await
            .unwrap()
            .is_none()
    );
}
