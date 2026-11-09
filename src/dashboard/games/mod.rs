//! User-owned game account selection, status and endgame challenge cache policy.
mod challenge_summary;
mod challenges;
pub(in crate::dashboard) mod service;
mod summary;

#[cfg(test)]
#[path = "challenge_summary_tests.rs"]
mod challenge_summary_tests;
#[cfg(test)]
#[path = "challenges_tests.rs"]
mod challenges_tests;
#[cfg(test)]
mod live_tests;
#[cfg(test)]
#[path = "service_tests.rs"]
mod service_tests;
#[cfg(test)]
#[path = "summary_tests.rs"]
mod summary_tests;

pub(in crate::dashboard) use challenges::GameChallenges;
pub(in crate::dashboard) use service::{GameStatus, Games};
