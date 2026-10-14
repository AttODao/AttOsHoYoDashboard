//! HoYoLAB linkage application policy, separate from upstream HTTP protocol.
pub(super) mod credentials;
pub(super) mod gateway;
pub(super) mod operations;
pub(super) mod store;
mod verification;
#[cfg(test)]
mod verification_tests;
