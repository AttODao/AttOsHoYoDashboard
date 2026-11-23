//! Redemption codes: the shared catalogue fetched from a third-party list, per-UID modes,
//! per-UID results, notify-only announcements and the administrator controls. Nothing here
//! talks to HoYoLAB.
pub(in crate::dashboard) mod admin;
mod announce;
pub(in crate::dashboard) mod catalog;
pub(in crate::dashboard) mod settings;
pub(in crate::dashboard) mod source;
#[cfg(test)]
pub(in crate::dashboard) mod test_support;
pub(in crate::dashboard) mod worker;
