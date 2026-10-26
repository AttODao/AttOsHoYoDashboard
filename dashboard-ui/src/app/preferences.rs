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

    /// Saved choices win; without one, the browser language and color scheme decide.
    #[cfg(any(target_arch = "wasm32", feature = "mobile", test))]
    fn from_saved(language: Option<&str>, theme: Option<&str>, browser_language: &str) -> Self {
        Self {
            language: match language {
                Some("en") => Language::English,
                Some("ja") => Language::Japanese,
                _ if browser_language.starts_with("ja") || browser_language.is_empty() => {
                    Language::Japanese
                }
                _ => Language::English,
            },
            theme: theme.map_or(Theme::System, Self::theme_from_code),
        }
    }

    pub(super) fn load() -> Self {
        #[cfg(target_arch = "wasm32")]
        if let Some(window) = web_sys::window() {
            let browser = window.navigator().language().unwrap_or_default();
            let storage = window.local_storage().ok().flatten();
            let saved = |key| {
                storage
                    .as_ref()
                    .and_then(|s| s.get_item(key).ok().flatten())
            };
            return Self::from_saved(
                saved("ahd-language").as_deref(),
                saved("ahd-theme").as_deref(),
                &browser,
            );
        }
        // The app keeps the same two codes in its private storage; without them it
        // starts in Japanese and the system theme.
        #[cfg(feature = "mobile")]
        {
            let saved: Option<std::collections::BTreeMap<String, String>> =
                crate::mobile::storage::read(APP_PREFERENCES)
                    .and_then(|text| serde_json::from_str(&text).ok());
            let saved = saved.unwrap_or_default();
            Self::from_saved(
                saved.get("language").map(String::as_str),
                saved.get("theme").map(String::as_str),
                "",
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
                let _ = storage.set_item("ahd-language", self.language_code());
                let _ = storage.set_item("ahd-theme", self.theme_code());
            }
        }
        #[cfg(feature = "mobile")]
        {
            crate::mobile::dom::apply_preferences(self.language_code(), self.theme_code());
            let saved =
                serde_json::json!({"language": self.language_code(), "theme": self.theme_code()});
            let _ = crate::mobile::storage::write(APP_PREFERENCES, &saved.to_string());
        }
    }
}

pub(super) fn use_preferences() -> Preferences {
    *use_context::<Signal<Preferences>>().read()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_preferences_are_not_interpreted_as_settings() {
        assert_eq!(
            Preferences::from_saved(Some("other"), Some("auto"), "ja-JP"),
            Preferences::default()
        );
        let saved = Preferences::from_saved(Some("en"), Some("light"), "ja-JP");
        assert_eq!(saved.language_code(), "en");
        assert_eq!(saved.theme_code(), "light");
        assert_eq!(
            Preferences::from_saved(Some("ja"), Some("dark"), "en-US").theme_code(),
            "dark"
        );
    }

    #[test]
    fn first_visit_follows_browser_language_and_color_scheme() {
        let first = Preferences::from_saved(None, None, "en-GB");
        assert_eq!(first.language_code(), "en");
        assert_eq!(first.theme, Theme::System);
        assert_eq!(first.resolved_theme(true), "light");
        assert_eq!(first.resolved_theme(false), "dark");
        assert_eq!(
            Preferences::from_saved(None, None, "ja").language_code(),
            "ja"
        );
        let dark = Preferences::from_saved(None, Some("dark"), "ja");
        assert_eq!(dark.resolved_theme(true), "dark");
    }
}
