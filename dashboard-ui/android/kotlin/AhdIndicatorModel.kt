package dev.dioxus.main

// Pure rules behind the home-screen widget indicators (AhdIndicators.kt). No Android imports,
// so `test-android-kotlin` compiles and runs them on the JVM
// (dashboard-ui/android/tests/AhdIndicatorModelTest.kt).

/** Game color theme of an indicator; the accent colors live in res/values(-night)/ahd_indicator_colors.xml. */
enum class AhdIndicatorTheme(val key: String) {
    GENSHIN("genshin"),
    STAR_RAIL("starrail"),
}

/** How close a value is to its cap. UNKNOWN is "no value" (null current or no usable maximum). */
enum class AhdFillLevel {
    UNKNOWN,
    NORMAL,
    NEAR_FULL,
    FULL,
}

/** Size variants of the arc indicator (RemoteViews cannot resize a view before API 31). */
enum class AhdArcSize(val layout: String) {
    LARGE("ahd_indicator_arc"),
    SMALL("ahd_indicator_arc_small"),
}

object AhdIndicatorModel {
    /** Drawable levels run from 0 to 10000 (Drawable.setLevel). */
    const val MAX_LEVEL = 10_000

    /** The arc is a 270° gauge with its gap at the bottom: a full arc is 3/4 of a ring. */
    const val ARC_LEVEL = MAX_LEVEL * 3 / 4

    /** Same rule as the dashboard (dashboard-ui/src/app/display.rs fill_level): "near" from 90%. */
    const val NEAR_FULL_PERCENT = 90

    /** Shown instead of a number when there is no value. */
    const val NO_VALUE = "—"

    /** View ids every indicator layout declares; AhdIndicators.kt looks them up by name. */
    val COMMON_IDS = listOf(
        "ahd_indicator_root",
        "ahd_indicator_track",
        "ahd_indicator_fill_genshin",
        "ahd_indicator_fill_starrail",
        "ahd_indicator_fill_near",
        "ahd_indicator_fill_full",
        "ahd_indicator_value",
        "ahd_indicator_value_full",
        "ahd_indicator_caption",
    )
    val BAR_IDS = COMMON_IDS + "ahd_indicator_label"
    val ARC_IDS = COMMON_IDS + "ahd_indicator_icon"

    /** current/max as 0.0..1.0, or null when either is missing or the maximum is not positive. */
    fun fraction(current: Long?, max: Long?): Double? {
        if (current == null || max == null || max <= 0) return null
        return (current.toDouble() / max.toDouble()).coerceIn(0.0, 1.0)
    }

    fun fillLevel(current: Long?, max: Long?, nearFullPercent: Int = NEAR_FULL_PERCENT): AhdFillLevel {
        if (current == null || max == null || max <= 0) return AhdFillLevel.UNKNOWN
        return when {
            current >= max -> AhdFillLevel.FULL
            current * 100 >= max * nearFullPercent -> AhdFillLevel.NEAR_FULL
            else -> AhdFillLevel.NORMAL
        }
    }

    /**
     * Drawable level for a filled share of [span] (MAX_LEVEL for the bar, ARC_LEVEL for the arc).
     * Any value above zero shows at least 1% of the span, so "almost empty" never looks empty.
     */
    fun level(current: Long?, max: Long?, span: Int): Int {
        val share = fraction(current, max) ?: return 0
        if (share <= 0.0) return 0
        return (share * span).toInt().coerceIn(span / 100, span)
    }

    /** "120/200", "120" without a maximum, or NO_VALUE. */
    fun countText(current: Long?, max: Long?): String = when {
        current == null -> NO_VALUE
        max == null -> current.toString()
        else -> "$current/$max"
    }

    /**
     * Remaining seconds as (hours, minutes), rounding up so unfinished work never shows zero
     * (dashboard-ui/src/app/display.rs hours_minutes).
     */
    fun hoursMinutes(seconds: Long): Pair<Long, Long> {
        val minutes = (seconds.coerceAtLeast(0) + 59) / 60
        return Pair(minutes / 60, minutes % 60)
    }
}
