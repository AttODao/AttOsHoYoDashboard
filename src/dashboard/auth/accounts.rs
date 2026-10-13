//! User lifecycle, authorization policy, and audit records.
use super::{
    password::Passwords,
    sessions::{Identity, IssuedSession, Sessions},
};
use crate::dashboard::{
    db::entities::{audit, session, user},
    error::{Error, Result},
};
use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseConnection, DatabaseTransaction,
    EntityTrait, IntoActiveModel, PaginatorTrait, QueryFilter, QuerySelect, Set, SqlErr,
    TransactionTrait,
};
use uuid::Uuid;

// User lifecycle decisions live here; HTTP only translates requests and responses.
pub(in crate::dashboard) struct Accounts {
    pub(in crate::dashboard) db: DatabaseConnection,
    pub(in crate::dashboard) sessions: Sessions,
    passwords: Passwords,
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
        })
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

    pub(in crate::dashboard) async fn change_password(
        &self,
        actor: &Identity,
        current: String,
        new: String,
    ) -> Result<(user::Model, IssuedSession)> {
        if !self
            .passwords
            .verify(current.clone(), Some(&actor.user.password_hash))
            .await?
        {
            return Err(Error::Unauthorized);
        }
        if current == new {
            return Err(Error::Invalid(
                "new password must differ from current password",
            ));
        }
        let password_hash = self.passwords.hash(new).await?;
        let tx = self.db.begin().await?;
        lock_user(&tx, actor.user.id).await?;
        let fresh = Sessions::authenticate(&tx, &actor.token_hash).await?;
        if fresh.user.password_hash != actor.user.password_hash {
            return Err(Error::Unauthorized);
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
        authorize_admin(&tx, actor, Some(target)).await?;
        let user = lock_user(&tx, target).await?;
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
        authorize_admin(&tx, actor, Some(target)).await?;
        let user = lock_user(&tx, target).await?;
        let mut update = user.into_active_model();
        update.active = Set(active);
        update.update(&tx).await?;
        Sessions::revoke_all(&tx, target).await?;
        // Re-enabling must also turn reward settings OFF once that module exists.
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

fn validate_username(username: &str) -> Result<()> {
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
