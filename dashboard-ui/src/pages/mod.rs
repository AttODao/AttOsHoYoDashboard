mod about;
mod accounts;
mod admin;
pub(super) mod app_login;
pub(super) mod app_nav;
#[cfg(feature = "mobile")]
pub(super) mod app_update;
pub(super) mod auth;
mod common;
mod dashboard;
mod deletion;
#[cfg(feature = "mobile")]
mod device_notifications;
pub(super) mod docs;
mod hoyolab;
mod notifications;
mod rewards;
mod security;
pub(super) mod shell;
#[allow(
    dead_code,
    reason = "the dashboard and settings tabs adopt SwipeableTabs next"
)]
mod swipe_tabs;
