mod actions;
mod credentials;
mod data;
mod display;
mod navigation;
mod preferences;
mod session;
mod state;

#[path = "pages/mod.rs"]
mod pages;

use data::load_dashboard_data;
use dioxus::prelude::*;
use pages::{
    auth::{LoadingPage, LoginPage, OnboardingPage},
    docs::DocsPage,
    shell::Shell,
};
use session::{bootstrap, refresh_me};
use state::{AppState, Page, Phase, current_ready_page, initial_page};

// External CSS uses these fingerprinted filenames; retain them without a second DX suffix.
#[used]
static FONT_JP: Asset = asset!(
    "/assets/fonts/noto-sans-jp-0ad1783080c4.woff2",
    AssetOptions::builder().with_hash_suffix(false)
);
#[used]
static FONT_DISPLAY: Asset = asset!(
    "/assets/fonts/orbitron-4354932df8f5.woff2",
    AssetOptions::builder().with_hash_suffix(false)
);

// The web manifest names this icon by a fixed path.
#[used]
static APP_ICON: Asset = asset!(
    "/assets/dashboard-icon.png",
    AssetOptions::builder().with_hash_suffix(false)
);

#[component]
pub fn App() -> Element {
    let preferences = use_context_provider(|| Signal::new(preferences::Preferences::load()));
    use_effect(move || preferences.read().apply());
    #[cfg(target_arch = "wasm32")]
    let _system_theme = use_hook(move || browser::on_system_theme_change(preferences));
    let state = use_signal(|| AppState::new(initial_page()));
    navigation::use_browser_history(state);
    use_effect(move || {
        let read = state.read();
        let title = page_title(read.page, read.phase, read.ready(), *preferences.read());
        drop(read);
        browser::set_title(&title);
    });

    use_future(move || async move {
        bootstrap(state).await;
    });

    // Refresh the dashboard every minute while it is on screen, and at once when
    // the tab becomes visible again; a hidden tab makes no requests.
    let mut last_refresh = use_signal(|| 0.0_f64);
    let refresh = move || async move {
        let snapshot = state.read().clone();
        if snapshot.busy
            || snapshot.phase != Phase::SignedIn
            || snapshot.page != Page::Dashboard
            || browser::hidden()
        {
            return;
        }
        last_refresh.set(browser::now_ms());
        if refresh_me(state).await && current_ready_page(state, Page::Dashboard) {
            load_dashboard_data(state, false).await;
        }
    };
    use_future(move || async move {
        loop {
            gloo_timers::future::TimeoutFuture::new(60_000).await;
            refresh().await;
        }
    });
    #[cfg(target_arch = "wasm32")]
    let _visible = use_hook(move || {
        browser::on_visible(move || {
            if browser::now_ms() - *last_refresh.peek() >= 60_000.0 {
                spawn(async move { refresh().await });
            }
        })
    });

    let owner_key = state
        .read()
        .me
        .as_ref()
        .map(|me| me.id.to_string())
        .unwrap_or_default();
    rsx! {
        document::Link { rel: "icon", r#type: "image/png", sizes: "32x32", href: asset!("/assets/favicon.png") }
        document::Link { rel: "apple-touch-icon", href: APP_ICON }
        document::Link { rel: "manifest", href: "/manifest.webmanifest" }
        document::Meta { id: "theme-color", name: "theme-color", content: "#0C0B0E" }
        document::Link { rel: "license", href: asset!("/assets/fonts/noto-sans-jp-OFL.txt") }
        document::Link { rel: "license", href: asset!("/assets/fonts/orbitron-OFL.txt") }
        document::Stylesheet { href: asset!("/assets/dx-components-theme.css") }
        document::Stylesheet { href: asset!("/assets/dashboard.css") }
        document::Link { rel: "license", href: asset!("/src/components/LICENSE-MIT") }
        fieldset { class: "app-shell", disabled: state.read().busy, "aria-busy": state.read().busy, "aria-label": preferences.read().text("app.dashboard_actions"),
            if state.read().page == Page::Docs {
                DocsPage { state }
            } else {
                match state.read().phase {
                    Phase::Unchecked | Phase::Loading => rsx! { LoadingPage {} },
                    Phase::SignedOut => rsx! { LoginPage { state } },
                    Phase::SignedIn => rsx! {
                        if !state.read().ready() {
                            OnboardingPage { key: "{owner_key}", state }
                        } else {
                            Shell { key: "{owner_key}", state }
                        }
                    },
                }
            }
        }
    }
}

/// Browser tab title: the current page, then the product name.
fn page_title(page: Page, phase: Phase, ready: bool, ui: preferences::Preferences) -> String {
    let app = ui.text("app.title");
    let key = match (phase, page) {
        (_, Page::Docs) => Some("docs.title"),
        (Phase::SignedOut, _) => Some("auth.sign_in"),
        (Phase::SignedIn, _) if !ready => Some("auth.setup"),
        (Phase::SignedIn, Page::Dashboard) => Some("dashboard.dashboard"),
        (Phase::SignedIn, Page::Uids) => Some("settings.uids"),
        (Phase::SignedIn, Page::Rewards) => Some("settings.bonus"),
        (Phase::SignedIn, Page::Link) => Some("settings.link"),
        (Phase::SignedIn, Page::Security) => Some("settings.security"),
        (Phase::SignedIn, Page::Delete) => Some("settings.delete"),
        (Phase::SignedIn, Page::Admin) => Some("admin.admin_settings"),
        _ => None,
    };
    match key {
        Some(key) => format!("{} – {app}", ui.text(key)),
        None => app.to_owned(),
    }
}

/// Small browser hooks; native tests see inert stand-ins.
mod browser {
    pub(super) fn set_title(title: &str) {
        #[cfg(target_arch = "wasm32")]
        if let Some(document) = web_sys::window().and_then(|w| w.document()) {
            document.set_title(title);
        }
        let _ = title;
    }

    pub(super) fn hidden() -> bool {
        #[cfg(target_arch = "wasm32")]
        {
            web_sys::window()
                .and_then(|w| w.document())
                .is_some_and(|d| d.hidden())
        }
        #[cfg(not(target_arch = "wasm32"))]
        false
    }

    pub(super) fn now_ms() -> f64 {
        #[cfg(target_arch = "wasm32")]
        {
            js_sys::Date::now()
        }
        #[cfg(not(target_arch = "wasm32"))]
        0.0
    }

    /// Keeps a browser listener for the lifetime of the owning hook.
    #[cfg(target_arch = "wasm32")]
    pub(super) struct Listener {
        target: web_sys::EventTarget,
        event: &'static str,
        callback: wasm_bindgen::closure::Closure<dyn FnMut(web_sys::Event)>,
    }

    #[cfg(target_arch = "wasm32")]
    impl Drop for Listener {
        fn drop(&mut self) {
            use wasm_bindgen::JsCast;
            let _ = self.target.remove_event_listener_with_callback(
                self.event,
                self.callback.as_ref().unchecked_ref(),
            );
        }
    }

    #[cfg(target_arch = "wasm32")]
    fn listen(
        target: web_sys::EventTarget,
        event: &'static str,
        handler: impl FnMut(web_sys::Event) + 'static,
    ) -> Option<std::rc::Rc<Listener>> {
        use wasm_bindgen::JsCast;
        let callback = wasm_bindgen::closure::Closure::<dyn FnMut(web_sys::Event)>::new(handler);
        target
            .add_event_listener_with_callback(event, callback.as_ref().unchecked_ref())
            .ok()?;
        Some(std::rc::Rc::new(Listener {
            target,
            event,
            callback,
        }))
    }

    #[cfg(target_arch = "wasm32")]
    pub(super) fn on_visible(mut handler: impl FnMut() + 'static) -> Option<std::rc::Rc<Listener>> {
        let document = web_sys::window()?.document()?;
        let watched = document.clone();
        listen(document.into(), "visibilitychange", move |_| {
            if !watched.hidden() {
                handler();
            }
        })
    }

    /// Re-applies a "system" theme when the browser's color scheme changes.
    #[cfg(target_arch = "wasm32")]
    pub(super) fn on_system_theme_change(
        preferences: dioxus::prelude::Signal<super::preferences::Preferences>,
    ) -> Option<std::rc::Rc<Listener>> {
        use dioxus::prelude::*;
        let media = web_sys::window()?
            .match_media("(prefers-color-scheme: light)")
            .ok()??;
        listen(media.into(), "change", move |_| preferences.peek().apply())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tab_titles_name_the_page_then_the_product() {
        let ui = preferences::Preferences::default();
        assert_eq!(
            page_title(Page::Dashboard, Phase::SignedIn, true, ui),
            "ダッシュボード – AHD"
        );
        assert_eq!(
            page_title(Page::Admin, Phase::SignedOut, false, ui),
            "ログイン – AHD"
        );
        assert_eq!(
            page_title(Page::Rewards, Phase::SignedIn, false, ui),
            "初期設定 – AHD"
        );
        assert_eq!(
            page_title(Page::Docs, Phase::Unchecked, false, ui),
            "APIドキュメント – AHD"
        );
        assert_eq!(
            page_title(Page::Dashboard, Phase::Loading, false, ui),
            "AHD"
        );
    }
}
