package dev.dioxus.main

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
