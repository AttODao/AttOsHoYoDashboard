//! Fresh identity/roles consistency, gated until the upstream principal contract is proven.
use super::{credentials::Credentials, gateway::Gateway, store::CookieStore};
use crate::dashboard::{
    auth::{accounts::record, sessions::Identity},
    db::entities::{game_account, hoyolab_account, reward_guard, reward_setting},
    error::{Error, Result},
};
use attosgameapi::models::account::Account;
use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseTransaction, EntityTrait, IntoActiveModel, QueryFilter,
    Set, SqlErr, TransactionTrait, sea_query::OnConflict,
};
use std::collections::HashSet;
use uuid::Uuid;

impl<G: Gateway> CookieStore<G> {
    pub(in crate::dashboard) async fn verify(&self, actor: &Identity) -> Result<()> {
        let _operation = self.operations.acquire(actor.user.id).await;
        self.require_binding()?;
        let tx = self.db.begin().await?;
        self.actor_locked(&tx, actor).await?;
        let row = hoyolab_account::Entity::find()
            .filter(hoyolab_account::Column::OwnerUserId.eq(actor.user.id))
            .one(&tx)
            .await?
            .ok_or(Error::InitialSetupRequired)?;
        let cookie = self.key.open(
            actor.user.id,
            row.hoyolab_uid.as_deref(),
            &row.encrypted_credentials,
        )?;
        tx.commit().await?;
        self.accept_verified(actor, &cookie, row.hoyolab_uid.as_deref())
            .await
    }
    pub(super) async fn accept_verified(
        &self,
        actor: &Identity,
        cookie: &Credentials,
        expected: Option<&str>,
    ) -> Result<()> {
        self.require_binding()?;
        // Account identity protocol is currently a v2 ltoken candidate only.
        if !cookie.as_str().starts_with("ltoken_v2=") {
            return Err(Error::Invalid(
                "v2 ltoken credentials are required for identity verification",
            ));
        }
        let (account_uid, community_uid) = self.gateway.identity(cookie).await?;
        if account_uid != community_uid || account_uid != cookie.claimed_uid() {
            return Err(Error::Forbidden);
        }
        if expected.is_some_and(|uid| uid != account_uid) {
            return Err(Error::Forbidden);
        }
        let tx = self.db.begin().await?;
        self.actor_locked(&tx, actor).await?;
        tx.commit().await?;
        let roles = self.gateway.roles(cookie).await?;
        validate_roles(&roles)?;
        let tx = self.db.begin().await?;
        self.actor_locked(&tx, actor).await?;
        let row = hoyolab_account::Entity::find()
            .filter(hoyolab_account::Column::OwnerUserId.eq(actor.user.id))
            .one(&tx)
            .await?
            .ok_or(Error::InitialSetupRequired)?;
        if row.hoyolab_uid.as_deref() != expected {
            return Err(Error::Forbidden);
        }
        let version = row
            .credential_version
            .checked_add(1)
            .ok_or(Error::Internal)?;
        let mut row = row.into_active_model();
        row.hoyolab_uid = Set(Some(account_uid.clone()));
        row.encrypted_credentials =
            Set(self.key.seal(actor.user.id, Some(&account_uid), cookie)?);
        row.credential_version = Set(version);
        row.auth_status = Set("verified".into());
        row.updated_at = Set(Utc::now());
        let row = row.update(&tx).await.map_err(|e| {
            if matches!(e.sql_err(), Some(SqlErr::UniqueConstraintViolation(_))) {
                Error::Forbidden
            } else {
                Error::Database(e)
            }
        })?;
        apply_roles(&tx, row.id, &roles).await?;
        for kind in ["genshin", "starrail"] {
            let setting = reward_setting::Entity::find()
                .filter(reward_setting::Column::HoyolabAccountId.eq(row.id))
                .filter(reward_setting::Column::GameKind.eq(kind))
                .one(&tx)
                .await?;
            if let Some(setting) = setting {
                let next = if setting.enabled {
                    Some(crate::dashboard::rewards::schedule::next_due(
                        Utc::now(),
                        &setting.local_time,
                        self.timezone,
                    )?)
                } else {
                    None
                };
                let mut setting = setting.into_active_model();
                setting.auth_status = Set("ready".into());
                setting.next_run_at = Set(next);
                setting.update(&tx).await?;
            } else {
                let id = Uuid::new_v4();
                reward_setting::ActiveModel {
                    id: Set(id),
                    hoyolab_account_id: Set(row.id),
                    game_kind: Set(kind.into()),
                    enabled: Set(false),
                    local_time: Set("06:00".into()),
                    auth_status: Set("ready".into()),
                    next_run_at: Set(None),
                }
                .insert(&tx)
                .await?;
                reward_guard::ActiveModel {
                    setting_id: Set(id),
                    post_may_have_happened: Set(false),
                    updated_at: Set(Utc::now()),
                }
                .insert(&tx)
                .await?;
            }
        }
        record(
            &tx,
            Some(actor.user.id),
            actor.user.id,
            "hoyolab_cookie_verified",
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }
    pub(in crate::dashboard) async fn sync(&self, actor: &Identity) -> Result<()> {
        let _operation = self.operations.acquire(actor.user.id).await;
        let (row, cookie) = self.ready_link(actor).await?;
        let roles = self.gateway.roles(&cookie).await.map_err(Error::from)?;
        validate_roles(&roles)?;
        let tx = self.db.begin().await?;
        self.actor_locked(&tx, actor).await?;
        let (fresh, _) = self.owner_link_in(&tx, actor.user.id).await?;
        if fresh.id != row.id || fresh.credential_version != row.credential_version {
            return Err(Error::Forbidden);
        }
        apply_roles(&tx, row.id, &roles).await?;
        tx.commit().await?;
        Ok(())
    }
}
fn validate_roles(roles: &[Account]) -> Result<()> {
    if roles.len() > 256 {
        return Err(Error::Invalid("too many upstream game accounts"));
    }
    for role in roles
        .iter()
        .filter(|role| matches!(role.game_biz.as_str(), "hk4e_global" | "hkrpg_global"))
    {
        if role
            .game_uid
            .parse::<u64>()
            .ok()
            .filter(|uid| *uid > 0)
            .is_none()
            || role.region.is_empty()
            || role.region.len() > 64
            || !role
                .region
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_')
        {
            return Err(Error::Invalid("invalid upstream game account"));
        }
    }
    Ok(())
}
async fn apply_roles(tx: &DatabaseTransaction, link: Uuid, roles: &[Account]) -> Result<()> {
    use game_account::Column;
    use sea_orm::sea_query::Expr;
    game_account::Entity::update_many()
        .col_expr(game_account::Column::Detected, Expr::value(false))
        .filter(game_account::Column::HoyolabAccountId.eq(link))
        .exec(tx)
        .await?;
    let mut seen = HashSet::new();
    for role in roles {
        let kind = match role.game_biz.as_str() {
            "hk4e_global" => "genshin",
            "hkrpg_global" => "starrail",
            _ => continue,
        };
        let uid = role
            .game_uid
            .parse::<u64>()
            .map_err(|_| Error::Invalid("invalid game UID"))?
            .to_string();
        if !seen.insert((kind, uid.clone(), role.region.clone())) {
            continue;
        }
        game_account::Entity::insert(game_account::ActiveModel {
            id: Set(Uuid::new_v4()),
            hoyolab_account_id: Set(link),
            game_kind: Set(kind.into()),
            game_uid: Set(uid),
            region: Set(role.region.clone()),
            visible: Set(false),
            detected: Set(true),
            last_detected_at: Set(Utc::now()),
        })
        .on_conflict(
            OnConflict::columns([
                Column::HoyolabAccountId,
                Column::GameKind,
                Column::GameUid,
                Column::Region,
            ])
            .update_columns([Column::Detected, Column::LastDetectedAt])
            .to_owned(),
        )
        .exec_without_returning(tx)
        .await?;
    }
    Ok(())
}
