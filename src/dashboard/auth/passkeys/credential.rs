//! Bounded persisted credential loading, decoding, and snapshot fingerprints.
use super::MAX_PASSKEYS;
use crate::dashboard::{
    db::entities::passkey,
    error::{Error, Result},
};
use sea_orm::{ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, QuerySelect};
use serde_json::Value;
use sha2::{Digest, Sha256};
use uuid::Uuid;
use webauthn_rs::prelude::Passkey;

pub(super) const MAX_CREDENTIAL_ID_BYTES: usize = 1024;
const MAX_CREDENTIAL_JSON_BYTES: usize = 64 * 1024;

// No Debug/Serialize on credential snapshots used by pending authentication.
pub(super) struct OfferedCredential {
    pub(super) id: Uuid,
    pub(super) credential_id: Vec<u8>,
    pub(super) fingerprint: [u8; 32],
}

pub(super) async fn load_credentials(
    db: &impl ConnectionTrait,
    owner: Uuid,
) -> Result<Vec<passkey::Model>> {
    let rows = passkey::Entity::find()
        .filter(passkey::Column::UserId.eq(owner))
        .limit(MAX_PASSKEYS + 1)
        .all(db)
        .await?;
    if rows.len() as u64 > MAX_PASSKEYS {
        return Err(Error::Internal);
    }
    Ok(rows)
}

pub(super) fn credential_fingerprint(credential: &Value) -> Result<[u8; 32]> {
    let bytes = serde_json::to_vec(credential).map_err(|_| Error::Internal)?;
    if bytes.len() > MAX_CREDENTIAL_JSON_BYTES {
        return Err(Error::Internal);
    }
    Ok(Sha256::digest(bytes).into())
}

pub(super) fn decode_credential(row: &passkey::Model) -> Result<Passkey> {
    credential_fingerprint(&row.credential)?;
    if row.credential_id.is_empty() || row.credential_id.len() > MAX_CREDENTIAL_ID_BYTES {
        return Err(Error::Internal);
    }
    let key: Passkey =
        serde_json::from_value(row.credential.clone()).map_err(|_| Error::Internal)?;
    if key.cred_id().as_ref() != row.credential_id.as_slice() {
        return Err(Error::Internal);
    }
    Ok(key)
}
