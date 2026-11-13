package dev.dioxus.main

import android.content.Context
import android.util.TypedValue
import android.view.Gravity
import android.view.View
import android.widget.RemoteViews

// The parts both home-screen widgets are built from, as RemoteViews: the tile of a secondary
// value (AhdIndicators.tile) and the lookups and small helpers AhdWidgetFrame.kt shares with it.
// What a tile shows is decided without Android in AhdIndicatorModel.kt (AhdTile).
//
// Drawing: RemoteViews only inflate framework views (no custom View with onDraw). An arc is a
// ring drawable in a plain ImageView whose sweep is ImageView.setImageLevel (remotable since
// API 1; ProgressBar colors can only be set from code since API 31, the app supports API 24).
// Colors come from res/values(-night) through android:tint and android:textColor in the
// layouts, so they follow dark mode when the launcher re-inflates a widget (a color set from
// code, or a bitmap, would keep the old mode until the next update). The game's colors are
// theme attributes set on each layout root (values/ahd_indicator_styles.xml AhdGame…); a part
// whose color depends on the state exists once per color in the layout ("…_accent", "…_warn",
// …) and the code only shows one (AhdViews.pick). This file must not use `R.` (the res overlay
// is added after dx's first Gradle run): resources are looked up by name.

/** Name lookups (no `R.`); a missing resource is a packaging error, so fail loudly. */
internal class AhdRes(val context: Context) {
    private fun find(name: String, type: String): Int =
        context.resources.getIdentifier(name, type, context.packageName).also {
            check(it != 0) { "Missing $type resource $name (dashboard-ui/android/res)" }
        }
    fun id(name: String) = find(name, "id")
    fun layout(name: String) = find(name, "layout")
    fun drawable(name: String) = find(name, "drawable")
    /** A dimension in dp, whatever unit it is written in at the default font size. */
    fun dp(name: String): Float = context.resources.getDimension(find(name, "dimen")) / context.resources.displayMetrics.density
    fun text(name: String, vararg args: Any): String = context.getString(find(name, "string"), *args)
}

/** A RemoteViews with the few operations the widgets need, by view name. */
internal class AhdViews(val res: AhdRes, layout: String) {
    val views = RemoteViews(res.context.packageName, res.layout(layout))

    fun show(name: String, visible: Boolean) = views.setViewVisibility(res.id(name), if (visible) View.VISIBLE else View.GONE)

    /** Shows [value] in the view [name], or hides the view when there is no text. */
    fun text(name: String, value: String?) {
        if (!value.isNullOrEmpty()) views.setTextViewText(res.id(name), value)
        show(name, !value.isNullOrEmpty())
    }

    fun image(name: String, glyph: AhdGlyph) = views.setImageViewResource(res.id(name), res.drawable(glyph.drawable))

    fun level(name: String, level: Int) = views.setInt(res.id(name), "setImageLevel", level)

    /** Draws the text of [name] at the dp size [dimen] times [scale] (AhdIndicatorModel.fit). */
    fun textSize(name: String, dimen: String, scale: Float) =
        views.setTextViewTextSize(res.id(name), TypedValue.COMPLEX_UNIT_DIP, res.dp(dimen) * scale)

    /**
     * The view exists once per color as "[base]_<key>" for every key in [keys]: shows the
     * [chosen] one, hides the rest (all of them for null) and returns the shown view's name.
     */
    fun pick(base: String, keys: List<String>, chosen: String?): String? {
        for (key in keys) show("${base}_$key", key == chosen)
        return chosen?.let { "${base}_$it" }
    }

    /** An arc's fill, once per color as "[base]_<key>": [chosen] at [level] (0..ARC_LEVEL), none when empty. */
    fun fill(base: String, keys: List<String>, chosen: String?, level: Int) {
        pick(base, keys, chosen?.takeIf { level > 0 })?.let { level(it, level) }
    }
}

object AhdIndicators {
    /** The colors an arc's fill and the main number exist in. */
    internal val FILLS = listOf("accent", "warn")
    private val TONES = AhdTone.values().map { it.key }
    private val VALUES = listOf("ahd_indicator_value", "ahd_indicator_value_warn", "ahd_indicator_value_none")

    /**
     * The tile of a secondary value as a RemoteViews to add into a slot of the widget:
     * layout/ahd_indicator_tile.xml, or the one-line layout/ahd_indicator_row.xml for [compact],
     * in the colors of [theme]. Its texts are drawn at [textScale]; [range] says whether a
     * tile showing two times has room for both (AhdIndicatorModel.fit, showsRange).
     */
    @JvmStatic fun tile(context: Context, tile: AhdTile, theme: AhdIndicatorTheme, compact: Boolean, textScale: Float, range: Boolean): RemoteViews {
        val ui = AhdViews(AhdRes(context), "ahd_indicator_${if (compact) "row" else "tile"}_${theme.key}")

        // A share has its icon inside the arc, anything else the plain icon.
        val ring = tile.ring
        ui.show("ahd_indicator_ring", ring != null)
        ui.show("ahd_indicator_icon", ring == null)
        ui.image(if (ring != null) "ahd_indicator_ring_icon" else "ahd_indicator_icon", tile.icon)
        ui.fill("ahd_indicator_fill", FILLS, if (tile.ringWarn) "warn" else "accent", ring ?: 0)
        // An arc and the two lines beside it are centered in the row of tiles; a count starts at the top.
        if (!compact && ring != null) ui.views.setInt(ui.res.id("ahd_indicator_root"), "setGravity", Gravity.CENTER_VERTICAL)

        // "value" is the normal text color, "value_warn" and "value_none" its twins.
        val value = when {
            tile.current == null -> "ahd_indicator_value_none"
            tile.warn -> "ahd_indicator_value_warn"
            else -> "ahd_indicator_value"
        }
        val valueText = if (compact) "ahd_indicator_value_text_compact" else "ahd_indicator_value_text"
        for (name in VALUES) {
            ui.text(name, if (name == value) tile.current?.toString() ?: AhdIndicatorModel.NO_VALUE else null)
            ui.textSize(name, valueText, textScale)
        }
        ui.text("ahd_indicator_max", if (tile.current != null && tile.max != null) "/${tile.max}" else null)
        ui.textSize("ahd_indicator_max", "ahd_indicator_small_text", textScale)

        ui.show("ahd_indicator_pips", tile.pips != null)
        for (index in 1..AhdIndicatorModel.MAX_PIPS) {
            val name = "ahd_indicator_pip_$index"
            val pips = tile.pips
            ui.show(name, pips != null && index <= pips.total)
            if (pips != null && index <= pips.total) ui.level(name, if (index <= pips.filled) 1 else 0)
        }

        val detail = shown(context, tile.detail, range)
        ui.show("ahd_indicator_gift", detail.gift)
        ui.pick("ahd_indicator_detail_icon", TONES, detail.glyph?.let { detail.tone.key })?.let { ui.image(it, detail.glyph!!) }
        for (tone in TONES) {
            ui.text("ahd_indicator_detail_$tone", detail.text.takeIf { tone == detail.tone.key })
            ui.textSize("ahd_indicator_detail_$tone", "ahd_indicator_small_text", textScale)
        }
        return ui.views
    }

    /** A detail as what the tile draws: an optional symbol, an optional text and their color. */
    private class Shown(val glyph: AhdGlyph?, val text: String?, val tone: AhdTone, val gift: Boolean = false)

    private fun shown(context: Context, detail: AhdDetail?, range: Boolean): Shown {
        val res = AhdRes(context)
        return when (detail) {
            null -> Shown(null, null, AhdTone.MUTED)
            is AhdDetail.Time ->
                if (detail.near) Shown(AhdGlyph.WARNING, hoursMinutes(context, detail.seconds), AhdTone.WARN)
                else Shown(AhdGlyph.HOURGLASS, hoursMinutes(context, detail.seconds), AhdTone.MUTED)
            // Without room for two durations: the time until all are back (both are spoken).
            is AhdDetail.Range -> Shown(
                AhdGlyph.HOURGLASS,
                if (range) res.text("ahd_widget_range", hoursMinutes(context, detail.first), hoursMinutes(context, detail.last))
                else hoursMinutes(context, detail.last),
                AhdTone.MUTED,
            )
            AhdDetail.AtCap -> Shown(AhdGlyph.WARNING, res.text("ahd_indicator_full"), AhdTone.WARN)
            is AhdDetail.Status -> Shown(detail.mark.glyph, res.text(detail.mark.word), detail.mark.tone)
            is AhdDetail.Reward -> Shown(detail.mark.glyph, null, detail.mark.tone, gift = true)
            is AhdDetail.Label -> Shown(null, res.text(detail.text), AhdTone.MUTED)
        }
    }

    /** What a screen reader hears for [tile]: the full name, the value and the detail in words. */
    @JvmStatic fun spoken(context: Context, tile: AhdTile): String {
        val res = AhdRes(context)
        val detail = when (val detail = tile.detail) {
            null -> null
            is AhdDetail.Time -> listOfNotNull(
                res.text("ahd_indicator_near_full").takeIf { detail.near },
                res.text("ahd_widget_full_in", hoursMinutes(context, detail.seconds)),
            ).joinToString(" ")
            is AhdDetail.Range -> res.text("ahd_widget_range_spoken", hoursMinutes(context, detail.first), hoursMinutes(context, detail.last))
            AhdDetail.AtCap -> res.text("ahd_indicator_full")
            is AhdDetail.Status -> res.text(detail.mark.word)
            is AhdDetail.Reward -> "${res.text("ahd_widget_genshin_extra_reward")} ${res.text(detail.mark.word)}"
            is AhdDetail.Label -> res.text(detail.text).takeIf { detail.spoken }
        }
        val value = tile.current?.let { AhdIndicatorModel.countText(it, tile.max) }
        return listOfNotNull(res.text(tile.name), value, detail).joinToString(" ")
    }

    /** "3h 12m" / "3時間12分" ("12m" / "12分" under an hour) in the context's locale, rounded up like the dashboard. */
    @JvmStatic fun hoursMinutes(context: Context, seconds: Long): String {
        val (hours, minutes) = AhdIndicatorModel.hoursMinutes(seconds)
        val res = AhdRes(context)
        val format = AhdIndicatorModel.durationString(hours)
        return if (hours == 0L) res.text(format, minutes) else res.text(format, hours, minutes)
    }
}
