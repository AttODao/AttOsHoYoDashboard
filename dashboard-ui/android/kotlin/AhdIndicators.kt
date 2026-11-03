package dev.dioxus.main

import android.content.Context
import android.view.View
import android.widget.RemoteViews

// Home-screen widget indicators as RemoteViews, for any AppWidgetProvider of the app:
//   views.removeAllViews(slotId)
//   views.addView(slotId, AhdIndicators.bar(context, AhdBarSpec(label, 120, 200, AhdIndicatorTheme.GENSHIN)))
// Usage and the full option list: dashboard-ui/android/README.md ("Widget indicators").
//
// Drawing: RemoteViews only inflate framework views (no custom View with onDraw), and
// ProgressBar colors can only be set from code since API 31 (setColorStateList), while the
// app supports API 24. Both indicators therefore use level drawables in plain ImageViews:
// the bar is a <clip> drawable and the arc a <rotate>d ring <shape android:useLevel="true">,
// whose filled share is ImageView.setImageLevel (remotable since API 1). A Bitmap drawn on
// a Canvas was the alternative; it was not used because its colors would be fixed when it
// is drawn (the launcher re-inflates the layout on a dark-mode switch, but a bitmap keeps the
// old colors until the next widget update), it is sized in pixels for one density, and every
// update would send the bitmap to the launcher (RemoteViews bitmap memory is limited).
// Colors come from res/values(-night)/ahd_indicator_colors.xml through android:tint in the
// layouts (tintMode src_in, so the translucent track color stays translucent over the white
// shapes), so they follow dark mode; each layout holds one fill view per color (game accent,
// near full, full) and the code only picks the visible one. This file must not use `R.`
// (the res overlay is added after dx's first Gradle run): resources are looked up by name.

/**
 * Horizontal bar: label and value text ("120/200") on one line, the bar below and an optional
 * caption (e.g. "3h 12m") under it. [current] null means "no value": an empty track and "—".
 */
data class AhdBarSpec(
    val label: String,
    val current: Long?,
    val max: Long?,
    val theme: AhdIndicatorTheme,
    /** Replaces the default "current/max" text. */
    val valueText: String? = null,
    val caption: String? = null,
    /** Overrides the computed level, e.g. FULL when every expedition is done. */
    val level: AhdFillLevel? = null,
    val nearFullPercent: Int = AhdIndicatorModel.NEAR_FULL_PERCENT,
    /** ARGB fill color instead of the theme/warning resources (fixed: it does not follow dark mode). */
    val fillColor: Int? = null,
    /** Spoken text for the whole indicator; defaults to "label value caption". */
    val contentDescription: String? = null,
)

/**
 * Arc (270° gauge): an icon or the value in the center and an optional caption below
 * (e.g. the time to full). The icon replaces the center value when [icon] is set.
 */
data class AhdArcSpec(
    val current: Long?,
    val max: Long?,
    val theme: AhdIndicatorTheme,
    val size: AhdArcSize = AhdArcSize.LARGE,
    /** Drawable resource id (AhdIndicators.icon(context, "ahd_indicator_icon_resin")); 0 shows the value. */
    val icon: Int = 0,
    /** Replaces the default center text (the current value). */
    val centerText: String? = null,
    val caption: String? = null,
    val level: AhdFillLevel? = null,
    val nearFullPercent: Int = AhdIndicatorModel.NEAR_FULL_PERCENT,
    val fillColor: Int? = null,
    /** Spoken text; set it when the center is an icon (it has no text of its own). */
    val contentDescription: String? = null,
)

object AhdIndicators {
    /** The bar as a RemoteViews to add into a widget layout container. */
    @JvmStatic fun bar(context: Context, spec: AhdBarSpec): RemoteViews {
        val res = Resources(context)
        val views = RemoteViews(context.packageName, res.layout("ahd_indicator_bar"))
        val level = spec.level ?: AhdIndicatorModel.fillLevel(spec.current, spec.max, spec.nearFullPercent)
        val value = spec.valueText ?: AhdIndicatorModel.countText(spec.current, spec.max)
        views.setTextViewText(res.id("ahd_indicator_label"), spec.label)
        bindValue(views, res, value, level)
        bindFill(views, res, spec.theme, level, AhdIndicatorModel.level(spec.current, spec.max, AhdIndicatorModel.MAX_LEVEL), spec.fillColor)
        bindCaption(views, res, spec.caption)
        views.setContentDescription(res.id("ahd_indicator_root"),
            spec.contentDescription ?: listOfNotNull(spec.label, spoken(context, value, level), spec.caption).joinToString(" "))
        return views
    }

    /** The arc as a RemoteViews to add into a widget layout container. */
    @JvmStatic fun arc(context: Context, spec: AhdArcSpec): RemoteViews {
        val res = Resources(context)
        val views = RemoteViews(context.packageName, res.layout(spec.size.layout))
        val level = spec.level ?: AhdIndicatorModel.fillLevel(spec.current, spec.max, spec.nearFullPercent)
        val value = spec.centerText ?: spec.current?.toString() ?: AhdIndicatorModel.NO_VALUE
        val icon = res.id("ahd_indicator_icon")
        if (spec.icon != 0) {
            views.setImageViewResource(icon, spec.icon)
            views.setViewVisibility(icon, View.VISIBLE)
            views.setViewVisibility(res.id("ahd_indicator_value"), View.GONE)
            views.setViewVisibility(res.id("ahd_indicator_value_full"), View.GONE)
        } else {
            views.setViewVisibility(icon, View.GONE)
            bindValue(views, res, value, level)
        }
        // The track is the whole 270° gauge; drawables have no level attribute in XML.
        views.setImageViewLevel(res.id("ahd_indicator_track"), AhdIndicatorModel.ARC_LEVEL)
        bindFill(views, res, spec.theme, level, AhdIndicatorModel.level(spec.current, spec.max, AhdIndicatorModel.ARC_LEVEL), spec.fillColor)
        bindCaption(views, res, spec.caption)
        val spokenValue = spoken(context, AhdIndicatorModel.countText(spec.current, spec.max), level)
        views.setContentDescription(res.id("ahd_indicator_root"),
            spec.contentDescription ?: listOfNotNull(spokenValue, spec.caption).joinToString(" "))
        return views
    }

    /** "3h 12m" / "3時間12分" in the context's locale, rounded up like the dashboard. */
    @JvmStatic fun hoursMinutes(context: Context, seconds: Long): String {
        val (hours, minutes) = AhdIndicatorModel.hoursMinutes(seconds)
        return context.getString(Resources(context).string("ahd_indicator_hours_minutes"), hours, minutes)
    }

    /** A bundled indicator icon (ahd_indicator_icon_resin, _power, _coin, _expedition, _daily) or 0. */
    @JvmStatic fun icon(context: Context, name: String): Int = Resources(context).drawable(name)

    private fun bindValue(views: RemoteViews, res: Resources, value: String, level: AhdFillLevel) {
        // The value turns to the "full" color at the cap, like the dashboard's number.
        val full = level == AhdFillLevel.FULL
        val shown = res.id(if (full) "ahd_indicator_value_full" else "ahd_indicator_value")
        val hidden = res.id(if (full) "ahd_indicator_value" else "ahd_indicator_value_full")
        views.setTextViewText(shown, value)
        views.setViewVisibility(shown, View.VISIBLE)
        views.setViewVisibility(hidden, View.GONE)
    }

    private fun bindFill(views: RemoteViews, res: Resources, theme: AhdIndicatorTheme, level: AhdFillLevel, drawableLevel: Int, color: Int?) {
        val visible = when (level) {
            AhdFillLevel.UNKNOWN -> null
            AhdFillLevel.NORMAL -> "ahd_indicator_fill_${theme.key}"
            AhdFillLevel.NEAR_FULL -> "ahd_indicator_fill_near"
            AhdFillLevel.FULL -> "ahd_indicator_fill_full"
        }?.takeIf { drawableLevel > 0 }
        for (name in FILLS) {
            val id = res.id(name)
            if (name == visible) {
                views.setImageViewLevel(id, drawableLevel)
                if (color != null) views.setInt(id, "setColorFilter", color)
                views.setViewVisibility(id, View.VISIBLE)
            } else {
                views.setViewVisibility(id, View.GONE)
            }
        }
    }

    private fun bindCaption(views: RemoteViews, res: Resources, caption: String?) {
        val id = res.id("ahd_indicator_caption")
        if (caption.isNullOrEmpty()) {
            views.setViewVisibility(id, View.GONE)
        } else {
            views.setTextViewText(id, caption)
            views.setViewVisibility(id, View.VISIBLE)
        }
    }

    private fun spoken(context: Context, value: String, level: AhdFillLevel): String {
        val res = Resources(context)
        return when (level) {
            AhdFillLevel.UNKNOWN -> context.getString(res.string("ahd_indicator_no_value"))
            AhdFillLevel.FULL -> "$value ${context.getString(res.string("ahd_indicator_full"))}"
            AhdFillLevel.NEAR_FULL -> "$value ${context.getString(res.string("ahd_indicator_near_full"))}"
            AhdFillLevel.NORMAL -> value
        }
    }

    private val FILLS = listOf("ahd_indicator_fill_genshin", "ahd_indicator_fill_starrail", "ahd_indicator_fill_near", "ahd_indicator_fill_full")

    private fun RemoteViews.setImageViewLevel(id: Int, level: Int) = setInt(id, "setImageLevel", level)

    /** Name lookups (no `R.`); a missing resource is a packaging error, so fail loudly. */
    private class Resources(private val context: Context) {
        private fun find(name: String, type: String): Int =
            context.resources.getIdentifier(name, type, context.packageName).also {
                check(it != 0) { "Missing $type resource $name (dashboard-ui/android/res)" }
            }
        fun id(name: String) = find(name, "id")
        fun layout(name: String) = find(name, "layout")
        fun string(name: String) = find(name, "string")
        fun drawable(name: String) = context.resources.getIdentifier(name, "drawable", context.packageName)
    }
}
