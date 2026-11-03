package dev.dioxus.main

import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test

class StarRailWidgetModelTest {
    private val time = parseRfc3339Ms("2026-11-04T00:00:00Z")!!
    private fun status(data: String) = JSONObject("""{"game_kind":"starrail","game_uid":"700000001",
        "fetched_at":"2026-11-04T00:00:00Z","stale":false,"data":$data}""")
    private fun ready(data: String, failed: Boolean = false) =
        WidgetState.Ready(WidgetSnapshot.fromStatus(status(data))!!, failed)

    private val normal = """{"current_stamina":180,"max_stamina":240,"stamina_full_at":"2026-11-04T06:00:00Z",
        "current_reserve_stamina":1200,"is_reserve_stamina_full":false,"current_train_score":400,"max_train_score":500,
        "current_rogue_score":8000,"max_rogue_score":14000,"accepted_expedition_num":4,"total_expedition_num":4,
        "expeditions":[{"status":"Finished","remaining_time":0},{"status":"Ongoing","remaining_time":3600},
        {"status":"Ongoing","remaining_time":7200},{"status":"Ongoing","remaining_time":7200}]}"""

    @Test fun normalStatusProjectsToTheRenderTime() {
        val model = StarRailWidgetModel.of(ready(normal), time)
        assertEquals(180, model.power)
        assertEquals(240, model.powerMax)
        assertEquals(6 * 3600L, model.powerFullInSeconds)
        assertFalse(model.powerFull)
        assertEquals(1200, model.reserve)
        assertFalse(model.reserveFull)
        assertEquals(1, model.assignmentsDone)
        assertEquals(4, model.assignmentsTotal)
        assertEquals(7200L, model.assignmentsLeftSeconds)
        assertFalse(model.assignmentsReady)
        assertEquals(400, model.daily)
        assertEquals(500, model.dailyMax)
        assertEquals(8000, model.weekly)
        assertEquals(14000, model.weeklyMax)
        assertEquals(time, model.fetchedAtMs)
        assertFalse(model.refreshFailed)

        // An hour later (a periodic re-render without a new fetch): 10 more power, 2 assignments done.
        val later = StarRailWidgetModel.of(ready(normal), time + 3600_000)
        assertEquals(190, later.power)
        assertEquals(5 * 3600L, later.powerFullInSeconds)
        assertEquals(2, later.assignmentsDone)
        assertEquals(3600L, later.assignmentsLeftSeconds)

        // After the full time and the last assignment: both warn.
        val done = StarRailWidgetModel.of(ready(normal), time + 6 * 3600_000)
        assertEquals(240, done.power)
        assertTrue(done.powerFull)
        assertEquals(0L, done.powerFullInSeconds)
        assertTrue(done.assignmentsReady)
        assertNull(done.assignmentsLeftSeconds)
    }

    @Test fun capsAndEmptyAssignments() {
        val model = StarRailWidgetModel.of(ready("""{"current_stamina":250,"max_stamina":240,
            "current_reserve_stamina":2400,"is_reserve_stamina_full":true,"total_expedition_num":4,"expeditions":[],
            "current_train_score":500,"max_train_score":500,"current_rogue_score":14000,"max_rogue_score":14000}""", failed = true), time)
        assertTrue(model.powerFull)
        assertEquals(0L, model.powerFullInSeconds)
        assertTrue(model.reserveFull)
        assertEquals(0, model.assignmentsTotal)
        assertTrue(model.noAssignments)
        assertFalse(model.assignmentsReady)
        assertNull(model.assignmentsLeftSeconds)
        assertTrue(model.refreshFailed)
    }

    @Test fun missingFieldsStayUnknown() {
        val model = StarRailWidgetModel.of(ready("{}"), time)
        assertNull(model.power)
        assertNull(model.powerMax)
        assertNull(model.powerFullInSeconds)
        assertFalse(model.powerFull)
        assertNull(model.reserve)
        assertNull(model.assignmentsDone)
        assertNull(model.assignmentsTotal)
        assertFalse(model.noAssignments)
        assertNull(model.daily)
        assertNull(model.weekly)
    }
}
