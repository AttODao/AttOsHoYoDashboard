//! Session issuance, authentication, expiry, and revocation.
use super::password;
use crate::dashboard::{
    db::entities::{session, user},
    error::{Error, Result},
};
use chrono::{DateTime, Duration, Utc};
use sea_orm::{ActiveModelTrait, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, Set};
use uuid::Uuid;

pub(in crate::dashboard) struct Identity {
    pub(in crate::dashboard) token_hash: String,
    pub(in crate::dashboard) user: user::Model,
    pub(in crate::dashboard) expires_at: DateTime<Utc>,
}

// The bearer value is transported only in an HttpOnly cookie, never Debug/JSON.
pub(in crate::dashboard) struct IssuedSession {
    pub(in crate::dashboard) token: String,
    pub(in crate::dashboard) expires_at: DateTime<Utc>,
}

pub(in crate::dashboard) struct Sessions {
    ttl: Duration,
}

impl Sessions {
    pub(in crate::dashboard) fn new(ttl: Duration) -> Self {
        Self { ttl }
    }

    // Issuance happens inside the caller's user-locked transaction.
    pub(in crate::dashboard) async fn issue(
        &self,
        db: &impl ConnectionTrait,
        user_id: Uuid,
    ) -> Result<IssuedSession> {
        let expires_at = Utc::now()
            .checked_add_signed(self.ttl)
            .ok_or(Error::Internal)?;
        Self::issue_until(db, user_id, expires_at).await
    }

    pub(in crate::dashboard) async fn issue_until(
        db: &impl ConnectionTrait,
        user_id: Uuid,
        expires_at: DateTime<Utc>,
    ) -> Result<IssuedSession> {
        let raw = password::random_secret()?;
        session::ActiveModel {
            token_hash: Set(password::token_hash(&raw)?),
            user_id: Set(user_id),
            expires_at: Set(expires_at),
        }
        .insert(db)
        .await?;
        Ok(IssuedSession {
            token: raw,
            expires_at,
        })
    }

    pub(in crate::dashboard) async fn authenticate(
        db: &impl ConnectionTrait,
        hash: &str,
    ) -> Result<Identity> {
        // One SQL snapshot cannot combine a revoked session with a newer user state.
        let (session, user) = session::Entity::find_by_id(hash)
            .find_also_related(user::Entity)
            .one(db)
            .await?
            .ok_or(Error::Unauthorized)?;
        if session.expires_at <= Utc::now() {
            return Err(Error::Unauthorized);
        }
        let user = user.filter(|u| u.active).ok_or(Error::Unauthorized)?;
        Ok(Identity {
            token_hash: hash.to_owned(),
            user,
            expires_at: session.expires_at,
        })
    }

    pub(in crate::dashboard) async fn revoke_all(
        db: &impl ConnectionTrait,
        user_id: Uuid,
    ) -> Result<()> {
        session::Entity::delete_many()
            .filter(session::Column::UserId.eq(user_id))
            .exec(db)
            .await?;
        Ok(())
    }

    pub(in crate::dashboard) async fn purge_expired(db: &impl ConnectionTrait) -> Result<()> {
        session::Entity::delete_many()
            .filter(session::Column::ExpiresAt.lte(Utc::now()))
            .exec(db)
            .await?;
        Ok(())
    }
}
