//! Per-UID code modes, the catalogue and per-UID history as users read them, and the mode
//! rules other features share (Cookie removal, settings transfer).
//!
//! Modes: `off` (default), `notify` (announce new codes) and `auto` (redeem automatically).
//! `auto` is a valid stored value, but until automatic redemption exists it is refused
//! (`AUTO_REDEEM_AVAILABLE`); switching that on is the one change that enables it.
use crate::dashboard::{
    auth::{
        accounts::{lock_user, require_ready},
        sessions::{Identity, Sessions, reauthenticate},
    },
    db::entities::{
        game_account, hoyolab_account, redemption_code, redemption_code_result,
        redemption_code_setting, redemption_code_source,
    },
    error::{Error, Result},
};
use chrono::{DateTime, Utc};
use sea_orm::{
    ColumnTrait, ConnectionTrait, DatabaseConnection, DatabaseTransaction, EntityTrait,
    QueryFilter, QueryOrder, Set, TransactionTrait, sea_query::OnConflict,
};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use uuid::Uuid;

/// Every mode a UID may store.
pub(in crate::dashboard) const MODES: [&str; 3] = ["off", "notify", "auto"];
/// Whether `auto` may be chosen. False until automatic redemption is implemented: the API
/// and settings import refuse it, so no UID silently waits for redemptions that never run.
pub(in crate::dashboard) const AUTO_REDEEM_AVAILABLE: bool = false;
/// The official redemption page for a code; codes are `[A-Z0-9]`, so no escaping is needed.
pub(in crate::dashboard) fn redeem_url(game_kind: &str, code: &str) -> Option<String> {
    match game_kind {
        "genshin" => Some(format!("https://genshin.hoyoverse.com/en/gift?code={code}")),
        "starrail" => Some(format!("https://hsr.hoyoverse.com/gift?code={code}")),
        _ => None,
    }
}

/// Rejects unknown modes and, while it is unavailable, `auto`.
pub(in crate::dashboard) fn validate_mode(mode: &str) -> Result<()> {
    if !MODES.contains(&mode) {
        return Err(Error::Invalid("mode must be off, notify or auto"));
    }
    if mode == "auto" && !AUTO_REDEEM_AVAILABLE {
        return Err(Error::Invalid(
            "automatic redemption is not available yet; use notify",
        ));
    }
    Ok(())
}

/// Stores `mode` for `accounts` inside the caller's transaction. `enabled_at` starts when
/// a UID leaves `off` and is kept while it stays on, so switching between `notify` and
/// `auto` never re-announces or skips codes. Refuses what `validate_mode` refuses.
pub(in crate::dashboard) async fn set_mode_in(
    tx: &DatabaseTransaction,
    accounts: &[Uuid],
    mode: &str,
    now: DateTime<Utc>,
) -> Result<()> {
    validate_mode(mode)?;
    let stored: BTreeMap<Uuid, redemption_code_setting::Model> =
        redemption_code_setting::Entity::find()
            .filter(redemption_code_setting::Column::GameAccountId.is_in(accounts.to_vec()))
            .all(tx)
            .await?
            .into_iter()
            .map(|row| (row.game_account_id, row))
            .collect();
    for account in accounts {
        let enabled_at = match (mode, stored.get(account).and_then(|row| row.enabled_at)) {
            ("off", _) => None,
            (_, Some(since)) => Some(since),
            (_, None) => Some(now),
        };
        redemption_code_setting::Entity::insert(redemption_code_setting::ActiveModel {
            game_account_id: Set(*account),
            mode: Set(mode.to_owned()),
            enabled_at: Set(enabled_at),
            updated_at: Set(now),
        })
        .on_conflict(
            OnConflict::column(redemption_code_setting::Column::GameAccountId)
                .update_columns([
                    redemption_code_setting::Column::Mode,
                    redemption_code_setting::Column::EnabledAt,
                    redemption_code_setting::Column::UpdatedAt,
                ])
                .to_owned(),
        )
        .exec_without_returning(tx)
        .await?;
    }
    Ok(())
}

/// The stored modes of `accounts` (`off` when none is stored).
pub(in crate::dashboard) async fn modes_in(
    db: &impl ConnectionTrait,
    accounts: &[Uuid],
    lock: bool,
) -> Result<BTreeMap<Uuid, String>> {
    let query = redemption_code_setting::Entity::find()
        .filter(redemption_code_setting::Column::GameAccountId.is_in(accounts.to_vec()));
    let rows = if lock {
        sea_orm::QuerySelect::lock_exclusive(query).all(db).await?
    } else {
        query.all(db).await?
    };
    let mut modes: BTreeMap<Uuid, String> = accounts
        .iter()
        .map(|account| (*account, "off".to_owned()))
        .collect();
    for row in rows {
        modes.insert(row.game_account_id, row.mode);
    }
    Ok(modes)
}

/// Cookie removal turns every UID of the link off, as it turns automatic claiming off;
/// results stay as history.
pub(in crate::dashboard) async fn turn_off_link(
    tx: &DatabaseTransaction,
    link: Uuid,
) -> Result<()> {
    let accounts: Vec<Uuid> = game_account::Entity::find()
        .filter(game_account::Column::HoyolabAccountId.eq(link))
        .all(tx)
        .await?
        .into_iter()
        .map(|row| row.id)
        .collect();
    if accounts.is_empty() {
        return Ok(());
    }
    redemption_code_setting::Entity::update_many()
        .col_expr(
            redemption_code_setting::Column::Mode,
            sea_orm::sea_query::Expr::value("off"),
        )
        .col_expr(
            redemption_code_setting::Column::EnabledAt,
            sea_orm::sea_query::Expr::value(Option::<DateTime<Utc>>::None),
        )
        .col_expr(
            redemption_code_setting::Column::UpdatedAt,
            sea_orm::sea_query::Expr::value(Utc::now()),
        )
        .filter(redemption_code_setting::Column::GameAccountId.is_in(accounts))
        .exec(tx)
        .await?;
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub(in crate::dashboard) struct SettingView {
    game_account_id: Uuid,
    game_kind: String,
    game_uid: String,
    region: String,
    mode: String,
    enabled_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub(in crate::dashboard) struct CodeView {
    id: Uuid,
    game_kind: String,
    code: String,
    rewards: String,
    first_seen_at: DateTime<Utc>,
    last_seen_at: DateTime<Utc>,
    listed: bool,
    redeem_url: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub(in crate::dashboard) struct CatalogueView {
    /// The administrator's server-wide switch; off means no fetches and no announcements.
    enabled: bool,
    last_success_at: Option<DateTime<Utc>>,
    codes: Vec<CodeView>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub(in crate::dashboard) struct HistoryEntry {
    game_account_id: Uuid,
    game_kind: String,
    game_uid: String,
    code_id: Uuid,
    code: String,
    listed: bool,
    /// The stored result (`announced`, or a redemption outcome: `pending`, `redeemed`,
    /// `already_used`, `expired`, `invalid`, `rank_too_low`, `cookie_invalid`,
    /// `captcha_required`, `unknown`), or `available` for a listed code with no result yet.
    state: String,
    /// When the result was last written; null for `available`.
    updated_at: Option<DateTime<Utc>>,
    redeem_url: Option<String>,
}

impl CodeView {
    fn from_row(row: redemption_code::Model) -> Self {
        Self {
            redeem_url: redeem_url(&row.game_kind, &row.code),
            id: row.id,
            game_kind: row.game_kind,
            code: row.code,
            rewards: row.rewards,
            first_seen_at: row.first_seen_at,
            last_seen_at: row.last_seen_at,
            listed: row.listed,
        }
    }
}

/// The owner's verified link, as reward settings require.
async fn verified_link(db: &impl ConnectionTrait, owner: Uuid) -> Result<hoyolab_account::Model> {
    hoyolab_account::Entity::find()
        .filter(hoyolab_account::Column::OwnerUserId.eq(owner))
        .one(db)
        .await?
        .filter(|link| link.auth_status == "verified" && link.hoyolab_uid.is_some())
        .ok_or(Error::InitialSetupRequired)
}

/// The link's detected UIDs, in a stable order.
async fn detected_accounts(
    db: &impl ConnectionTrait,
    link: Uuid,
) -> Result<Vec<game_account::Model>> {
    Ok(game_account::Entity::find()
        .filter(game_account::Column::HoyolabAccountId.eq(link))
        .filter(game_account::Column::Detected.eq(true))
        .order_by_asc(game_account::Column::GameKind)
        .order_by_asc(game_account::Column::GameUid)
        .order_by_asc(game_account::Column::Region)
        .all(db)
        .await?)
}

pub(in crate::dashboard) struct CodeSettings {
    db: DatabaseConnection,
}

impl CodeSettings {
    pub(in crate::dashboard) fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }

    /// A fresh identity for reads; API tokens may read (see `auth::api_tokens::permits`).
    async fn reader(&self, tx: &DatabaseTransaction, actor: &Identity) -> Result<Uuid> {
        let fresh = reauthenticate(tx, actor).await?;
        if fresh.user.id != actor.user.id {
            return Err(Error::Unauthorized);
        }
        require_ready(&fresh.user)?;
        Ok(fresh.user.id)
    }

    /// Mode per detected UID of the verified link.
    pub(in crate::dashboard) async fn list(&self, actor: &Identity) -> Result<Vec<SettingView>> {
        let tx = self.db.begin().await?;
        let user = self.reader(&tx, actor).await?;
        let link = verified_link(&tx, user).await?;
        let accounts = detected_accounts(&tx, link.id).await?;
        let ids: Vec<Uuid> = accounts.iter().map(|row| row.id).collect();
        let stored: BTreeMap<Uuid, redemption_code_setting::Model> =
            redemption_code_setting::Entity::find()
                .filter(redemption_code_setting::Column::GameAccountId.is_in(ids))
                .all(&tx)
                .await?
                .into_iter()
                .map(|row| (row.game_account_id, row))
                .collect();
        tx.commit().await?;
        Ok(accounts
            .into_iter()
            .map(|account| {
                let setting = stored.get(&account.id);
                SettingView {
                    game_account_id: account.id,
                    game_kind: account.game_kind,
                    game_uid: account.game_uid,
                    region: account.region,
                    mode: setting.map_or_else(|| "off".to_owned(), |row| row.mode.clone()),
                    enabled_at: setting.and_then(|row| row.enabled_at),
                }
            })
            .collect())
    }

    /// Sets one UID's mode. Session only: `Sessions::authenticate` never accepts an API
    /// token, whatever the route allow-list says.
    pub(in crate::dashboard) async fn update(
        &self,
        actor: &Identity,
        game_account: Uuid,
        mode: &str,
    ) -> Result<()> {
        validate_mode(mode)?;
        let tx = self.db.begin().await?;
        lock_user(&tx, actor.user.id).await?;
        let fresh = Sessions::authenticate(&tx, &actor.token_hash).await?;
        if fresh.user.id != actor.user.id {
            return Err(Error::Unauthorized);
        }
        require_ready(&fresh.user)?;
        let link = verified_link(&tx, fresh.user.id).await?;
        let account = game_account::Entity::find_by_id(game_account)
            .filter(game_account::Column::HoyolabAccountId.eq(link.id))
            .filter(game_account::Column::Detected.eq(true))
            .one(&tx)
            .await?
            .ok_or(Error::NotFound)?;
        set_mode_in(&tx, &[account.id], mode, Utc::now()).await?;
        tx.commit().await?;
        Ok(())
    }

    /// The shared catalogue, newest first per game. Any ready user may read it.
    pub(in crate::dashboard) async fn catalogue(&self, actor: &Identity) -> Result<CatalogueView> {
        let tx = self.db.begin().await?;
        self.reader(&tx, actor).await?;
        let source = redemption_code_source::Entity::find_by_id(1_i16)
            .one(&tx)
            .await?;
        let codes = redemption_code::Entity::find()
            .order_by_asc(redemption_code::Column::GameKind)
            .order_by_desc(redemption_code::Column::FirstSeenAt)
            .order_by_asc(redemption_code::Column::Code)
            .all(&tx)
            .await?;
        tx.commit().await?;
        Ok(CatalogueView {
            enabled: source.as_ref().is_none_or(|row| row.enabled),
            last_success_at: source.and_then(|row| row.last_success_at),
            codes: codes.into_iter().map(CodeView::from_row).collect(),
        })
    }

    /// Every code of each detected UID's game: its result, or `available` while it is
    /// listed and has none. Codes no longer listed appear only with a result.
    pub(in crate::dashboard) async fn history(
        &self,
        actor: &Identity,
    ) -> Result<Vec<HistoryEntry>> {
        let tx = self.db.begin().await?;
        let user = self.reader(&tx, actor).await?;
        let link = verified_link(&tx, user).await?;
        let accounts = detected_accounts(&tx, link.id).await?;
        let ids: Vec<Uuid> = accounts.iter().map(|row| row.id).collect();
        let games: BTreeSet<String> = accounts.iter().map(|row| row.game_kind.clone()).collect();
        let codes = redemption_code::Entity::find()
            .filter(redemption_code::Column::GameKind.is_in(games))
            .order_by_desc(redemption_code::Column::FirstSeenAt)
            .order_by_asc(redemption_code::Column::Code)
            .all(&tx)
            .await?;
        let results: BTreeMap<(Uuid, Uuid), redemption_code_result::Model> =
            redemption_code_result::Entity::find()
                .filter(redemption_code_result::Column::GameAccountId.is_in(ids))
                .all(&tx)
                .await?
                .into_iter()
                .map(|row| ((row.game_account_id, row.code_id), row))
                .collect();
        tx.commit().await?;
        let mut entries = Vec::new();
        for account in &accounts {
            for code in codes
                .iter()
                .filter(|code| code.game_kind == account.game_kind)
            {
                let result = results.get(&(account.id, code.id));
                if result.is_none() && !code.listed {
                    continue;
                }
                entries.push(HistoryEntry {
                    game_account_id: account.id,
                    game_kind: account.game_kind.clone(),
                    game_uid: account.game_uid.clone(),
                    code_id: code.id,
                    code: code.code.clone(),
                    listed: code.listed,
                    state: result.map_or_else(|| "available".to_owned(), |row| row.state.clone()),
                    updated_at: result.map(|row| row.updated_at),
                    redeem_url: redeem_url(&code.game_kind, &code.code),
                });
            }
        }
        Ok(entries)
    }
}

#[cfg(test)]
#[path = "settings_tests.rs"]
mod tests;
