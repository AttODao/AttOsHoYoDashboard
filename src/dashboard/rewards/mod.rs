//! Reward execution independent of dashboard visits.
pub(in crate::dashboard) mod deletion;
mod guard;
pub(in crate::dashboard) mod schedule;
pub(in crate::dashboard) mod service;
