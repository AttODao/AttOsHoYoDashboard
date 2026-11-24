//! The redemption worker: after each catalogue run (and on the owner's request) it sends
//! each owner's due codes, one request at a time per HoYoLAB account.
//!
//! Limits: requests to one account are spaced by `spacing` (HoYoLAB's cooldown is about
//! 5 s; a cooldown answer waits `cooldown_wait` and the code is tried once more later in
//! the run, without counting as a failure), at most `max_posts` requests per account and
//! run (the rest waits for the next run), and at most `concurrency` owners at a time. The
//! administrator's switch and server shutdown are checked in the admission transaction
//! right before every request. An owner is never run twice at once: a request while a run
//! is going on is folded into one more pass after it.
use super::{
    ledger::{self, Answered, Outcome},
    plan::{self, Admission, Item},
};
use crate::dashboard::{
    error::{Error, Result},
    hoyolab::{
        gateway::{self, Gateway, LiveGateway, LiveRedeemGateway, RedeemGateway},
        store::CookieStore,
    },
    notifications::{channel::Notifications, code_notices::RedeemedCode},
};
use attoshoyodashboard::hoyolab::RedemptionRequest;
use chrono::Utc;
use std::{
    collections::{BTreeMap, BTreeSet, HashMap, VecDeque},
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::{
    sync::Semaphore,
    task::JoinSet,
    time::{Instant, sleep_until},
};
use uuid::Uuid;

/// Request pacing and limits; see the module documentation.
#[derive(Clone, Copy, Debug)]
pub(in crate::dashboard) struct RedeemTuning {
    pub(in crate::dashboard) spacing: Duration,
    pub(in crate::dashboard) cooldown_wait: Duration,
    pub(in crate::dashboard) max_posts: usize,
    pub(in crate::dashboard) concurrency: usize,
    /// A row `sending` for longer than this is recorded as `unknown` by the next run.
    pub(in crate::dashboard) stale_after: Duration,
}

impl Default for RedeemTuning {
    fn default() -> Self {
        Self {
            spacing: Duration::from_secs(7),
            cooldown_wait: Duration::from_secs(30),
            max_posts: 10,
            concurrency: 4,
            stale_after: Duration::from_secs(10 * 60),
        }
    }
}

/// The retcodes genshin.py reads as an invalid Cookie; `/metrics` counts them one by one
/// so a production answer can be told apart without reading the database.
pub(in crate::dashboard) const COOKIE_RETCODES: [i64; 3] = [-1071, -100, 10001];

/// Answers since start, by game and outcome, and Cookie-invalid answers by retcode.
#[derive(Default)]
pub(in crate::dashboard) struct RedeemStats {
    answers: Mutex<BTreeMap<(&'static str, &'static str), u64>>,
    cookie_retcodes: Mutex<BTreeMap<(&'static str, i64), u64>>,
}

impl RedeemStats {
    fn count(&self, game: &str, outcome: Outcome, retcode: Option<i64>) {
        let Some(game) = super::catalog::GAMES.iter().find(|known| **known == game) else {
            return;
        };
        *self
            .answers
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .entry((game, outcome.label()))
            .or_default() += 1;
        if let Some(retcode) = retcode.filter(|code| COOKIE_RETCODES.contains(code)) {
            *self
                .cookie_retcodes
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .entry((game, retcode))
                .or_default() += 1;
        }
    }

    /// `(game, retcode, count)` for every game and Cookie-invalid retcode.
    pub(in crate::dashboard) fn cookie_retcodes(&self) -> Vec<(&'static str, i64, u64)> {
        let counts = self
            .cookie_retcodes
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        super::catalog::GAMES
            .iter()
            .flat_map(|game| {
                COOKIE_RETCODES.iter().map(|retcode| {
                    (
                        *game,
                        *retcode,
                        counts.get(&(*game, *retcode)).copied().unwrap_or(0),
                    )
                })
            })
            .collect()
    }

    /// `(game, outcome, count)` for every game and outcome.
    pub(in crate::dashboard) fn answers(&self) -> Vec<(&'static str, &'static str, u64)> {
        let counts = self
            .answers
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        super::catalog::GAMES
            .iter()
            .flat_map(|game| {
                Outcome::ALL.iter().map(|outcome| {
                    (
                        *game,
                        outcome.label(),
                        counts.get(&(*game, outcome.label())).copied().unwrap_or(0),
                    )
                })
            })
            .collect()
    }
}

/// What one owner's run did.
#[derive(Clone, Debug, Default, PartialEq)]
pub(in crate::dashboard) struct OwnerRun {
    pub(in crate::dashboard) posts: usize,
    pub(in crate::dashboard) redeemed: usize,
    /// The action the run stopped for, if any.
    pub(in crate::dashboard) action: Option<&'static str>,
}

pub(in crate::dashboard) struct Redeemer<
    R: RedeemGateway = LiveRedeemGateway,
    G: Gateway = LiveGateway,
> {
    store: Arc<CookieStore<G>>,
    gateway: R,
    notifications: Arc<Notifications>,
    tuning: RedeemTuning,
    slots: Semaphore,
    /// Owners being run, and whether another pass was requested meanwhile.
    running: Mutex<HashMap<Uuid, bool>>,
    /// Per HoYoLAB account (link): the earliest time of its next request.
    next_post: Mutex<HashMap<Uuid, Instant>>,
    pub(in crate::dashboard) stats: RedeemStats,
}

impl Redeemer {
    pub(in crate::dashboard) fn new(
        store: Arc<CookieStore>,
        notifications: Arc<Notifications>,
    ) -> Self {
        Self::with(
            store,
            LiveRedeemGateway,
            notifications,
            RedeemTuning::default(),
        )
    }
}

impl<R: RedeemGateway, G: Gateway> Redeemer<R, G> {
    pub(in crate::dashboard) fn with(
        store: Arc<CookieStore<G>>,
        gateway: R,
        notifications: Arc<Notifications>,
        tuning: RedeemTuning,
    ) -> Self {
        Self {
            store,
            gateway,
            notifications,
            slots: Semaphore::new(tuning.concurrency.max(1)),
            tuning,
            running: Mutex::new(HashMap::new()),
            next_post: Mutex::new(HashMap::new()),
            stats: RedeemStats::default(),
        }
    }

    /// At startup no request can be in flight: every `sending` row becomes `unknown`.
    pub(in crate::dashboard) async fn recover_ledger(&self) -> Result<u64> {
        let recovered = ledger::recover(&self.store.db, None).await?;
        if recovered > 0 {
            tracing::warn!(
                rows = recovered,
                "redemption requests of unknown outcome recorded"
            );
        }
        Ok(recovered)
    }

    /// Whether a run for `owner` is going on.
    pub(in crate::dashboard) fn is_running(&self, owner: Uuid) -> bool {
        self.running
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .contains_key(&owner)
    }

    /// Runs every owner with work, at most `concurrency` at a time, and returns the runs
    /// that sent something or stopped for the owner. Does nothing while the
    /// administrator's switch is off.
    pub(in crate::dashboard) async fn run_every_owner(self: &Arc<Self>) -> Result<Vec<OwnerRun>> {
        if !plan::switched_on(&self.store.db).await? {
            return Ok(Vec::new());
        }
        let stale = Utc::now()
            - chrono::Duration::from_std(self.tuning.stale_after).map_err(|_| Error::Internal)?;
        ledger::recover(&self.store.db, Some(stale)).await?;
        let mut tasks = JoinSet::new();
        for owner in plan::owners(&self.store.db).await? {
            let this = Arc::clone(self);
            tasks.spawn(async move { this.run_owner(owner).await });
        }
        let mut runs = Vec::new();
        while let Some(joined) = tasks.join_next().await {
            match joined {
                Ok(Ok(run)) if run != OwnerRun::default() => runs.push(run),
                Ok(Ok(_)) => {}
                Ok(Err(error)) => tracing::error!(error = %error, "code redemption run failed"),
                Err(_) => tracing::error!("code redemption task failed"),
            }
        }
        Ok(runs)
    }

    /// Starts a run for `owner` in the background (the owner's request).
    pub(in crate::dashboard) fn start_owner(self: &Arc<Self>, owner: Uuid) {
        let this = Arc::clone(self);
        tokio::spawn(async move {
            if let Err(error) = this.run_owner(owner).await {
                tracing::error!(error = %error, "requested code redemption failed");
            }
        });
    }

    /// One owner's run, plus one more pass for each request that arrived meanwhile.
    pub(in crate::dashboard) async fn run_owner(&self, owner: Uuid) -> Result<OwnerRun> {
        {
            let mut running = self
                .running
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if let Some(again) = running.get_mut(&owner) {
                *again = true;
                return Ok(OwnerRun::default());
            }
            running.insert(owner, false);
        }
        let mut total = OwnerRun::default();
        loop {
            let result = self.pass(owner).await;
            let again = {
                let mut running = self
                    .running
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                if result.is_ok() && running.get(&owner) == Some(&true) {
                    running.insert(owner, false);
                    true
                } else {
                    running.remove(&owner);
                    false
                }
            };
            let run = result?;
            total.posts += run.posts;
            total.redeemed += run.redeemed;
            total.action = total.action.or(run.action);
            if !again {
                return Ok(total);
            }
        }
    }

    /// Waits until the account may receive its next request.
    async fn wait_turn(&self, link: Uuid) {
        let at = self
            .next_post
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(&link)
            .copied();
        if let Some(at) = at {
            sleep_until(at).await;
        }
    }

    fn hold(&self, link: Uuid, wait: Duration) {
        self.next_post
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(link, Instant::now() + wait);
    }

    async fn pass(&self, owner: Uuid) -> Result<OwnerRun> {
        let _slot = self.slots.acquire().await.map_err(|_| Error::Internal)?;
        let Some(plan) = plan::plan(&self.store.db, owner, Utc::now()).await? else {
            return Ok(OwnerRun::default());
        };
        let mut queue: VecDeque<(Item, bool)> = plan
            .items
            .iter()
            .cloned()
            .map(|item| (item, false))
            .collect();
        let mut run = OwnerRun::default();
        let mut redeemed = Vec::new();
        // UIDs HoYoLAB did not find on the Cookie's account in this pass.
        let mut not_found = BTreeSet::new();
        while let Some((item, retried)) = queue.pop_front() {
            if run.posts >= self.tuning.max_posts {
                break;
            }
            if not_found.contains(&item.game_account) {
                continue;
            }
            let game = gateway::game(&item.game_kind)?;
            self.wait_turn(plan.link).await;
            // Serialized with reward attempts and Cookie changes of the same owner, but
            // never held across the spacing waits.
            let operation = self.store.operations.acquire(owner).await;
            let cookie = match plan::admit(&self.store, &plan, &item, Utc::now()).await? {
                Admission::Stop => break,
                Admission::Skip => continue,
                Admission::Send(cookie) => cookie,
            };
            let request = RedemptionRequest {
                game,
                uid: &item.game_uid,
                region: &item.region,
                code: &item.code,
            };
            let answer = self.gateway.redeem(&cookie, request).await;
            self.hold(plan.link, self.tuning.spacing);
            run.posts += 1;
            let outcome = Outcome::of(&answer);
            let retcode = answer.as_ref().ok().map(|answer| answer.retcode);
            // A failure here leaves the row `sending`: it becomes `unknown`, never resent.
            ledger::record(
                &self.store.db,
                &Answered {
                    link: plan.link,
                    game_account: item.game_account,
                    game_kind: &item.game_kind,
                    code: &item.code,
                },
                outcome,
                retcode,
                Utc::now(),
                chrono::Duration::from_std(self.tuning.cooldown_wait)
                    .map_err(|_| Error::Internal)?,
            )
            .await?;
            drop(operation);
            self.stats.count(&item.game_kind, outcome, retcode);
            // Never the code, the user, the UID or the message: the class and the number.
            tracing::info!(
                game = %item.game_kind,
                source = item.source,
                outcome = outcome.label(),
                retcode = retcode.map_or_else(|| "none".to_owned(), |code| code.to_string()),
                "redemption code answered"
            );
            match outcome {
                Outcome::Redeemed => {
                    run.redeemed += 1;
                    redeemed.push(RedeemedCode {
                        game_kind: item.game_kind.clone(),
                        game_uid: item.game_uid.clone(),
                        code: item.code.clone(),
                        rewards: item.rewards.clone(),
                    });
                }
                // Only this UID stops; the owner's other UIDs go on.
                Outcome::NoGameAccount => {
                    not_found.insert(item.game_account);
                }
                Outcome::Cooldown => {
                    self.hold(plan.link, self.tuning.cooldown_wait);
                    if !retried {
                        queue.push_back((item, true));
                    }
                }
                outcome if outcome.action().is_some() => {
                    run.action = outcome.action();
                    break;
                }
                _ => {}
            }
        }
        self.notify(owner, &redeemed, run.action).await;
        Ok(run)
    }

    /// One summary of this pass's redeemed codes, and the action notice if it stopped.
    /// Delivery problems are logged, never retried: the ledger is the record.
    async fn notify(&self, owner: Uuid, redeemed: &[RedeemedCode], action: Option<&'static str>) {
        let run = Uuid::new_v4();
        if !redeemed.is_empty()
            && let Err(error) = self
                .notifications
                .announce_redeemed(owner, &format!("code_redeemed:{run}"), redeemed)
                .await
        {
            tracing::warn!(error = %error, "redeemed code summary failed");
        }
        if let Some(action) = action
            && let Err(error) = self
                .notifications
                .redemption_action(owner, &format!("code_action:{run}"), action)
                .await
        {
            tracing::warn!(error = %error, "code redemption action notice failed");
        }
    }
}

type Boxed<T> = Pin<Box<dyn Future<Output = Result<T>> + Send>>;

/// The worker as the server holds it, whatever gateway it was built with (tests give the
/// HTTP layer a local mock, so no test can reach HoYoLAB).
pub(in crate::dashboard) trait RedemptionRuns: Send + Sync {
    /// Starts a run for `owner` in the background.
    fn trigger(self: Arc<Self>, owner: Uuid);
    fn is_running(&self, owner: Uuid) -> bool;
    fn stats(&self) -> &RedeemStats;
    /// Runs every owner with work (after a catalogue run).
    fn run_all(self: Arc<Self>) -> Boxed<Vec<OwnerRun>>;
    /// Records rows left `sending` by an earlier process as `unknown` (at startup).
    fn recover(self: Arc<Self>) -> Boxed<u64>;
}

impl<R: RedeemGateway, G: Gateway> RedemptionRuns for Redeemer<R, G> {
    fn trigger(self: Arc<Self>, owner: Uuid) {
        self.start_owner(owner);
    }
    fn is_running(&self, owner: Uuid) -> bool {
        Redeemer::is_running(self, owner)
    }
    fn stats(&self) -> &RedeemStats {
        &self.stats
    }
    fn run_all(self: Arc<Self>) -> Boxed<Vec<OwnerRun>> {
        Box::pin(async move { self.run_every_owner().await })
    }
    fn recover(self: Arc<Self>) -> Boxed<u64> {
        Box::pin(async move { self.recover_ledger().await })
    }
}

#[cfg(test)]
#[path = "redeem_tests.rs"]
mod tests;
