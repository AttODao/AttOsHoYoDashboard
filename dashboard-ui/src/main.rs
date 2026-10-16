//! Dashboard browser entry; credentials remain in the browser form/request and HttpOnly session.
mod api;
mod app;
mod components;
mod i18n;
mod models;
mod webauthn;
fn main() {
    dioxus::launch(app::App);
}
