package dev.dioxus.main

import android.content.Context
import android.text.format.DateFormat
import android.widget.RemoteViews
import java.util.Date
import java.util.TimeZone

/**
 * Draws a game widget (GenshinWidget.kt, StarRailWidget.kt) in the look of the dashboard's game
 * cards: the header (game mark, name, fetch time), then either a message (signed out, no
 * account, no status) or the content: the main resource as an arc with its icon and number
 * inside, beside it the hourglass with the time left and the clock time, and the other values
 * as tiles (AhdIndicators.tile). Both games share layout/ahd_widget_body(_compact).xml; what is
 * shown is an AhdWidgetContent built without Android by the game's model.
 *
 * Status is always a symbol plus words: the words are on screen where they fit and always in
 * the widget's content description, which names every value (the parts themselves are hidden
 * from accessibility, so the widget is read as one item). Texts are set from here in the app's
 * language ([context] is AhdLocale.localized); the launcher would resolve layout texts in the
 * system locale. No `R.` (android/README.md).
 */
object AhdWidgetFrame {
    private val MAIN_VALUES = AhdIndicators.FILLS + "none"

    /** The dp size (at the default font size) of the small texts that are sized from code. */
    private const val SMALL_TEXT = "ahd_indicator_small_text"

    /**
     * [layout] is the widget's layout resource name for [kind]; [widthDp] x [heightDp] is the
     * widget's size (0 = unknown); [content] turns a ready state into what to draw at [nowMs]
     * and is only called for one.
     */
    fun render(
        context: Context, game: WidgetGame, layout: String, kind: AhdWidgetLayout, widthDp: Int, heightDp: Int,
        state: WidgetState, nowMs: Long, content: (WidgetState.Ready) -> AhdWidgetContent,
    ): RemoteViews {
        val res = AhdRes(context)
        val ui = AhdViews(res, layout)
        val title = res.text("ahd_widget_${game.key}_label")
        ui.text("ahd_widget_title", title)
        val spoken = when (state) {
            WidgetState.SignedOut -> message(ui, AhdGlyph.USER, "ahd_widget_sign_in")
            WidgetState.NoAccount -> message(ui, AhdGlyph.LINK, "ahd_widget_no_account")
            WidgetState.Unavailable -> message(ui, AhdGlyph.FAILED, "ahd_widget_unavailable")
            is WidgetState.Ready -> content(ui, game, kind, widthDp, heightDp, content(state), nowMs)
        }
        ui.views.setContentDescription(res.id("ahd_widget_root"), (listOf(title) + spoken).joinToString(", "))
        return ui.views
    }

    /** No data: a symbol for the reason, what to do, and the tap hint. */
    private fun message(ui: AhdViews, glyph: AhdGlyph, text: String): List<String> {
        ui.show("ahd_widget_content", false)
        ui.show("ahd_widget_message_box", true)
        ui.show("ahd_widget_message_icon", true)
        ui.image("ahd_widget_message_icon", glyph)
        ui.text("ahd_widget_status", ui.res.text(text))
        ui.text("ahd_widget_hint", ui.res.text("ahd_widget_open_hint"))
        fetched(ui, null, false)
        return listOf(ui.res.text(text))
    }

    private fun content(
        ui: AhdViews, game: WidgetGame, kind: AhdWidgetLayout, widthDp: Int, heightDp: Int, content: AhdWidgetContent, nowMs: Long,
    ): List<String> {
        val context = ui.res.context
        val theme = if (game == WidgetGame.GENSHIN) AhdIndicatorTheme.GENSHIN else AhdIndicatorTheme.STAR_RAIL
        val compact = kind == AhdWidgetLayout.COMPACT
        ui.show("ahd_widget_message_box", false)
        ui.show("ahd_widget_content", true)
        // The small texts follow the font size as far as the widget has room (the title and
        // the time left shrink by themselves, autoSize).
        val fit = AhdIndicatorModel.fit(kind, widthDp, heightDp, content, context.resources.configuration.fontScale)
        val spoken = mutableListOf(main(ui, kind, content.main, nowMs, fit.textScale))

        // The regular body has its slots twice: in equal columns and in 5:6.
        val wide = !compact && content.wideEndColumn
        if (!compact) {
            ui.show("ahd_widget_tiles", !wide)
            ui.show("ahd_widget_tiles_wide", wide)
        }
        for (index in 0 until AhdIndicatorModel.MAX_TILES) {
            val slot = "ahd_widget_${if (wide) "wide_slot" else "slot"}_${index + 1}"
            val tile = content.tiles.getOrNull(index)
            ui.show(slot, tile != null)
            ui.views.removeAllViews(ui.res.id(slot))
            if (tile != null) {
                val range = AhdIndicatorModel.showsRange(kind, fit.columnsDp[index], fit.textScale)
                ui.views.addView(ui.res.id(slot), AhdIndicators.tile(context, tile, theme, compact, fit.textScale, range))
                spoken += AhdIndicators.spoken(context, tile)
            }
        }
        val clock = DateFormat.getTimeFormat(context).format(Date(content.fetchedAtMs))
        fetched(ui, clock, content.refreshFailed)
        for (name in listOf("ahd_widget_fetched", "ahd_widget_fetched_warn")) ui.textSize(name, SMALL_TEXT, fit.textScale)
        spoken += ui.res.text(if (content.refreshFailed) "ahd_widget_update_failed" else "ahd_widget_fetched_at", clock)
        return spoken
    }

    /**
     * The fetch time at the end of the header: clock + time, or after a failed refresh the
     * warning symbol + the time of the values still shown, in the warning color.
     */
    private fun fetched(ui: AhdViews, clock: String?, failed: Boolean) {
        ui.show("ahd_widget_fetched_icon", clock != null && !failed)
        ui.text("ahd_widget_fetched", clock.takeIf { !failed })
        ui.show("ahd_widget_fetched_warn_icon", clock != null && failed)
        ui.text("ahd_widget_fetched_warn", clock.takeIf { failed })
    }

    /** The main resource; returns its spoken text. */
    private fun main(ui: AhdViews, kind: AhdWidgetLayout, main: AhdMain, nowMs: Long, textScale: Float): String {
        val res = ui.res
        val context = res.context
        val compact = kind == AhdWidgetLayout.COMPACT
        val level = main.level
        val full = level == AhdFillLevel.FULL
        val near = level == AhdFillLevel.NEAR_FULL
        ui.image("ahd_widget_main_icon", main.icon)

        // The number in the game color, in the warning color at the cap, "—" without a value;
        // the arc in the warning color from "near the cap" on, its track alone without a value.
        val value = when {
            main.current == null -> "none"
            full -> "warn"
            else -> "accent"
        }
        for (key in MAIN_VALUES) {
            ui.text("ahd_widget_main_value_$key", if (key == value) main.current?.toString() ?: AhdIndicatorModel.NO_VALUE else null)
        }
        ui.text("ahd_widget_main_max", if (main.current != null && main.max != null) "/${main.max}" else null)
        ui.fill(
            "ahd_widget_main_fill", AhdIndicators.FILLS, if (near || full) "warn" else "accent",
            AhdIndicatorModel.level(main.current, main.max, AhdIndicatorModel.ARC_LEVEL),
        )

        // At the cap the badge "上限" alone. Near the cap the regular layout has the badge
        // with its words above the time, the compact one the warning symbol alone beside the
        // clock time (the words are spoken).
        ui.show("ahd_widget_main_badge", full || (near && !compact))
        ui.text("ahd_widget_main_badge_text", res.text(if (full) "ahd_indicator_full" else "ahd_indicator_near_full"))
        if (compact) ui.show("ahd_widget_main_near", near)

        // Hourglass + "あと1時間52分" and the small clock time; nothing at the cap (the badge
        // says it); the unknown symbol + "データなし" without a value.
        val seconds = main.secondsToFull.takeIf { main.current != null && !full }
        val left = seconds?.let { res.text("ahd_widget_full_in", AhdIndicators.hoursMinutes(context, it)) }
        val fontScale = context.resources.configuration.fontScale
        val at = seconds?.let { clockTime(context, nowMs, nowMs + it * 1000) }
        ui.show("ahd_widget_main_time_row", left != null)
        ui.text("ahd_widget_main_time", left)
        ui.text("ahd_widget_main_clock", at.takeIf { AhdIndicatorModel.showsClock(kind, fontScale) })
        ui.show("ahd_widget_main_unknown", main.current == null)
        ui.text("ahd_widget_main_time_none", res.text("ahd_indicator_no_value").takeIf { main.current == null })
        // The compact layout's clock time shrinks by itself (autoSize) instead.
        for (name in listOfNotNull("ahd_widget_main_badge_text", "ahd_widget_main_time_none", "ahd_widget_main_clock".takeIf { !compact })) {
            ui.textSize(name, SMALL_TEXT, textScale)
        }

        return listOfNotNull(
            res.text(main.name),
            if (main.current == null) res.text("ahd_indicator_no_value") else AhdIndicatorModel.countText(main.current, main.max),
            res.text("ahd_indicator_near_full").takeIf { near },
            res.text("ahd_indicator_full").takeIf { full },
            left, at,
        ).joinToString(" ")
    }

    /** "23:18", "明日 13:14", or "11/15 1:40" further ahead, in the device's time zone and 12/24-hour setting. */
    private fun clockTime(context: Context, nowMs: Long, thenMs: Long): String {
        val clock = DateFormat.getTimeFormat(context).format(Date(thenMs))
        return when (AhdIndicatorModel.dayOffset(nowMs, thenMs, TimeZone.getDefault())) {
            0 -> clock
            1 -> AhdRes(context).text("ahd_widget_tomorrow", clock)
            else -> "${DateFormat.format("MM/dd", thenMs)} $clock"
        }
    }
}
