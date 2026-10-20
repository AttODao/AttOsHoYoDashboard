use super::{
    MAX_CREDENTIAL_ID_BYTES, MAX_CREDENTIAL_JSON_BYTES, credential_fingerprint, decode_credential,
    load_credentials,
};
use crate::dashboard::auth::passkeys::test_support::Fixture;
use crate::dashboard::{db::entities::passkey, error::Error};
use chrono::Utc;
use sea_orm::EntityTrait;
use serde_json::Value;
use serde_json::json;
use uuid::Uuid;

fn row(credential: Value, credential_id: Vec<u8>) -> passkey::Model {
    passkey::Model {
        id: Uuid::new_v4(),
        user_id: Uuid::new_v4(),
        credential_id,
        label: "key".into(),
        credential,
        created_at: Utc::now(),
        last_used_at: None,
    }
}

#[test]
fn corrupt_json_and_invalid_credential_id_lengths_are_internal_errors() {
    for id in [vec![], vec![1], vec![1; MAX_CREDENTIAL_ID_BYTES + 1]] {
        assert!(matches!(
            decode_credential(&row(json!({"corrupt": true}), id)),
            Err(Error::Internal)
        ));
    }
}

#[test]
fn fingerprint_has_an_exact_64_kib_serialized_boundary() {
    let boundary = Value::String("x".repeat(MAX_CREDENTIAL_JSON_BYTES - 2));
    assert!(credential_fingerprint(&boundary).is_ok());
    let oversized = Value::String("x".repeat(MAX_CREDENTIAL_JSON_BYTES - 1));
    assert!(matches!(
        credential_fingerprint(&oversized),
        Err(Error::Internal)
    ));
    assert_ne!(
        credential_fingerprint(&json!({"counter": 1})).unwrap(),
        credential_fingerprint(&json!({"counter": 2})).unwrap()
    );
}

#[tokio::test]
async fn decoded_credential_must_match_the_persisted_credential_id() {
    let fixture = Fixture::new().await;
    let mut row = passkey::Entity::find_by_id(fixture.key_id)
        .one(&fixture.db.db)
        .await
        .unwrap()
        .unwrap();
    assert!(decode_credential(&row).is_ok());
    row.credential_id[0] ^= 1;
    assert!(matches!(decode_credential(&row), Err(Error::Internal)));
    row.credential_id = vec![];
    assert!(matches!(decode_credential(&row), Err(Error::Internal)));
    row.credential_id = vec![1; MAX_CREDENTIAL_ID_BYTES + 1];
    assert!(matches!(decode_credential(&row), Err(Error::Internal)));
}

#[tokio::test]
async fn loader_is_owner_scoped_and_rejects_more_than_twenty_rows() {
    use sea_orm::{ActiveModelTrait, Set};
    let fixture = Fixture::new().await;
    let template = passkey::Entity::find_by_id(fixture.key_id)
        .one(&fixture.db.db)
        .await
        .unwrap()
        .unwrap();
    assert!(
        load_credentials(&fixture.db.db, Uuid::new_v4())
            .await
            .unwrap()
            .is_empty()
    );
    for n in 0..20u8 {
        passkey::ActiveModel {
            id: Set(Uuid::new_v4()),
            user_id: Set(fixture.actor.user.id),
            credential_id: Set(vec![n]),
            label: Set("synthetic stored row".into()),
            credential: Set(template.credential.clone()),
            created_at: Set(Utc::now()),
            last_used_at: Set(None),
        }
        .insert(&fixture.db.db)
        .await
        .unwrap();
        if n == 18 {
            assert_eq!(
                load_credentials(&fixture.db.db, fixture.actor.user.id)
                    .await
                    .unwrap()
                    .len(),
                20
            );
        }
    }
    assert!(matches!(
        load_credentials(&fixture.db.db, fixture.actor.user.id).await,
        Err(Error::Internal)
    ));
}
