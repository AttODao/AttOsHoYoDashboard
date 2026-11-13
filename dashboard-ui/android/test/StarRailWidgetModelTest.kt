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

    @Test fun layoutFollowsTheWidgetAndFontSize() {
        // 3x3 on a 1080px phone at density 420 is about 270x343dp; at density 560 about 200x248dp.
        assertEquals(StarRailWidgetLayout.REGULAR, StarRailWidgetLayout.forSize(270, 343, 1.0f))
        assertEquals(StarRailWidgetLayout.REGULAR, StarRailWidgetLayout.forSize(270, 343, 1.15f))
        assertEquals(StarRailWidgetLayout.REGULAR, StarRailWidgetLayout.forSize(0, 0, 2.0f))
        assertEquals(StarRailWidgetLayout.COMPACT, StarRailWidgetLayout.forSize(200, 248, 1.0f))
        assertEquals(StarRailWidgetLayout.COMPACT, StarRailWidgetLayout.forSize(270, 343, 1.3f))
        assertEquals(StarRailWidgetLayout.COMPACT, StarRailWidgetLayout.forSize(270, 343, 2.0f))
        // The same boundaries as the Genshin widget (AhdWidgetLayout): 210x250dp per font scale.
        assertEquals(StarRailWidgetLayout.REGULAR, StarRailWidgetLayout.forSize(210, 250, 0.85f))
        assertEquals(StarRailWidgetLayout.COMPACT, StarRailWidgetLayout.forSize(209, 343, 1.0f))
        assertEquals(StarRailWidgetLayout.COMPACT, StarRailWidgetLayout.forSize(270, 249, 1.0f))
        for ((width, height, scale) in listOf(Triple(270, 343, 1.0f), Triple(200, 248, 1.3f), Triple(231, 260, 1.1f))) {
            assertEquals(GenshinWidgetLayout.forSize(width, height, scale).kind, StarRailWidgetLayout.forSize(width, height, scale).kind)
        }
        assertEquals("ahd_widget_starrail", StarRailWidgetLayout.REGULAR.layout)
        assertEquals("ahd_widget_starrail_compact", StarRailWidgetLayout.COMPACT.layout)
    }

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

        // After the full time and the last assignment: power at its cap, every assignment done.
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

    @Test fun contentShowsPowerThenReserveAssignmentsTrainingAndUniverse() {
        val content = StarRailWidgetModel.of(ready(normal), time).content()
        assertEquals(AhdMain(AhdGlyph.POWER, "ahd_widget_starrail_power", 180, 240, 6 * 3600L), content.main)
        assertEquals(listOf(
            AhdTile(AhdGlyph.RESERVE, "ahd_widget_starrail_reserve", 1200, detail = AhdDetail.Label("ahd_widget_starrail_reserve_short", spoken = false)),
            AhdTile(AhdGlyph.EXPEDITION, "ahd_widget_starrail_assignments", 1, 4, pips = AhdPips(1, 4), detail = AhdDetail.Time(7200)),
            AhdTile(AhdGlyph.DAILY, "ahd_widget_starrail_daily", 400, 500, ring = 6000, detail = AhdDetail.Label("ahd_widget_starrail_daily_short", spoken = false)),
            AhdTile(AhdGlyph.WEEKLY, "ahd_widget_starrail_weekly", 8000, 14000, ring = 4285, detail = AhdDetail.Label("ahd_widget_starrail_weekly_short", spoken = false)),
        ), content.tiles)
        assertEquals(time, content.fetchedAtMs)
        assertFalse(content.refreshFailed)
        // "14000/14000" beside its arc: the second column is the wider one.
        assertTrue(content.wideEndColumn)

        // Every assignment back is done (the check), as on the dashboard; power at its cap warns.
        val done = StarRailWidgetModel.of(ready(normal), time + 6 * 3600_000).content()
        assertEquals(AhdFillLevel.FULL, done.main.level)
        assertEquals(AhdDetail.Status(AhdMark.DONE), done.tiles[1].detail)
        assertEquals(AhdPips(4, 4), done.tiles[1].pips)
    }

    @Test fun contentWarnsOnlyForWhatIsWasted() {
        val content = StarRailWidgetModel.of(ready("""{"current_stamina":250,"max_stamina":240,
            "current_reserve_stamina":2400,"is_reserve_stamina_full":true,"total_expedition_num":4,"expeditions":[],
            "current_train_score":500,"max_train_score":500,"current_rogue_score":14000,"max_rogue_score":14000}""", failed = true), time).content()
        assertEquals(AhdFillLevel.FULL, content.main.level)
        assertTrue(content.refreshFailed)
        val (reserve, assignments, daily, weekly) = content.tiles
        // A full reserve wastes power: warning color and the cap badge.
        assertTrue(reserve.warn)
        assertEquals(AhdDetail.AtCap, reserve.detail)
        assertEquals(AhdDetail.Label("ahd_widget_expeditions_none"), assignments.detail)
        assertNull(assignments.pips)
        // Finished training and reached points are done, never a warning.
        for (score in listOf(daily, weekly)) {
            assertEquals(AhdDetail.Status(AhdMark.GOAL_REACHED), score.detail)
            assertFalse(score.warn || score.ringWarn)
            assertEquals(AhdIndicatorModel.ARC_LEVEL, score.ring)
        }
    }

    @Test fun missingValuesBecomeUnknownTiles() {
        val content = StarRailWidgetModel.of(ready("{}"), time).content()
        assertNull(content.main.current)
        assertEquals(listOf(
            AhdTile.unknown(AhdGlyph.RESERVE, "ahd_widget_starrail_reserve"),
            AhdTile.unknown(AhdGlyph.EXPEDITION, "ahd_widget_starrail_assignments"),
            AhdTile.unknown(AhdGlyph.DAILY, "ahd_widget_starrail_daily", ring = true),
            AhdTile.unknown(AhdGlyph.WEEKLY, "ahd_widget_starrail_weekly", ring = true),
        ), content.tiles)
    }
}
