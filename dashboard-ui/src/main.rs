//! Dashboard entry for the browser and the Android app (`mobile` feature). In the browser,
//! credentials remain in the form/request and HttpOnly session; see `mobile::http` for the app.
mod api;
mod app;
mod components;
mod i18n;
// Test runs build the platform-free parts of the app modules as well.
#[cfg(any(feature = "mobile", test))]
#[cfg_attr(not(feature = "mobile"), allow(dead_code))]
mod mobile;
mod models;
mod platform;
mod webauthn;
fn main() {
    dioxus::launch(app::App);
}
