//! Android app bridges: native API client with a stored session cookie, app-private
//! storage, and DOM calls the browser build makes through web-sys. `session` and `time`
//! hold no platform calls and are unit-tested in every native test run.
#[cfg(feature = "mobile")]
pub(crate) mod dom;
#[cfg(feature = "mobile")]
pub(crate) mod http;
pub(crate) mod session;
#[cfg(feature = "mobile")]
pub(crate) mod storage;
pub(crate) mod time;
