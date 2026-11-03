package dev.dioxus.main

import java.io.File

// JVM tests for AhdIndicatorModel and the indicator layouts it names; run with
// `devenv --profile android shell -- test-android-kotlin` (plain kotlinc, no Gradle/JUnit).

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
    val bar = AhdIndicatorModel.MAX_LEVEL
    val arc = AhdIndicatorModel.ARC_LEVEL
    check("arc span", arc, 7500)
    check("null level", AhdIndicatorModel.level(null, 200, bar), 0)
    check("0% level", AhdIndicatorModel.level(0, 200, bar), 0)
    check("50% bar", AhdIndicatorModel.level(100, 200, bar), 5000)
    check("50% arc", AhdIndicatorModel.level(100, 200, arc), 3750)
    check("100% arc", AhdIndicatorModel.level(200, 200, arc), arc)
    check("over cap clamps", AhdIndicatorModel.level(300, 200, bar), bar)
    check("tiny value stays visible", AhdIndicatorModel.level(1, 100_000, bar), 100)
    check("negative is empty", AhdIndicatorModel.level(-5, 200, bar), 0)
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

private fun strings(res: File) {
    // Both duration formats exist in Japanese (default) and English.
    val namePattern = Regex("""<string name="([a-z_]+)">""")
    for (locale in listOf("values", "values-en")) {
        val defined = namePattern.findAll(File(res, "$locale/ahd_indicator_strings.xml").readText()).map { it.groupValues[1] }.toSet()
        for (hours in listOf(0L, 1L)) {
            check("$locale ${AhdIndicatorModel.durationString(hours)}", AhdIndicatorModel.durationString(hours) in defined, true)
        }
    }
}

private val idPattern = Regex("""android:id="@\+id/([a-z_]+)"""")

private fun ids(res: File, layout: String): Set<String> =
    idPattern.findAll(File(res, "layout/$layout.xml").readText()).map { it.groupValues[1] }.toSet()

private fun layouts(res: File) {
    check("bar ids", AhdIndicatorModel.BAR_IDS.toSet() - ids(res, "ahd_indicator_bar"), emptySet<String>())
    for (size in AhdArcSize.values()) {
        check("${size.layout} ids", AhdIndicatorModel.ARC_IDS.toSet() - ids(res, size.layout), emptySet<String>())
    }
    // Each game theme needs its fill view; AhdIndicators picks "ahd_indicator_fill_<key>".
    for (theme in AhdIndicatorTheme.values()) {
        check("${theme.key} fill", "ahd_indicator_fill_${theme.key}" in AhdIndicatorModel.COMMON_IDS, true)
    }
    // Colors referenced by the layouts exist for light and dark mode.
    val colorPattern = Regex("""@color/([a-z_]+)""")
    val used = File(res, "layout").listFiles()!!.filter { it.name.startsWith("ahd_indicator") }
        .flatMap { file -> colorPattern.findAll(file.readText()).map { it.groupValues[1] }.toList() }.toSet() +
        colorPattern.findAll(File(res, "values/ahd_indicator_styles.xml").readText()).map { it.groupValues[1] }
    val namePattern = Regex("""<color name="([a-z_]+)">""")
    for (mode in listOf("values", "values-night")) {
        val defined = namePattern.findAll(File(res, "$mode/ahd_indicator_colors.xml").readText()).map { it.groupValues[1] }.toSet()
        check("$mode colors", used - defined, emptySet<String>())
    }
    // ImageView's default tint mode (src_atop) keeps the white shape under a translucent tint,
    // so the track would turn white: every tinted view uses src_in.
    for (file in File(res, "layout").listFiles()!!.filter { it.name.startsWith("ahd_indicator") }) {
        val text = file.readText()
        check("${file.name} tint modes", Regex("android:tintMode=\"src_in\"").findAll(text).count(),
            Regex("android:tint=\"").findAll(text).count())
    }
    for (icon in listOf("resin", "power", "coin", "expedition", "daily")) {
        check("icon $icon", File(res, "drawable/ahd_indicator_icon_$icon.xml").isFile, true)
    }
}

fun main(args: Array<String>) {
    val res = File(args.firstOrNull() ?: "dashboard-ui/android/res")
    fillLevels()
    levels()
    texts()
    strings(res)
    layouts(res)
    if (failures > 0) {
        println("$failures indicator check(s) failed")
        kotlin.system.exitProcess(1)
    }
    println("indicator checks passed")
}
