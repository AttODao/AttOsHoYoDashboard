package dev.dioxus.main

import android.appwidget.AppWidgetManager
import android.content.Context
import android.os.Bundle
import android.text.format.DateFormat
import android.view.View
import android.widget.RemoteViews
import java.util.Date

/**
 * Genshin Impact 3x3 home-screen widget: Original Resin as the large arc, Realm Currency,
 * expeditions and daily commissions below (GenshinWidgetModel.kt decides the values, the
 * indicators come from AhdIndicators.kt). Resized below the regular size, or with a system font
 * size the regular layout cannot hold (GenshinWidgetLayout.forSize), it shows four bars.
 * Resources are looked up by name: no `R.` (android/README.md "Home-screen widgets").
 */
class GenshinWidget : AhdGameWidget(WidgetGame.GENSHIN) {
    override fun render(context: Context, state: WidgetState, nowMs: Long, options: Bundle): RemoteViews {
        val layout = GenshinWidgetLayout.forSize(
            options.getInt(AppWidgetManager.OPTION_APPWIDGET_MIN_WIDTH),
            options.getInt(AppWidgetManager.OPTION_APPWIDGET_MAX_HEIGHT),
            context.resources.configuration.fontScale,
        )
        val ui = Ui(context, RemoteViews(context.packageName, find(context, layout.layout, "layout")))
        when (state) {
            WidgetState.SignedOut -> ui.message("ahd_widget_sign_in")
            WidgetState.NoAccount -> ui.message("ahd_widget_no_account")
            WidgetState.Unavailable -> ui.message("ahd_widget_unavailable")
            is WidgetState.Ready -> {
                val model = GenshinWidgetModel.of(state.snapshot, state.refreshFailed, nowMs)
                ui.content(model)
                if (layout == GenshinWidgetLayout.REGULAR) ui.regular(model) else ui.compact(model)
            }
        }
        return ui.views
    }

    private class Ui(val context: Context, val views: RemoteViews) {
        fun id(name: String) = find(context, name, "id")
        fun text(name: String, vararg args: Any): String = context.getString(find(context, name, "string"), *args)
        fun time(seconds: Long) = AhdIndicators.hoursMinutes(context, seconds)
        fun shown(name: String, visible: Boolean) = views.setViewVisibility(id(name), if (visible) View.VISIBLE else View.GONE)

        /** Sets [value] on `name` or, when [warn], on its `name_warn` twin (colors stay resources: dark mode). */
        fun warnText(name: String, value: String?, warn: Boolean) {
            shown(name, value != null && !warn)
            shown("${name}_warn", value != null && warn)
            if (value != null) views.setTextViewText(id(if (warn) "${name}_warn" else name), value)
        }

        fun slot(name: String, child: RemoteViews) {
            views.removeAllViews(id(name))
            views.addView(id(name), child)
        }

        fun message(key: String) {
            shown("ahd_widget_content", false)
            shown("ahd_widget_message_box", true)
            shown("ahd_widget_hint", true)
            views.setTextViewText(id("ahd_widget_status"), text(key))
            warnText("ahd_widget_updated", null, false)
        }

        /** Header and the switch from the message to the values (both layouts). */
        fun content(model: GenshinWidgetModel) {
            shown("ahd_widget_message_box", false)
            shown("ahd_widget_content", true)
            val clock = DateFormat.getTimeFormat(context).format(Date(model.fetchedAtMs))
            warnText(
                "ahd_widget_updated",
                text(if (model.refreshFailed) "ahd_widget_update_failed" else "ahd_widget_updated_at", clock),
                model.refreshFailed,
            )
        }

        fun regular(model: GenshinWidgetModel) {
            val resin = model.resin
            slot("ahd_widget_resin_slot", AhdIndicators.arc(context, AhdArcSpec(
                current = resin?.current?.toLong(), max = resin?.max?.toLong(), theme = AhdIndicatorTheme.GENSHIN,
                size = AhdArcSize.LARGE,
                contentDescription = listOfNotNull(text("ahd_widget_genshin_resin"), resinValue(model), resinTime(model)).joinToString(" "),
            )))
            warnText("ahd_widget_resin_value", resinValue(model) ?: AhdIndicatorModel.NO_VALUE, model.resinFull)
            shown("ahd_widget_resin_until", !model.resinFull && model.resinSeconds != null)
            warnText("ahd_widget_resin_time", resinTime(model), model.resinFull)

            val coin = model.coin
            item("coin", "ahd_indicator_icon_coin", "ahd_widget_genshin_coin",
                coin?.current?.toLong(), coin?.max?.toLong(), null,
                coin?.let { "${it.current}/${it.max}" }, model.coinFull, coinTime(model), null)

            val expeditions = model.expeditions
            item("expedition", "ahd_indicator_icon_expedition", "ahd_widget_genshin_expeditions",
                expeditions?.finished?.toLong(), expeditions?.total?.toLong(), expeditions?.level,
                expeditions?.let { "${it.finished}/${it.total}" }, expeditions?.allDone == true,
                expeditionNext(expeditions), expeditions?.lastSeconds?.let { text("ahd_widget_expedition_last", time(it)) })

            val daily = model.daily
            item("daily", "ahd_indicator_icon_daily", "ahd_widget_genshin_daily",
                daily?.current?.toLong(), daily?.max?.toLong(), model.dailyLevel,
                daily?.let { "${it.current}/${it.max}" }, model.dailyLevel == AhdFillLevel.NEAR_FULL, dailyReward(model), null)
        }

        /** One column below the resin: small arc with the icon, label, value and up to two details. */
        private fun item(
            name: String, icon: String, label: String, current: Long?, max: Long?, level: AhdFillLevel?,
            value: String?, warn: Boolean, detail: String?, detail2: String?,
        ) {
            val prefix = "ahd_widget_$name"
            val labelText = text(label)
            slot("${prefix}_slot", AhdIndicators.arc(context, AhdArcSpec(
                current = current, max = max, theme = AhdIndicatorTheme.GENSHIN, size = AhdArcSize.SMALL,
                icon = AhdIndicators.icon(context, icon), level = level,
                contentDescription = listOfNotNull(labelText, value, detail, detail2).joinToString(" "),
            )))
            views.setTextViewText(id("${prefix}_label"), labelText)
            warnText("${prefix}_value", value ?: AhdIndicatorModel.NO_VALUE, warn)
            warnText("${prefix}_detail", detail, warn && detail2 == null)
            shown("${prefix}_detail2", detail2 != null)
            if (detail2 != null) views.setTextViewText(id("${prefix}_detail2"), detail2)
        }

        fun compact(model: GenshinWidgetModel) {
            val resin = model.resin
            slot("ahd_widget_resin_slot", AhdIndicators.bar(context, AhdBarSpec(
                label = text("ahd_widget_genshin_resin"), current = resin?.current?.toLong(), max = resin?.max?.toLong(),
                theme = AhdIndicatorTheme.GENSHIN,
                caption = resinTime(model)?.let { if (model.resinFull) it else "${text("ahd_widget_until_full")} $it" },
            )))
            val coin = model.coin
            slot("ahd_widget_coin_slot", AhdIndicators.bar(context, AhdBarSpec(
                label = text("ahd_widget_genshin_coin"), current = coin?.current?.toLong(), max = coin?.max?.toLong(),
                theme = AhdIndicatorTheme.GENSHIN,
            )))
            val expeditions = model.expeditions
            slot("ahd_widget_expedition_slot", AhdIndicators.bar(context, AhdBarSpec(
                label = text("ahd_widget_genshin_expeditions"), current = expeditions?.finished?.toLong(),
                max = expeditions?.total?.toLong(), theme = AhdIndicatorTheme.GENSHIN, level = expeditions?.level,
            )))
            val daily = model.daily
            slot("ahd_widget_daily_slot", AhdIndicators.bar(context, AhdBarSpec(
                label = text("ahd_widget_genshin_daily"), current = daily?.current?.toLong(), max = daily?.max?.toLong(),
                theme = AhdIndicatorTheme.GENSHIN, level = model.dailyLevel,
                valueText = daily?.let { listOfNotNull("${it.current}/${it.max}", dailyShort(model)).joinToString(" · ") },
                contentDescription = listOfNotNull(text("ahd_widget_genshin_daily"), daily?.let { "${it.current}/${it.max}" }, dailyReward(model)).joinToString(" "),
            )))
        }

        fun resinValue(model: GenshinWidgetModel) = model.resin?.let { "${it.current}/${it.max}" }
        fun resinTime(model: GenshinWidgetModel): String? = when {
            model.resin == null -> null
            model.resinFull -> text("ahd_widget_genshin_resin_full")
            else -> model.resinSeconds?.let { time(it) }
        }
        fun coinTime(model: GenshinWidgetModel): String? = when {
            model.coin == null -> null
            model.coinFull -> text("ahd_widget_genshin_coin_full")
            else -> model.coinSeconds?.let { text("ahd_widget_full_in", time(it)) }
        }
        fun expeditionNext(expeditions: GenshinExpeditionsView?): String? = when {
            expeditions == null -> null
            expeditions.total == 0 -> text("ahd_widget_expeditions_none")
            expeditions.allDone -> text("ahd_widget_expeditions_done")
            else -> expeditions.nextSeconds?.let { text("ahd_widget_expedition_next", time(it)) }
        }
        fun dailyReward(model: GenshinWidgetModel): String? = when (model.daily?.extraRewardReceived) {
            true -> text("ahd_widget_genshin_daily_claimed")
            false -> text("ahd_widget_genshin_daily_unclaimed")
            null -> null
        }
        /** Short reward state next to the compact bar's value. */
        fun dailyShort(model: GenshinWidgetModel): String? = when (model.daily?.extraRewardReceived) {
            true -> text("ahd_widget_genshin_daily_claimed_short")
            false -> text("ahd_widget_genshin_daily_unclaimed_short")
            null -> null
        }
    }

    private companion object {
        /** Name lookups (no `R.`); a missing resource is a packaging error, so fail loudly. */
        fun find(context: Context, name: String, type: String): Int =
            context.resources.getIdentifier(name, type, context.packageName).also {
                check(it != 0) { "Missing $type resource $name (dashboard-ui/android/res)" }
            }
    }
}
