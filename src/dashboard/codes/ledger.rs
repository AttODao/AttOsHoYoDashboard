//! The redemption ledger: one row per game UID and code text, written before every POST and
//! never pruned with the catalogue, so a code is never sent twice for a UID.
//!
//! A row is `pending` until it is admitted, then `sending` while a POST may be in flight,
//! then an outcome. `sending` left behind by a crash or an error after the POST becomes
//! `unknown`, and `unknown` is never sent again automatically (like the reward guard).
//! Cooldown answers put the row back to `pending` after a wait; `rank_too_low` is retried at
//! most once a day while the source lists the code; `cookie_invalid`, `captcha_required` and
//! `no_game_account` go back to `pending` only when the Cookie is updated or the owner
//! retries explicitly. Every answered row keeps HoYoLAB's numeric retcode (never a message).
//! Expired and invalid codes are also recorded globally (`redemption_code_rejections`), so
//! nobody sends them again, whatever the source lists later.
use crate::dashboard::{
    db::entities::{game_account, hoyolab_account, redemption_code_rejection, redemption_ledger},
    error::Result,
};
use attoshoyodashboard::hoyolab::{Error as UpstreamError, RedemptionAnswer, RedemptionReply};
use chrono::{DateTime, Duration, Utc};
use sea_orm::{
    ColumnTrait, ConnectionTrait, DatabaseConnection, EntityTrait, QueryFilter, QuerySelect,
    TransactionTrait,
    sea_query::{Expr, OnConflict},
};
use uuid::Uuid;

pub(in crate::dashboard) const PENDING: &str = "pending";
pub(in crate::dashboard) const SENDING: &str = "sending";
pub(in crate::dashboard) const CATALOGUE: &str = "catalogue";
pub(in crate::dashboard) const MANUAL: &str = "manual";
/// HoYoLAB found no game account for the UID and server on the Cookie's account: that UID
/// sends nothing more until the Cookie is updated or the owner retries.
pub(in crate::dashboard) const NO_GAME_ACCOUNT: &str = "no_game_account";
/// States the owner's action (a Cookie update or an explicit retry) returns to `pending`.
const REOPENED: [&str; 3] = ["cookie_invalid", "captcha_required", NO_GAME_ACCOUNT];
/// Every state a row may have, as `/metrics` reports them.
pub(in crate::dashboard) const STATES: [&str; 11] = [
    "pending",
    "sending",
    "redeemed",
    "already_used",
    "expired",
    "invalid",
    "rank_too_low",
    "cookie_invalid",
    "captcha_required",
    "no_game_account",
    "unknown",
];
/// A rank-too-low code is tried again after this long, while the source lists it.
pub(super) const RANK_RETRY: Duration = Duration::days(1);

/// What one redemption answer means.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::dashboard) enum Outcome {
    Redeemed,
    AlreadyUsed,
    Expired,
    Invalid,
    /// Region lock or not activated for this account: `invalid` for this UID only.
    NotForAccount,
    RankTooLow,
    CookieInvalid,
    CaptchaRequired,
    /// No game account for this UID and server on the Cookie's account (-1073): this UID
    /// only, never a pause of the whole owner.
    NoGameAccount,
    /// Not processed (cooldown or too frequent): the code waits and is tried again.
    Cooldown,
    /// Transport error, unreadable answer or an unclassified retcode.
    Unknown,
}

impl Outcome {
    /// Every outcome, in the order `/metrics` reports them.
    pub(in crate::dashboard) const ALL: [Self; 11] = [
        Self::Redeemed,
        Self::AlreadyUsed,
        Self::Expired,
        Self::Invalid,
        Self::NotForAccount,
        Self::RankTooLow,
        Self::CookieInvalid,
        Self::CaptchaRequired,
        Self::NoGameAccount,
        Self::Cooldown,
        Self::Unknown,
    ];

    /// Any error means the request may have been processed: the outcome is unknown.
    pub(in crate::dashboard) fn of(
        answer: &std::result::Result<RedemptionAnswer, UpstreamError>,
    ) -> Self {
        let Ok(answer) = answer else {
            return Self::Unknown;
        };
        match answer.reply {
            RedemptionReply::Redeemed => Self::Redeemed,
            RedemptionReply::AlreadyUsed => Self::AlreadyUsed,
            RedemptionReply::Expired => Self::Expired,
            RedemptionReply::Invalid => Self::Invalid,
            RedemptionReply::NotForAccount => Self::NotForAccount,
            RedemptionReply::RankTooLow => Self::RankTooLow,
            RedemptionReply::CookieInvalid => Self::CookieInvalid,
            RedemptionReply::CaptchaRequired => Self::CaptchaRequired,
            RedemptionReply::NoGameAccount => Self::NoGameAccount,
            RedemptionReply::Cooldown => Self::Cooldown,
            RedemptionReply::Other(_) => Self::Unknown,
        }
    }

    /// The `/metrics` label.
    pub(in crate::dashboard) fn label(self) -> &'static str {
        match self {
            Self::Redeemed => "redeemed",
            Self::AlreadyUsed => "already_used",
            Self::Expired => "expired",
            Self::Invalid => "invalid",
            Self::NotForAccount => "not_for_account",
            Self::RankTooLow => "rank_too_low",
            Self::CookieInvalid => "cookie_invalid",
            Self::CaptchaRequired => "captcha_required",
            Self::NoGameAccount => NO_GAME_ACCOUNT,
            Self::Cooldown => "cooldown",
            Self::Unknown => "unknown",
        }
    }

    /// The ledger state it leaves.
    pub(super) fn state(self) -> &'static str {
        match self {
            Self::NotForAccount => "invalid",
            Self::Cooldown => PENDING,
            other => other.label(),
        }
    }

    /// The reason recorded for every user (`expired`, `invalid`), if the code is dead.
    pub(super) fn global(self) -> Option<&'static str> {
        match self {
            Self::Expired => Some("expired"),
            Self::Invalid => Some("invalid"),
            _ => None,
        }
    }

    /// The owner's action this asks for; no further POST is made for them until then.
    pub(in crate::dashboard) fn action(self) -> Option<&'static str> {
        match self {
            Self::CookieInvalid => Some("cookie_invalid"),
            Self::CaptchaRequired => Some("captcha_required"),
            _ => None,
        }
    }
}

/// The row a POST answered: which UID, game and code, under which link.
pub(super) struct Answered<'a> {
    pub(super) link: Uuid,
    pub(super) game_account: Uuid,
    pub(super) game_kind: &'a str,
    pub(super) code: &'a str,
}

/// Records an answer to an admitted row (`sending`) with HoYoLAB's retcode (None only when
/// nothing was answered: a transport or parse error). Expired/invalid codes are marked for everyone, and other UIDs' rows still
/// waiting for that code take the same state without a request; an account-specific answer
/// never touches other rows. An action answer pauses the link's redemption.
pub(super) async fn record(
    db: &DatabaseConnection,
    answered: &Answered<'_>,
    outcome: Outcome,
    retcode: Option<i64>,
    now: DateTime<Utc>,
    cooldown_wait: Duration,
) -> Result<()> {
    let tx = db.begin().await?;
    let next = match outcome {
        Outcome::Cooldown => Some(now + cooldown_wait),
        Outcome::RankTooLow => Some(now + RANK_RETRY),
        _ => None,
    };
    redemption_ledger::Entity::update_many()
        .col_expr(
            redemption_ledger::Column::State,
            Expr::value(outcome.state()),
        )
        .col_expr(
            redemption_ledger::Column::Retcode,
            Expr::value(retcode.and_then(|code| i32::try_from(code).ok())),
        )
        .col_expr(redemption_ledger::Column::NextAttemptAt, Expr::value(next))
        .col_expr(redemption_ledger::Column::UpdatedAt, Expr::value(now))
        .filter(redemption_ledger::Column::GameAccountId.eq(answered.game_account))
        .filter(redemption_ledger::Column::Code.eq(answered.code))
        // A row recovered as `unknown` meanwhile keeps that state (fail closed).
        .filter(redemption_ledger::Column::State.eq(SENDING))
        .exec(&tx)
        .await?;
    if let Some(reason) = outcome.global() {
        reject_in(&tx, answered.game_kind, answered.code, reason, now).await?;
    }
    if let Some(action) = outcome.action() {
        hoyolab_account::Entity::update_many()
            .col_expr(
                hoyolab_account::Column::RedemptionAction,
                Expr::value(Some(action)),
            )
            .col_expr(
                hoyolab_account::Column::RedemptionActionAt,
                Expr::value(Some(now)),
            )
            .filter(hoyolab_account::Column::Id.eq(answered.link))
            .filter(hoyolab_account::Column::RedemptionAction.is_null())
            .exec(&tx)
            .await?;
    }
    tx.commit().await?;
    Ok(())
}

/// Marks `code` of `game_kind` dead for every user and settles the rows still waiting for
/// it. A later listing by the source never reopens it: codes are not reused.
pub(super) async fn reject_in(
    db: &impl ConnectionTrait,
    game_kind: &str,
    code: &str,
    reason: &str,
    now: DateTime<Utc>,
) -> Result<()> {
    redemption_code_rejection::Entity::insert(redemption_code_rejection::ActiveModel {
        game_kind: sea_orm::Set(game_kind.to_owned()),
        code: sea_orm::Set(code.to_owned()),
        reason: sea_orm::Set(reason.to_owned()),
        rejected_at: sea_orm::Set(now),
    })
    .on_conflict(
        OnConflict::columns([
            redemption_code_rejection::Column::GameKind,
            redemption_code_rejection::Column::Code,
        ])
        .do_nothing()
        .to_owned(),
    )
    .exec_without_returning(db)
    .await?;
    let accounts: Vec<Uuid> = game_account::Entity::find()
        .select_only()
        .column(game_account::Column::Id)
        .filter(game_account::Column::GameKind.eq(game_kind))
        .into_tuple()
        .all(db)
        .await?;
    if accounts.is_empty() {
        return Ok(());
    }
    redemption_ledger::Entity::update_many()
        .col_expr(redemption_ledger::Column::State, Expr::value(reason))
        .col_expr(
            redemption_ledger::Column::NextAttemptAt,
            Expr::value(Option::<DateTime<Utc>>::None),
        )
        .col_expr(redemption_ledger::Column::UpdatedAt, Expr::value(now))
        .filter(redemption_ledger::Column::Code.eq(code))
        .filter(redemption_ledger::Column::State.is_in([PENDING, "rank_too_low"]))
        .filter(redemption_ledger::Column::GameAccountId.is_in(accounts))
        .exec(db)
        .await?;
    Ok(())
}

/// The global rejection of `code`, if any (`expired` or `invalid`).
pub(super) async fn rejection(
    db: &impl ConnectionTrait,
    game_kind: &str,
    code: &str,
) -> Result<Option<String>> {
    Ok(
        redemption_code_rejection::Entity::find_by_id((game_kind.to_owned(), code.to_owned()))
            .one(db)
            .await?
            .map(|row| row.reason),
    )
}

/// Turns rows left `sending` into `unknown`: all of them at startup (`before` None, no POST
/// can be in flight yet), or those older than `before` during a run.
pub(in crate::dashboard) async fn recover(
    db: &DatabaseConnection,
    before: Option<DateTime<Utc>>,
) -> Result<u64> {
    let mut update = redemption_ledger::Entity::update_many()
        .col_expr(redemption_ledger::Column::State, Expr::value("unknown"))
        .col_expr(
            redemption_ledger::Column::UpdatedAt,
            Expr::value(Utc::now()),
        )
        .filter(redemption_ledger::Column::State.eq(SENDING));
    if let Some(before) = before {
        update = update.filter(redemption_ledger::Column::UpdatedAt.lt(before));
    }
    Ok(update.exec(db).await?.rows_affected)
}

/// The owner's action is done (a Cookie update or an explicit retry): the link's pause
/// ends and the codes it stopped (including UIDs HoYoLAB did not find) are pending again.
/// Other outcomes stay as they are.
pub(in crate::dashboard) async fn reopen_in(
    tx: &impl ConnectionTrait,
    link: Uuid,
    now: DateTime<Utc>,
) -> Result<()> {
    hoyolab_account::Entity::update_many()
        .col_expr(
            hoyolab_account::Column::RedemptionAction,
            Expr::value(Option::<String>::None),
        )
        .col_expr(
            hoyolab_account::Column::RedemptionActionAt,
            Expr::value(Option::<DateTime<Utc>>::None),
        )
        .filter(hoyolab_account::Column::Id.eq(link))
        .exec(tx)
        .await?;
    let accounts: Vec<Uuid> = game_account::Entity::find()
        .select_only()
        .column(game_account::Column::Id)
        .filter(game_account::Column::HoyolabAccountId.eq(link))
        .into_tuple()
        .all(tx)
        .await?;
    if accounts.is_empty() {
        return Ok(());
    }
    redemption_ledger::Entity::update_many()
        .col_expr(redemption_ledger::Column::State, Expr::value(PENDING))
        .col_expr(
            redemption_ledger::Column::NextAttemptAt,
            Expr::value(Option::<DateTime<Utc>>::None),
        )
        .col_expr(redemption_ledger::Column::UpdatedAt, Expr::value(now))
        .filter(redemption_ledger::Column::GameAccountId.is_in(accounts))
        .filter(redemption_ledger::Column::State.is_in(REOPENED))
        .exec(tx)
        .await?;
    Ok(())
}

#[cfg(test)]
#[path = "ledger_tests.rs"]
mod tests;
