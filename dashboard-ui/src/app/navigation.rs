//! Hash-based browser history; authentication/onboarding gates remain in App.
use super::state::{AppState, Page};
use dioxus::prelude::*;

fn page_hash(page: Page) -> &'static str {
    match page {
        Page::Dashboard => "#dashboard",
        Page::Uids => "#settings/games",
        Page::Rewards => "#settings/bonus",
        Page::Link => "#settings/link",
        Page::Security => "#settings/security",
        Page::Delete => "#settings/delete",
        Page::Admin => "#admin",
        Page::Docs => "#docs",
    }
}

fn page_from_location(path: &str, hash: &str) -> Page {
    match hash {
        "#dashboard" => Page::Dashboard,
        "#settings/games" => Page::Uids,
        "#settings/bonus" => Page::Rewards,
        "#settings/link" => Page::Link,
        "#settings/security" => Page::Security,
        "#settings/delete" => Page::Delete,
        "#admin" => Page::Admin,
        "#docs" => Page::Docs,
        _ if path == "/docs" => Page::Docs,
        _ => Page::Dashboard,
    }
}

pub(super) fn initial_page() -> Page {
    #[cfg(target_arch = "wasm32")]
    if let Some(window) = web_sys::window() {
        let location = window.location();
        return page_from_location(
            &location.pathname().unwrap_or_default(),
            &location.hash().unwrap_or_default(),
        );
    }
    page_from_location("/", "")
}

pub(super) fn push_page(page: Page) {
    #[cfg(target_arch = "wasm32")]
    if let Some(window) = web_sys::window() {
        let location = window.location();
        if location.hash().ok().as_deref() != Some(page_hash(page)) {
            let _ = location.set_hash(page_hash(page));
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    let _ = page_hash(page);
}

pub(super) fn use_browser_history(state: Signal<AppState>) {
    #[cfg(target_arch = "wasm32")]
    let _listener = use_hook(move || {
        use wasm_bindgen::JsCast;
        let window = web_sys::window()?;
        let callback =
            wasm_bindgen::closure::Closure::<dyn FnMut(web_sys::Event)>::new(move |_| {
                // Back/forward must not add a new history entry, even while a request is busy.
                super::state::restore_page(state, initial_page());
            });
        window
            .add_event_listener_with_callback("hashchange", callback.as_ref().unchecked_ref())
            .ok()?;
        Some(std::rc::Rc::new(HistoryListener { window, callback }))
    });
    #[cfg(not(target_arch = "wasm32"))]
    let _ = state;
}

#[cfg(target_arch = "wasm32")]
struct HistoryListener {
    window: web_sys::Window,
    callback: wasm_bindgen::closure::Closure<dyn FnMut(web_sys::Event)>,
}

#[cfg(target_arch = "wasm32")]
impl Drop for HistoryListener {
    fn drop(&mut self) {
        use wasm_bindgen::JsCast;
        let _ = self.window.remove_event_listener_with_callback(
            "hashchange",
            self.callback.as_ref().unchecked_ref(),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_page_roundtrips_and_hash_overrides_docs_path() {
        for page in [
            Page::Dashboard,
            Page::Uids,
            Page::Rewards,
            Page::Link,
            Page::Security,
            Page::Delete,
            Page::Admin,
            Page::Docs,
        ] {
            assert!(page_from_location("/", page_hash(page)) == page);
            assert!(page_from_location("/docs", page_hash(page)) == page);
        }
        assert!(page_from_location("/docs", "") == Page::Docs);
        assert!(page_from_location("/", "#unknown") == Page::Dashboard);
    }
}
