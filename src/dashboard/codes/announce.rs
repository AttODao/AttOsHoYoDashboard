//! Notify-only mode: after each run, every user with `notify` on a UID gets one summary of
//! the codes new to that UID.
//!
//! A code is announced to a UID when it is listed, was not `seeded`, was first seen after
//! the UID's mode left `off` (`enabled_at`) and at most `ANNOUNCE_WITHIN_DAYS` ago, and the
//! UID has no result for it yet and the owner did not queue it for that UID (`bulk`: its
//! redemption result is the news). Codes already known when the mode was switched on are
//! never announced; the history shows them as `available`. Only UIDs that are detected, on
//! a verified link of an active user, take part. A summary that reached at least one route
//! records `announced` for each of its codes and UIDs; one that reached none is retried by
//! the next run.
use crate::dashboard::{
    codes::ledger::BULK,
    codes::settings::redeem_url,
    db::entities::{
        game_account, hoyolab_account, redemption_code, redemption_code_result,
        redemption_code_setting, redemption_ledger, user,
    },
    error::Result,
    notifications::{channel::Notifications, code_notices::NewCode},
};
use chrono::{DateTime, Duration, Utc};
use sea_orm::{
    ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder, Set,
    sea_query::OnConflict,
};
use std::collections::{BTreeMap, BTreeSet};
use uuid::Uuid;

/// Codes first seen longer ago than this are no longer news.
const ANNOUNCE_WITHIN_DAYS: i64 = 7;

/// What one user is told: the UID/code pairs it covers and the codes, in notice order.
#[derive(Default)]
struct Summary {
    pairs: Vec<(Uuid, Uuid)>,
    codes: BTreeSet<Uuid>,
}

/// Sends this run's summaries; returns how many went out.
pub(super) async fn announce(
    db: &DatabaseConnection,
    notifications: &Notifications,
    now: DateTime<Utc>,
) -> Result<u64> {
    // Wave 2 note: `auto` UIDs are not announced here; their redemption result is the news.
    let settings = redemption_code_setting::Entity::find()
        .filter(redemption_code_setting::Column::Mode.eq("notify"))
        .all(db)
        .await?;
    if settings.is_empty() {
        return Ok(0);
    }
    let codes = redemption_code::Entity::find()
        .filter(redemption_code::Column::Listed.eq(true))
        .filter(redemption_code::Column::Seeded.eq(false))
        .filter(
            redemption_code::Column::FirstSeenAt.gte(now - Duration::days(ANNOUNCE_WITHIN_DAYS)),
        )
        .order_by_asc(redemption_code::Column::GameKind)
        .order_by_desc(redemption_code::Column::FirstSeenAt)
        .order_by_asc(redemption_code::Column::Code)
        .all(db)
        .await?;
    if codes.is_empty() {
        return Ok(0);
    }
    let ids: Vec<Uuid> = settings.iter().map(|row| row.game_account_id).collect();
    let accounts: BTreeMap<Uuid, game_account::Model> = game_account::Entity::find()
        .filter(game_account::Column::Id.is_in(ids.clone()))
        .filter(game_account::Column::Detected.eq(true))
        .all(db)
        .await?
        .into_iter()
        .map(|row| (row.id, row))
        .collect();
    let links: BTreeMap<Uuid, hoyolab_account::Model> = hoyolab_account::Entity::find()
        .filter(
            hoyolab_account::Column::Id.is_in(
                accounts
                    .values()
                    .map(|row| row.hoyolab_account_id)
                    .collect::<Vec<_>>(),
            ),
        )
        .filter(hoyolab_account::Column::AuthStatus.eq("verified"))
        .all(db)
        .await?
        .into_iter()
        .map(|row| (row.id, row))
        .collect();
    let active: BTreeSet<Uuid> = user::Entity::find()
        .filter(
            user::Column::Id.is_in(
                links
                    .values()
                    .map(|row| row.owner_user_id)
                    .collect::<Vec<_>>(),
            ),
        )
        .filter(user::Column::Active.eq(true))
        .all(db)
        .await?
        .into_iter()
        .map(|row| row.id)
        .collect();
    let done: BTreeSet<(Uuid, Uuid)> = redemption_code_result::Entity::find()
        .filter(redemption_code_result::Column::GameAccountId.is_in(ids.clone()))
        .all(db)
        .await?
        .into_iter()
        .map(|row| (row.game_account_id, row.code_id))
        .collect();
    // Codes the owner queued once for a UID (sent, answered or still waiting).
    let queued: BTreeSet<(Uuid, String)> = redemption_ledger::Entity::find()
        .filter(redemption_ledger::Column::GameAccountId.is_in(ids))
        .filter(redemption_ledger::Column::Source.eq(BULK))
        .all(db)
        .await?
        .into_iter()
        .map(|row| (row.game_account_id, row.code))
        .collect();

    let mut summaries: BTreeMap<Uuid, Summary> = BTreeMap::new();
    for setting in &settings {
        let Some(since) = setting.enabled_at else {
            continue;
        };
        let Some(account) = accounts.get(&setting.game_account_id) else {
            continue;
        };
        let Some(link) = links.get(&account.hoyolab_account_id) else {
            continue;
        };
        if !active.contains(&link.owner_user_id) {
            continue;
        }
        for code in codes.iter().filter(|code| {
            code.game_kind == account.game_kind
                && code.first_seen_at >= since
                && !done.contains(&(account.id, code.id))
                && !queued.contains(&(account.id, code.code.clone()))
        }) {
            let summary = summaries.entry(link.owner_user_id).or_default();
            summary.pairs.push((account.id, code.id));
            summary.codes.insert(code.id);
        }
    }

    let key = format!("codes:{}", now.timestamp());
    let mut sent = 0;
    for (owner, summary) in summaries {
        // `codes` is already in notice order: game, newest first, code.
        let listed: Vec<NewCode> = codes
            .iter()
            .filter(|code| summary.codes.contains(&code.id))
            .filter_map(|code| {
                Some(NewCode {
                    redeem_url: redeem_url(&code.game_kind, &code.code)?,
                    game_kind: code.game_kind.clone(),
                    code: code.code.clone(),
                    rewards: code.rewards.clone(),
                })
            })
            .collect();
        // One user's delivery problem must not hold back everyone else's summary.
        match notifications.announce_codes(owner, &key, &listed).await {
            Ok(true) => {
                sent += 1;
                // Unrecorded pairs are offered again next run (a repeat beats a silent loss).
                if let Err(error) = record(db, &summary.pairs, now).await {
                    tracing::error!(error = %error, "recording announced redemption codes failed");
                }
            }
            Ok(false) => {}
            Err(error) => tracing::warn!(error = %error, "redemption code summary failed"),
        }
    }
    Ok(sent)
}

async fn record(db: &DatabaseConnection, pairs: &[(Uuid, Uuid)], now: DateTime<Utc>) -> Result<()> {
    for (account, code) in pairs {
        redemption_code_result::Entity::insert(redemption_code_result::ActiveModel {
            game_account_id: Set(*account),
            code_id: Set(*code),
            state: Set("announced".to_owned()),
            created_at: Set(now),
            updated_at: Set(now),
        })
        .on_conflict(
            OnConflict::columns([
                redemption_code_result::Column::GameAccountId,
                redemption_code_result::Column::CodeId,
            ])
            .do_nothing()
            .to_owned(),
        )
        .exec_without_returning(db)
        .await?;
    }
    Ok(())
}
