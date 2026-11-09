use crate::i18n::Language;
use dioxus::prelude::*;

/// The app's saved language/theme codes, in its private storage.
#[cfg(feature = "mobile")]
const APP_PREFERENCES: &str = "preferences.json";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub(super) enum Theme {
    /// Follow the browser's light/dark preference.
    #[default]
    System,
    Dark,
    Light,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub(super) struct Preferences {
    pub(super) language: Language,
    /// True once the language is the user's own choice, which is saved. Until then the
    /// language follows the browser or device locale and is not saved, so a later locale
    /// change still applies.
    pub(super) language_chosen: bool,
    pub(super) theme: Theme,
}

impl Preferences {
    pub(super) fn text(self, key: &str) -> &'static str {
        self.language.text(key)
    }

    pub(super) fn format(self, key: &str, arguments: &[(&str, &str)]) -> String {
        self.language.format(key, arguments)
    }

    #[cfg(any(target_arch = "wasm32", feature = "mobile", test))]
    pub(super) fn language_code(self) -> &'static str {
        match self.language {
            Language::Japanese => "ja",
            Language::English => "en",
        }
    }

    pub(super) fn theme_code(self) -> &'static str {
        match self.theme {
            Theme::System => "system",
            Theme::Dark => "dark",
            Theme::Light => "light",
        }
    }

    pub(super) fn theme_from_code(code: &str) -> Theme {
        match code {
            "dark" => Theme::Dark,
            "light" => Theme::Light,
            _ => Theme::System,
        }
    }

    /// The concrete palette: a system choice resolves through the browser preference.
    #[cfg(any(target_arch = "wasm32", test))]
    fn resolved_theme(self, prefers_light: bool) -> &'static str {
        match self.theme {
            Theme::System if prefers_light => "light",
            Theme::System | Theme::Dark => "dark",
            Theme::Light => "light",
        }
    }

    /// Saved choices win; without one, the locale list (browser `navigator.languages`, or the
    /// app's per-app then system locales) and the color scheme decide.
    #[cfg(any(target_arch = "wasm32", feature = "mobile", test))]
    fn from_saved(language: Option<&str>, theme: Option<&str>, locales: &[String]) -> Self {
        let saved = match language {
            Some("en") => Some(Language::English),
            Some("ja") => Some(Language::Japanese),
            _ => None,
        };
        Self {
            language: saved
                .unwrap_or_else(|| Language::for_locales(locales.iter().map(String::as_str))),
            language_chosen: saved.is_some(),
            theme: theme.map_or(Theme::System, Self::theme_from_code),
        }
    }

    /// The same preferences after the locale list changed: only an unchosen language follows.
    #[cfg(any(feature = "mobile", test))]
    fn follow_locales(self, locales: &[String]) -> Self {
        if self.language_chosen {
            return self;
        }
        Self {
            language: Language::for_locales(locales.iter().map(String::as_str)),
            ..self
        }
    }

    /// The menu's language choice, saved from now on.
    pub(super) fn choose_language(&mut self, language: Language) {
        self.language = language;
        self.language_chosen = true;
    }

    pub(super) fn load() -> Self {
        #[cfg(target_arch = "wasm32")]
        if let Some(window) = web_sys::window() {
            let navigator = window.navigator();
            let mut locales: Vec<String> = navigator
                .languages()
                .iter()
                .filter_map(|tag| tag.as_string())
                .collect();
            if locales.is_empty() {
                locales.extend(navigator.language());
            }
            let storage = window.local_storage().ok().flatten();
            let saved = |key| {
                storage
                    .as_ref()
                    .and_then(|s| s.get_item(key).ok().flatten())
            };
            return Self::from_saved(
                saved("ahd-language").as_deref(),
                saved("ahd-theme").as_deref(),
                &locales,
            );
        }
        // The app keeps the same two codes in its private storage; without a saved language
        // the device locale decides (docs/android-changes.md 5.10).
        #[cfg(feature = "mobile")]
        {
            let saved: Option<std::collections::BTreeMap<String, String>> =
                crate::mobile::storage::read(APP_PREFERENCES)
                    .and_then(|text| serde_json::from_str(&text).ok());
            let saved = saved.unwrap_or_default();
            Self::from_saved(
                saved.get("language").map(String::as_str),
                saved.get("theme").map(String::as_str),
                &crate::mobile::locale::device_locales(),
            )
        }
        #[cfg(not(feature = "mobile"))]
        Self::default()
    }

    pub(super) fn apply(self) {
        #[cfg(target_arch = "wasm32")]
        if let Some(window) = web_sys::window() {
            let prefers_light = window
                .match_media("(prefers-color-scheme: light)")
                .ok()
                .flatten()
                .is_some_and(|media| media.matches());
            let theme = self.resolved_theme(prefers_light);
            if let Some(root) = window.document().and_then(|d| d.document_element()) {
                let _ = root.set_attribute("lang", self.language_code());
                let _ = root.set_attribute("data-theme", theme);
            }
            // Browser chrome follows the resolved page canvas.
            if let Some(meta) = window
                .document()
                .and_then(|d| d.get_element_by_id("theme-color"))
            {
                let canvas = if theme == "light" {
                    "#F5F4F8"
                } else {
                    "#0C0B0E"
                };
                let _ = meta.set_attribute("content", canvas);
            }
            if let Ok(Some(storage)) = window.local_storage() {
                // Only non-sensitive preferences persist; never credentials or reveal state.
                // An unchosen language is not saved, so it keeps following the browser.
                if self.language_chosen {
                    let _ = storage.set_item("ahd-language", self.language_code());
                }
                let _ = storage.set_item("ahd-theme", self.theme_code());
            }
        }
        #[cfg(feature = "mobile")]
        {
            crate::mobile::dom::apply_preferences(self.language_code(), self.theme_code());
            let mut saved = serde_json::json!({"theme": self.theme_code()});
            if self.language_chosen {
                saved["language"] = self.language_code().into();
            }
            let _ = crate::mobile::storage::write(APP_PREFERENCES, &saved.to_string());
            // Widgets read the saved choice (or the device locale) from Kotlin.
            crate::mobile::locale::language_changed();
        }
    }
}

pub(super) fn use_preferences() -> Preferences {
    *use_context::<Signal<Preferences>>().read()
}

/// Follows a per-app or system language change made while the app was in the background
/// (the activity is not recreated for locale changes): re-read the locales when the page
/// becomes visible again.
#[cfg(feature = "mobile")]
pub(super) fn use_device_language(mut preferences: Signal<Preferences>) {
    use_hook(move || {
        let mut eval = document::eval(
            "document.addEventListener('visibilitychange', () => { if (!document.hidden) dioxus.send(true); });",
        );
        spawn(async move {
            while eval.recv::<bool>().await.is_ok() {
                let current = *preferences.peek();
                let next = current.follow_locales(&crate::mobile::locale::device_locales());
                if next != current {
                    preferences.set(next);
                }
            }
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tags(tags: &[&str]) -> Vec<String> {
        tags.iter().map(|tag| (*tag).to_owned()).collect()
    }

    #[test]
    fn unknown_preferences_are_not_interpreted_as_settings() {
        assert_eq!(
            Preferences::from_saved(Some("other"), Some("auto"), &tags(&["ja-JP"])),
            Preferences::default()
        );
        let saved = Preferences::from_saved(Some("en"), Some("light"), &tags(&["ja-JP"]));
        assert_eq!(saved.language_code(), "en");
        assert!(saved.language_chosen);
        assert_eq!(saved.theme_code(), "light");
        assert_eq!(
            Preferences::from_saved(Some("ja"), Some("dark"), &tags(&["en-US"])).theme_code(),
            "dark"
        );
    }

    #[test]
    fn a_saved_language_wins_over_any_locale() {
        for locales in [tags(&["en-US"]), tags(&["fr-FR"]), tags(&[])] {
            let saved = Preferences::from_saved(Some("ja"), None, &locales);
            assert_eq!(saved.language_code(), "ja", "{locales:?}");
            assert_eq!(saved.follow_locales(&tags(&["en-US"])), saved);
        }
        let saved = Preferences::from_saved(Some("en"), None, &tags(&["ja-JP"]));
        assert_eq!(
            saved.follow_locales(&tags(&["ja-JP"])).language_code(),
            "en"
        );
    }

    #[test]
    fn first_visit_follows_browser_language_and_color_scheme() {
        let first = Preferences::from_saved(None, None, &tags(&["en-GB"]));
        assert_eq!(first.language_code(), "en");
        assert!(!first.language_chosen);
        assert_eq!(first.theme, Theme::System);
        assert_eq!(first.resolved_theme(true), "light");
        assert_eq!(first.resolved_theme(false), "dark");
        assert_eq!(
            Preferences::from_saved(None, None, &tags(&["ja"])).language_code(),
            "ja"
        );
        // Neither Japanese nor English: English. Nothing readable: the Japanese default.
        assert_eq!(
            Preferences::from_saved(None, None, &tags(&["de-DE"])).language_code(),
            "en"
        );
        assert_eq!(
            Preferences::from_saved(None, None, &[]).language_code(),
            "ja"
        );
        let dark = Preferences::from_saved(None, Some("dark"), &tags(&["ja"]));
        assert_eq!(dark.resolved_theme(true), "dark");
    }

    #[test]
    fn an_unchosen_language_follows_locale_changes_until_the_user_picks_one() {
        let first = Preferences::from_saved(None, Some("light"), &tags(&["ja-JP"]));
        let moved = first.follow_locales(&tags(&["en-US", "ja-JP"]));
        assert_eq!(moved.language_code(), "en");
        assert_eq!(moved.theme_code(), "light");
        assert!(!moved.language_chosen);
        let mut chosen = moved;
        chosen.choose_language(Language::Japanese);
        assert!(chosen.language_chosen);
        assert_eq!(
            chosen.follow_locales(&tags(&["en-US"])).language_code(),
            "ja"
        );
    }
}
