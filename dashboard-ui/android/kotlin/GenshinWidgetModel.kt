package dev.dioxus.main

// What the Genshin widget shows (GenshinWidget.kt), without Android imports so the JVM tests
// (android/test/GenshinWidgetModelTest.kt) cover it.

/** Layout chosen for the widget's size in dp (AppWidgetManager OPTION_APPWIDGET_* values). */
enum class GenshinWidgetLayout(val layout: String, val kind: AhdWidgetLayout) {
    /** Resin as the large arc with its time; Realm Currency, commissions and expeditions as tiles. */
    REGULAR("ahd_widget_genshin", AhdWidgetLayout.REGULAR),

    /** The same values as one-line rows, for a widget resized below the regular size or large text. */
    COMPACT("ahd_widget_genshin_compact", AhdWidgetLayout.COMPACT);

    companion object {
        /** The shared size rule (AhdWidgetLayout.forSize): both games switch at the same sizes. */
        fun forSize(widthDp: Int, heightDp: Int, fontScale: Float = 1f): GenshinWidgetLayout =
            if (AhdWidgetLayout.forSize(widthDp, heightDp, fontScale) == AhdWidgetLayout.REGULAR) REGULAR else COMPACT
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
     * Commissions are not a resource that overflows: done with the reward claimed needs nothing;
     * all four done but the reward unclaimed asks for action (NEAR_FULL = warning); otherwise
     * nothing warns (never FULL, which means "overflowing" elsewhere).
     */
    val dailyLevel: AhdFillLevel? get() = daily?.let {
        when {
            it.max <= 0 -> AhdFillLevel.UNKNOWN
            it.current < it.max -> AhdFillLevel.NORMAL
            it.extraRewardReceived == false -> AhdFillLevel.NEAR_FULL
            else -> AhdFillLevel.NORMAL
        }
    }

    /**
     * The extra commission reward as a status symbol: claimed, not claimed yet, not claimed with
     * all commissions done (act now: warning color, [dailyLevel]), or unknown when the field was
     * not returned (never shown as "unclaimed").
     */
    val rewardMark: AhdMark? get() = daily?.let {
        when (it.extraRewardReceived) {
            true -> AhdMark.CLAIMED
            false -> if (dailyLevel == AhdFillLevel.NEAR_FULL) AhdMark.UNCLAIMED_DUE else AhdMark.UNCLAIMED
            null -> AhdMark.UNKNOWN
        }
    }

    /** What the widget draws: resin, then Realm Currency, commissions and expeditions (the dashboard card's order). */
    fun content(): AhdWidgetContent = AhdWidgetContent(
        AhdMain(AhdGlyph.RESIN, "ahd_widget_genshin_resin", resin?.current?.toLong(), resin?.max?.toLong(), resinSeconds),
        listOf(
            AhdTile.gauge(AhdGlyph.COIN, "ahd_widget_genshin_coin", coin?.current?.toLong(), coin?.max?.toLong(), coinSeconds),
            dailyTile(),
            expeditionsTile(),
        ),
        fetchedAtMs, refreshFailed,
    )

    private fun dailyTile(): AhdTile {
        val daily = daily ?: return AhdTile.unknown(AhdGlyph.DAILY, "ahd_widget_genshin_daily")
        return AhdTile(
            AhdGlyph.DAILY, "ahd_widget_genshin_daily", daily.current.toLong(), daily.max.toLong(),
            pips = AhdIndicatorModel.pips(daily.current, daily.max),
            detail = rewardMark?.let { AhdDetail.Reward(it) },
        )
    }

    /** Finished/sent with pips, then the time until the next one and until all are back. */
    private fun expeditionsTile(): AhdTile {
        val expeditions = expeditions ?: return AhdTile.unknown(AhdGlyph.EXPEDITION, "ahd_widget_genshin_expeditions")
        val next = expeditions.nextSeconds
        val last = expeditions.lastSeconds
        return AhdTile(
            AhdGlyph.EXPEDITION, "ahd_widget_genshin_expeditions", expeditions.finished.toLong(), expeditions.total.toLong(),
            pips = AhdIndicatorModel.pips(expeditions.finished, expeditions.total),
            detail = when {
                expeditions.total == 0 -> AhdDetail.Label("ahd_widget_expeditions_none")
                expeditions.allDone -> AhdDetail.Status(AhdMark.DONE)
                next != null && last != null && last > next -> AhdDetail.Range(next, last)
                else -> (next ?: last)?.let { AhdDetail.Time(it) }
            },
        )
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
