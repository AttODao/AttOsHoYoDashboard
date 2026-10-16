use crate::i18n::Language;
use dioxus::prelude::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub(super) enum Theme {
    #[default]
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

    #[cfg(any(target_arch = "wasm32", test))]
    pub(super) fn language_code(self) -> &'static str {
        match self.language {
            Language::Japanese => "ja",
            Language::English => "en",
        }
    }

    #[cfg(any(target_arch = "wasm32", test))]
    pub(super) fn theme_code(self) -> &'static str {
        match self.theme {
            Theme::Dark => "dark",
            Theme::Light => "light",
        }
    }

    #[cfg(any(target_arch = "wasm32", test))]
    fn from_saved(language: Option<&str>, theme: Option<&str>) -> Self {
        Self {
            language: if language == Some("en") {
                Language::English
            } else {
                Language::Japanese
            },
            theme: if theme == Some("light") {
                Theme::Light
            } else {
                Theme::Dark
            },
        }
    }

    pub(super) fn load() -> Self {
        #[cfg(target_arch = "wasm32")]
        if let Some(storage) = web_sys::window().and_then(|w| w.local_storage().ok().flatten()) {
            return Self::from_saved(
                storage.get_item("agd-language").ok().flatten().as_deref(),
                storage.get_item("agd-theme").ok().flatten().as_deref(),
            );
        }
        Self::default()
    }

    pub(super) fn apply(self) {
        #[cfg(target_arch = "wasm32")]
        if let Some(window) = web_sys::window() {
            if let Some(document) = window.document() {
                document.set_title(self.text("app.title"));
            }
            if let Some(root) = window.document().and_then(|d| d.document_element()) {
                let _ = root.set_attribute("lang", self.language_code());
                let _ = root.set_attribute("data-theme", self.theme_code());
            }
            if let Ok(Some(storage)) = window.local_storage() {
                // Only non-sensitive preferences persist; never credentials or reveal state.
                let _ = storage.set_item("agd-language", self.language_code());
                let _ = storage.set_item("agd-theme", self.theme_code());
            }
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
            Preferences::from_saved(Some("other"), Some("auto")),
            Preferences::default()
        );
        assert_eq!(
            Preferences::from_saved(Some("en"), Some("light")).language_code(),
            "en"
        );
        assert_eq!(
            Preferences::from_saved(Some("en"), Some("light")).theme_code(),
            "light"
        );
    }
}
