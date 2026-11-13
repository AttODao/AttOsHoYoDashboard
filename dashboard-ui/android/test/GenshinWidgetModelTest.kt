package dev.dioxus.main

import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test

class GenshinWidgetModelTest {
    private val fetched = parseRfc3339Ms("2026-11-04T00:00:00Z")!!
    private fun snapshot(data: String): WidgetSnapshot = WidgetSnapshot.fromStatus(JSONObject("""{
        "game_kind":"genshin","game_uid":"800000000","fetched_at":"2026-11-04T00:00:00Z","stale":false,"data":$data}"""))!!
    private val normal = snapshot("""{
        "current_resin":120,"max_resin":200,"resin_full_at":"2026-11-04T10:40:00Z",
        "current_home_coin":1200,"max_home_coin":2400,"home_coin_full_at":"2026-11-04T12:00:00Z",
        "finished_task_num":3,"total_task_num":4,"is_extra_task_reward_received":false,
        "max_expedition_num":5,"expeditions":[{"status":"Finished","remained_time":"0"},
            {"status":"Ongoing","remained_time":"4800"},{"status":"Ongoing","remained_time":"18120"}]}""")

    @Test fun layoutFollowsTheWidgetSize() {
        assertEquals(GenshinWidgetLayout.REGULAR, GenshinWidgetLayout.forSize(0, 0))
        assertEquals(GenshinWidgetLayout.REGULAR, GenshinWidgetLayout.forSize(300, 330))
        assertEquals(GenshinWidgetLayout.REGULAR, GenshinWidgetLayout.forSize(210, 250))
        assertEquals(GenshinWidgetLayout.COMPACT, GenshinWidgetLayout.forSize(180, 330))
        assertEquals(GenshinWidgetLayout.COMPACT, GenshinWidgetLayout.forSize(300, 180))
        assertEquals(GenshinWidgetLayout.COMPACT, GenshinWidgetLayout.forSize(209, 249))
    }

    @Test fun largeFontsNeedTheCompactLayout() {
        // 3x3 on a 1080px phone at density 420 is about 270x343dp.
        assertEquals(GenshinWidgetLayout.REGULAR, GenshinWidgetLayout.forSize(270, 343, 1.0f))
        assertEquals(GenshinWidgetLayout.REGULAR, GenshinWidgetLayout.forSize(270, 343, 1.15f))
        // Font size 200%: the resin time was cut off at the bottom.
        assertEquals(GenshinWidgetLayout.COMPACT, GenshinWidgetLayout.forSize(270, 343, 2.0f))
        // Density 560 (about 200x248dp) is compact at any font size.
        assertEquals(GenshinWidgetLayout.COMPACT, GenshinWidgetLayout.forSize(200, 248, 1.3f))
        // Small fonts do not shrink the minimum.
        assertEquals(GenshinWidgetLayout.COMPACT, GenshinWidgetLayout.forSize(209, 250, 0.85f))
        assertEquals(GenshinWidgetLayout.REGULAR, GenshinWidgetLayout.forSize(0, 0, 2.0f))
    }

    @Test fun valuesAndTimesAtFetch() {
        val model = GenshinWidgetModel.of(normal, false, fetched)
        assertEquals(120, model.resin!!.current)
        assertEquals(38400L, model.resinSeconds)
        assertFalse(model.resinFull)
        assertEquals(1200, model.coin!!.current)
        assertEquals(43200L, model.coinSeconds)
        val expeditions = model.expeditions!!
        assertEquals(GenshinExpeditionsView(1, 3, 5, 4800, 18120), expeditions)
        assertFalse(expeditions.allDone)
        assertEquals(3, model.daily!!.current)
        assertEquals(AhdFillLevel.NORMAL, model.dailyLevel)
        assertEquals(fetched, model.fetchedAtMs)
        assertFalse(model.refreshFailed)
    }

    @Test fun valuesMoveWithTheClock() {
        // One resin per 8 minutes; the expedition at 4800 s is back after 80 minutes.
        val later = GenshinWidgetModel.of(normal, true, fetched + 80 * 60_000L)
        assertEquals(130, later.resin!!.current)
        assertEquals(38400L - 4800, later.resinSeconds)
        assertTrue(later.coin!!.current > 1200)
        assertEquals(GenshinExpeditionsView(2, 3, 5, 18120 - 4800, 18120 - 4800), later.expeditions)
        assertTrue(later.refreshFailed)
        val end = GenshinWidgetModel.of(normal, false, fetched + 13 * 3600_000L)
        assertTrue(end.resinFull)
        assertEquals(0L, end.resinSeconds)
        assertTrue(end.coinFull)
        assertTrue(end.expeditions!!.allDone)
        assertNull(end.expeditions!!.nextSeconds)
        assertNull(end.expeditions!!.lastSeconds)
    }

    @Test fun dailyRewardDecidesTheColor() {
        fun level(done: Int, received: String) = GenshinWidgetModel.of(snapshot("""{
            "finished_task_num":$done,"total_task_num":4,"is_extra_task_reward_received":$received}"""), false, fetched).dailyLevel
        assertEquals(AhdFillLevel.NORMAL, level(0, "false"))
        assertEquals(AhdFillLevel.NORMAL, level(3, "false"))
        assertEquals(AhdFillLevel.NEAR_FULL, level(4, "false"))
        assertEquals(AhdFillLevel.NORMAL, level(4, "true"))
        assertEquals(AhdFillLevel.NORMAL, level(4, "null"))
    }

    @Test fun missingPartsStayEmpty() {
        val model = GenshinWidgetModel.of(snapshot("{}"), false, fetched)
        assertNull(model.resin)
        assertNull(model.resinSeconds)
        assertNull(model.coin)
        assertNull(model.expeditions)
        assertNull(model.daily)
        assertNull(model.dailyLevel)
        assertFalse(model.resinFull)
        val none = GenshinWidgetModel.of(snapshot("""{"expeditions":[]}"""), false, fetched).expeditions!!
        assertEquals(0, none.total)
        assertFalse(none.allDone)
    }

    @Test fun contentShowsResinThenCurrencyCommissionsAndExpeditions() {
        val content = GenshinWidgetModel.of(normal, false, fetched).content()
        assertEquals(AhdMain(AhdGlyph.RESIN, "ahd_widget_genshin_resin", 120, 200, 38400), content.main)
        assertEquals(AhdFillLevel.NORMAL, content.main.level)
        assertEquals(listOf(
            AhdTile(AhdGlyph.COIN, "ahd_widget_genshin_coin", 1200, 2400, ring = 3750, detail = AhdDetail.Time(43200)),
            // 3/4 with the reward open: nothing to act on yet, so the open circle is quiet.
            AhdTile(AhdGlyph.DAILY, "ahd_widget_genshin_daily", 3, 4, pips = AhdPips(3, 4), detail = AhdDetail.Reward(AhdMark.UNCLAIMED)),
            // One of three back; the next in 80 minutes, the last in 302.
            AhdTile(AhdGlyph.EXPEDITION, "ahd_widget_genshin_expeditions", 1, 3, pips = AhdPips(1, 3), detail = AhdDetail.Range(4800, 18120)),
        ), content.tiles)
        assertEquals(fetched, content.fetchedAtMs)
        assertFalse(content.refreshFailed)
        assertFalse(content.wideEndColumn)
        assertTrue(GenshinWidgetModel.of(normal, true, fetched).content().refreshFailed)
    }

    @Test fun contentWarnsAtCapsAndMarksWhatIsDone() {
        // 13 hours later: resin and currency at their caps, every expedition back.
        val end = GenshinWidgetModel.of(normal, false, fetched + 13 * 3600_000L).content()
        assertEquals(AhdFillLevel.FULL, end.main.level)
        assertEquals(0L, end.main.secondsToFull)
        val (coin, _, expeditions) = end.tiles
        assertTrue(coin.warn && coin.ringWarn)
        assertEquals(AhdDetail.AtCap, coin.detail)
        assertEquals(AhdDetail.Status(AhdMark.DONE), expeditions.detail)
        assertFalse(expeditions.warn)
        // One expedition left: a single time, not a range.
        val one = GenshinWidgetModel.of(normal, false, fetched + 4800_000L).content().tiles[2]
        assertEquals(AhdDetail.Time(18120 - 4800), one.detail)
        // Near the cap the main resource warns before it is full.
        val near = GenshinWidgetModel.of(snapshot("""{"current_resin":186,"max_resin":200,"resin_full_at":"2026-11-04T01:52:00Z"}"""), false, fetched).content()
        assertEquals(AhdFillLevel.NEAR_FULL, near.main.level)
    }

    @Test fun extraRewardIsClaimedDueOpenOrUnknown() {
        fun mark(done: Int, received: String) = GenshinWidgetModel.of(snapshot("""{
            "finished_task_num":$done,"total_task_num":4,"is_extra_task_reward_received":$received}"""), false, fetched).let {
            assertEquals(AhdDetail.Reward(it.rewardMark!!), it.content().tiles[1].detail)
            it.rewardMark
        }
        assertEquals(AhdMark.UNCLAIMED, mark(3, "false"))
        // All four done and the reward not claimed: act now (warning color).
        assertEquals(AhdMark.UNCLAIMED_DUE, mark(4, "false"))
        assertEquals(AhdMark.CLAIMED, mark(4, "true"))
        // Not returned is unknown, never "unclaimed".
        assertEquals(AhdMark.UNKNOWN, mark(4, "null"))
    }

    @Test fun missingValuesBecomeUnknownTiles() {
        val content = GenshinWidgetModel.of(snapshot("{}"), false, fetched).content()
        assertNull(content.main.current)
        assertEquals(AhdFillLevel.UNKNOWN, content.main.level)
        assertEquals(listOf(
            // A share keeps its (empty) arc.
            AhdTile.unknown(AhdGlyph.COIN, "ahd_widget_genshin_coin", ring = true),
            AhdTile.unknown(AhdGlyph.DAILY, "ahd_widget_genshin_daily"),
            AhdTile.unknown(AhdGlyph.EXPEDITION, "ahd_widget_genshin_expeditions"),
        ), content.tiles)
        // Nothing sent is a known state with its own words, not "unknown".
        val none = GenshinWidgetModel.of(snapshot("""{"expeditions":[]}"""), false, fetched).content().tiles[2]
        assertEquals(AhdTile(AhdGlyph.EXPEDITION, "ahd_widget_genshin_expeditions", 0, 0, detail = AhdDetail.Label("ahd_widget_expeditions_none")), none)
    }
}
