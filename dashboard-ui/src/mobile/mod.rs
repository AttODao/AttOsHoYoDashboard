//! Android app bridges: native API client with a stored session cookie, app-private
//! storage, the device locale, notifications, home-screen widgets, update checks and installs,
//! the Back key, and DOM calls the browser build makes through web-sys. `back` (its dialog
//! stack), `browser_login`, `locale` (tag parsing), `session`, `time`, `reminders`, `widgets`,
//! `inbox`, `push`, `update_install` (pure checks/sink) and `updates` (except its request) are
//! platform-free and tested in native test runs.
pub(crate) mod back;
pub(crate) mod browser_login;
#[cfg(feature = "mobile")]
pub(crate) mod dom;
#[cfg(feature = "mobile")]
pub(crate) mod http;
pub(crate) mod inbox;
pub(crate) mod locale;
#[cfg(feature = "mobile")]
pub(crate) mod log;
#[cfg(feature = "mobile")]
pub(crate) mod login_intent;
#[cfg(feature = "mobile")]
pub(crate) mod notify;
pub(crate) mod push;
pub(crate) mod reminders;
pub(crate) mod session;
#[cfg(feature = "mobile")]
pub(crate) mod storage;
#[cfg(feature = "mobile")]
pub(crate) mod swipe;
pub(crate) mod time;
pub(crate) mod update_install;
pub(crate) mod updates;
pub(crate) mod widgets;
