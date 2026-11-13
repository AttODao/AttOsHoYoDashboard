package dev.dioxus.main

import java.util.Calendar
import java.util.TimeZone

// Pure rules and the "what to draw" types behind the home-screen widgets (AhdIndicators.kt
// draws them). No Android imports, so `test-android-kotlin` compiles and runs this file on the
// JVM (dashboard-ui/android/tests/AhdIndicatorModelTest.kt) and `test-android` covers the game
// models that build these types (android/test/).

/** The game whose colors a tile is drawn in: the theme overlay AhdGame… of layout/ahd_indicator_(tile|row)_<key>.xml. */
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

/**
 * The color a symbol or text is drawn in, as on the dashboard: DONE is mint for both games,
 * WARN the warning color, MUTED the quiet text color. Every tone comes with a symbol or a
 * word, so the color is never the only signal.
 */
enum class AhdTone(val key: String) {
    MUTED("muted"),
    WARN("warn"),
    DONE("done"),
}

/**
 * Glyphs of the widgets: drawable `ahd_indicator_icon_<key>`, each the path of the same icon in
 * dashboard-ui/src/pages/icons.rs ([icon] is its `Icon` variant; the JVM test compares the paths).
 */
enum class AhdGlyph(val key: String, val icon: String) {
    RESIN("resin", "Resin"),
    COIN("coin", "Coin"),
    DAILY("daily", "Daily"),
    EXPEDITION("expedition", "Flag"),
    POWER("power", "Bolt"),
    RESERVE("reserve", "Reserve"),
    WEEKLY("weekly", "Cube"),
    GIFT("gift", "Gift"),
    USER("user", "User"),
    LINK("link", "Link"),
    CHECK("check", "Check"),
    PENDING("pending", "Pending"),
    WARNING("warning", "Warning"),
    FAILED("failed", "Failed"),
    UNKNOWN("unknown", "Unknown"),
    HOURGLASS("hourglass", "Hourglass"),
    CLOCK("clock", "Clock");

    val drawable: String get() = "ahd_indicator_icon_$key"
}

/**
 * A status symbol with the word a screen reader hears ([word] is a string resource name), the
 * dashboard's `Status`: check = done or claimed, open circle = still to do, circled question
 * mark = no value. (The warning triangle belongs to caps and failed refreshes.)
 */
enum class AhdMark(val glyph: AhdGlyph, val tone: AhdTone, val word: String) {
    DONE(AhdGlyph.CHECK, AhdTone.DONE, "ahd_indicator_done"),
    GOAL_REACHED(AhdGlyph.CHECK, AhdTone.DONE, "ahd_indicator_goal_reached"),
    CLAIMED(AhdGlyph.CHECK, AhdTone.DONE, "ahd_indicator_claimed"),

    /** Not claimed yet, and nothing to act on (the commissions are not finished). */
    UNCLAIMED(AhdGlyph.PENDING, AhdTone.MUTED, "ahd_indicator_unclaimed"),

    /** Not claimed although everything is done: act now. */
    UNCLAIMED_DUE(AhdGlyph.PENDING, AhdTone.WARN, "ahd_indicator_unclaimed"),
    UNKNOWN(AhdGlyph.UNKNOWN, AhdTone.MUTED, "ahd_indicator_no_value"),
}

/** Small circles for a count out of a few: the first [filled] of [total] are on. */
data class AhdPips(val filled: Int, val total: Int)

/** The line under a tile's value (at the end of a compact row). */
sealed class AhdDetail {
    /** Hourglass and a duration; [near] (close to the cap) swaps in the warning symbol and color. */
    data class Time(val seconds: Long, val near: Boolean = false) : AhdDetail()

    /** Hourglass and "first〜last": the next and the last of several things to finish. */
    data class Range(val first: Long, val last: Long) : AhdDetail()

    /** Warning symbol and "上限". */
    object AtCap : AhdDetail()

    /** A status symbol with its word shown. */
    data class Status(val mark: AhdMark) : AhdDetail()

    /** The gift (extra commission reward) and its status symbol; the word is only spoken. */
    data class Reward(val mark: AhdMark) : AhdDetail()

    /**
     * A quiet text ([text] is a string resource): "none sent", or with [spoken] false a short
     * name where the icon alone would be ambiguous (the full name is spoken anyway).
     */
    data class Label(val text: String, val spoken: Boolean = true) : AhdDetail()
}

/**
 * One secondary value: icon and "current/max" ("—" when [current] is null), the icon inside a
 * small arc for a share, pips for a count, and one detail. [name] is the string resource of
 * the full name (spoken, not shown).
 */
data class AhdTile(
    val icon: AhdGlyph,
    val name: String,
    val current: Long?,
    val max: Long? = null,
    /** The value in the warning color (at a cap). */
    val warn: Boolean = false,
    /** Drawable level (0..ARC_LEVEL) of the arc around the icon, 0 = the track alone; null draws no arc. */
    val ring: Int? = null,
    /** The arc in the warning color (near or at a cap) instead of the game color. */
    val ringWarn: Boolean = false,
    val pips: AhdPips? = null,
    val detail: AhdDetail? = null,
) {
    companion object {
        /** A gauge whose cap wastes what comes after it: near and at the cap warn, as the dashboard tile does at the cap. */
        fun gauge(icon: AhdGlyph, name: String, current: Long?, max: Long?, secondsToFull: Long?): AhdTile {
            if (current == null) return unknown(icon, name, ring = true)
            val level = AhdIndicatorModel.fillLevel(current, max)
            return AhdTile(
                icon, name, current, max,
                warn = level == AhdFillLevel.FULL,
                ring = AhdIndicatorModel.level(current, max, AhdIndicatorModel.ARC_LEVEL),
                ringWarn = level == AhdFillLevel.NEAR_FULL || level == AhdFillLevel.FULL,
                detail = when {
                    level == AhdFillLevel.FULL -> AhdDetail.AtCap
                    secondsToFull != null -> AhdDetail.Time(secondsToFull, near = level == AhdFillLevel.NEAR_FULL)
                    else -> null
                },
            )
        }

        /** A score towards a goal: reaching it is done (check), never a warning. [label] shows until then. */
        fun score(icon: AhdGlyph, name: String, current: Long?, max: Long?, label: String?): AhdTile {
            if (current == null) return unknown(icon, name, ring = true)
            val reached = max != null && max > 0 && current >= max
            return AhdTile(
                icon, name, current, max,
                ring = AhdIndicatorModel.level(current, max, AhdIndicatorModel.ARC_LEVEL),
                detail = if (reached) AhdDetail.Status(AhdMark.GOAL_REACHED) else label?.let { AhdDetail.Label(it, spoken = false) },
            )
        }

        /** No value was returned: "—" and the unknown mark, never a number; a share keeps its empty arc ([ring]). */
        fun unknown(icon: AhdGlyph, name: String, ring: Boolean = false) =
            AhdTile(icon, name, null, ring = if (ring) 0 else null, detail = AhdDetail.Status(AhdMark.UNKNOWN))
    }
}

/** The main resource: the large arc with the icon and number inside, and the time to full beside it. */
data class AhdMain(
    val icon: AhdGlyph,
    /** String resource of the resource's name (spoken, not shown). */
    val name: String,
    val current: Long?,
    val max: Long?,
    /** Seconds until full; 0 at the cap, null without an estimate. */
    val secondsToFull: Long?,
) {
    val level: AhdFillLevel get() = AhdIndicatorModel.fillLevel(current, max)
}

/** Everything a game widget shows once it has data; AhdWidgetFrame draws it for both games. */
data class AhdWidgetContent(
    val main: AhdMain,
    /** Three or four tiles, in reading order. */
    val tiles: List<AhdTile>,
    val fetchedAtMs: Long,
    /** The last refresh failed: the values are the previous ones. */
    val refreshFailed: Boolean,
    /**
     * The regular layout's second column is wider (5:6): Star Rail's "14000/14000" beside its
     * arc needs more than half the width.
     */
    val wideEndColumn: Boolean = false,
)

/**
 * How the texts of the tiles fit a widget (AhdIndicatorModel.fit): [textScale] is the font
 * scale they are drawn at, and [columnsDp] the width each tile has (compact rows: the row).
 */
data class AhdFit(val textScale: Float, val columnsDp: List<Int>)

/** Layout chosen for a widget's size in dp (AppWidgetManager OPTION_APPWIDGET_* values) and the font size. */
enum class AhdWidgetLayout {
    /** Main resource as the 96dp arc with its time, then the tiles in two columns. */
    REGULAR,

    /** The same values as one-line rows, for a widget resized below the regular size or large text. */
    COMPACT;

    companion object {
        // The regular layout at the default font size: 12dp padding, the header, the 96dp arc
        // with the time beside it, and two rows of two tiles. The texts grow with the font
        // size, so the minimum does too; between this minimum and the size the design was
        // drawn at (272x296dp) AhdIndicatorModel.fit draws the tile texts a little smaller.
        const val REGULAR_MIN_WIDTH_DP = 210
        const val REGULAR_MIN_HEIGHT_DP = 250

        /**
         * Portrait size: the width is OPTION_APPWIDGET_MIN_WIDTH, the height MAX_HEIGHT. 0 = unknown
         * (regular). [fontScale] is Configuration.fontScale (values below 1 count as 1).
         */
        fun forSize(widthDp: Int, heightDp: Int, fontScale: Float = 1f): AhdWidgetLayout {
            val scale = fontScale.coerceAtLeast(1f)
            return when {
                widthDp <= 0 || heightDp <= 0 -> REGULAR
                widthDp >= REGULAR_MIN_WIDTH_DP * scale && heightDp >= REGULAR_MIN_HEIGHT_DP * scale -> REGULAR
                else -> COMPACT
            }
        }
    }
}

object AhdIndicatorModel {
    /**
     * Drawable level of a full arc: a ring's sweep is level/10000 of the circle
     * (Drawable.setLevel), the arcs are 270° (drawable/ahd_indicator_arc_*.xml).
     */
    const val ARC_LEVEL = 7_500

    /** Same rule as the dashboard (dashboard-ui/src/app/display.rs fill_level): "near" from 90%. */
    const val NEAR_FULL_PERCENT = 90

    /** Shown instead of a number when there is no value. */
    const val NO_VALUE = "—"

    /** More pips than the tile layouts hold stay a number only (layout ids ahd_indicator_pip_1..). */
    const val MAX_PIPS = 5

    /** The most tiles the widget bodies have slots for (layout ids ahd_widget_slot_1..). */
    const val MAX_TILES = 4

    /** From this font scale a compact widget has no room for the clock time beside the time left. */
    const val CLOCK_MAX_FONT_SCALE = 1.5f

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
     * Drawable level for a filled share of [span] (ARC_LEVEL for an arc).
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
     * Pips for a count out of a few, like the dashboard tiles (status_tiles.rs `pips`): nothing
     * for unknown, inconsistent or large counts, which stay a number only.
     */
    fun pips(filled: Int?, total: Int?): AhdPips? {
        if (filled == null || total == null) return null
        return if (total in 1..MAX_PIPS && filled in 0..total) AhdPips(filled, total) else null
    }

    /**
     * Remaining seconds as (hours, minutes), rounding up so unfinished work never shows zero
     * (dashboard-ui/src/app/display.rs hours_minutes).
     */
    fun hoursMinutes(seconds: Long): Pair<Long, Long> {
        val minutes = (seconds.coerceAtLeast(0) + 59) / 60
        return Pair(minutes / 60, minutes % 60)
    }

    /**
     * String resource for a duration of [hours] full hours: "10分" under an hour (the widgets
     * showed "0時間10分"), "3時間12分" otherwise. The dashboard keeps its own format.
     */
    fun durationString(hours: Long): String = if (hours == 0L) "ahd_indicator_minutes" else "ahd_indicator_hours_minutes"

    /**
     * Calendar days from [nowMs] to [thenMs] in [zone] (0 = today, 1 = tomorrow), for the small
     * clock time beside the time left: "23:18", "明日 13:14" (display.rs short_time).
     */
    fun dayOffset(nowMs: Long, thenMs: Long, zone: TimeZone): Int {
        fun day(ms: Long): Long {
            val calendar = Calendar.getInstance(zone)
            calendar.timeInMillis = ms
            // Days since the epoch of the local date, so DST changes do not shift the count.
            val local = Calendar.getInstance(TimeZone.getTimeZone("UTC"))
            local.clear()
            local.set(calendar.get(Calendar.YEAR), calendar.get(Calendar.MONTH), calendar.get(Calendar.DAY_OF_MONTH))
            return Math.floorDiv(local.timeInMillis, 86_400_000L)
        }
        return (day(thenMs) - day(nowMs)).toInt()
    }

    // ---- room for the tile texts ----
    //
    // The tile and row texts are sized from code (their dimens are dp): the system font scale,
    // but no larger than the widget has room for, so that no number is cut. Widths are
    // estimated from the platform font (Roboto: a bold digit is 0.58 of the text size, a
    // regular one 0.56, "/" 0.37; a full-width character 1.0) and the dp sizes of the layouts
    // (layout/ahd_indicator_tile.xml, ahd_indicator_row.xml, ahd_widget_body*.xml).

    /** The tile texts never get smaller than this share of their design size. */
    const val MIN_TEXT_SCALE = 0.8f

    /** Size the regular and the compact layout were designed at; used while the widget's size is unknown. */
    private const val REGULAR_WIDTH_DP = 272
    private const val REGULAR_HEIGHT_DP = 296
    private const val COMPACT_WIDTH_DP = 196
    private const val COMPACT_HEIGHT_DP = 208

    private const val SMALL_TEXT = 11.0
    private const val TIME_UNITS = 58.0 // "35時間12分" at 11
    private const val WORD_UNITS = 48.0 // "Complete", "Finished" at 11 bold ("完了" is 22)
    private const val RANGE_UNITS = 120.0 // "3時間20分〜11時間45分" at 11
    private const val LABEL_UNITS = 70.0 // "Sim. Universe" at 11 ("模擬宇宙" is 44)

    private fun digits(value: Long) = value.toString().length

    /** Width of "value/max" at font scale 1 with the value at [valueText] dp. */
    private fun countUnits(tile: AhdTile, valueText: Double): Double {
        val value = tile.current?.let { digits(it) * 0.58 * valueText } ?: valueText
        val max = if (tile.current != null && tile.max != null) 1 + (0.37 + digits(tile.max) * 0.56) * SMALL_TEXT else 0.0
        return value + max
    }

    /** Width of the detail's text at font scale 1 when it must not be cut (a short name may be). */
    private fun detailUnits(detail: AhdDetail?): Double = when (detail) {
        is AhdDetail.Time, is AhdDetail.Range -> TIME_UNITS
        AhdDetail.AtCap, is AhdDetail.Status -> WORD_UNITS
        is AhdDetail.Reward, is AhdDetail.Label, null -> 0.0
    }

    /**
     * The font scale for the tile texts of a widget of [widthDp] x [heightDp] (0 = unknown)
     * showing [content] in [layout] at the system [fontScale], and the width of each tile.
     */
    fun fit(layout: AhdWidgetLayout, widthDp: Int, heightDp: Int, content: AhdWidgetContent, fontScale: Float): AhdFit {
        val regular = layout == AhdWidgetLayout.REGULAR
        val width = if (widthDp > 0) widthDp else if (regular) REGULAR_WIDTH_DP else COMPACT_WIDTH_DP
        val height = if (heightDp > 0) heightDp else if (regular) REGULAR_HEIGHT_DP else COMPACT_HEIGHT_DP
        val tiles = content.tiles
        val columns: List<Int>
        var room = Double.MAX_VALUE
        // A short name gives way before a number does, but it only has to fit up to the default font size.
        var labelRoom = Double.MAX_VALUE
        if (regular) {
            // 12dp padding on both sides, 8dp between the columns.
            val inner = width - 24
            columns = tiles.indices.map { index ->
                val alone = index % 2 == 0 && index == tiles.lastIndex
                when {
                    alone -> inner
                    content.wideEndColumn -> (inner - 8) * (if (index % 2 == 0) 5 else 6) / 11
                    else -> (inner - 8) / 2
                }
            }
            // Beside the 36dp arc and its 6dp gap, or the 18dp icon and its 5dp gap.
            tiles.forEachIndexed { index, tile ->
                val fixed = if (tile.ring != null) 42 else 23
                room = minOf(room, (columns[index] - fixed) / countUnits(tile, 15.0))
                // The detail line under the count: 12dp symbol and 3dp gap.
                val detail = detailUnits(tile.detail)
                if (detail > 0) room = minOf(room, (columns[index] - (if (tile.ring != null) 57 else 15)) / detail)
                if (tile.detail is AhdDetail.Label) labelRoom = minOf(labelRoom, (columns[index] - (if (tile.ring != null) 42 else 0)) / LABEL_UNITS)
            }
            // Padding, header, arc and hairline are fixed; a row of tiles is two lines of text
            // and, with pips, 16dp more.
            val rows = tiles.chunked(2)
            val fixed = 151 + 18 * fontScale.coerceAtLeast(1f) + rows.sumOf { row -> if (row.any { it.pips != null }) 16.0 else 4.0 } + 8 * (rows.size - 1)
            if (rows.isNotEmpty()) room = minOf(room, (height - fixed) / (31.0 * rows.size))
        } else {
            // 10dp padding on both sides.
            columns = tiles.map { width - 20 }
            for (tile in tiles) {
                // The 22dp icon box and its gap, the pips, and before the detail a 4dp gap and its symbol.
                val pips = tile.pips?.let { 6 + 9 * it.total } ?: 0
                val fixed = 27 + pips + if (tile.detail != null) 19 else 0
                val gift = if (tile.detail is AhdDetail.Reward) 16 else 0
                room = minOf(room, (width - 20 - fixed - gift) / (countUnits(tile, 13.0) + detailUnits(tile.detail)))
                if (tile.detail is AhdDetail.Label) labelRoom = minOf(labelRoom, (width - 20 - 31 - pips) / (countUnits(tile, 13.0) + LABEL_UNITS))
            }
            // Rows are at least the 22dp of their arc; larger text makes them taller.
            val spare = height - 88 - 17 * fontScale.coerceAtLeast(1f)
            if (tiles.isNotEmpty()) room = minOf(room, maxOf(22.0 / 17, spare / (17.0 * tiles.size)))
        }
        // The estimate is not a measurement: keep 2% in reserve.
        val numbers = minOf(fontScale.toDouble(), maxOf(MIN_TEXT_SCALE.toDouble(), room * 0.98))
        val scale = minOf(numbers, maxOf(minOf(1.0, numbers), labelRoom * 0.98))
        return AhdFit(scale.toFloat(), columns)
    }

    /** Whether "first〜last" fits a tile [columnDp] wide at [textScale]; otherwise only the last time is shown. */
    fun showsRange(layout: AhdWidgetLayout, columnDp: Int, textScale: Float): Boolean =
        layout == AhdWidgetLayout.REGULAR && columnDp >= 15 + RANGE_UNITS * textScale

    /** Whether the clock time of "full at" is shown: always in the regular layout, in compact only at moderate font sizes. */
    fun showsClock(layout: AhdWidgetLayout, fontScale: Float): Boolean =
        layout == AhdWidgetLayout.REGULAR || fontScale < CLOCK_MAX_FONT_SCALE
}
