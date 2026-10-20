//! Offline rotation of the Cookie encryption key; run with the server stopped.
use super::credentials::CookieKey;
use crate::dashboard::{db::entities::hoyolab_account, error::Result};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, IntoActiveModel, QueryFilter,
    QuerySelect, Set, TransactionTrait,
};

/// Re-encrypts every stored Cookie from `old` to `new` in one transaction.
/// Any row the old key cannot open aborts the whole rotation unchanged.
pub(in crate::dashboard) async fn rekey(
    db: &DatabaseConnection,
    old: &CookieKey,
    new: &CookieKey,
) -> Result<usize> {
    let tx = db.begin().await?;
    let rows = hoyolab_account::Entity::find()
        .filter(hoyolab_account::Column::EncryptedCredentials.is_not_null())
        .lock_exclusive()
        .all(&tx)
        .await?;
    let count = rows.len();
    for row in rows {
        let uid = row.hoyolab_uid.clone();
        let cookie = old.open(row.owner_user_id, uid.as_deref(), row.sealed_credentials()?)?;
        let sealed = new.seal(row.owner_user_id, uid.as_deref(), &cookie)?;
        let mut row = row.into_active_model();
        row.encrypted_credentials = Set(Some(sealed));
        row.update(&tx).await?;
    }
    tx.commit().await?;
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dashboard::{
        hoyolab::{credentials::Credentials, store::CookieStore},
        test_support::TestDatabase,
    };
    use std::sync::Arc;

    const RAW: &str = "ltuid_v2=123; ltoken_v2=synthetic-cookie-token; ltmid_v2=synthetic-mid";

    #[tokio::test]
    async fn rotation_reseals_every_cookie_and_aborts_on_a_wrong_key() {
        let fixture = TestDatabase::new().await;
        let (_, actor) = fixture.ready_admin().await;
        let old = Arc::new(CookieKey::from_hex(&"11".repeat(32)).unwrap());
        let new = CookieKey::from_hex(&"22".repeat(32)).unwrap();
        let wrong = CookieKey::from_hex(&"33".repeat(32)).unwrap();
        CookieStore::new(fixture.db.clone(), Arc::clone(&old))
            .submit(&actor, RAW)
            .await
            .unwrap();
        let before = hoyolab_account::Entity::find()
            .one(&fixture.db)
            .await
            .unwrap()
            .unwrap();

        assert!(rekey(&fixture.db, &wrong, &new).await.is_err());
        let unchanged = hoyolab_account::Entity::find()
            .one(&fixture.db)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            unchanged.encrypted_credentials,
            before.encrypted_credentials
        );

        assert_eq!(rekey(&fixture.db, &old, &new).await.unwrap(), 1);
        let after = hoyolab_account::Entity::find()
            .one(&fixture.db)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(after.credential_version, before.credential_version);
        let opened = new
            .open(actor.user.id, None, after.sealed_credentials().unwrap())
            .unwrap();
        assert_eq!(opened.as_str(), Credentials::parse(RAW).unwrap().as_str());
        assert!(
            old.open(actor.user.id, None, after.sealed_credentials().unwrap())
                .is_err()
        );
    }
}
