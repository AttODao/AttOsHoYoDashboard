package dev.dioxus.main

import java.io.File
import java.util.TimeZone

// JVM tests for AhdIndicatorModel and the widget resources it names; run with
// `devenv --profile android shell -- test-android-kotlin` (plain kotlinc, no Gradle/JUnit).
// The resources are only compiled by a full `build-android`, so this also checks what aapt
// would: every reference resolves, light and dark define the same colors, Japanese and English
// the same strings; and what aapt cannot: the icons are the paths of pages/icons.rs, the views
// the Kotlin code looks up by name exist, and no string is left unused.

private var failures = 0

private fun check(name: String, actual: Any?, expected: Any?) {
    if (actual != expected) {
        failures++
        println("FAIL $name: expected <$expected>, got <$actual>")
    }
}

private fun fillLevels() {
    check("no current", AhdIndicatorModel.fillLevel(null, 200), AhdFillLevel.UNKNOWN)
    check("no max", AhdIndicatorModel.fillLevel(10, null), AhdFillLevel.UNKNOWN)
    check("zero max", AhdIndicatorModel.fillLevel(0, 0), AhdFillLevel.UNKNOWN)
    check("0%", AhdIndicatorModel.fillLevel(0, 200), AhdFillLevel.NORMAL)
    check("50%", AhdIndicatorModel.fillLevel(100, 200), AhdFillLevel.NORMAL)
    check("just below 90%", AhdIndicatorModel.fillLevel(179, 200), AhdFillLevel.NORMAL)
    check("90%", AhdIndicatorModel.fillLevel(180, 200), AhdFillLevel.NEAR_FULL)
    check("100%", AhdIndicatorModel.fillLevel(200, 200), AhdFillLevel.FULL)
    check("over cap", AhdIndicatorModel.fillLevel(260, 240), AhdFillLevel.FULL)
    check("custom near threshold", AhdIndicatorModel.fillLevel(160, 200, nearFullPercent = 80), AhdFillLevel.NEAR_FULL)
}

private fun levels() {
    // A ring's sweep is level/10000 of the circle; the arcs are 270°.
    val arc = AhdIndicatorModel.ARC_LEVEL
    check("arc level", arc, 7500)
    check("null level", AhdIndicatorModel.level(null, 200, arc), 0)
    check("0% level", AhdIndicatorModel.level(0, 200, arc), 0)
    check("50% arc", AhdIndicatorModel.level(100, 200, arc), 3750)
    check("over cap clamps", AhdIndicatorModel.level(300, 200, arc), arc)
    check("tiny value stays visible", AhdIndicatorModel.level(1, 100_000, arc), 75)
    check("negative is empty", AhdIndicatorModel.level(-5, 200, arc), 0)
    check("fraction", AhdIndicatorModel.fraction(50, 200), 0.25)
    check("fraction without max", AhdIndicatorModel.fraction(50, null), null)
}

private fun texts() {
    check("count pair", AhdIndicatorModel.countText(120, 200), "120/200")
    check("count only", AhdIndicatorModel.countText(120, null), "120")
    check("no value", AhdIndicatorModel.countText(null, 200), "—")
    check("rounds up", AhdIndicatorModel.hoursMinutes(3 * 3600 + 11 * 60 + 1), Pair(3L, 12L))
    check("exact", AhdIndicatorModel.hoursMinutes(3 * 3600 + 12 * 60), Pair(3L, 12L))
    check("under a minute", AhdIndicatorModel.hoursMinutes(1), Pair(0L, 1L))
    check("done", AhdIndicatorModel.hoursMinutes(0), Pair(0L, 0L))
    check("negative", AhdIndicatorModel.hoursMinutes(-30), Pair(0L, 0L))
    check("minutes only under an hour", AhdIndicatorModel.durationString(0), "ahd_indicator_minutes")
    check("hours and minutes", AhdIndicatorModel.durationString(1), "ahd_indicator_hours_minutes")
}

private fun pips() {
    check("2 of 3", AhdIndicatorModel.pips(2, 3), AhdPips(2, 3))
    check("none of 4", AhdIndicatorModel.pips(0, 4), AhdPips(0, 4))
    check("all of 5", AhdIndicatorModel.pips(5, 5), AhdPips(5, 5))
    // Unknown, impossible or large counts draw nothing rather than a guess (status_tiles.rs).
    check("unknown filled", AhdIndicatorModel.pips(null, 3), null)
    check("unknown total", AhdIndicatorModel.pips(1, null), null)
    check("more than total", AhdIndicatorModel.pips(5, 4), null)
    check("negative", AhdIndicatorModel.pips(-1, 4), null)
    check("zero total", AhdIndicatorModel.pips(0, 0), null)
    check("more than the layout holds", AhdIndicatorModel.pips(1, AhdIndicatorModel.MAX_PIPS + 1), null)
}

private fun clock() {
    val tokyo = TimeZone.getTimeZone("Asia/Tokyo")
    val evening = 1794571200000L // 2026-11-13T12:00:00Z = 21:00 JST
    check("same day", AhdIndicatorModel.dayOffset(evening, evening + 2 * 3600_000L, tokyo), 0)
    check("after midnight", AhdIndicatorModel.dayOffset(evening, evening + 4 * 3600_000L, tokyo), 1)
    check("a full day and more", AhdIndicatorModel.dayOffset(evening, evening + 27 * 3600_000L, tokyo), 2)
    // The same moments are still "today" where it is noon.
    check("other zone", AhdIndicatorModel.dayOffset(evening, evening + 4 * 3600_000L, TimeZone.getTimeZone("UTC")), 0)
    // The day a DST change makes 23 hours long still counts as one day.
    val newYork = TimeZone.getTimeZone("America/New_York")
    val beforeSpring = 1741410000000L // 2025-03-08T05:00:00Z = 00:00 EST
    check("across DST", AhdIndicatorModel.dayOffset(beforeSpring, beforeSpring + 47 * 3600_000L, newYork), 2)
    check("regular always", AhdIndicatorModel.showsClock(AhdWidgetLayout.REGULAR, 2.0f), true)
    check("compact, default font", AhdIndicatorModel.showsClock(AhdWidgetLayout.COMPACT, 1.0f), true)
    check("compact, 130%", AhdIndicatorModel.showsClock(AhdWidgetLayout.COMPACT, 1.3f), true)
    check("compact, 200%", AhdIndicatorModel.showsClock(AhdWidgetLayout.COMPACT, 2.0f), false)
}

private fun sizes() {
    // 3x3 on a 1080px phone at density 420 is about 270x343dp; at density 560 about 200x248dp.
    check("unknown size", AhdWidgetLayout.forSize(0, 0, 2.0f), AhdWidgetLayout.REGULAR)
    check("3x3", AhdWidgetLayout.forSize(270, 343, 1.0f), AhdWidgetLayout.REGULAR)
    check("3x3 at 115%", AhdWidgetLayout.forSize(270, 343, 1.15f), AhdWidgetLayout.REGULAR)
    check("3x3 at 130%", AhdWidgetLayout.forSize(270, 343, 1.3f), AhdWidgetLayout.COMPACT)
    check("3x3 at 200%", AhdWidgetLayout.forSize(270, 343, 2.0f), AhdWidgetLayout.COMPACT)
    check("density 560", AhdWidgetLayout.forSize(200, 248, 1.0f), AhdWidgetLayout.COMPACT)
    check("minimum", AhdWidgetLayout.forSize(210, 250, 1.0f), AhdWidgetLayout.REGULAR)
    check("below the minimum width", AhdWidgetLayout.forSize(209, 343, 1.0f), AhdWidgetLayout.COMPACT)
    check("below the minimum height", AhdWidgetLayout.forSize(270, 249, 1.0f), AhdWidgetLayout.COMPACT)
    check("small fonts do not shrink the minimum", AhdWidgetLayout.forSize(209, 250, 0.85f), AhdWidgetLayout.COMPACT)
}

private fun tiles() {
    val gauge = AhdTile.gauge(AhdGlyph.COIN, "name", 1200, 2400, 3600)
    check("gauge", gauge, AhdTile(AhdGlyph.COIN, "name", 1200, 2400, ring = 3750, detail = AhdDetail.Time(3600)))
    val near = AhdTile.gauge(AhdGlyph.COIN, "name", 2300, 2400, 600)
    check("near: the value stays, arc and time warn", Triple(near.warn, near.ringWarn, near.detail), Triple(false, true, AhdDetail.Time(600, near = true)))
    val full = AhdTile.gauge(AhdGlyph.COIN, "name", 2400, 2400, 0)
    check("at the cap", Triple(full.warn, full.ringWarn, full.detail), Triple(true, true, AhdDetail.AtCap as AhdDetail?))
    check("gauge without an estimate", AhdTile.gauge(AhdGlyph.COIN, "name", 10, 2400, null).detail, null)
    // No value is never a number, a filled arc or a time; a share keeps its empty arc.
    val unknown = AhdTile.unknown(AhdGlyph.COIN, "name", ring = true)
    check("unknown gauge", AhdTile.gauge(AhdGlyph.COIN, "name", null, 2400, 60), unknown)
    check("unknown score", AhdTile.score(AhdGlyph.COIN, "name", null, 500, "short"), unknown)
    check("unknown shows the mark", Triple(unknown.current, unknown.ring, unknown.detail), Triple(null, 0, AhdDetail.Status(AhdMark.UNKNOWN) as AhdDetail?))
    check("unknown count has no arc", AhdTile.unknown(AhdGlyph.DAILY, "name").ring, null)
    // A reached goal is done, not a warning.
    val score = AhdTile.score(AhdGlyph.DAILY, "name", 300, 500, "short")
    check("score", score, AhdTile(AhdGlyph.DAILY, "name", 300, 500, ring = 4500, detail = AhdDetail.Label("short", spoken = false)))
    val reached = AhdTile.score(AhdGlyph.DAILY, "name", 500, 500, "short")
    check("reached", Triple(reached.warn, reached.ringWarn, reached.detail), Triple(false, false, AhdDetail.Status(AhdMark.GOAL_REACHED) as AhdDetail?))
    check("main level", AhdMain(AhdGlyph.RESIN, "name", 186, 200, 60).level, AhdFillLevel.NEAR_FULL)
    check("main without a value", AhdMain(AhdGlyph.RESIN, "name", null, 200, null).level, AhdFillLevel.UNKNOWN)
}

private val genshinTiles = listOf(
    AhdTile.gauge(AhdGlyph.COIN, "coin", 1320, 2400, 126_720),
    AhdTile(AhdGlyph.DAILY, "daily", 3, 4, pips = AhdPips(3, 4), detail = AhdDetail.Reward(AhdMark.UNCLAIMED)),
    AhdTile(AhdGlyph.EXPEDITION, "expeditions", 2, 5, pips = AhdPips(2, 5), detail = AhdDetail.Range(12_000, 42_300)),
)
private val starRailTiles = listOf(
    AhdTile(AhdGlyph.RESERVE, "reserve", 2400, warn = true, detail = AhdDetail.AtCap),
    AhdTile(AhdGlyph.EXPEDITION, "assignments", 1, 4, pips = AhdPips(1, 4), detail = AhdDetail.Time(29_400)),
    AhdTile.score(AhdGlyph.DAILY, "daily", 500, 500, "short"),
    AhdTile.score(AhdGlyph.WEEKLY, "weekly", 14000, 14000, "short"),
)

private fun content(tiles: List<AhdTile>, wide: Boolean = false) =
    AhdWidgetContent(AhdMain(AhdGlyph.RESIN, "main", 124, 200, 36_480), tiles, 0, false, wideEndColumn = wide)

/** The tile texts follow the font size only as far as the widget has room (no number is cut). */
private fun fits() {
    val regular = AhdWidgetLayout.REGULAR
    val compact = AhdWidgetLayout.COMPACT
    val genshin = content(genshinTiles)
    val starRail = content(starRailTiles, wide = true)
    fun scale(layout: AhdWidgetLayout, width: Int, height: Int, content: AhdWidgetContent, font: Float) =
        AhdIndicatorModel.fit(layout, width, height, content, font).textScale

    // The sizes the design was drawn at hold the default font size, for both games.
    for (game in listOf(genshin, starRail)) {
        check("regular design size", scale(regular, 272, 296, game, 1f), 1f)
        check("compact design size", scale(compact, 196, 208, game, 1f), 1f)
        check("unknown size is the design size", scale(regular, 0, 0, game, 1f), 1f)
        check("small fonts are followed", scale(regular, 272, 296, game, 0.85f), 0.85f)
        // Large fonts are capped by the room, never below the design size there.
        for (font in listOf(1.3f, 2f)) {
            val large = scale(compact, 196, 208, game, font)
            check("compact $font stays readable", large >= 1f && large <= font, true)
        }
        // A 3x3 widget (the launcher reports 267x332dp) in the compact layout has room for more.
        check("3x3 compact at 130%", scale(compact, 267, 332, game, 1.3f), 1.3f)
        check("3x3 compact at 200% is capped", scale(compact, 267, 332, game, 2f).let { it > 1.3f && it < 2f }, true)
        // At the smallest regular size the texts shrink a little instead of being cut.
        val small = scale(regular, 210, 250, game, 1f)
        check("smallest regular", small >= AhdIndicatorModel.MIN_TEXT_SCALE && small < 1f, true)
    }
    // A short name beside a number holds large fonts back to where it still fits, never below the default size.
    val named = content(listOf(AhdTile.score(AhdGlyph.WEEKLY, "weekly", 8400, 14000, "short")))
    val plain = content(listOf(AhdTile.gauge(AhdGlyph.COIN, "coin", 8400, 14000, null)))
    check("no name, large font", scale(compact, 200, 249, plain, 2f) > 1.5f, true)
    check("a name caps large fonts", scale(compact, 200, 249, named, 2f).let { it >= 1f && it < 1.2f }, true)
    check("a name with room", scale(compact, 267, 332, named, 2f).let { it > 1.2f && it < 2f }, true)
    // The smallest size keeps the minimum scale rather than an unreadable one.
    check("minimum scale", scale(compact, 120, 120, starRail, 1f), AhdIndicatorModel.MIN_TEXT_SCALE)

    // Columns: equal halves, a last tile alone takes the row; Star Rail's are 5:6.
    check("genshin columns", AhdIndicatorModel.fit(regular, 272, 296, genshin, 1f).columnsDp, listOf(120, 120, 248))
    check("star rail columns", AhdIndicatorModel.fit(regular, 272, 296, starRail, 1f).columnsDp, listOf(109, 130, 109, 130))
    check("compact rows", AhdIndicatorModel.fit(compact, 196, 208, genshin, 1f).columnsDp, listOf(176, 176, 176))

    // "first〜last" needs a tile of its own row; a column or a compact row shows the last time.
    check("range in a full row", AhdIndicatorModel.showsRange(regular, 248, 1f), true)
    check("range in a column", AhdIndicatorModel.showsRange(regular, 120, 1f), false)
    check("range in a full row, large text", AhdIndicatorModel.showsRange(regular, 186, 1.5f), false)
    check("range in a compact row", AhdIndicatorModel.showsRange(compact, 250, 1f), false)
}

// ---- resources ----

private class Res(val dir: File) {
    fun text(path: String) = File(dir, path).readText()
    fun files(folder: String): List<File> = File(dir, folder).listFiles()!!.filter { it.name.startsWith("ahd_") && it.extension == "xml" }.sortedBy { it.name }
    fun names(folder: String, tag: String): Set<String> =
        files(folder).flatMap { Regex("""<$tag name="([A-Za-z_.]+)"""").findAll(it.readText()).map { match -> match.groupValues[1] } }.toSet()
    fun ids(layout: String): Set<String> = Regex("""android:id="@\+id/([a-z_0-9]+)"""").findAll(text("layout/$layout.xml")).map { it.groupValues[1] }.toSet()
    fun includes(layout: String): List<String> = Regex("""<include layout="@layout/([a-z_]+)"""").findAll(text("layout/$layout.xml")).map { it.groupValues[1] }.toList()

    /** Ids of a layout with everything it includes. */
    fun allIds(layout: String): Set<String> = ids(layout) + includes(layout).flatMap { allIds(it) }
}

/** Every `@type/name` in the widget resources and manifest snippets is defined, in both modes and languages. */
private fun references(res: Res, android: File) {
    val defined = mapOf(
        "string" to res.names("values", "string"),
        "color" to res.names("values", "color"),
        "dimen" to res.names("values", "dimen"),
        "style" to res.names("values", "style"),
        "drawable" to res.files("drawable").map { it.nameWithoutExtension }.toSet(),
        "layout" to res.files("layout").map { it.nameWithoutExtension }.toSet(),
        "xml" to res.files("xml").map { it.nameWithoutExtension }.toSet(),
    )
    val sources = listOf("values", "values-night", "values-en", "drawable", "layout", "xml").flatMap { res.files(it) } +
        android.listFiles()!!.filter { it.name.startsWith("AndroidManifest") }
    for (file in sources) {
        for (match in Regex("""@(string|color|dimen|style|drawable|layout|xml)/([A-Za-z_.0-9]+)""").findAll(file.readText())) {
            val (type, name) = match.destructured
            if (!name.startsWith("ahd_") && !name.startsWith("Ahd")) continue
            check("${file.name} @$type/$name", name in defined.getValue(type), true)
        }
        // A style's dotted name inherits from its prefix, which must exist too.
        for (match in Regex("""<style name="([A-Za-z.]+)\.[A-Za-z]+"""").findAll(file.readText())) {
            check("${file.name} parent ${match.groupValues[1]}", match.groupValues[1] in defined.getValue("style"), true)
        }
    }
    // The game's theme attributes: each one used is declared, and both overlays set all of them.
    val attrs = res.names("values", "attr")
    for (file in sources) for (match in Regex("""\?attr/([A-Za-z]+)""").findAll(file.readText())) {
        check("${file.name} ?attr/${match.groupValues[1]}", match.groupValues[1] in attrs, true)
    }
    val styles = res.text("values/ahd_indicator_styles.xml")
    for (overlay in listOf("AhdGameGenshin", "AhdGameStarRail")) {
        val body = Regex("""(?s)<style name="$overlay">(.*?)</style>""").find(styles)?.groupValues?.get(1).orEmpty()
        check("$overlay sets every attribute", Regex("""<item name="([A-Za-z]+)"""").findAll(body).map { it.groupValues[1] }.toSet(), attrs)
    }
    check("dark colors", res.names("values-night", "color"), defined.getValue("color").filter { it != "ahd_launcher_background" }.toSet())
    check("English strings", res.names("values-en", "string"), defined.getValue("string"))
    // Formats take the same arguments in both languages.
    for (file in res.files("values").filter { it.name.endsWith("strings.xml") }) {
        fun formats(text: String) = Regex("""<string name="([a-z_]+)">([^<]*)""").findAll(text)
            .associate { it.groupValues[1] to Regex("""%\d\$\d*[ds]""").findAll(it.groupValues[2]).map { arg -> arg.value }.toSortedSet() }
        check("${file.name} formats", formats(res.text("values-en/${file.name}")), formats(file.readText()))
    }
}

/** No string stays behind unused: each is named by a layout, a manifest snippet or the Kotlin sources. */
private fun usedStrings(res: Res, android: File) {
    val kotlin = (File(android, "kotlin").listFiles()!!.toList() + File(android, "MainActivity.kt")).joinToString("\n") { it.readText() }
    // "ahd_widget_${game.key}_label" in the sources names one string per game.
    val patterns = Regex(""""(ahd_[a-z_]*(?:\$\{[^}]+\}|\$[a-z]+)[a-z_$\{\}.]*)"""").findAll(kotlin)
        .map { Regex(it.groupValues[1].replace(Regex("""\$\{[^}]+\}|\$[a-z]+"""), "[a-z]+")) }.toList()
    val xml = (listOf("layout", "xml").flatMap { res.files(it) } + android.listFiles()!!.filter { it.name.startsWith("AndroidManifest") })
        .joinToString("\n") { it.readText() }
    for (name in res.names("values", "string").filter { it.startsWith("ahd_widget") || it.startsWith("ahd_indicator") }) {
        val used = "\"$name\"" in kotlin || "@string/$name\"" in xml || patterns.any { it.matches(name) }
        check("string $name is used", used, true)
    }
}

/** The views the code looks up by name (AhdIndicators.kt, AhdWidgetFrame.kt) exist in every layout that is drawn. */
private fun layouts(res: Res) {
    val themes = AhdIndicatorTheme.values().map { it.key }
    val tones = AhdTone.values().map { it.key }
    val fills = listOf("accent", "warn")
    val tile = setOf(
        "ahd_indicator_root", "ahd_indicator_ring", "ahd_indicator_ring_icon", "ahd_indicator_icon", "ahd_indicator_value", "ahd_indicator_value_warn",
        "ahd_indicator_value_none", "ahd_indicator_max", "ahd_indicator_pips", "ahd_indicator_gift",
    ) + fills.map { "ahd_indicator_fill_$it" } + (1..AhdIndicatorModel.MAX_PIPS).map { "ahd_indicator_pip_$it" } +
        tones.flatMap { listOf("ahd_indicator_detail_icon_$it", "ahd_indicator_detail_$it") }
    for (kind in listOf("tile", "row")) for (game in themes) {
        // A tile added from code needs the game's theme overlay on its own root.
        val layout = "ahd_indicator_${kind}_$game"
        check("$layout ids", tile - res.allIds(layout), emptySet<String>())
        check("$layout includes", res.includes(layout), listOf("ahd_indicator_$kind"))
    }
    val slots = (1..AhdIndicatorModel.MAX_TILES).map { "ahd_widget_slot_$it" }
    val widget = setOf(
        "ahd_widget_root", "ahd_widget_title", "ahd_widget_fetched_icon", "ahd_widget_fetched", "ahd_widget_fetched_warn_icon", "ahd_widget_fetched_warn",
        "ahd_widget_message_box", "ahd_widget_message_icon", "ahd_widget_status", "ahd_widget_hint", "ahd_widget_content",
        "ahd_widget_main_icon", "ahd_widget_main_max", "ahd_widget_main_badge", "ahd_widget_main_badge_text", "ahd_widget_main_time_row", "ahd_widget_main_time",
        "ahd_widget_main_clock", "ahd_widget_main_unknown", "ahd_widget_main_time_none",
    ) + (fills + "none").map { "ahd_widget_main_value_$it" } + fills.map { "ahd_widget_main_fill_$it" } + slots
    // The regular body has its slots in equal columns and in 5:6; the compact one marks "near the cap" with a symbol of its own.
    val regular = setOf("ahd_widget_tiles", "ahd_widget_tiles_wide") + (1..AhdIndicatorModel.MAX_TILES).map { "ahd_widget_wide_slot_$it" }
    val compact = setOf("ahd_widget_main_near")
    for (game in themes) for (suffix in listOf("", "_compact")) {
        val layout = "ahd_widget_$game$suffix"
        check("$layout ids", widget + (if (suffix.isEmpty()) regular else compact) - res.allIds(layout), emptySet<String>())
        // The provider info names the regular layout and a preview that needs no code.
        if (suffix.isEmpty()) {
            val info = res.text("xml/ahd_widget_${game}_info.xml")
            check("$game initial layout", "android:initialLayout=\"@layout/$layout\"" in info, true)
            check("$game preview layout", "android:previewLayout=\"@layout/${layout}_preview\"" in info, true)
        }
    }
    // Every layout root that is inflated by itself names its game: the widgets, their previews and the tiles added from code.
    val overlays = mapOf("genshin" to "AhdGameGenshin", "starrail" to "AhdGameStarRail")
    for ((game, overlay) in overlays) {
        val roots = listOf("ahd_widget_$game", "ahd_widget_${game}_compact", "ahd_widget_${game}_preview", "ahd_indicator_tile_$game", "ahd_indicator_row_$game")
        for (layout in roots) check("$layout theme", "android:theme=\"@style/$overlay\"" in res.text("layout/$layout.xml"), true)
    }
    // RemoteViews inflate only these classes; anything else (View, Space) fails at the launcher.
    val allowed = setOf("LinearLayout", "FrameLayout", "ImageView", "TextView", "include")
    for (file in res.files("layout")) {
        val tags = Regex("""<([A-Za-z.]+)""").findAll(file.readText().replace(Regex("(?s)<!--.*?-->"), "")).map { it.groupValues[1] }.toSet()
        check("${file.name} view classes", tags - allowed, emptySet<String>())
    }
    // An arc exists in the four sizes the layouts use, as a track and as a fill.
    val drawables = res.files("drawable").map { it.nameWithoutExtension }.toSet()
    for (size in listOf(96, 60, 36, 22)) {
        check("arc $size", drawables.containsAll(listOf("ahd_indicator_arc_$size", "ahd_indicator_arc_track_$size")), true)
        check("arc $size is used", res.files("layout").any { "@drawable/ahd_indicator_arc_$size\"" in it.readText() }, true)
    }
}

/** Each glyph and game mark is the path of the same icon in pages/icons.rs: one icon language for web, app and widgets. */
private fun icons(res: Res, iconsRs: File) {
    val source = iconsRs.readText()
    fun path(icon: String): String? = Regex("""Self::$icon => \{?\s*"([^"]+)"""").find(source)?.groupValues?.get(1)
    val drawables = AhdGlyph.values().map { it.drawable to it.icon } +
        listOf("ahd_widget_mark_genshin" to "MarkGenshin", "ahd_widget_mark_starrail" to "MarkStarRail")
    for ((drawable, icon) in drawables) {
        val file = File(res.dir, "drawable/$drawable.xml")
        check("$drawable exists", file.isFile, true)
        if (!file.isFile) continue
        val xml = file.readText()
        check("$drawable is Icon::$icon", Regex("""android:pathData="([^"]+)"""").find(xml)?.groupValues?.get(1), path(icon))
        // Line art: one stroked path with round ends, no fill.
        check("$drawable is one path", Regex("<path").findAll(xml).count(), 1)
        check("$drawable has no fill", "android:fillColor=\"#00000000\"" in xml, true)
        check("$drawable round caps", "android:strokeLineCap=\"round\"" in xml && "android:strokeLineJoin=\"round\"" in xml, true)
    }
    val known = drawables.map { it.first }.toSet()
    val icons = res.files("drawable").map { it.nameWithoutExtension }.filter { it.startsWith("ahd_indicator_icon_") || it.startsWith("ahd_widget_mark_") }
    check("no icon outside the set", icons.filter { it !in known }, emptyList<String>())
    // The words of the status symbols exist as strings.
    val strings = res.names("values", "string")
    for (mark in AhdMark.values()) check("${mark.name} word", mark.word in strings, true)
}

fun main(args: Array<String>) {
    val dir = File(args.firstOrNull() ?: "dashboard-ui/android/res")
    val res = Res(dir)
    val android = dir.parentFile
    fillLevels()
    levels()
    texts()
    pips()
    clock()
    sizes()
    tiles()
    fits()
    references(res, android)
    usedStrings(res, android)
    layouts(res)
    icons(res, File(android.parentFile, "src/pages/icons.rs"))
    if (failures > 0) {
        println("$failures widget check(s) failed")
        kotlin.system.exitProcess(1)
    }
    println("widget checks passed")
}
