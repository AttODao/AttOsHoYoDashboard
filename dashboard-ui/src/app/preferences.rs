use dioxus::prelude::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub(super) enum Language {
    #[default]
    Japanese,
    English,
}

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
    pub(super) fn text(self, japanese: &'static str, english: &'static str) -> &'static str {
        match self.language {
            Language::Japanese => japanese,
            Language::English => english,
        }
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
                document.set_title("AGD");
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

    pub(super) fn message(self, message: &str) -> String {
        if self.language == Language::English {
            return message.to_owned();
        }
        match message {
            "Please sign in." => "ログインしてください。",
            "Please sign in again." => "もう一度ログインしてください。",
            "Signed out." => "ログアウトしました。",
            "The dashboard could not load your session." => "セッションを読み込めませんでした。",
            "Local data cleared, but server logout is unconfirmed. Retry server logout before leaving this browser." => "ローカル情報は消去しましたが、サーバーのログアウトは未確認です。再試行してください。",
            "Account deletion accepted." => "ユーザー削除を受け付けました。処理の完了は未確認です。",
            "Manual reward run queued." => "実行要求を受け付けました。受け取り完了ではありません。",
            "Reward setting saved." => "ログインボーナス設定を保存しました。",
            "Visibility updated." => "表示設定を保存しました。",
            "Cookie stored for server verification." => "Cookieを保存しました。本人確認を実行してください。",
            "Verification requested." => "本人確認を要求しました。",
            "Sync requested." => "UID同期を要求しました。",
            _ => message,
        }.to_owned()
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

    #[test]
    fn both_languages_keep_acceptance_distinct_from_completion() {
        let japanese = Preferences::default();
        assert_eq!(japanese.text("保存", "Save"), "保存");
        assert!(
            japanese
                .message("Manual reward run queued.")
                .contains("完了ではありません")
        );
        let english = Preferences {
            language: Language::English,
            ..japanese
        };
        assert_eq!(english.text("保存", "Save"), "Save");
        assert_eq!(english.message("unknown"), "unknown");
    }
}
