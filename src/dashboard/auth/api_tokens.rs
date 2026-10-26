//! Per-user API tokens: issuance, revocation, bearer authentication, and the scope policy.
#[cfg(test)]
#[path = "api_tokens_tests.rs"]
mod tests;
use super::{
    accounts::{lock_user, record, require_ready},
    password,
    sessions::{Credential, Identity, Sessions},
};
use crate::dashboard::{
    db::entities::{api_token, user},
    error::{Error, Result},
};
use axum::http::Method;
use chrono::{Duration, Utc};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseConnection, EntityTrait,
    PaginatorTrait, QueryFilter, QueryOrder, Set, TransactionTrait, sea_query::Expr,
};
use uuid::Uuid;

/// Marks AHD tokens for people and secret scanners; the random part is 64 hex digits.
const PREFIX: &str = "ahd_";
const MAX_PER_USER: u64 = 10;
const NAME_LIMIT: usize = 100;
const TOUCH_INTERVAL: Duration = Duration::minutes(5);

/// Dashboard data reads that a token of either scope may perform.
const READ_ROUTES: [&str; 7] = [
    "/api/v1/me",
    "/api/v1/server-settings",
    "/api/v1/me/hoyolab",
    "/api/v1/game-accounts",
    "/api/v1/game-accounts/{id}/status",
    "/api/v1/reward-settings",
    "/api/v1/reward-runs",
];
const RUN_ROUTE: &str = "/api/v1/reward-settings/{id}/run";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::dashboard) enum Scope {
    /// Dashboard data reads only.
    Read,
    /// Reads plus the manual reward run.
    Run,
}

impl Scope {
    pub(in crate::dashboard) fn as_str(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::Run => "run",
        }
    }

    pub(in crate::dashboard) fn parse(value: &str) -> Result<Self> {
        match value {
            "read" => Ok(Self::Read),
            "run" => Ok(Self::Run),
            _ => Err(Error::Invalid("scope must be read or run")),
        }
    }
}

/// Tokens reach only the dashboard data reads and the manual reward run; every other
/// route (account, session, passkey, notification, HoYoLAB link, administration and
/// token management) stays session-only. `route` is the router's matched path template.
pub(in crate::dashboard) fn permits(scope: Scope, method: &Method, route: &str) -> bool {
    if matches!(*method, Method::GET | Method::HEAD) {
        return READ_ROUTES.contains(&route);
    }
    *method == Method::POST && route == RUN_ROUTE && scope == Scope::Run
}

/// The stored hash of a presented `ahd_…` bearer value.
pub(in crate::dashboard) fn hash(presented: &str) -> Result<String> {
    password::token_hash(presented.strip_prefix(PREFIX).ok_or(Error::Unauthorized)?)
}

fn valid_name(name: &str) -> Result<String> {
    let name = name.trim();
    let length = name.chars().count();
    if length == 0 || length > NAME_LIMIT || name.chars().any(char::is_control) {
        return Err(Error::Invalid("API token name must be 1 to 100 characters"));
    }
    Ok(name.to_owned())
}

fn require_session(actor: &Identity) -> Result<()> {
    match actor.credential {
        Credential::Session { .. } => Ok(()),
        Credential::ApiToken { .. } => Err(Error::Forbidden),
    }
}

// The plaintext exists only in this response value and is never Debug/logged.
pub(in crate::dashboard) struct IssuedToken {
    pub(in crate::dashboard) row: api_token::Model,
    pub(in crate::dashboard) token: String,
}

pub(in crate::dashboard) struct ApiTokens;

impl ApiTokens {
    /// Issues a token for the signed-in session's user; a token cannot issue tokens.
    pub(in crate::dashboard) async fn issue(
        db: &DatabaseConnection,
        actor: &Identity,
        name: &str,
        scope: Scope,
    ) -> Result<IssuedToken> {
        require_session(actor)?;
        let name = valid_name(name)?;
        let raw = password::random_secret()?;
        let tx = db.begin().await?;
        lock_user(&tx, actor.user.id).await?;
        let fresh = Sessions::authenticate(&tx, &actor.token_hash).await?;
        if fresh.user.id != actor.user.id {
            return Err(Error::Unauthorized);
        }
        require_ready(&fresh.user)?;
        let existing = api_token::Entity::find()
            .filter(api_token::Column::UserId.eq(fresh.user.id))
            .count(&tx)
            .await?;
        if existing >= MAX_PER_USER {
            return Err(Error::Invalid("too many API tokens"));
        }
        let row = api_token::ActiveModel {
            id: Set(Uuid::new_v4()),
            user_id: Set(fresh.user.id),
            name: Set(name),
            scope: Set(scope.as_str().to_owned()),
            token_hash: Set(password::token_hash(&raw)?),
            created_at: Set(Utc::now()),
            last_used_at: Set(None),
        }
        .insert(&tx)
        .await?;
        record(&tx, Some(fresh.user.id), fresh.user.id, "api_token_created").await?;
        tx.commit().await?;
        Ok(IssuedToken {
            row,
            token: format!("{PREFIX}{raw}"),
        })
    }

    /// The user's tokens, newest first.
    pub(in crate::dashboard) async fn list(
        db: &impl ConnectionTrait,
        user_id: Uuid,
    ) -> Result<Vec<api_token::Model>> {
        Ok(api_token::Entity::find()
            .filter(api_token::Column::UserId.eq(user_id))
            .order_by_desc(api_token::Column::CreatedAt)
            .all(db)
            .await?)
    }

    /// Revokes one of the session user's own tokens.
    pub(in crate::dashboard) async fn revoke(
        db: &DatabaseConnection,
        actor: &Identity,
        id: Uuid,
    ) -> Result<()> {
        require_session(actor)?;
        let tx = db.begin().await?;
        lock_user(&tx, actor.user.id).await?;
        let fresh = Sessions::authenticate(&tx, &actor.token_hash).await?;
        if fresh.user.id != actor.user.id {
            return Err(Error::Unauthorized);
        }
        let deleted = api_token::Entity::delete_many()
            .filter(api_token::Column::Id.eq(id))
            .filter(api_token::Column::UserId.eq(fresh.user.id))
            .exec(&tx)
            .await?;
        if deleted.rows_affected != 1 {
            return Err(Error::NotFound);
        }
        record(&tx, Some(fresh.user.id), fresh.user.id, "api_token_revoked").await?;
        tx.commit().await?;
        Ok(())
    }

    /// Administrator lifecycle actions end every token together with the sessions.
    pub(in crate::dashboard) async fn revoke_all(
        db: &impl ConnectionTrait,
        user_id: Uuid,
    ) -> Result<()> {
        api_token::Entity::delete_many()
            .filter(api_token::Column::UserId.eq(user_id))
            .exec(db)
            .await?;
        Ok(())
    }

    pub(in crate::dashboard) async fn authenticate(
        db: &impl ConnectionTrait,
        hash: &str,
    ) -> Result<Identity> {
        // One SQL snapshot cannot combine a revoked token with a newer user state.
        let (token, user) = api_token::Entity::find()
            .filter(api_token::Column::TokenHash.eq(hash))
            .find_also_related(user::Entity)
            .one(db)
            .await?
            .ok_or(Error::Unauthorized)?;
        let user = user.filter(|u| u.active).ok_or(Error::Unauthorized)?;
        Ok(Identity {
            token_hash: hash.to_owned(),
            user,
            credential: Credential::ApiToken {
                id: token.id,
                scope: Scope::parse(&token.scope).map_err(|_| Error::Unauthorized)?,
            },
        })
    }

    /// Records use at most every few minutes, so ordinary requests rarely write.
    pub(in crate::dashboard) async fn touch(db: &impl ConnectionTrait, id: Uuid) -> Result<()> {
        let now = Utc::now();
        api_token::Entity::update_many()
            .col_expr(api_token::Column::LastUsedAt, Expr::value(Some(now)))
            .filter(api_token::Column::Id.eq(id))
            .filter(
                sea_orm::Condition::any()
                    .add(api_token::Column::LastUsedAt.is_null())
                    .add(api_token::Column::LastUsedAt.lt(now - TOUCH_INTERVAL)),
            )
            .exec(db)
            .await?;
        Ok(())
    }
}
