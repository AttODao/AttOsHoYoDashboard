package dev.dioxus.main

import android.appwidget.AppWidgetManager
import android.content.Context
import android.os.Bundle
import android.widget.RemoteViews

/**
 * Genshin Impact 3x3 home-screen widget: Original Resin as the main resource, then Realm
 * Currency, daily commissions (with the extra reward) and expeditions as tiles. Resized below
 * the regular size, or with a system font size the regular layout cannot hold
 * (GenshinWidgetLayout.forSize), the tiles become one-line rows. GenshinWidgetModel.kt decides
 * the values, AhdWidgetFrame.kt draws them (android/README.md "Home-screen widgets").
 */
class GenshinWidget : AhdGameWidget(WidgetGame.GENSHIN) {
    override fun render(context: Context, state: WidgetState, nowMs: Long, options: Bundle): RemoteViews = guarded(context) {
        val width = options.getInt(AppWidgetManager.OPTION_APPWIDGET_MIN_WIDTH)
        val height = options.getInt(AppWidgetManager.OPTION_APPWIDGET_MAX_HEIGHT)
        val layout = GenshinWidgetLayout.forSize(width, height, context.resources.configuration.fontScale)
        AhdWidgetFrame.render(context, game, layout.layout, layout.kind, width, height, state, nowMs) {
            GenshinWidgetModel.of(it.snapshot, it.refreshFailed, nowMs).content()
        }
    }
}
