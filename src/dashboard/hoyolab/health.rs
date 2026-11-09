//! Per-owner HoYoLAB link health for administrators, read from stored rows only.
//!
//! Nothing here calls HoYoLAB. The model carries states, times and the sanitized
//! error codes that are already stored; never Cookies, UIDs or game payloads.
use crate::dashboard::{
    db::entities::{
        game_account, game_status_cache, hoyolab_account, reward_attempt, reward_run,
        reward_setting,
    },
    error::Result,
};
use chrono::{DateTime, Utc};
use sea_orm::{
    ColumnTrait, ConnectionTrait, DbBackend, EntityTrait, QueryFilter, QueryOrder, QuerySelect,
    Statement,
};
use std::collections::HashMap;
use uuid::Uuid;

#[cfg(test)]
#[path = "health_tests.rs"]
mod tests;

/// Link state of a user without a `hoyolab_accounts` row.
pub(in crate::dashboard) const NOT_CONFIGURED: &str = "not_configured";

/// Final run states that mean the claim did not (provably) happen.
const ATTENTION_RUN_STATES: [&str; 3] = ["failed", "action_required", "unknown"];

#[derive(Debug, Clone, PartialEq)]
pub(in crate::dashboard) struct LinkHealth {
    /// `hoyolab_accounts.auth_status`, or [`NOT_CONFIGURED`].
    pub(in crate::dashboard) status: String,
    /// Last successful identity verification.
    pub(in crate::dashboard) verified_at: Option<DateTime<Utc>>,
    /// Newest successful game status fetch over the link's game accounts.
    pub(in crate::dashboard) status_fetched_at: Option<DateTime<Utc>>,
    /// One entry per reward setting, ordered by game kind.
    pub(in crate::dashboard) games: Vec<GameHealth>,
}

#[derive(Debug, Clone, PartialEq)]
pub(in crate::dashboard) struct GameHealth {
    pub(in crate::dashboard) game_kind: String,
    /// Automatic claiming is ON.
    pub(in crate::dashboard) automatic: bool,
    /// The setting's `auth_status` (`ready` or `action_required`).
    pub(in crate::dashboard) setting_status: String,
    /// The newest run that has ended; runs still in flight are not reported.
    pub(in crate::dashboard) last_run: Option<LastRun>,
}

#[derive(Debug, Clone, PartialEq)]
pub(in crate::dashboard) struct LastRun {
    pub(in crate::dashboard) state: String,
    pub(in crate::dashboard) ended_at: DateTime<Utc>,
    /// The sanitized code of the run's last attempt, if it recorded one.
    pub(in crate::dashboard) error_code: Option<String>,
}

impl LinkHealth {
    pub(in crate::dashboard) fn not_configured() -> Self {
        Self {
            status: NOT_CONFIGURED.to_owned(),
            verified_at: None,
            status_fetched_at: None,
            games: Vec::new(),
        }
    }

    /// Games with automatic claiming ON.
    pub(in crate::dashboard) fn automatic_claims(&self) -> u64 {
        self.games.iter().filter(|game| game.automatic).count() as u64
    }

    /// The owner probably needs help: the Cookie stopped working, or on a
    /// verified link a game needs action on HoYoLAB or its newest ended run did
    /// not claim. Missing, pending or removed links are the owner's choice or
    /// still in setup; their kept history does not count.
    pub(in crate::dashboard) fn needs_attention(&self) -> bool {
        match self.status.as_str() {
            "cookie_invalid" => true,
            "verified" => self.games.iter().any(|game| {
                game.setting_status == "action_required"
                    || game
                        .last_run
                        .as_ref()
                        .is_some_and(|run| ATTENTION_RUN_STATES.contains(&run.state.as_str()))
            }),
            _ => false,
        }
    }
}

/// Link health of every user that has a link row, keyed by owner user ID.
pub(in crate::dashboard) async fn by_owner(
    db: &impl ConnectionTrait,
) -> Result<HashMap<Uuid, LinkHealth>> {
    let links = hoyolab_account::Entity::find().all(db).await?;
    let mut fetched: HashMap<Uuid, DateTime<Utc>> = HashMap::new();
    let game_links: HashMap<Uuid, Uuid> = game_account::Entity::find()
        .all(db)
        .await?
        .into_iter()
        .map(|account| (account.id, account.hoyolab_account_id))
        .collect();
    // Only the account and the fetch time: game payloads stay out of the admin path.
    let fetches: Vec<(Uuid, Option<DateTime<Utc>>)> = game_status_cache::Entity::find()
        .select_only()
        .column(game_status_cache::Column::GameAccountId)
        .column(game_status_cache::Column::FetchedAt)
        .filter(game_status_cache::Column::FetchedAt.is_not_null())
        .into_tuple()
        .all(db)
        .await?;
    for (game_account_id, fetched_at) in fetches {
        let (Some(link), Some(at)) = (game_links.get(&game_account_id), fetched_at) else {
            continue;
        };
        let newest = fetched.entry(*link).or_insert(at);
        *newest = (*newest).max(at);
    }

    // DISTINCT ON keeps one row per setting: the newest ended run.
    let runs = reward_run::Entity::find()
        .from_raw_sql(Statement::from_string(
            DbBackend::Postgres,
            "SELECT DISTINCT ON (setting_id) * FROM reward_runs WHERE ended_at IS NOT NULL \
             ORDER BY setting_id, ended_at DESC, id DESC",
        ))
        .all(db)
        .await?;
    let mut codes: HashMap<Uuid, Option<String>> = HashMap::new();
    for attempt in reward_attempt::Entity::find()
        .filter(reward_attempt::Column::RunId.is_in(runs.iter().map(|run| run.id)))
        .order_by_asc(reward_attempt::Column::Number)
        .all(db)
        .await?
    {
        // Ascending order: the last attempt of each run wins.
        codes.insert(attempt.run_id, attempt.error_code);
    }
    let mut last_runs: HashMap<Uuid, LastRun> = HashMap::new();
    for run in runs {
        let Some(ended_at) = run.ended_at else {
            continue;
        };
        last_runs.insert(
            run.setting_id,
            LastRun {
                error_code: codes.remove(&run.id).flatten(),
                state: run.state,
                ended_at,
            },
        );
    }

    let mut games: HashMap<Uuid, Vec<GameHealth>> = HashMap::new();
    for setting in reward_setting::Entity::find()
        .order_by_asc(reward_setting::Column::GameKind)
        .all(db)
        .await?
    {
        games
            .entry(setting.hoyolab_account_id)
            .or_default()
            .push(GameHealth {
                last_run: last_runs.remove(&setting.id),
                game_kind: setting.game_kind,
                automatic: setting.enabled,
                setting_status: setting.auth_status,
            });
    }

    Ok(links
        .into_iter()
        .map(|link| {
            (
                link.owner_user_id,
                LinkHealth {
                    status_fetched_at: fetched.get(&link.id).copied(),
                    games: games.remove(&link.id).unwrap_or_default(),
                    verified_at: link.verified_at,
                    status: link.auth_status,
                },
            )
        })
        .collect())
}
