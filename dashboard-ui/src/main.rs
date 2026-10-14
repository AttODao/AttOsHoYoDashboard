//! Dashboard browser entry; credentials remain in the browser form/request and HttpOnly session.
mod api;
mod app;
mod components;
mod models;
mod webauthn;
fn main() {
    dioxus::launch(app::App);
}
