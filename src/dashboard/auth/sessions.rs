//! Session issuance, authentication, expiry, and revocation.
#[cfg(test)]
#[path = "sessions_tests.rs"]
mod tests;
use super::{
    api_tokens::{ApiTokens, Scope},
    password,
};
use crate::dashboard::{
    db::entities::{session, user},
    error::{Error, Result},
};
use chrono::{DateTime, Duration, Utc};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, QueryOrder, Set,
    sea_query::Expr,
};
use uuid::Uuid;

/// How a request authenticated; `token_hash` in `Identity` is the hash of that secret.
pub(in crate::dashboard) enum Credential {
    Session { expires_at: DateTime<Utc> },
    ApiToken { id: Uuid, scope: Scope },
}

pub(in crate::dashboard) struct Identity {
    pub(in crate::dashboard) token_hash: String,
    pub(in crate::dashboard) user: user::Model,
    pub(in crate::dashboard) credential: Credential,
}

impl Identity {
    /// The session deadline; API tokens have none and cannot use session-only operations.
    pub(in crate::dashboard) fn session_expires_at(&self) -> Result<DateTime<Utc>> {
        match self.credential {
            Credential::Session { expires_at } => Ok(expires_at),
            Credential::ApiToken { .. } => Err(Error::Forbidden),
        }
    }
}

/// Fresh re-authentication for the operations an API token may also reach; session-only
/// operations keep calling `Sessions::authenticate`, which never accepts a token hash.
pub(in crate::dashboard) async fn reauthenticate(
    db: &impl ConnectionTrait,
    actor: &Identity,
) -> Result<Identity> {
    let fresh = match actor.credential {
        Credential::Session { .. } => Sessions::authenticate(db, &actor.token_hash).await?,
        Credential::ApiToken { .. } => ApiTokens::authenticate(db, &actor.token_hash).await?,
    };
    if fresh.user.id != actor.user.id {
        return Err(Error::Unauthorized);
    }
    Ok(fresh)
}

// The bearer value is transported only in an HttpOnly cookie, never Debug/JSON.
pub(in crate::dashboard) struct IssuedSession {
    pub(in crate::dashboard) token: String,
    pub(in crate::dashboard) expires_at: DateTime<Utc>,
}

const TOUCH_INTERVAL: Duration = Duration::minutes(5);

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
            id: Set(Uuid::new_v4()),
            created_at: Set(Utc::now()),
            last_seen_at: Set(None),
            user_agent: Set(None),
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
            credential: Credential::Session {
                expires_at: session.expires_at,
            },
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

    /// Notes which browser holds a newly issued session (truncated, display only).
    pub(in crate::dashboard) async fn describe(
        db: &impl ConnectionTrait,
        token: &str,
        user_agent: Option<&str>,
    ) -> Result<()> {
        let agent = user_agent
            .map(|agent| {
                agent
                    .chars()
                    .filter(|c| !c.is_control())
                    .take(256)
                    .collect::<String>()
            })
            .filter(|agent| !agent.trim().is_empty());
        session::Entity::update_many()
            .col_expr(session::Column::UserAgent, Expr::value(agent))
            .filter(session::Column::TokenHash.eq(password::token_hash(token)?))
            .exec(db)
            .await?;
        Ok(())
    }

    /// Records use at most every few minutes, so ordinary requests rarely write.
    pub(in crate::dashboard) async fn touch(db: &impl ConnectionTrait, hash: &str) -> Result<()> {
        let now = Utc::now();
        session::Entity::update_many()
            .col_expr(session::Column::LastSeenAt, Expr::value(Some(now)))
            .filter(session::Column::TokenHash.eq(hash))
            .filter(
                sea_orm::Condition::any()
                    .add(session::Column::LastSeenAt.is_null())
                    .add(session::Column::LastSeenAt.lt(now - TOUCH_INTERVAL)),
            )
            .exec(db)
            .await?;
        Ok(())
    }

    /// The user's unexpired sessions, newest first.
    pub(in crate::dashboard) async fn list(
        db: &impl ConnectionTrait,
        user_id: Uuid,
    ) -> Result<Vec<session::Model>> {
        Ok(session::Entity::find()
            .filter(session::Column::UserId.eq(user_id))
            .filter(session::Column::ExpiresAt.gt(Utc::now()))
            .order_by_desc(session::Column::CreatedAt)
            .all(db)
            .await?)
    }

    /// Ends one of the actor's other sessions; the current one ends by signing out.
    pub(in crate::dashboard) async fn revoke(
        db: &impl ConnectionTrait,
        actor: &Identity,
        id: Uuid,
    ) -> Result<()> {
        let target = session::Entity::find()
            .filter(session::Column::Id.eq(id))
            .filter(session::Column::UserId.eq(actor.user.id))
            .one(db)
            .await?
            .ok_or(Error::NotFound)?;
        if target.token_hash == actor.token_hash {
            return Err(Error::Conflict);
        }
        session::Entity::delete_by_id(target.token_hash)
            .exec(db)
            .await?;
        Ok(())
    }

    /// Ends every session of the actor except the current one; returns how many.
    pub(in crate::dashboard) async fn revoke_others(
        db: &impl ConnectionTrait,
        actor: &Identity,
    ) -> Result<u64> {
        Ok(session::Entity::delete_many()
            .filter(session::Column::UserId.eq(actor.user.id))
            .filter(session::Column::TokenHash.ne(actor.token_hash.as_str()))
            .exec(db)
            .await?
            .rows_affected)
    }

    pub(in crate::dashboard) async fn purge_expired(db: &impl ConnectionTrait) -> Result<()> {
        session::Entity::delete_many()
            .filter(session::Column::ExpiresAt.lte(Utc::now()))
            .exec(db)
            .await?;
        Ok(())
    }
}
