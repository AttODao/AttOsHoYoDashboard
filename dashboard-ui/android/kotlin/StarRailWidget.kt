package dev.dioxus.main

import android.content.Context
import android.util.Log
import android.view.View
import android.widget.RemoteViews
import java.text.DateFormat
import java.util.Date

/**
 * Honkai: Star Rail 3x3 home-screen widget (layout/ahd_widget_starrail.xml): Trailblaze Power as
 * an arc with the time to full, the reserve and the fetch time beside it, then assignments,
 * daily training and Simulated Universe points as full-width bars. Signed out, no account and no
 * usable status show a message instead; a failed refresh keeps the last values and marks the
 * time. Values come from StarRailWidgetModel. See android/README.md "Home-screen widgets".
 */
class StarRailWidget : AhdGameWidget(WidgetGame.STARRAIL) {
    override fun render(context: Context, state: WidgetState, nowMs: Long): RemoteViews = try {
        renderState(context, state, nowMs)
    } catch (error: Exception) {
        // A broken resource must not crash the app process from a broadcast receiver.
        Log.w("AHD", "Star Rail widget render failed", error)
        placeholder(context, WidgetState.Unavailable)
    }

    private fun renderState(context: Context, state: WidgetState, nowMs: Long): RemoteViews {
        val res = Names(context)
        val views = RemoteViews(context.packageName, res.layout("ahd_widget_starrail"))
        val message = when (state) {
            WidgetState.SignedOut -> "ahd_widget_sign_in"
            WidgetState.NoAccount -> "ahd_widget_no_account"
            WidgetState.Unavailable -> "ahd_widget_unavailable"
            is WidgetState.Ready -> null
        }
        // Texts from the layout are resolved by the launcher in the system locale; setting them
        // here keeps the whole widget in the app's locale (per-app language, API 33+).
        views.setTextViewText(res.id("ahd_widget_title"), res.text("ahd_widget_starrail_label"))
        views.setViewVisibility(res.id("ahd_widget_content"), if (message == null) View.VISIBLE else View.GONE)
        views.setViewVisibility(res.id("ahd_widget_status"), if (message == null) View.GONE else View.VISIBLE)
        if (message != null) {
            views.setTextViewText(res.id("ahd_widget_status"), res.text(message))
            show(views, res, "ahd_widget_updated", null)
            show(views, res, "ahd_widget_updated_full", null)
            return views
        }
        bind(context, views, res, StarRailWidgetModel.of(state as WidgetState.Ready, nowMs))
        return views
    }

    private fun bind(context: Context, views: RemoteViews, res: Names, model: StarRailWidgetModel) {
        val theme = AhdIndicatorTheme.STAR_RAIL
        fun duration(seconds: Long) = AhdIndicators.hoursMinutes(context, seconds)

        // Trailblaze Power: the value in the arc, "/max" in its open bottom, the time to full and
        // the reserve beside it.
        val fullText = if (model.powerFull) res.text("ahd_indicator_full")
        else model.powerFullInSeconds?.let { res.text("ahd_widget_starrail_full_in", duration(it)) }
        val powerSlot = res.id("ahd_widget_power")
        views.removeAllViews(powerSlot)
        views.addView(powerSlot, AhdIndicators.arc(context, AhdArcSpec(
            current = model.power?.toLong(), max = model.powerMax?.toLong(), theme = theme,
            caption = model.powerMax?.let { "/$it" },
            contentDescription = listOfNotNull(
                res.text("ahd_widget_starrail_power"),
                AhdIndicatorModel.countText(model.power?.toLong(), model.powerMax?.toLong()),
                fullText,
            ).joinToString(" "),
        )))
        swap(views, res, "ahd_widget_power_time", fullText, warn = model.powerFull)
        swap(views, res, "ahd_widget_reserve", model.reserve?.let { res.text("ahd_widget_starrail_reserve", it) }, warn = model.reserveFull)

        // Assignments: done/dispatched and the time until all are done on the value line (no
        // caption row, so the widget stays within 3x3 at large display and font sizes); all
        // done is the "collect now" warning.
        val count = AhdIndicatorModel.countText(model.assignmentsDone?.toLong(), model.assignmentsTotal?.toLong())
        val assignmentsValue = when {
            model.noAssignments -> res.text("ahd_widget_starrail_no_assignments")
            model.assignmentsReady -> res.text("ahd_widget_starrail_all_done", count)
            else -> model.assignmentsLeftSeconds?.let { res.text("ahd_widget_starrail_left", count, duration(it)) }
        }
        bar(context, views, res, "ahd_widget_assignments", AhdBarSpec(
            label = res.text("ahd_widget_starrail_assignments"),
            current = model.assignmentsDone?.toLong(), max = model.assignmentsTotal?.toLong(), theme = theme,
            valueText = assignmentsValue,
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

        val time = DateFormat.getTimeInstance(DateFormat.SHORT).format(Date(model.fetchedAtMs))
        val updated = if (model.refreshFailed) res.text("ahd_widget_starrail_refresh_failed", time)
        else res.text("ahd_widget_placeholder_ready", time)
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
