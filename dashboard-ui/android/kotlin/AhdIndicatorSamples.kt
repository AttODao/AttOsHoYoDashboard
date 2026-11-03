package dev.dioxus.main

import android.app.Activity
import android.content.Context
import android.content.res.Configuration
import android.graphics.Color
import android.os.Bundle
import android.util.TypedValue
import android.view.View
import android.view.ViewGroup
import android.widget.LinearLayout
import android.widget.ScrollView
import android.widget.TextView

// Debug builds only (scripts/build-android.sh adds it to the manifest): every indicator state
// (no value, 0%, 50%, 90% = near full, 100% = full) for both games, in light and dark mode,
// inflated from the same RemoteViews a widget gets (RemoteViews.apply), so the layouts and
// drawables are checked without placing a widget:
//   adb shell am start -n cc.attodao.ahd/dev.dioxus.main.AhdIndicatorSamplesActivity
// Try small densities and large fonts with `wm density 160` / `settings put system font_scale 2.0`.
class AhdIndicatorSamplesActivity : Activity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        title = "AHD widget indicators"
        val column = LinearLayout(this).apply { orientation = LinearLayout.VERTICAL }
        for (night in listOf(false, true)) column.addView(section(modeContext(night), night))
        setContentView(ScrollView(this).apply { addView(column) })
    }

    private fun modeContext(night: Boolean): Context {
        val config = Configuration(resources.configuration)
        val mode = if (night) Configuration.UI_MODE_NIGHT_YES else Configuration.UI_MODE_NIGHT_NO
        config.uiMode = (config.uiMode and Configuration.UI_MODE_NIGHT_MASK.inv()) or mode
        return createConfigurationContext(config)
    }

    private fun section(context: Context, night: Boolean): View {
        val section = vertical(context, if (night) "#0C0B0E" else "#F5F4F8", 12)
        section.addView(text(context, if (night) "Dark" else "Light", 18f))
        for (theme in AhdIndicatorTheme.values()) section.addView(panel(context, theme, night))
        return section
    }

    private fun panel(context: Context, theme: AhdIndicatorTheme, night: Boolean): View {
        val background = when (theme) {
            AhdIndicatorTheme.GENSHIN -> if (night) "#132B27" else "#E5F4EC"
            AhdIndicatorTheme.STAR_RAIL -> if (night) "#27223E" else "#EEE8FA"
        }
        val panel = vertical(context, background, 12)
        (panel.layoutParams as? ViewGroup.MarginLayoutParams)?.topMargin = dp(12)
        panel.addView(text(context, theme.name, 14f))
        val max = if (theme == AhdIndicatorTheme.GENSHIN) 200L else 240L
        // seconds per point: resin 8 min, trailblaze power 6 min
        val perPoint = if (theme == AhdIndicatorTheme.GENSHIN) 480L else 360L
        val samples = listOf(
            "null" to null,
            "0%" to 0L,
            "50%" to max / 2,
            "90%" to max * 9 / 10,
            "100%" to max,
        )
        fun caption(current: Long?): String? = when (current) {
            null -> null
            max -> context.getString(context.resources.getIdentifier("ahd_indicator_full", "string", context.packageName))
            else -> AhdIndicators.hoursMinutes(context, (max - current) * perPoint)
        }
        for ((label, current) in samples) {
            add(panel, AhdIndicators.bar(context, AhdBarSpec(label, current, max, theme, caption = caption(current))), context)
        }
        add(panel, AhdIndicators.bar(context, AhdBarSpec("custom fillColor", max / 3, max, theme, fillColor = Color.parseColor("#49EEFF"))), context)

        val icon = AhdIndicators.icon(context, if (theme == AhdIndicatorTheme.GENSHIN) "ahd_indicator_icon_resin" else "ahd_indicator_icon_power")
        for (row in listOf(samples.take(3), samples.drop(3))) {
            val line = LinearLayout(context).apply { orientation = LinearLayout.HORIZONTAL }
            for ((_, current) in row) add(line, AhdIndicators.arc(context, AhdArcSpec(current, max, theme, caption = caption(current))), context)
            panel.addView(line)
        }
        val small = LinearLayout(context).apply { orientation = LinearLayout.HORIZONTAL }
        for ((_, current) in samples) {
            add(small, AhdIndicators.arc(context, AhdArcSpec(current, max, theme, AhdArcSize.SMALL, icon = icon, caption = current?.let { "$it" })), context)
        }
        panel.addView(small)
        return panel
    }

    private fun add(parent: ViewGroup, views: android.widget.RemoteViews, context: Context) {
        val view = views.apply(context, parent)
        val params = if (parent is LinearLayout && parent.orientation == LinearLayout.HORIZONTAL) {
            LinearLayout.LayoutParams(ViewGroup.LayoutParams.WRAP_CONTENT, ViewGroup.LayoutParams.WRAP_CONTENT).apply { marginEnd = dp(6) }
        } else {
            LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT)
        }
        params.topMargin = dp(10)
        parent.addView(view, params)
    }

    private fun vertical(context: Context, color: String, padding: Int) = LinearLayout(context).apply {
        orientation = LinearLayout.VERTICAL
        setBackgroundColor(Color.parseColor(color))
        setPadding(dp(padding), dp(padding), dp(padding), dp(padding))
        layoutParams = LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT)
    }

    private fun text(context: Context, value: String, size: Float) = TextView(context).apply {
        text = value
        setTextSize(TypedValue.COMPLEX_UNIT_SP, size)
        setTextColor(context.getColor(context.resources.getIdentifier("ahd_indicator_text", "color", context.packageName)))
    }

    private fun dp(value: Int) = (value * resources.displayMetrics.density).toInt()
}
