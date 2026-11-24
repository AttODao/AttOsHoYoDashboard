//! What a run may send for one owner, and the admission right before each POST.
//!
//! Work comes from two places: every listed catalogue code for each `auto` UID (including
//! codes listed before the mode was switched on), and the owner's own manual entries for
//! any detected UID. Only verified links whose sealed Cookie carries the redemption
//! credentials, with no action pending, of active users that finished setup take part.
use super::ledger::{CATALOGUE, MANUAL, NO_GAME_ACCOUNT, PENDING, SENDING, rejection};
use crate::dashboard::{
    db::entities::{
        game_account, hoyolab_account, redemption_code, redemption_code_rejection,
        redemption_code_setting, redemption_code_source, redemption_ledger, user,
    },
    error::{Error, Result},
    hoyolab::{credentials::Credentials, gateway::Gateway, store::CookieStore},
};
use chrono::{DateTime, Utc};
use sea_orm::{
    ColumnTrait, ConnectionTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder,
    QuerySelect, Set, TransactionTrait,
    sea_query::{Expr, OnConflict},
};
use std::collections::{BTreeMap, BTreeSet};
use uuid::Uuid;

/// One code for one UID.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Item {
    pub(super) game_account: Uuid,
    pub(super) game_kind: String,
    pub(super) game_uid: String,
    pub(super) region: String,
    pub(super) code: String,
    /// `catalogue` or `manual`.
    pub(super) source: &'static str,
    /// The catalogue's reward text; empty for manual codes.
    pub(super) rewards: String,
}

/// One owner's work, bound to the Cookie version it was planned with.
#[derive(Debug)]
pub(super) struct Plan {
    pub(super) owner: Uuid,
    pub(super) link: Uuid,
    pub(super) credential_version: i64,
    pub(super) items: Vec<Item>,
}

/// The admission decision for one item.
pub(super) enum Admission {
    /// The row is now `sending`: POST with this Cookie.
    Send(Credentials),
    /// This item no longer applies; continue with the next.
    Skip,
    /// The owner, link or server no longer allows POSTs: end this owner's run.
    Stop,
}

/// Whether a row may be sent now.
fn due(row: &redemption_ledger::Model, now: DateTime<Utc>) -> bool {
    let waited = row.next_attempt_at.is_none_or(|at| at <= now);
    match row.state.as_str() {
        PENDING => waited,
        // Retried once a day, and only while the source lists it (catalogue rows).
        "rank_too_low" => row.source == CATALOGUE && row.next_attempt_at.is_some() && waited,
        _ => false,
    }
}

/// A link that may redeem now.
fn link_ready(link: &hoyolab_account::Model) -> bool {
    link.auth_status == "verified"
        && link.hoyolab_uid.is_some()
        && link.encrypted_credentials.is_some()
        && link.redemption_credentials
        && link.redemption_action.is_none()
}

/// Owners with possible work: an `auto` UID or a waiting manual row on a ready link.
pub(super) async fn owners(db: &DatabaseConnection) -> Result<Vec<Uuid>> {
    let auto: Vec<Uuid> = redemption_code_setting::Entity::find()
        .select_only()
        .column(redemption_code_setting::Column::GameAccountId)
        .filter(redemption_code_setting::Column::Mode.eq("auto"))
        .into_tuple()
        .all(db)
        .await?;
    let manual: Vec<Uuid> = redemption_ledger::Entity::find()
        .select_only()
        .column(redemption_ledger::Column::GameAccountId)
        .filter(redemption_ledger::Column::Source.eq(MANUAL))
        .filter(redemption_ledger::Column::State.eq(PENDING))
        .distinct()
        .into_tuple()
        .all(db)
        .await?;
    let accounts: BTreeSet<Uuid> = auto.into_iter().chain(manual).collect();
    if accounts.is_empty() {
        return Ok(Vec::new());
    }
    let links: BTreeSet<Uuid> = game_account::Entity::find()
        .filter(game_account::Column::Id.is_in(accounts))
        .filter(game_account::Column::Detected.eq(true))
        .all(db)
        .await?
        .into_iter()
        .map(|row| row.hoyolab_account_id)
        .collect();
    let owners: Vec<Uuid> = hoyolab_account::Entity::find()
        .filter(hoyolab_account::Column::Id.is_in(links))
        .all(db)
        .await?
        .into_iter()
        .filter(link_ready)
        .map(|row| row.owner_user_id)
        .collect();
    if owners.is_empty() {
        return Ok(owners);
    }
    Ok(user::Entity::find()
        .filter(user::Column::Id.is_in(owners))
        .filter(user::Column::Active.eq(true))
        .filter(user::Column::MustChangePassword.eq(false))
        .order_by_asc(user::Column::Id)
        .all(db)
        .await?
        .into_iter()
        .map(|row| row.id)
        .collect())
}

/// The owner's due items: manual entries first (oldest first), then catalogue codes for
/// `auto` UIDs, oldest listing first. A UID HoYoLAB did not find on the Cookie's account
/// (`no_game_account`) is left out until a Cookie update or an explicit retry reopens it.
/// None when the owner cannot redeem now.
pub(super) async fn plan(
    db: &DatabaseConnection,
    owner: Uuid,
    now: DateTime<Utc>,
) -> Result<Option<Plan>> {
    let Some(link) = hoyolab_account::Entity::find()
        .filter(hoyolab_account::Column::OwnerUserId.eq(owner))
        .one(db)
        .await?
        .filter(link_ready)
    else {
        return Ok(None);
    };
    let accounts = game_account::Entity::find()
        .filter(game_account::Column::HoyolabAccountId.eq(link.id))
        .filter(game_account::Column::Detected.eq(true))
        .order_by_asc(game_account::Column::GameKind)
        .order_by_asc(game_account::Column::GameUid)
        .order_by_asc(game_account::Column::Region)
        .all(db)
        .await?;
    let ids: Vec<Uuid> = accounts.iter().map(|row| row.id).collect();
    let auto: BTreeSet<Uuid> = redemption_code_setting::Entity::find()
        .filter(redemption_code_setting::Column::GameAccountId.is_in(ids.clone()))
        .filter(redemption_code_setting::Column::Mode.eq("auto"))
        .all(db)
        .await?
        .into_iter()
        .map(|row| row.game_account_id)
        .collect();
    let rows = redemption_ledger::Entity::find()
        .filter(redemption_ledger::Column::GameAccountId.is_in(ids))
        .order_by_asc(redemption_ledger::Column::CreatedAt)
        .order_by_asc(redemption_ledger::Column::Code)
        .all(db)
        .await?;
    let ledger: BTreeMap<(Uuid, &str), &redemption_ledger::Model> = rows
        .iter()
        .map(|row| ((row.game_account_id, row.code.as_str()), row))
        .collect();
    let not_found: BTreeSet<Uuid> = rows
        .iter()
        .filter(|row| row.state == NO_GAME_ACCOUNT)
        .map(|row| row.game_account_id)
        .collect();
    let accounts: Vec<game_account::Model> = accounts
        .into_iter()
        .filter(|account| !not_found.contains(&account.id))
        .collect();
    let rejected: BTreeSet<(String, String)> = redemption_code_rejection::Entity::find()
        .all(db)
        .await?
        .into_iter()
        .map(|row| (row.game_kind, row.code))
        .collect();
    let item = |account: &game_account::Model, code: &str, source, rewards: &str| Item {
        game_account: account.id,
        game_kind: account.game_kind.clone(),
        game_uid: account.game_uid.clone(),
        region: account.region.clone(),
        code: code.to_owned(),
        source,
        rewards: rewards.to_owned(),
    };
    let mut items = Vec::new();
    for row in rows
        .iter()
        .filter(|row| row.source == MANUAL && due(row, now))
    {
        if let Some(account) = accounts.iter().find(|a| a.id == row.game_account_id) {
            items.push(item(account, &row.code, MANUAL, ""));
        }
    }
    let games: BTreeSet<&str> = accounts
        .iter()
        .filter(|account| auto.contains(&account.id))
        .map(|account| account.game_kind.as_str())
        .collect();
    if !games.is_empty() {
        let codes = redemption_code::Entity::find()
            .filter(redemption_code::Column::GameKind.is_in(games))
            .filter(redemption_code::Column::Listed.eq(true))
            .order_by_asc(redemption_code::Column::FirstSeenAt)
            .order_by_asc(redemption_code::Column::Code)
            .all(db)
            .await?;
        for account in accounts.iter().filter(|account| auto.contains(&account.id)) {
            for code in codes.iter().filter(|code| {
                code.game_kind == account.game_kind
                    && !rejected.contains(&(code.game_kind.clone(), code.code.clone()))
            }) {
                let wanted = match ledger.get(&(account.id, code.code.as_str())) {
                    None => true,
                    Some(row) => row.source == CATALOGUE && due(row, now),
                };
                if wanted {
                    items.push(item(account, &code.code, CATALOGUE, &code.rewards));
                }
            }
        }
    }
    if items.is_empty() {
        return Ok(None);
    }
    Ok(Some(Plan {
        owner,
        link: link.id,
        credential_version: link.credential_version,
        items,
    }))
}

/// The last check before a POST, in one transaction under the owner's row lock: the user,
/// link and Cookie version still match the plan, the administrator's switch is on, the UID
/// is still detected (and `auto` for catalogue codes, which must still be listed), the code
/// is not rejected, and the row is due. The row is then `sending` with its POST counted,
/// so whatever happens next it is never sent again automatically.
pub(super) async fn admit<G: Gateway>(
    store: &CookieStore<G>,
    plan: &Plan,
    item: &Item,
    now: DateTime<Utc>,
) -> Result<Admission> {
    let tx = store.db.begin().await?;
    let (link, cookie) = match store.owner_link_in(&tx, plan.owner).await {
        Ok(value) => value,
        Err(
            Error::Unauthorized
            | Error::PasswordChangeRequired
            | Error::InitialSetupRequired
            | Error::Unavailable(_),
        ) => return Ok(Admission::Stop),
        Err(error) => return Err(error),
    };
    if link.id != plan.link
        || link.credential_version != plan.credential_version
        || !link_ready(&link)
        || cookie.redemption().is_none()
        || !switched_on(&tx).await?
    {
        return Ok(Admission::Stop);
    }
    let Some(account) = game_account::Entity::find_by_id(item.game_account)
        .filter(game_account::Column::HoyolabAccountId.eq(link.id))
        .filter(game_account::Column::Detected.eq(true))
        .one(&tx)
        .await?
    else {
        return Ok(Admission::Skip);
    };
    if item.source == CATALOGUE {
        let modes = super::settings::modes_in(&tx, &[account.id], true).await?;
        let listed = redemption_code::Entity::find()
            .filter(redemption_code::Column::GameKind.eq(&account.game_kind))
            .filter(redemption_code::Column::Code.eq(&item.code))
            .filter(redemption_code::Column::Listed.eq(true))
            .one(&tx)
            .await?
            .is_some();
        if modes.get(&account.id).map(String::as_str) != Some("auto") || !listed {
            return Ok(Admission::Skip);
        }
        redemption_ledger::Entity::insert(redemption_ledger::ActiveModel {
            game_account_id: Set(account.id),
            code: Set(item.code.clone()),
            source: Set(CATALOGUE.to_owned()),
            state: Set(PENDING.to_owned()),
            posts: Set(0),
            retcode: Set(None),
            next_attempt_at: Set(None),
            last_sent_at: Set(None),
            created_at: Set(now),
            updated_at: Set(now),
        })
        .on_conflict(
            OnConflict::columns([
                redemption_ledger::Column::GameAccountId,
                redemption_ledger::Column::Code,
            ])
            .do_nothing()
            .to_owned(),
        )
        .exec_without_returning(&tx)
        .await?;
    }
    if let Some(reason) = rejection(&tx, &account.game_kind, &item.code).await? {
        super::ledger::reject_in(&tx, &account.game_kind, &item.code, &reason, now).await?;
        tx.commit().await?;
        return Ok(Admission::Skip);
    }
    let Some(row) = redemption_ledger::Entity::find_by_id((account.id, item.code.clone()))
        .lock_exclusive()
        .one(&tx)
        .await?
        .filter(|row| due(row, now))
    else {
        return Ok(Admission::Skip);
    };
    redemption_ledger::Entity::update_many()
        .col_expr(redemption_ledger::Column::State, Expr::value(SENDING))
        .col_expr(redemption_ledger::Column::Posts, Expr::value(row.posts + 1))
        .col_expr(
            redemption_ledger::Column::LastSentAt,
            Expr::value(Some(now)),
        )
        .col_expr(
            redemption_ledger::Column::NextAttemptAt,
            Expr::value(Option::<DateTime<Utc>>::None),
        )
        .col_expr(redemption_ledger::Column::UpdatedAt, Expr::value(now))
        .filter(redemption_ledger::Column::GameAccountId.eq(account.id))
        .filter(redemption_ledger::Column::Code.eq(&item.code))
        .exec(&tx)
        .await?;
    tx.commit().await?;
    Ok(Admission::Send(cookie))
}

/// The administrator's server-wide switch (on when the row is missing, as for fetches).
pub(super) async fn switched_on(db: &impl ConnectionTrait) -> Result<bool> {
    Ok(redemption_code_source::Entity::find_by_id(1_i16)
        .one(db)
        .await?
        .is_none_or(|row| row.enabled))
}

#[cfg(test)]
#[path = "plan_tests.rs"]
mod tests;
