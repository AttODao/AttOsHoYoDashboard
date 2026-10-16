//! User-owned game account selection and status cache policy.
mod service;
mod summary;

#[cfg(test)]
mod live_tests;
#[cfg(test)]
#[path = "service_tests.rs"]
mod service_tests;
#[cfg(test)]
#[path = "summary_tests.rs"]
mod summary_tests;

pub(in crate::dashboard) use service::{GameStatus, Games};
