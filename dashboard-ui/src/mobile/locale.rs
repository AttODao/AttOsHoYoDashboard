//! The device locale list for the default UI language (Kotlin `AhdLocale`); the decision is
//! `i18n::Language::for_locales`.
#[cfg(feature = "mobile")]
const LOCALE: &str = "dev.dioxus.main.AhdLocale";

/// The per-app language list when one is set (Android 13+), else the system list, most
/// preferred first. Empty when the bridge is unavailable.
#[cfg(feature = "mobile")]
pub(crate) fn device_locales() -> Vec<String> {
    super::notify::call_on(
        LOCALE,
        "deviceLocales",
        "(Landroid/content/Context;)Ljava/lang/String;",
        &[],
        true,
    )
    .map(|tags| parse_tags(&tags))
    .unwrap_or_default()
}

/// Redraws the home-screen widgets after a preference change, so they follow the app's
/// language (saved choice or device default).
#[cfg(feature = "mobile")]
pub(crate) fn language_changed() {
    let _ = super::notify::call_on(
        LOCALE,
        "languageChanged",
        "(Landroid/content/Context;)Z",
        &[],
        true,
    );
}

/// `LocaleList.toLanguageTags()` output: comma-separated BCP 47 tags.
#[cfg(any(feature = "mobile", test))]
fn parse_tags(tags: &str) -> Vec<String> {
    tags.split(',')
        .map(str::trim)
        .filter(|tag| !tag.is_empty())
        .map(str::to_owned)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn language_tag_lists_split_on_commas_and_drop_blanks() {
        assert_eq!(parse_tags("ja-JP,en-US"), ["ja-JP", "en-US"]);
        assert_eq!(parse_tags(" en-GB , ,fr-FR"), ["en-GB", "fr-FR"]);
        assert!(parse_tags("").is_empty());
    }
}
