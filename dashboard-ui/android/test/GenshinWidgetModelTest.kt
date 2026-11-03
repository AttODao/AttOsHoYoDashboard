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
        assertNull(expeditions.level)
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
        assertEquals(AhdFillLevel.FULL, end.expeditions!!.level)
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
        assertNull(none.level)
    }
}
