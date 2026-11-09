package dev.dioxus.main

import android.appwidget.AppWidgetManager
import android.content.Context
import android.os.Bundle
import android.util.Log
import android.view.View
import android.widget.RemoteViews
import java.text.DateFormat
import java.util.Date

/**
 * Honkai: Star Rail 3x3 home-screen widget. Regular (layout/ahd_widget_starrail.xml): Trailblaze
 * Power as an arc with the time to full, the reserve and the fetch time beside it, then
 * assignments, daily training and Simulated Universe points as full-width bars. Narrow widgets
 * or large system font sizes (StarRailWidgetLayout.forSize) get layout/ahd_widget_starrail_compact.xml:
 * the same values as four bars, the time to full as the power bar's caption. Signed out, no
 * account and no usable status show a message instead; a failed refresh keeps the last values
 * and marks the time. Values come from StarRailWidgetModel. See android/README.md "Home-screen widgets".
 */
class StarRailWidget : AhdGameWidget(WidgetGame.STARRAIL) {
    override fun render(context: Context, state: WidgetState, nowMs: Long, options: Bundle): RemoteViews = try {
        val layout = StarRailWidgetLayout.forSize(
            options.getInt(AppWidgetManager.OPTION_APPWIDGET_MIN_WIDTH),
            options.getInt(AppWidgetManager.OPTION_APPWIDGET_MAX_HEIGHT),
            context.resources.configuration.fontScale,
        )
        renderState(context, state, nowMs, layout)
    } catch (error: Exception) {
        // A broken resource must not crash the app process from a broadcast receiver.
        Log.w("AHD", "Star Rail widget render failed", error)
        placeholder(context, WidgetState.Unavailable)
    }

    private fun renderState(context: Context, state: WidgetState, nowMs: Long, layout: StarRailWidgetLayout): RemoteViews {
        val res = Names(context)
        val views = RemoteViews(context.packageName, res.layout(layout.layout))
        val message = when (state) {
            WidgetState.SignedOut -> "ahd_widget_sign_in"
            WidgetState.NoAccount -> "ahd_widget_no_account"
            WidgetState.Unavailable -> "ahd_widget_unavailable"
            is WidgetState.Ready -> null
        }
        // Texts from the layout are resolved by the launcher in the system locale; setting them
        // here keeps the whole widget in the app's language (AhdLocale).
        views.setTextViewText(res.id("ahd_widget_title"), res.text("ahd_widget_starrail_label"))
        views.setViewVisibility(res.id("ahd_widget_content"), if (message == null) View.VISIBLE else View.GONE)
        views.setViewVisibility(res.id("ahd_widget_message_box"), if (message == null) View.GONE else View.VISIBLE)
        if (message != null) {
            // Same message box as the Genshin widget: icon, what to do, and the tap hint.
            views.setTextViewText(res.id("ahd_widget_status"), res.text(message))
            show(views, res, "ahd_widget_hint", res.text("ahd_widget_open_hint"))
            show(views, res, "ahd_widget_updated", null)
            show(views, res, "ahd_widget_updated_full", null)
            return views
        }
        bind(context, views, res, StarRailWidgetModel.of(state as WidgetState.Ready, nowMs), layout)
        return views
    }

    private fun bind(context: Context, views: RemoteViews, res: Names, model: StarRailWidgetModel, layout: StarRailWidgetLayout) {
        val theme = AhdIndicatorTheme.STAR_RAIL
        fun duration(seconds: Long) = AhdIndicators.hoursMinutes(context, seconds)

        // Trailblaze Power: "上限" at the cap, otherwise the time to full, under a "全回復まで"
        // label in the regular layout and after it in the compact caption (like the Genshin widget).
        val untilFull = res.text("ahd_widget_until_full")
        val powerTime = if (model.powerFull) res.text("ahd_indicator_full") else model.powerFullInSeconds?.let(::duration)
        val powerCaption = powerTime?.let { if (model.powerFull) it else "$untilFull $it" }
        val powerDescription = listOfNotNull(
            res.text("ahd_widget_starrail_power"),
            AhdIndicatorModel.countText(model.power?.toLong(), model.powerMax?.toLong()),
            powerCaption,
        ).joinToString(" ")
        val powerSlot = res.id("ahd_widget_power")
        views.removeAllViews(powerSlot)
        when (layout) {
            StarRailWidgetLayout.REGULAR -> {
                // The value in the arc and "/max" in its open bottom; the time and reserve beside it.
                views.addView(powerSlot, AhdIndicators.arc(context, AhdArcSpec(
                    current = model.power?.toLong(), max = model.powerMax?.toLong(), theme = theme,
                    caption = model.powerMax?.let { "/$it" },
                    contentDescription = powerDescription,
                )))
                show(views, res, "ahd_widget_power_until", if (model.powerFull || powerTime == null) null else untilFull)
                swap(views, res, "ahd_widget_power_time", powerTime, warn = model.powerFull)
            }
            StarRailWidgetLayout.COMPACT -> views.addView(powerSlot, AhdIndicators.bar(context, AhdBarSpec(
                label = res.text("ahd_widget_starrail_power"),
                current = model.power?.toLong(), max = model.powerMax?.toLong(), theme = theme,
                caption = powerCaption,
                contentDescription = powerDescription,
            )))
        }
        swap(views, res, "ahd_widget_reserve", model.reserve?.let { res.text("ahd_widget_starrail_reserve", it) }, warn = model.reserveFull)

        // Assignments: done/dispatched and the time until all are done; all done is the "collect
        // now" warning. Regular: both on the value line (no caption row, so the widget stays
        // within 3x3). Compact: the time is the caption, otherwise the long value line leaves
        // the label only "…" at large fonts.
        val count = AhdIndicatorModel.countText(model.assignmentsDone?.toLong(), model.assignmentsTotal?.toLong())
        val assignmentsLeft = model.assignmentsLeftSeconds?.let(::duration)
        val assignmentsValue = when {
            model.noAssignments -> res.text("ahd_widget_starrail_no_assignments")
            model.assignmentsReady -> res.text("ahd_widget_starrail_all_done", count)
            layout == StarRailWidgetLayout.COMPACT -> count
            else -> assignmentsLeft?.let { res.text("ahd_widget_starrail_left", count, it) }
        }
        val assignmentsCaption = if (layout == StarRailWidgetLayout.COMPACT && !model.noAssignments && !model.assignmentsReady) assignmentsLeft else null
        bar(context, views, res, "ahd_widget_assignments", AhdBarSpec(
            label = res.text("ahd_widget_starrail_assignments"),
            current = model.assignmentsDone?.toLong(), max = model.assignmentsTotal?.toLong(), theme = theme,
            valueText = assignmentsValue,
            caption = assignmentsCaption,
            level = when {
                model.assignmentsTotal == null -> null
                model.assignmentsReady -> AhdFillLevel.FULL
                else -> AhdFillLevel.NORMAL
            },
        ))
        // Daily training and weekly points: reaching the cap is done, not a warning.
        bar(context, views, res, "ahd_widget_daily", AhdBarSpec(
            label = res.text("ahd_widget_starrail_daily"),
            current = model.daily?.toLong(), max = model.dailyMax?.toLong(), theme = theme,
            level = calm(model.daily, model.dailyMax),
        ))
        bar(context, views, res, "ahd_widget_weekly", AhdBarSpec(
            label = res.text("ahd_widget_starrail_weekly"),
            current = model.weekly?.toLong(), max = model.weeklyMax?.toLong(), theme = theme,
            level = calm(model.weekly, model.weeklyMax),
        ))

        // Same wording as the Genshin widget ("3:31 更新" / "更新失敗 3:40").
        val time = DateFormat.getTimeInstance(DateFormat.SHORT).format(Date(model.fetchedAtMs))
        val updated = res.text(if (model.refreshFailed) "ahd_widget_update_failed" else "ahd_widget_updated_at", time)
        swap(views, res, "ahd_widget_updated", updated, warn = model.refreshFailed)
    }

    /** NORMAL whenever a value exists, so a reached cap keeps the game color. */
    private fun calm(current: Int?, max: Int?): AhdFillLevel? =
        if (current != null && max != null && max > 0) AhdFillLevel.NORMAL else null

    private fun bar(context: Context, views: RemoteViews, res: Names, slot: String, spec: AhdBarSpec) {
        val id = res.id(slot)
        views.removeAllViews(id)
        views.addView(id, AhdIndicators.bar(context, spec))
    }

    /**
     * Each warnable line exists twice in the layout ([name] and [name]_full, the latter in the
     * warning color from values(-night)): resource colors follow dark mode when the launcher
     * re-inflates the widget, while RemoteViews.setTextColor would fix one color.
     */
    private fun swap(views: RemoteViews, res: Names, name: String, text: String?, warn: Boolean) {
        show(views, res, if (warn) "${name}_full" else name, text)
        show(views, res, if (warn) name else "${name}_full", null)
    }

    /** Shows [text] in the view called [name], or hides the view when there is no text. */
    private fun show(views: RemoteViews, res: Names, name: String, text: String?) {
        val id = res.id(name)
        if (text.isNullOrEmpty()) {
            views.setViewVisibility(id, View.GONE)
        } else {
            views.setTextViewText(id, text)
            views.setViewVisibility(id, View.VISIBLE)
        }
    }

    /** Name lookups (no `R.`, see android/README.md); a missing resource is a packaging error. */
    private class Names(private val context: Context) {
        private fun find(name: String, type: String): Int =
            context.resources.getIdentifier(name, type, context.packageName).also {
                check(it != 0) { "Missing $type resource $name (dashboard-ui/android/res)" }
            }
        fun id(name: String) = find(name, "id")
        fun layout(name: String) = find(name, "layout")
        fun text(name: String, vararg args: Any): String = context.getString(find(name, "string"), *args)
    }
}
