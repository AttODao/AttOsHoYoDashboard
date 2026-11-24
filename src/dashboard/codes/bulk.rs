//! Redeeming the listed codes once for UIDs outside `auto` (`off` or `notify`), on the
//! owner's explicit request and without changing their mode.
//!
//! The request queues one `bulk` ledger row per chosen UID and listed code of its game
//! that the UID has no ledger row for yet and HoYoLAB has not rejected for everyone. The
//! runs then send those rows like manual entries (whatever the mode, with the usual
//! pacing and per-run cap, so a long list continues over the following runs), but only
//! while the source lists the code: a queued code that leaves the list is dropped
//! (`ledger::drop_unlisted_in`). The ledger key keeps every code at one send per UID,
//! whatever the mode becomes later.
use super::{
    ledger::{BULK, PENDING},
    manual::{detected, session_owner},
    settings::modes_in,
};
use crate::dashboard::{
    auth::sessions::Identity,
    db::entities::{game_account, redemption_code, redemption_code_rejection, redemption_ledger},
    error::{Error, Result},
};
use chrono::Utc;
use sea_orm::{
    ColumnTrait, ConnectionTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder,
    QuerySelect, Set, TransactionTrait, sea_query::OnConflict,
};
use serde::Serialize;
use std::collections::BTreeSet;
use uuid::Uuid;

/// One UID outside `auto` and how many listed codes a request would queue for it now.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub(in crate::dashboard) struct Candidate {
    game_account_id: Uuid,
    game_kind: String,
    game_uid: String,
    region: String,
    /// `off` or `notify`; it stays as it is.
    mode: String,
    /// Listed codes of the UID's game without a ledger row for it and not rejected.
    codes: u64,
}

/// What a request queued for one UID, and what it left out.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub(in crate::dashboard) struct Queued {
    game_account_id: Uuid,
    game_kind: String,
    game_uid: String,
    queued: u64,
    /// Listed codes this UID already has a ledger row for (sent, answered or waiting).
    already_recorded: u64,
    /// Listed codes HoYoLAB rejected for everyone (expired or invalid).
    rejected: u64,
}

/// The request's answer: one entry per chosen UID and the totals.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub(in crate::dashboard) struct Summary {
    accounts: Vec<Queued>,
    queued: u64,
    already_recorded: u64,
    rejected: u64,
}

/// The listed codes of one UID's game, split by what a request does with them.
struct Selection<'a> {
    account: &'a game_account::Model,
    codes: Vec<String>,
    already_recorded: u64,
    rejected: u64,
}

/// What a request would do for `accounts`, oldest listing first. With `lock`, the listed
/// codes are read under a shared lock, so the catalogue cannot unlist one before the rows
/// are written (and its drop would miss them).
async fn select<'a>(
    db: &impl ConnectionTrait,
    accounts: &'a [game_account::Model],
    lock: bool,
) -> Result<Vec<Selection<'a>>> {
    if accounts.is_empty() {
        return Ok(Vec::new());
    }
    let games: BTreeSet<&str> = accounts.iter().map(|row| row.game_kind.as_str()).collect();
    let query = redemption_code::Entity::find()
        .filter(redemption_code::Column::GameKind.is_in(games))
        .filter(redemption_code::Column::Listed.eq(true))
        .order_by_asc(redemption_code::Column::FirstSeenAt)
        .order_by_asc(redemption_code::Column::Code);
    let listed = if lock {
        query.lock_shared().all(db).await?
    } else {
        query.all(db).await?
    };
    let ids: Vec<Uuid> = accounts.iter().map(|row| row.id).collect();
    let recorded: BTreeSet<(Uuid, String)> = redemption_ledger::Entity::find()
        .filter(redemption_ledger::Column::GameAccountId.is_in(ids))
        .all(db)
        .await?
        .into_iter()
        .map(|row| (row.game_account_id, row.code))
        .collect();
    let rejected: BTreeSet<(String, String)> = redemption_code_rejection::Entity::find()
        .all(db)
        .await?
        .into_iter()
        .map(|row| (row.game_kind, row.code))
        .collect();
    Ok(accounts
        .iter()
        .map(|account| {
            let mut selection = Selection {
                account,
                codes: Vec::new(),
                already_recorded: 0,
                rejected: 0,
            };
            for code in listed
                .iter()
                .filter(|code| code.game_kind == account.game_kind)
            {
                if recorded.contains(&(account.id, code.code.clone())) {
                    selection.already_recorded += 1;
                } else if rejected.contains(&(code.game_kind.clone(), code.code.clone())) {
                    selection.rejected += 1;
                } else {
                    selection.codes.push(code.code.clone());
                }
            }
            selection
        })
        .collect())
}

/// The link's detected UIDs with their modes, in a stable order.
async fn outside_auto(
    db: &impl ConnectionTrait,
    link: Uuid,
) -> Result<(Vec<game_account::Model>, Vec<(Uuid, String)>)> {
    let mut accounts = detected(db, link, None).await?;
    accounts.sort_by(|a, b| {
        (&a.game_kind, &a.game_uid, &a.region).cmp(&(&b.game_kind, &b.game_uid, &b.region))
    });
    let ids: Vec<Uuid> = accounts.iter().map(|row| row.id).collect();
    let modes = modes_in(db, &ids, false).await?;
    let modes: Vec<(Uuid, String)> = accounts
        .iter()
        .map(|row| (row.id, modes.get(&row.id).cloned().unwrap_or_default()))
        .collect();
    Ok((accounts, modes))
}

/// The UIDs a request may choose and their counts, for `GET /code-redemption`.
pub(super) async fn candidates(db: &impl ConnectionTrait, link: Uuid) -> Result<Vec<Candidate>> {
    let (accounts, modes) = outside_auto(db, link).await?;
    let accounts: Vec<game_account::Model> = accounts
        .into_iter()
        .zip(&modes)
        .filter(|(_, (_, mode))| mode != "auto")
        .map(|(account, _)| account)
        .collect();
    Ok(select(db, &accounts, false)
        .await?
        .into_iter()
        .map(|selection| {
            let account = selection.account;
            Candidate {
                game_account_id: account.id,
                game_kind: account.game_kind.clone(),
                game_uid: account.game_uid.clone(),
                region: account.region.clone(),
                mode: modes
                    .iter()
                    .find(|(id, _)| *id == account.id)
                    .map_or_else(|| "off".to_owned(), |(_, mode)| mode.clone()),
                codes: selection.codes.len() as u64,
            }
        })
        .collect())
}

/// Queues the listed codes for the owner's UIDs outside `auto`: every one of them, or
/// `only` (each must be a detected UID of the owner's, 404 otherwise, and not `auto`, 400
/// otherwise). Session only, like code entry, and refused while redemption waits for the
/// owner's action (409): the queue would only wait behind it. The caller starts the run.
pub(in crate::dashboard) async fn enqueue(
    db: &DatabaseConnection,
    actor: &Identity,
    only: Option<Vec<Uuid>>,
) -> Result<Summary> {
    let now = Utc::now();
    let tx = db.begin().await?;
    let link = session_owner(&tx, actor).await?;
    if link.redemption_action.is_some() {
        return Err(Error::Conflict);
    }
    // Mode changes take the same user lock (`session_owner`), so none lands in between.
    let (accounts, modes) = outside_auto(&tx, link.id).await?;
    let is_auto = |id: Uuid| {
        modes
            .iter()
            .any(|(known, mode)| *known == id && mode == "auto")
    };
    let chosen: Vec<game_account::Model> = match only {
        None => accounts
            .into_iter()
            .filter(|account| !is_auto(account.id))
            .collect(),
        Some(ids) => {
            if ids.is_empty() {
                return Err(Error::Invalid("game_account_ids must not be empty"));
            }
            let ids: BTreeSet<Uuid> = ids.into_iter().collect();
            if ids
                .iter()
                .any(|id| !accounts.iter().any(|account| account.id == *id))
            {
                return Err(Error::NotFound);
            }
            if ids.iter().any(|id| is_auto(*id)) {
                return Err(Error::Invalid(
                    "game_account_ids must name UIDs outside automatic mode",
                ));
            }
            accounts
                .into_iter()
                .filter(|account| ids.contains(&account.id))
                .collect()
        }
    };
    if chosen.is_empty() {
        return Err(Error::NotFound);
    }
    let selections = select(&tx, &chosen, true).await?;
    for selection in &selections {
        for code in &selection.codes {
            redemption_ledger::Entity::insert(redemption_ledger::ActiveModel {
                game_account_id: Set(selection.account.id),
                code: Set(code.clone()),
                source: Set(BULK.to_owned()),
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
    }
    tx.commit().await?;
    let accounts: Vec<Queued> = selections
        .into_iter()
        .map(|selection| Queued {
            game_account_id: selection.account.id,
            game_kind: selection.account.game_kind.clone(),
            game_uid: selection.account.game_uid.clone(),
            queued: selection.codes.len() as u64,
            already_recorded: selection.already_recorded,
            rejected: selection.rejected,
        })
        .collect();
    Ok(Summary {
        queued: accounts.iter().map(|row| row.queued).sum(),
        already_recorded: accounts.iter().map(|row| row.already_recorded).sum(),
        rejected: accounts.iter().map(|row| row.rejected).sum(),
        accounts,
    })
}

#[cfg(test)]
#[path = "bulk_tests.rs"]
mod tests;
