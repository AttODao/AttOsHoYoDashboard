package dev.dioxus.main

/** Layout chosen for the Star Rail widget's size in dp and the system font size. */
enum class StarRailWidgetLayout(val layout: String) {
    /** Trailblaze Power as the arc with its time, reserve and fetch time beside it, three bars below. */
    REGULAR("ahd_widget_starrail"),

    /** Four bars (Trailblaze Power with its time as the caption), for narrow widgets or large text. */
    COMPACT("ahd_widget_starrail_compact");

    companion object {
        // Regular width: 12dp padding on both sides, the 88dp arc, its 12dp margin and the 18dp
        // bolt beside it are fixed; the time to full ("23時間59分", 14sp bold) needs about 90dp at
        // the default font size and grows with it. At 200dp (density 560) or font size 200% it
        // was ellipsized to "あと9…".
        const val REGULAR_FIXED_WIDTH_DP = 142
        const val TIME_WIDTH_DP = 90

        // Regular height: padding and the arc with its caption are fixed (148dp); the header and
        // the three bars (about 98dp at the default font size) grow with the font size.
        const val REGULAR_FIXED_HEIGHT_DP = 148
        const val TEXT_HEIGHT_DP = 98

        /**
         * Portrait size like GenshinWidgetLayout.forSize (MIN_WIDTH, MAX_HEIGHT; 0 = unknown).
         * [fontScale] is Configuration.fontScale (values below 1 count as 1).
         */
        fun forSize(widthDp: Int, heightDp: Int, fontScale: Float = 1f): StarRailWidgetLayout {
            val scale = fontScale.coerceAtLeast(1f)
            return when {
                widthDp <= 0 || heightDp <= 0 -> REGULAR
                widthDp >= REGULAR_FIXED_WIDTH_DP + TIME_WIDTH_DP * scale &&
                    heightDp >= REGULAR_FIXED_HEIGHT_DP + TEXT_HEIGHT_DP * scale -> REGULAR
                else -> COMPACT
            }
        }
    }
}

/**
 * What the Star Rail home-screen widget shows at one instant: the last fetch projected to the
 * render time, plus the warning rules. Android-free so `test-android` checks it on the JVM
 * (android/test/StarRailWidgetModelTest.kt); StarRailWidget.kt binds it to RemoteViews.
 *
 * Warning color means "act before something is wasted": Trailblaze Power at its cap, a full
 * reserve, and every dispatched assignment done (rewards waiting). Finished daily training and
 * Simulated Universe points need nothing more, so they keep the game color.
 */
data class StarRailWidgetModel(
    val power: Int?,
    val powerMax: Int?,
    /** Seconds until Trailblaze Power is full; 0 at the cap, null without an estimate. */
    val powerFullInSeconds: Long?,
    val reserve: Int?,
    val reserveFull: Boolean,
    val assignmentsDone: Int?,
    val assignmentsTotal: Int?,
    /** Seconds until every dispatched assignment is done; null when none is still running. */
    val assignmentsLeftSeconds: Long?,
    val daily: Int?,
    val dailyMax: Int?,
    /** Simulated Universe weekly points: the slot Genshin uses for the realm currency. */
    val weekly: Int?,
    val weeklyMax: Int?,
    val fetchedAtMs: Long,
    val refreshFailed: Boolean,
) {
    val powerFull: Boolean get() = power != null && powerMax != null && powerMax > 0 && power >= powerMax
    val assignmentsReady: Boolean get() = assignmentsTotal != null && assignmentsTotal > 0 && assignmentsDone == assignmentsTotal
    val noAssignments: Boolean get() = assignmentsTotal == 0

    companion object {
        fun of(state: WidgetState.Ready, nowMs: Long): StarRailWidgetModel {
            val now = state.snapshot.at(nowMs)
            val stamina = now.stamina
            val expeditions = now.expeditions
            val done = expeditions?.finishedAt(nowMs)
            val running = expeditions != null && done != null && done < expeditions.total
            return StarRailWidgetModel(
                power = stamina?.current,
                powerMax = stamina?.max,
                powerFullInSeconds = stamina?.remainingSecondsAt(nowMs),
                reserve = now.reserveStamina,
                reserveFull = now.reserveFull,
                assignmentsDone = done,
                assignmentsTotal = expeditions?.total,
                assignmentsLeftSeconds = if (running) expeditions!!.allDoneRemainingSecondsAt(nowMs) else null,
                daily = now.daily?.current,
                dailyMax = now.daily?.max,
                weekly = now.weeklyPoints?.current,
                weeklyMax = now.weeklyPoints?.max,
                fetchedAtMs = now.fetchedAtMs,
                refreshFailed = state.refreshFailed,
            )
        }
    }
}
