package dev.dioxus.main

import android.appwidget.AppWidgetManager
import android.content.Context
import android.os.Bundle
import android.widget.RemoteViews

/**
 * Honkai: Star Rail 3x3 home-screen widget: Trailblaze Power as the main resource, then the
 * reserve, assignments, daily training and Simulated Universe points as tiles. Narrow widgets
 * or large system font sizes (StarRailWidgetLayout.forSize) get one-line rows instead.
 * StarRailWidgetModel.kt decides the values, AhdWidgetFrame.kt draws them (android/README.md
 * "Home-screen widgets").
 */
class StarRailWidget : AhdGameWidget(WidgetGame.STARRAIL) {
    override fun render(context: Context, state: WidgetState, nowMs: Long, options: Bundle): RemoteViews = guarded(context) {
        val width = options.getInt(AppWidgetManager.OPTION_APPWIDGET_MIN_WIDTH)
        val height = options.getInt(AppWidgetManager.OPTION_APPWIDGET_MAX_HEIGHT)
        val layout = StarRailWidgetLayout.forSize(width, height, context.resources.configuration.fontScale)
        AhdWidgetFrame.render(context, game, layout.layout, layout.kind, width, height, state, nowMs) {
            StarRailWidgetModel.of(it, nowMs).content()
        }
    }
}
