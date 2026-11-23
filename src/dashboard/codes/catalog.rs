//! The shared code catalogue: what the source listed per game, when, and whether it still does.
use super::source::Listing;
use crate::dashboard::{db::entities::redemption_code, error::Result};
use chrono::{DateTime, Duration, Utc};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, IntoActiveModel, QueryFilter,
    QuerySelect, Set, TransactionTrait, sea_query::Expr,
};
use std::collections::BTreeSet;
use uuid::Uuid;

/// Games the catalogue follows (`game_kind` values); AHD supports no others.
pub(in crate::dashboard) const GAMES: [&str; 2] = ["genshin", "starrail"];
/// Codes the source stopped listing are kept this long as history, then deleted with
/// their per-UID results.
const KEEP_UNLISTED_DAYS: i64 = 90;

/// Stores a successful listing of `game` at `now`: unknown codes are added, listed ones
/// refreshed and the rest marked unlisted. With `first_run` (no run has fully succeeded
/// yet) codes added while the game has none are `seeded`: they predate AHD watching and
/// are never announced. Later, an empty game (the source listed nothing, or pruning
/// emptied it) takes new codes as news. Returns how many codes were new. A failed fetch
/// never reaches this, so failures never remove codes.
pub(super) async fn store(
    db: &DatabaseConnection,
    game: &str,
    listing: &Listing,
    now: DateTime<Utc>,
    first_run: bool,
) -> Result<u64> {
    let tx = db.begin().await?;
    let existing = redemption_code::Entity::find()
        .filter(redemption_code::Column::GameKind.eq(game))
        .lock_exclusive()
        .all(&tx)
        .await?;
    let seeded = first_run && existing.is_empty();
    let mut listed = BTreeSet::new();
    let mut added = 0;
    for code in &listing.codes {
        listed.insert(code.code.as_str());
        match existing.iter().find(|row| row.code == code.code) {
            Some(row) => {
                let mut row = row.clone().into_active_model();
                // An empty text never replaces a known one.
                if !code.rewards.is_empty() {
                    row.rewards = Set(code.rewards.clone());
                }
                row.last_seen_at = Set(now);
                row.listed = Set(true);
                row.update(&tx).await?;
            }
            None => {
                redemption_code::ActiveModel {
                    id: Set(Uuid::new_v4()),
                    game_kind: Set(game.to_owned()),
                    code: Set(code.code.clone()),
                    rewards: Set(code.rewards.clone()),
                    first_seen_at: Set(now),
                    last_seen_at: Set(now),
                    listed: Set(true),
                    seeded: Set(seeded),
                }
                .insert(&tx)
                .await?;
                added += 1;
            }
        }
    }
    let gone: Vec<Uuid> = existing
        .iter()
        .filter(|row| row.listed && !listed.contains(row.code.as_str()))
        .map(|row| row.id)
        .collect();
    if !gone.is_empty() {
        redemption_code::Entity::update_many()
            .col_expr(redemption_code::Column::Listed, Expr::value(false))
            .filter(redemption_code::Column::Id.is_in(gone))
            .exec(&tx)
            .await?;
    }
    tx.commit().await?;
    Ok(added)
}

/// Deletes codes unlisted for longer than `KEEP_UNLISTED_DAYS`.
pub(super) async fn prune(db: &DatabaseConnection, now: DateTime<Utc>) -> Result<u64> {
    Ok(redemption_code::Entity::delete_many()
        .filter(redemption_code::Column::Listed.eq(false))
        .filter(redemption_code::Column::LastSeenAt.lt(now - Duration::days(KEEP_UNLISTED_DAYS)))
        .exec(db)
        .await?
        .rows_affected)
}

#[cfg(test)]
#[path = "catalog_tests.rs"]
mod tests;
