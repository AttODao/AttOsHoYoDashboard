//! The per-game tab bar shared by the dashboard and the events page. Both pages keep the
//! chosen game in `AppState::dashboard_game`, so switching games on one page carries over to
//! the other. Narrow screens (<=900px) show one game panel at a time; wider screens show
//! every panel and hide the tab bar (`dashboard.css`, `.game-tabs`).
use super::common::{focus_element, game_name};
use crate::app::preferences::use_preferences;
use crate::app::state::{AppState, DASHBOARD_GAMES};
use dioxus::prelude::*;

/// The tab bar. Tabs are `{prefix}-tab-{game}` and control `{prefix}-panel-{game}`.
#[component]
pub(super) fn GameTabs(state: Signal<AppState>, prefix: &'static str) -> Element {
    let ui = use_preferences();
    let active = use_memo(move || state.read().dashboard_game);
    let select = move |game: &'static str| {
        let mut state = state;
        state.write().dashboard_game = game;
    };
    rsx! {
        div { class: "game-tabs tabs", role: "tablist", "aria-label": ui.text("dashboard.games"),
            for game in DASHBOARD_GAMES {
                button {
                    r#type: "button", role: "tab", id: "{prefix}-tab-{game}",
                    "aria-controls": "{prefix}-panel-{game}", "aria-selected": active() == game,
                    tabindex: if active() == game { "0" } else { "-1" },
                    onclick: move |_| select(game),
                    onkeydown: move |event: KeyboardEvent| {
                        if let Some(next) = next_game_tab(game, &event.key()) {
                            event.prevent_default();
                            select(next);
                            focus_element(&format!("{prefix}-tab-{next}"));
                        }
                    },
                    {game_name(game, ui)}
                }
            }
        }
    }
}

/// Switches the shown game after a swipe on the panels (`SwipeableTabs::on_change`).
pub(super) fn select_game_index(mut state: Signal<AppState>, index: usize) {
    if let Some(&game) = DASHBOARD_GAMES.get(index) {
        state.write().dashboard_game = game;
    }
}

// One media listener, removed on unmount. CSS and semantics use the same 900px boundary.
#[cfg(feature = "mobile")]
pub(super) fn use_narrow_games() -> Signal<bool> {
    crate::mobile::dom::use_media_query("(max-width:900px)")
}

#[cfg(not(feature = "mobile"))]
pub(super) fn use_narrow_games() -> Signal<bool> {
    let mut narrow = use_signal(|| false);
    #[cfg(target_arch = "wasm32")]
    {
        let _listener = use_hook(move || {
            use wasm_bindgen::JsCast;
            let media = web_sys::window()?.match_media("(max-width:900px)").ok()??;
            narrow.set(media.matches());
            let observed = media.clone();
            let callback =
                wasm_bindgen::closure::Closure::<dyn FnMut(web_sys::Event)>::new(move |_| {
                    narrow.set(observed.matches())
                });
            media
                .add_event_listener_with_callback("change", callback.as_ref().unchecked_ref())
                .ok()?;
            Some(std::rc::Rc::new(NarrowGamesListener { media, callback }))
        });
    }
    #[cfg(not(target_arch = "wasm32"))]
    let _ = &mut narrow;
    narrow
}

#[cfg(target_arch = "wasm32")]
struct NarrowGamesListener {
    media: web_sys::MediaQueryList,
    callback: wasm_bindgen::closure::Closure<dyn FnMut(web_sys::Event)>,
}

#[cfg(target_arch = "wasm32")]
impl Drop for NarrowGamesListener {
    fn drop(&mut self) {
        use wasm_bindgen::JsCast;
        let _ = self
            .media
            .remove_event_listener_with_callback("change", self.callback.as_ref().unchecked_ref());
    }
}

fn next_game_tab(game: &str, key: &Key) -> Option<&'static str> {
    match key {
        Key::Home => Some("genshin"),
        Key::End => Some("starrail"),
        Key::ArrowLeft | Key::ArrowRight => Some(if game == "genshin" {
            "starrail"
        } else {
            "genshin"
        }),
        _ => None,
    }
}

/// The tab position of `game` for `SwipeableTabs`; unknown games count as the first tab.
pub(super) fn game_index(game: &str) -> usize {
    DASHBOARD_GAMES
        .iter()
        .position(|candidate| *candidate == game)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn game_tab_keyboard_navigation_wraps_and_reaches_edges() {
        assert_eq!(next_game_tab("genshin", &Key::ArrowLeft), Some("starrail"));
        assert_eq!(next_game_tab("starrail", &Key::ArrowRight), Some("genshin"));
        assert_eq!(next_game_tab("starrail", &Key::Home), Some("genshin"));
        assert_eq!(next_game_tab("genshin", &Key::End), Some("starrail"));
        assert_eq!(next_game_tab("genshin", &Key::Enter), None);
    }

    #[test]
    fn swipe_index_follows_the_tab_order() {
        assert_eq!(game_index("genshin"), 0);
        assert_eq!(game_index("starrail"), 1);
        assert_eq!(game_index("zzz"), 0);
        for (index, game) in DASHBOARD_GAMES.iter().enumerate() {
            assert_eq!(game_index(game), index);
        }
    }
}
