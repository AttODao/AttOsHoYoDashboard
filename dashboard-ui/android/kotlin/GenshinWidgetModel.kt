package dev.dioxus.main

// What the Genshin widget shows (GenshinWidget.kt), without Android imports so the JVM tests
// (android/test/GenshinWidgetModelTest.kt) cover it.

/** Layout chosen for the widget's size in dp (AppWidgetManager OPTION_APPWIDGET_* values). */
enum class GenshinWidgetLayout(val layout: String) {
    /** Resin as the large arc, Realm Currency, expeditions and daily commissions below. */
    REGULAR("ahd_widget_genshin"),

    /** Four bars, for a widget resized below the regular layout's height or width. */
    COMPACT("ahd_widget_genshin_compact");

    companion object {
        // Three 56dp arcs with their texts side by side, and the 88dp arc above them.
        const val REGULAR_MIN_WIDTH_DP = 210
        const val REGULAR_MIN_HEIGHT_DP = 250

        /** Portrait size: the width is OPTION_APPWIDGET_MIN_WIDTH, the height MAX_HEIGHT. 0 = unknown. */
        fun forSize(widthDp: Int, heightDp: Int): GenshinWidgetLayout = when {
            widthDp <= 0 || heightDp <= 0 -> REGULAR
            widthDp >= REGULAR_MIN_WIDTH_DP && heightDp >= REGULAR_MIN_HEIGHT_DP -> REGULAR
            else -> COMPACT
        }
    }
}

/** Expedition counts and times at one moment; times are null when nothing is still out. */
data class GenshinExpeditionsView(
    val finished: Int,
    val total: Int,
    val max: Int?,
    val nextSeconds: Long?,
    val lastSeconds: Long?,
) {
    val allDone: Boolean get() = total > 0 && finished >= total

    /** Every expedition back means rewards waiting: the indicator uses the cap (warning) color. */
    val level: AhdFillLevel? get() = if (allDone) AhdFillLevel.FULL else null
}

data class GenshinWidgetModel(
    val resin: Gauge?,
    val resinSeconds: Long?,
    val coin: Gauge?,
    val coinSeconds: Long?,
    val expeditions: GenshinExpeditionsView?,
    val daily: Daily?,
    val fetchedAtMs: Long,
    val refreshFailed: Boolean,
) {
    val resinFull: Boolean get() = resin?.isFull == true
    val coinFull: Boolean get() = coin?.isFull == true

    /**
     * Commissions are not a resource that overflows: done with the reward claimed is the normal
     * accent; all four done but the reward unclaimed asks for action (warning); otherwise the
     * share decides (never the cap color, which means "overflowing" elsewhere).
     */
    val dailyLevel: AhdFillLevel? get() = daily?.let {
        when {
            it.max <= 0 -> AhdFillLevel.UNKNOWN
            it.current < it.max -> AhdFillLevel.NORMAL
            it.extraRewardReceived == false -> AhdFillLevel.NEAR_FULL
            else -> AhdFillLevel.NORMAL
        }
    }

    companion object {
        fun of(snapshot: WidgetSnapshot, refreshFailed: Boolean, nowMs: Long): GenshinWidgetModel {
            val now = snapshot.at(nowMs)
            val expeditions = snapshot.expeditions?.let {
                val finished = it.finishedAt(nowMs)
                GenshinExpeditionsView(
                    finished, it.total, it.max,
                    it.nextRemainingSecondsAt(nowMs),
                    if (finished < it.total) it.allDoneRemainingSecondsAt(nowMs) else null,
                )
            }
            return GenshinWidgetModel(
                now.stamina, now.stamina?.remainingSecondsAt(nowMs),
                now.currency, now.currency?.remainingSecondsAt(nowMs),
                expeditions, snapshot.daily, snapshot.fetchedAtMs, refreshFailed,
            )
        }
    }
}
