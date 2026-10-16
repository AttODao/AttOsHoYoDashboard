mod actions;
mod credentials;
mod data;
mod display;
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
    "/assets/fonts/noto-sans-jp-293812b054e0.woff2",
    AssetOptions::builder().with_hash_suffix(false)
);
#[used]
static FONT_DISPLAY: Asset = asset!(
    "/assets/fonts/orbitron-4354932df8f5.woff2",
    AssetOptions::builder().with_hash_suffix(false)
);

#[component]
pub fn App() -> Element {
    let preferences = use_context_provider(|| Signal::new(preferences::Preferences::load()));
    use_effect(move || preferences.read().apply());
    let state = use_signal(|| AppState::new(initial_page()));

    use_future(move || async move {
        bootstrap(state).await;
    });

    use_future(move || async move {
        loop {
            gloo_timers::future::TimeoutFuture::new(60_000).await;
            let snapshot = state.read().clone();
            if snapshot.busy
                || snapshot.phase != Phase::SignedIn
                || snapshot.page != Page::Dashboard
            {
                continue;
            }
            if refresh_me(state).await && current_ready_page(state, Page::Dashboard) {
                load_dashboard_data(state, false).await;
            }
        }
    });

    let owner_key = state
        .read()
        .me
        .as_ref()
        .map(|me| me.id.to_string())
        .unwrap_or_default();
    rsx! {
        document::Link { rel: "license", href: asset!("/assets/fonts/noto-sans-jp-OFL.txt") }
        document::Link { rel: "license", href: asset!("/assets/fonts/orbitron-OFL.txt") }
        document::Stylesheet { href: asset!("/assets/dx-components-theme.css") }
        document::Stylesheet { href: asset!("/assets/dashboard.css") }
        document::Link { rel: "license", href: asset!("/src/components/LICENSE-MIT") }
        fieldset { class: "app-shell", disabled: state.read().busy, "aria-label": preferences.read().text("app.dashboard_actions"),
            if state.read().page == Page::Docs {
                DocsPage { state }
            } else {
                match state.read().phase {
                    Phase::Loading => rsx! { LoadingPage {} },
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
