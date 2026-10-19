//! Same-origin path history; authentication/onboarding gates remain in App.
use super::state::{AppState, Page, Phase};
use dioxus::prelude::*;

fn page_path(page: Page) -> &'static str {
    match page {
        Page::Dashboard => "/dashboard",
        Page::Uids => "/settings/games",
        Page::Rewards => "/settings/bonus",
        Page::Link => "/settings/link",
        Page::Security => "/settings/security",
        Page::Delete => "/settings/delete",
        Page::Admin => "/admin",
        Page::Docs => "/docs",
    }
}

fn page_from_path(path: &str) -> Page {
    match path {
        "/settings" | "/settings/games" => Page::Uids,
        "/settings/bonus" => Page::Rewards,
        "/settings/link" => Page::Link,
        "/settings/security" => Page::Security,
        "/settings/delete" => Page::Delete,
        "/admin" => Page::Admin,
        "/docs" => Page::Docs,
        _ => Page::Dashboard,
    }
}

pub(super) fn initial_page() -> Page {
    #[cfg(target_arch = "wasm32")]
    if let Some(window) = web_sys::window() {
        return page_from_path(&window.location().pathname().unwrap_or_default());
    }
    page_from_path("/")
}

pub(super) fn push_page(page: Page) {
    write_path(page_path(page), false);
}

fn write_path(path: &'static str, replace: bool) {
    #[cfg(target_arch = "wasm32")]
    if let Some(window) = web_sys::window() {
        let location = window.location();
        if (location.pathname().ok().as_deref() != Some(path)
            || !location.hash().unwrap_or_default().is_empty())
            && let Ok(history) = window.history()
        {
            // Only fixed absolute paths from this module reach History: no
            // caller-supplied URL can redirect to another origin.
            if replace {
                let _ =
                    history.replace_state_with_url(&wasm_bindgen::JsValue::NULL, "", Some(path));
            } else {
                let _ = history.push_state_with_url(&wasm_bindgen::JsValue::NULL, "", Some(path));
            }
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    let _ = (path, replace);
}

// Login is an authentication gate, not a shell page. Loading must retain the
// requested deep link until /me determines whether the session is valid.
fn canonical_path(page: Page, phase: Phase) -> Option<&'static str> {
    match (page, phase) {
        (Page::Docs, _) => Some("/docs"),
        (_, Phase::Unchecked | Phase::Loading) => None,
        (_, Phase::SignedOut) => Some("/login"),
        (_, Phase::SignedIn) => Some(page_path(page)),
    }
}

fn replace_canonical(page: Page, phase: Phase) {
    if let Some(path) = canonical_path(page, phase) {
        write_path(path, true);
    }
}

pub(super) fn use_browser_history(state: Signal<AppState>) {
    // React only to route/auth transitions, not unrelated data/busy updates.
    let route = use_memo(move || {
        let read = state.read();
        (read.page, read.phase)
    });
    use_effect(move || {
        let (page, phase) = route();
        replace_canonical(page, phase);
    });
    #[cfg(target_arch = "wasm32")]
    let _listener = use_hook(move || {
        use wasm_bindgen::JsCast;
        let window = web_sys::window()?;
        let callback =
            wasm_bindgen::closure::Closure::<dyn FnMut(web_sys::Event)>::new(move |_| {
                // Back/forward must not add a new history entry, even while a request is busy.
                super::state::restore_page(state, initial_page());
                // Normalize even when the resolved page did not change (e.g.
                // signed-in /login or signed-out /settings/security).
                let read = state.read();
                replace_canonical(read.page, read.phase);
            });
        window
            .add_event_listener_with_callback("popstate", callback.as_ref().unchecked_ref())
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
            "popstate",
            self.callback.as_ref().unchecked_ref(),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_routes_follow_auth_without_discarding_loading_deep_links() {
        for page in [Page::Dashboard, Page::Security, Page::Admin] {
            assert_eq!(canonical_path(page, Phase::Unchecked), None);
            assert_eq!(canonical_path(page, Phase::Loading), None);
            assert_eq!(canonical_path(page, Phase::SignedOut), Some("/login"));
            assert_eq!(canonical_path(page, Phase::SignedIn), Some(page_path(page)));
        }
        for phase in [
            Phase::Unchecked,
            Phase::Loading,
            Phase::SignedOut,
            Phase::SignedIn,
        ] {
            assert_eq!(canonical_path(Page::Docs, phase), Some("/docs"));
        }
        for path in ["/", "/login", "/dashboard"] {
            assert!(page_from_path(path) == Page::Dashboard);
        }
        assert!(page_from_path("/settings") == Page::Uids);
    }

    #[test]
    fn every_page_roundtrips_to_a_fixed_same_origin_path() {
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
            let path = page_path(page);
            assert!(page_from_path(path) == page);
            assert!(path.starts_with('/') && !path.starts_with("//"));
            assert!(!path.contains('#'));
        }
        assert!(page_from_path("/unknown") == Page::Dashboard);
    }
}
