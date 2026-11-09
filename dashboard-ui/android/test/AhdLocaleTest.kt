package dev.dioxus.main

import org.junit.Assert.*
import org.junit.Test

class AhdLocaleTest {
    // Same vectors as Rust i18n::tests::locale_lists_choose_the_first_supported_language_and_default_to_english.
    @Test fun localeListsChooseTheFirstSupportedLanguageAndDefaultToEnglish() {
        for ((tags, expected) in listOf(
            listOf("ja-JP") to "ja",
            listOf("ja") to "ja",
            listOf("JA_jp") to "ja",
            listOf("en-US") to "en",
            listOf("en-US", "ja-JP") to "en",
            listOf("fr-FR") to "en",
            listOf("zh-Hant-TW", "ko-KR") to "en",
            listOf("fr-FR", "ja-JP") to "ja",
            listOf("fr-FR", "en-GB", "ja-JP") to "en",
            listOf("jav-ID") to "en",
            listOf(" ", "") to "ja",
            emptyList<String>() to "ja",
        )) {
            assertEquals(tags.toString(), expected, AhdLocale.languageFor(tags))
        }
        // deviceLocales() gives "" when the list cannot be read.
        assertEquals("ja", AhdLocale.languageFor("".split(',')))
    }

    @Test fun onlyAValidSavedLanguageCounts() {
        assertEquals("en", AhdLocale.savedLanguage("""{"language":"en","theme":"system"}"""))
        assertEquals("ja", AhdLocale.savedLanguage("""{"language":"ja"}"""))
        assertNull(AhdLocale.savedLanguage("""{"theme":"dark"}"""))
        assertNull(AhdLocale.savedLanguage("""{"language":"fr"}"""))
        assertNull(AhdLocale.savedLanguage("not json"))
        assertNull(AhdLocale.savedLanguage(null))
    }
}
