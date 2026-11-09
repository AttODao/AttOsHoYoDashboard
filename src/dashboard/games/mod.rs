//! User-owned game account selection, status, endgame challenge and event calendar
//! cache policy, and the resin/Trailblaze Power history.
mod calendar;
mod calendar_summary;
mod cap_time;
mod challenge_summary;
mod challenges;
mod resource_history;
pub(in crate::dashboard) mod service;
mod summary;
mod upstream_json;

#[cfg(test)]
#[path = "calendar_summary_tests.rs"]
mod calendar_summary_tests;
#[cfg(test)]
#[path = "calendar_tests.rs"]
mod calendar_tests;
#[cfg(test)]
#[path = "cap_time_tests.rs"]
mod cap_time_tests;
#[cfg(test)]
#[path = "challenge_summary_tests.rs"]
mod challenge_summary_tests;
#[cfg(test)]
#[path = "challenges_tests.rs"]
mod challenges_tests;
#[cfg(test)]
mod live_tests;
#[cfg(test)]
#[path = "resource_history_tests.rs"]
mod resource_history_tests;
#[cfg(test)]
#[path = "service_tests.rs"]
mod service_tests;
#[cfg(test)]
#[path = "summary_tests.rs"]
mod summary_tests;

pub(in crate::dashboard) use calendar::GameCalendar;
pub(in crate::dashboard) use challenges::GameChallenges;
pub(in crate::dashboard) use resource_history::{MAX_GAP, ResourceHistory};
pub(in crate::dashboard) use service::{GameStatus, Games};
