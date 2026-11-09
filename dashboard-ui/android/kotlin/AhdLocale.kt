package dev.dioxus.main

import android.app.LocaleManager
import android.content.Context
import android.content.res.Configuration
import android.content.res.Resources
import android.os.Build
import android.os.LocaleList
import android.util.Log
import org.json.JSONObject
import java.io.File
import java.util.Locale

/**
 * The app's UI language outside the WebView (home-screen widgets): the language saved in the
 * app (`files/preferences.json`, written by Rust `app/preferences.rs`) or, without one, the
 * device locales by the same rule as Rust `Language::for_locales` ([languageFor]). Rust reads
 * [deviceLocales] for its own default and calls [languageChanged] after every preference change.
 */
object AhdLocale {
    private const val PREFERENCES = "preferences.json"
    private inline fun <T> guarded(fallback: T, operation: () -> T): T = try { operation() } catch (error: Exception) {
        Log.w("AHD", "Locale operation failed", error)
        fallback
    }

    /** Per-app language list (Android 13+, Settings or `cmd locale set-app-locales`) if set, else the system list, as language tags. */
    @JvmStatic fun deviceLocales(context: Context): String = guarded("") { deviceLocaleList(context).toLanguageTags() }

    private fun deviceLocaleList(context: Context): LocaleList {
        if (Build.VERSION.SDK_INT >= 33) {
            val manager = context.getSystemService(LocaleManager::class.java)
            val app = manager.applicationLocales
            return if (app.isEmpty) manager.systemLocales else app
        }
        return Resources.getSystem().configuration.locales
    }

    /** "ja" or "en" for language tags in preference order; mirrors Rust `Language::for_locales`. */
    fun languageFor(tags: List<String>): String {
        var any = false
        for (tag in tags) {
            val primary = tag.trim().split('-', '_').first()
            if (primary.isEmpty()) continue
            any = true
            if (primary.equals("ja", ignoreCase = true)) return "ja"
            if (primary.equals("en", ignoreCase = true)) return "en"
        }
        return if (any) "en" else "ja"
    }

    /** The language the user picked in the app, from preferences.json content; null when none is saved. */
    fun savedLanguage(json: String?): String? {
        if (json == null) return null
        return try { JSONObject(json).optString("language").takeIf { it == "ja" || it == "en" } } catch (error: Exception) { null }
    }

    fun language(context: Context): String {
        val saved = guarded(null as String?) { File(context.filesDir, PREFERENCES).takeIf { it.isFile }?.readText() }
        return savedLanguage(saved) ?: languageFor(deviceLocales(context).split(','))
    }

    /** [context] with its resources in the app's language (values/ is Japanese, values-en/ English). */
    fun localized(context: Context): Context = guarded(context) {
        val configuration = Configuration(context.resources.configuration)
        configuration.setLocales(LocaleList(Locale.forLanguageTag(language(context))))
        context.createConfigurationContext(configuration)
    }

    /** Redraws the widgets in the app's current language (cheap; called after every preference change). */
    @JvmStatic fun languageChanged(context: Context): Boolean = guarded(false) {
        AhdWidgets.renderAll(context)
        true
    }
}
