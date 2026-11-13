package dev.dioxus.main

/** Layout chosen for the Star Rail widget's size in dp and the system font size. */
enum class StarRailWidgetLayout(val layout: String, val kind: AhdWidgetLayout) {
    /** Trailblaze Power as the large arc with its time; reserve, assignments, daily training and Simulated Universe as tiles. */
    REGULAR("ahd_widget_starrail", AhdWidgetLayout.REGULAR),

    /** The same values as one-line rows, for narrow widgets or large text. */
    COMPACT("ahd_widget_starrail_compact", AhdWidgetLayout.COMPACT);

    companion object {
        /** The shared size rule (AhdWidgetLayout.forSize): both games switch at the same sizes. */
        fun forSize(widthDp: Int, heightDp: Int, fontScale: Float = 1f): StarRailWidgetLayout =
            if (AhdWidgetLayout.forSize(widthDp, heightDp, fontScale) == AhdWidgetLayout.REGULAR) REGULAR else COMPACT
    }
}

/**
 * What the Star Rail home-screen widget shows at one instant: the last fetch projected to the
 * render time, plus the warning rules. Android-free so `test-android` checks it on the JVM
 * (android/test/StarRailWidgetModelTest.kt); StarRailWidget.kt binds it to RemoteViews.
 *
 * Warning color means "act before something is wasted": Trailblaze Power near or at its cap and
 * a full reserve. Every dispatched assignment done, finished daily training and reached
 * Simulated Universe points are "done" (the check), as on the dashboard.
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

    /** What the widget draws: Trailblaze Power, then the reserve, assignments, daily training and Simulated Universe. */
    fun content(): AhdWidgetContent = AhdWidgetContent(
        AhdMain(AhdGlyph.POWER, "ahd_widget_starrail_power", power?.toLong(), powerMax?.toLong(), powerFullInSeconds),
        listOf(
            reserveTile(),
            assignmentsTile(),
            // The check-circle and the cube are not self-explanatory at widget size: short names.
            AhdTile.score(AhdGlyph.DAILY, "ahd_widget_starrail_daily", daily?.toLong(), dailyMax?.toLong(), "ahd_widget_starrail_daily_short"),
            AhdTile.score(AhdGlyph.WEEKLY, "ahd_widget_starrail_weekly", weekly?.toLong(), weeklyMax?.toLong(), "ahd_widget_starrail_weekly_short"),
        ),
        fetchedAtMs, refreshFailed,
        // "14000/14000" beside its arc needs the wider second column.
        wideEndColumn = true,
    )

    /** The reserve has no maximum in the status: the number, and the cap badge when the game says it is full. */
    private fun reserveTile(): AhdTile {
        val reserve = reserve ?: return AhdTile.unknown(AhdGlyph.RESERVE, "ahd_widget_starrail_reserve")
        return AhdTile(
            AhdGlyph.RESERVE, "ahd_widget_starrail_reserve", reserve.toLong(),
            warn = reserveFull,
            detail = if (reserveFull) AhdDetail.AtCap else AhdDetail.Label("ahd_widget_starrail_reserve_short", spoken = false),
        )
    }

    /** Done/dispatched with pips, then the time until all are done. */
    private fun assignmentsTile(): AhdTile {
        val total = assignmentsTotal
        val done = assignmentsDone
        if (total == null || done == null) return AhdTile.unknown(AhdGlyph.EXPEDITION, "ahd_widget_starrail_assignments")
        return AhdTile(
            AhdGlyph.EXPEDITION, "ahd_widget_starrail_assignments", done.toLong(), total.toLong(),
            pips = AhdIndicatorModel.pips(done, total),
            detail = when {
                noAssignments -> AhdDetail.Label("ahd_widget_expeditions_none")
                assignmentsReady -> AhdDetail.Status(AhdMark.DONE)
                else -> assignmentsLeftSeconds?.let { AhdDetail.Time(it) }
            },
        )
    }

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
