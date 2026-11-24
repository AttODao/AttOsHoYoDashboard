//! Redemption codes: the shared catalogue fetched from a third-party list, per-UID modes,
//! notify-only announcements, automatic and requested redemption on HoYoLAB (through
//! `hoyolab::gateway::RedeemGateway`) with its ledger, redeeming the listed codes once for
//! UIDs outside `auto` (`bulk`), and the administrator controls.
pub(in crate::dashboard) mod admin;
mod announce;
pub(in crate::dashboard) mod bulk;
pub(in crate::dashboard) mod catalog;
pub(in crate::dashboard) mod ledger;
pub(in crate::dashboard) mod manual;
mod plan;
pub(in crate::dashboard) mod redeem;
pub(in crate::dashboard) mod settings;
pub(in crate::dashboard) mod source;
#[cfg(test)]
pub(in crate::dashboard) mod test_support;
pub(in crate::dashboard) mod worker;
