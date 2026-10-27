//! Same-origin path history (in-WebView history in the app); authentication/onboarding
//! gates remain in App.
use super::state::{AppState, Page, Phase};
use dioxus::prelude::*;

#[cfg(any(target_arch = "wasm32", test))]
fn app_login_challenge(path: &str, search: &str) -> Option<String> {
    if path != "/app-login" {
        return None;
    }
    let search = search.strip_prefix('?').unwrap_or(search);
    let mut pairs = search.split('&');
    let value = pairs.next()?.strip_prefix("challenge=")?;
    if pairs.next().is_some()
        || value.len() != 43
        || !value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
    {
        return None;
    }
    Some(value.to_owned())
}

pub(super) fn app_login_request() -> Option<String> {
    #[cfg(target_arch = "wasm32")]
    {
        let location = web_sys::window()?.location();
        app_login_challenge(&location.pathname().ok()?, &location.search().ok()?)
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        None
    }
}

#[cfg(any(target_arch = "wasm32", test))]
pub(super) fn app_callback_url(code: &str) -> String {
    format!("cc.attodao.ahd://login?code={code}")
}

pub(super) fn open_app_callback(code: &str) {
    #[cfg(target_arch = "wasm32")]
    if let Some(window) = web_sys::window() {
        let _ = window.location().set_href(&app_callback_url(code));
    }
    #[cfg(not(target_arch = "wasm32"))]
    let _ = code;
}

fn page_path(page: Page) -> &'static str {
    match page {
        Page::Dashboard => "/dashboard",
        Page::Uids => "/settings/games",
        Page::Rewards => "/settings/bonus",
        Page::Link => "/settings/link",
        Page::Security => "/settings/security",
        Page::Notifications => "/settings/notifications",
        Page::Delete => "/settings/delete",
        Page::Admin => "/admin",
        Page::Docs => "/docs",
        Page::About => "/about",
    }
}

fn page_from_path(path: &str) -> Page {
    match path {
        "/settings" | "/settings/games" => Page::Uids,
        "/settings/bonus" => Page::Rewards,
        "/settings/link" => Page::Link,
        "/settings/security" => Page::Security,
        "/settings/notifications" => Page::Notifications,
        "/settings/delete" => Page::Delete,
        "/admin" => Page::Admin,
        "/docs" => Page::Docs,
        "/about" => Page::About,
        _ => Page::Dashboard,
    }
}

/// The page a tapped notification opens: a dashboard path from the server ("/settings/bonus"),
/// or "dashboard" from the app's own predicted notices.
#[cfg_attr(not(feature = "mobile"), allow(dead_code))]
pub(super) fn notification_target(target: &str) -> Page {
    page_from_path(if target == "dashboard" {
        "/dashboard"
    } else {
        target
    })
}

/// The web version's URL for `page` on the server at `origin` (scheme://host[:port]).
fn web_page_url(origin: &str, page: Page) -> String {
    format!("{origin}{}", page_path(page))
}

/// Opens the web version of `page` in the device's default browser (app only); shows an
/// error line when no browser could be opened or the server URL is unusable.
pub(super) fn open_web_version(state: Signal<AppState>, page: Page) {
    #[cfg(feature = "mobile")]
    let origin = crate::mobile::http::server_origin();
    #[cfg(not(feature = "mobile"))]
    let origin: Option<&str> = None;
    let opened = origin
        .ok_or(())
        .and_then(|origin| crate::platform::open_external(&web_page_url(origin, page)));
    if opened.is_err() {
        super::state::set_status(state, "messages.could_not_open_the_browser", "error");
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
    #[cfg(feature = "mobile")]
    crate::mobile::dom::write_path(path, replace);
    #[cfg(not(any(target_arch = "wasm32", feature = "mobile")))]
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
    // The app has no address bar; its in-WebView history serves the Android back button.
    #[cfg(feature = "mobile")]
    crate::mobile::dom::use_popstate(move |path| {
        super::state::restore_page(state, page_from_path(&path));
        let read = state.read();
        replace_canonical(read.page, read.phase);
    });
    #[cfg(not(any(target_arch = "wasm32", feature = "mobile")))]
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
    fn notification_targets_open_their_screens() {
        assert!(notification_target("dashboard") == Page::Dashboard);
        assert!(notification_target("/dashboard") == Page::Dashboard);
        assert!(notification_target("/settings/bonus") == Page::Rewards);
        assert!(notification_target("/settings/link") == Page::Link);
        assert!(notification_target("/settings/security") == Page::Security);
        assert!(notification_target("/settings/notifications") == Page::Notifications);
        assert!(notification_target("unknown") == Page::Dashboard);
    }

    #[test]
    fn app_handoff_requires_one_exact_url_safe_challenge() {
        let challenge = "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM";
        for prefix in ["", "?"] {
            assert_eq!(
                app_login_challenge("/app-login", &format!("{prefix}challenge={challenge}")),
                Some(challenge.into())
            );
        }
        for path in ["/", "/login", "/app-login/", "/other"] {
            assert_eq!(
                app_login_challenge(path, &format!("challenge={challenge}")),
                None
            );
        }
        for search in [
            "".into(),
            "challenge=short".into(),
            format!("challenge={challenge}x"),
            format!("challenge={challenge}&challenge={challenge}"),
            format!("challenge={challenge}&other=x"),
            format!("other=x&challenge={challenge}"),
            format!("challenge={challenge}&"),
            format!("challenge={}!", &challenge[..42]),
            format!("challenge={}=", &challenge[..42]),
            format!("challenge={}%", &challenge[..42]),
            format!("other={challenge}"),
        ] {
            assert_eq!(app_login_challenge("/app-login", &search), None, "{search}");
        }
        assert_eq!(
            app_callback_url("0123456789abcdef"),
            "cc.attodao.ahd://login?code=0123456789abcdef"
        );
    }

    #[test]
    fn web_version_urls_join_the_server_origin_and_the_page_path() {
        assert_eq!(
            web_page_url("https://ahd.example.com", Page::Dashboard),
            "https://ahd.example.com/dashboard"
        );
        assert_eq!(
            web_page_url("http://localhost:18080", Page::Security),
            "http://localhost:18080/settings/security"
        );
        assert_eq!(
            web_page_url("https://ahd.example.com", Page::Notifications),
            "https://ahd.example.com/settings/notifications"
        );
    }

    #[test]
    fn canonical_routes_follow_auth_without_discarding_loading_deep_links() {
        for page in [Page::Dashboard, Page::Security, Page::Admin, Page::About] {
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
            Page::About,
        ] {
            let path = page_path(page);
            assert!(page_from_path(path) == page);
            assert!(path.starts_with('/') && !path.starts_with("//"));
            assert!(!path.contains('#'));
        }
        assert!(page_from_path("/unknown") == Page::Dashboard);
    }
}
