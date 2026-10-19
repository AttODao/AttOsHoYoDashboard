//! User lifecycle, authorization policy, and audit records.
#[path = "deletion.rs"]
mod deletion;
#[cfg(test)]
#[path = "accounts_tests.rs"]
mod tests;
use super::{
    login_limit::{AttemptLimiter, LoginLimiter},
    password::Passwords,
    sessions::{Identity, IssuedSession, Sessions},
};
use crate::dashboard::{
    db::entities::{audit, deletion_request, hoyolab_account, reward_setting, session, user},
    error::{Error, Result},
};
use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseConnection, DatabaseTransaction,
    EntityTrait, IntoActiveModel, PaginatorTrait, QueryFilter, QuerySelect, Set, SqlErr,
    TransactionTrait,
};
use std::time::Instant;
use uuid::Uuid;

// User lifecycle decisions live here; HTTP only translates requests and responses.
pub(in crate::dashboard) struct Accounts {
    pub(in crate::dashboard) db: DatabaseConnection,
    pub(in crate::dashboard) sessions: Sessions,
    passwords: Passwords,
    deletion_retries: deletion::DeletionRetries,
    login_limit: LoginLimiter,
    reauth_limit: AttemptLimiter<Uuid>,
}

impl Accounts {
    pub(in crate::dashboard) async fn new(
        db: DatabaseConnection,
        ttl: chrono::Duration,
    ) -> Result<Self> {
        Ok(Self {
            db,
            sessions: Sessions::new(ttl),
            passwords: Passwords::new().await?,
            deletion_retries: deletion::DeletionRetries::default(),
            login_limit: LoginLimiter::default(),
            reauth_limit: AttemptLimiter::default(),
        })
    }

    pub(in crate::dashboard) async fn bootstrap_if_empty(
        &self,
        credentials: impl FnOnce() -> Result<(String, String)>,
    ) -> Result<()> {
        if user::Entity::find().count(&self.db).await? != 0 {
            return Ok(());
        }
        let (username, password) = credentials()?;
        // bootstrap_admin owns the final empty-table check and exclusive lock.
        self.bootstrap_admin(username, password).await?;
        Ok(())
    }

    pub(in crate::dashboard) async fn bootstrap_admin(
        &self,
        username: String,
        password: String,
    ) -> Result<user::Model> {
        validate_username(&username)?;
        let password_hash = self.passwords.hash(password).await?;
        let tx = self.db.begin().await?;
        // PostgreSQL serializes the empty-table check too, unlike a row lock.
        tx.execute_unprepared("LOCK TABLE users IN EXCLUSIVE MODE")
            .await?;
        if user::Entity::find().count(&tx).await? != 0 {
            return Err(Error::Invalid(
                "bootstrap-admin requires an empty users table",
            ));
        }
        let user = insert_user(&tx, username, password_hash, true).await?;
        record(&tx, None, user.id, "bootstrap_admin").await?;
        tx.commit().await?;
        Ok(user)
    }

    pub(in crate::dashboard) async fn login(
        &self,
        username: String,
        password: String,
    ) -> Result<(user::Model, IssuedSession)> {
        self.admit_login(&username)?;
        Sessions::purge_expired(&self.db).await?;
        let snapshot = user::Entity::find()
            .filter(user::Column::Username.eq(username))
            .one(&self.db)
            .await?;
        // Unknown users do the same password work; never disclose account existence.
        let valid = self
            .passwords
            .verify(
                password,
                snapshot.as_ref().map(|u| u.password_hash.as_str()),
            )
            .await?;
        let snapshot = snapshot.ok_or(Error::Unauthorized)?;
        let tx = self.db.begin().await?;
        let user = lock_user(&tx, snapshot.id).await?;
        if !valid || !user.active || user.password_hash != snapshot.password_hash {
            return Err(Error::Unauthorized);
        }
        let token = self.sessions.issue(&tx, user.id).await?;
        tx.commit().await?;
        Ok((user, token))
    }

    /// Spends one attempt from the attempted username's sign-in budget.
    pub(super) fn admit_login(&self, username: &str) -> Result<()> {
        self.login_limit.admit_username(username, Instant::now())
    }

    #[cfg(test)]
    pub(in crate::dashboard) fn login_attempts(&self, username: &str) -> u32 {
        self.login_limit.username_attempts(username)
    }

    /// Spends one attempt from a signed-in user's current-password check budget.
    fn admit_reauth(&self, user: Uuid) -> Result<()> {
        self.reauth_limit.admit(user, Instant::now())
    }

    pub(in crate::dashboard) async fn change_password(
        &self,
        actor: &Identity,
        current: Option<String>,
        new: String,
    ) -> Result<(user::Model, IssuedSession)> {
        // Every path below verifies against the stored hash, so each request spends the budget.
        self.admit_reauth(actor.user.id)?;
        // Do expensive password work before locking, then validate the snapshot below.
        let (current_valid, unchanged) = if let Some(current) = current {
            let unchanged = current == new;
            (
                Some(
                    self.passwords
                        .verify(current, Some(&actor.user.password_hash))
                        .await?,
                ),
                unchanged,
            )
        } else {
            (
                None,
                self.passwords
                    .verify(new.clone(), Some(&actor.user.password_hash))
                    .await?,
            )
        };
        let password_hash = self.passwords.hash(new).await?;
        let tx = self.db.begin().await?;
        lock_user(&tx, actor.user.id).await?;
        let fresh = Sessions::authenticate(&tx, &actor.token_hash).await?;
        if fresh.user.id != actor.user.id
            || fresh.user.password_hash != actor.user.password_hash
            || fresh.user.must_change_password != actor.user.must_change_password
        {
            return Err(Error::Unauthorized);
        }
        // Only a freshly authenticated mandatory-setup session may omit the current password.
        match current_valid {
            Some(true) => {}
            None if fresh.user.must_change_password => {}
            _ => return Err(Error::Unauthorized),
        }
        if unchanged {
            return Err(Error::Invalid(
                "new password must differ from current password",
            ));
        }
        let mut update = fresh.user.into_active_model();
        update.password_hash = Set(password_hash);
        update.must_change_password = Set(false);
        let user = update.update(&tx).await?;
        Sessions::revoke_all(&tx, user.id).await?;
        // Changing a password rotates the secret but never extends its login deadline.
        let token = Sessions::issue_until(&tx, user.id, fresh.expires_at).await?;
        record(&tx, Some(user.id), user.id, "password_changed").await?;
        tx.commit().await?;
        Ok((user, token))
    }

    pub(in crate::dashboard) async fn logout(&self, actor: &Identity) -> Result<()> {
        let tx = self.db.begin().await?;
        lock_user(&tx, actor.user.id).await?;
        Sessions::authenticate(&tx, &actor.token_hash).await?;
        session::Entity::delete_by_id(actor.token_hash.clone())
            .exec(&tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }

    pub(in crate::dashboard) async fn list_users(
        &self,
        actor: &Identity,
    ) -> Result<Vec<user::Model>> {
        let fresh = Sessions::authenticate(&self.db, &actor.token_hash).await?;
        require_admin(&fresh.user)?;
        Ok(user::Entity::find().all(&self.db).await?)
    }

    pub(in crate::dashboard) async fn create_user(
        &self,
        actor: &Identity,
        username: String,
        temporary_password: String,
    ) -> Result<user::Model> {
        validate_username(&username)?;
        require_admin(&actor.user)?;
        let password_hash = self.passwords.hash(temporary_password).await?;
        let tx = self.db.begin().await?;
        authorize_admin(&tx, actor, None).await?;
        let user = insert_user(&tx, username, password_hash, false).await?;
        record(&tx, Some(actor.user.id), user.id, "user_created").await?;
        tx.commit().await?;
        Ok(user)
    }

    pub(in crate::dashboard) async fn reset_password(
        &self,
        actor: &Identity,
        target: Uuid,
        temporary_password: String,
    ) -> Result<()> {
        require_admin(&actor.user)?;
        let password_hash = self.passwords.hash(temporary_password).await?;
        let tx = self.db.begin().await?;
        // Same lock order as set_active and request_deletion: membership lock, then user rows.
        lock_membership(&tx).await?;
        authorize_admin(&tx, actor, Some(target)).await?;
        let user = lock_user(&tx, target).await?;
        reject_pending_deletion(&tx, target).await?;
        let mut update = user.into_active_model();
        update.password_hash = Set(password_hash);
        update.must_change_password = Set(true);
        update.update(&tx).await?;
        Sessions::revoke_all(&tx, target).await?;
        super::passkeys::Passkeys::revoke_all(&tx, target).await?;
        record(&tx, Some(actor.user.id), target, "password_reset").await?;
        tx.commit().await?;
        Ok(())
    }

    pub(in crate::dashboard) async fn set_active(
        &self,
        actor: &Identity,
        target: Uuid,
        active: bool,
    ) -> Result<()> {
        require_admin(&actor.user)?;
        if target == actor.user.id && !active {
            return Err(Error::Invalid(
                "cannot disable your own administrator account",
            ));
        }
        let tx = self.db.begin().await?;
        lock_membership(&tx).await?;
        authorize_admin(&tx, actor, Some(target)).await?;
        let user = lock_user(&tx, target).await?;
        reject_pending_deletion(&tx, target).await?;
        let mut update = user.into_active_model();
        update.active = Set(active);
        update.update(&tx).await?;
        Sessions::revoke_all(&tx, target).await?;
        if active {
            use sea_orm::sea_query::Expr;
            if let Some(link) = hoyolab_account::Entity::find()
                .filter(hoyolab_account::Column::OwnerUserId.eq(target))
                .one(&tx)
                .await?
            {
                reward_setting::Entity::update_many()
                    .col_expr(reward_setting::Column::Enabled, Expr::value(false))
                    .col_expr(
                        reward_setting::Column::NextRunAt,
                        Expr::value(Option::<chrono::DateTime<Utc>>::None),
                    )
                    .filter(reward_setting::Column::HoyolabAccountId.eq(link.id))
                    .exec(&tx)
                    .await?;
            }
        }
        record(
            &tx,
            Some(actor.user.id),
            target,
            if active {
                "user_enabled"
            } else {
                "user_disabled"
            },
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }
}

pub(super) fn validate_username(username: &str) -> Result<()> {
    if username.is_empty()
        || username.len() > 64
        || !username
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.'))
    {
        return Err(Error::Invalid(
            "username must contain 1 to 64 ASCII letters, digits, '.', '_' or '-'",
        ));
    }
    Ok(())
}

async fn insert_user(
    db: &impl ConnectionTrait,
    username: String,
    password_hash: String,
    is_admin: bool,
) -> Result<user::Model> {
    user::ActiveModel {
        id: Set(Uuid::new_v4()),
        username: Set(username),
        password_hash: Set(password_hash),
        is_admin: Set(is_admin),
        active: Set(true),
        must_change_password: Set(true),
        created_at: Set(Utc::now()),
    }
    .insert(db)
    .await
    .map_err(|error| {
        if matches!(error.sql_err(), Some(SqlErr::UniqueConstraintViolation(_))) {
            Error::Conflict
        } else {
            Error::Database(error)
        }
    })
}

pub(in crate::dashboard) async fn lock_user(
    db: &DatabaseTransaction,
    id: Uuid,
) -> Result<user::Model> {
    user::Entity::find_by_id(id)
        .lock_exclusive()
        .one(db)
        .await?
        .ok_or(Error::NotFound)
}

/// Serializes administrator membership and lifecycle changes; take it before any user row lock.
pub(super) async fn lock_membership(tx: &DatabaseTransaction) -> Result<()> {
    tx.execute_unprepared("SELECT pg_advisory_xact_lock(198642015)")
        .await?;
    Ok(())
}

/// A pending deletion owns the target's lifecycle; admin recovery must not race the completion.
async fn reject_pending_deletion(tx: &DatabaseTransaction, target: Uuid) -> Result<()> {
    if deletion_request::Entity::find_by_id(target)
        .one(tx)
        .await?
        .is_some()
    {
        return Err(Error::Invalid("user deletion is already in progress"));
    }
    Ok(())
}

async fn authorize_admin(
    tx: &DatabaseTransaction,
    actor: &Identity,
    target: Option<Uuid>,
) -> Result<()> {
    // Consistent lock order prevents two administrators deadlocking each other.
    let mut ids = vec![actor.user.id];
    if let Some(target) = target {
        ids.push(target);
    }
    ids.sort_unstable();
    ids.dedup();
    for id in ids {
        lock_user(tx, id).await?;
    }
    let fresh = Sessions::authenticate(tx, &actor.token_hash).await?;
    if fresh.user.id != actor.user.id {
        return Err(Error::Unauthorized);
    }
    require_admin(&fresh.user)
}

pub(in crate::dashboard) fn require_ready(user: &user::Model) -> Result<()> {
    if !user.active {
        return Err(Error::Unauthorized);
    }
    if user.must_change_password {
        return Err(Error::PasswordChangeRequired);
    }
    Ok(())
}

fn require_admin(user: &user::Model) -> Result<()> {
    require_ready(user)?;
    if !user.is_admin {
        return Err(Error::Forbidden);
    }
    Ok(())
}

pub(in crate::dashboard) async fn record(
    db: &impl ConnectionTrait,
    actor: Option<Uuid>,
    target: Uuid,
    action: &str,
) -> Result<()> {
    audit::ActiveModel {
        id: Set(Uuid::new_v4()),
        actor_user_id: Set(actor),
        target_user_id: Set(target),
        action: Set(action.to_owned()),
        created_at: Set(Utc::now()),
    }
    .insert(db)
    .await?;
    Ok(())
}
