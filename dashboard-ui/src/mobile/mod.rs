//! Android app bridges: native API client with a stored session cookie, app-private
//! storage, notifications, and DOM calls the browser build makes through web-sys.
//! `session`, `time`, `reminders` and `inbox` are platform-free and tested in native test runs.
#[cfg(feature = "mobile")]
pub(crate) mod dom;
#[cfg(feature = "mobile")]
pub(crate) mod http;
pub(crate) mod inbox;
#[cfg(feature = "mobile")]
pub(crate) mod notify;
pub(crate) mod reminders;
pub(crate) mod session;
#[cfg(feature = "mobile")]
pub(crate) mod storage;
pub(crate) mod time;
