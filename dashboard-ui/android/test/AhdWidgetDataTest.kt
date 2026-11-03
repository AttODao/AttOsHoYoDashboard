package dev.dioxus.main

import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test

class AhdWidgetDataTest {
    private val time = parseRfc3339Ms("2026-11-04T00:00:00Z")!!
    private fun genshin(): JSONObject = JSONObject("""{
        "game_kind":"genshin","game_uid":"123456789","region":"os_asia","visible":true,"detected":true,
        "fetched_at":"2026-11-04T00:00:00Z","stale":false,"data":{
            "current_resin":100,"max_resin":200,"resin_full_at":"2026-11-04T13:20:00Z",
            "current_home_coin":1000,"max_home_coin":2400,"home_coin_full_at":"2026-11-04T14:00:00Z",
            "finished_task_num":4,"total_task_num":4,"is_extra_task_reward_received":true,
            "max_expedition_num":5,"expeditions":[{"status":"Finished","remained_time":999},
                {"status":"Ongoing","remained_time":120},{"status":"Ongoing","remained_time":"3600"},
                {"status":"Ongoing"},{"remained_time":-1}]}}
    """)
    private fun starrail(): JSONObject = JSONObject("""{
        "game_kind":"starrail","game_uid":"987654321","fetched_at":"2026-11-04T09:00:00+09:00",
        "stale":true,"data":{"current_stamina":100,"max_stamina":300,"stamina_full_at":"2026-11-04T20:00:00Z",
        "current_reserve_stamina":50,"current_train_score":500,"max_train_score":500,
        "current_rogue_score":8000,"max_rogue_score":14000,"is_reserve_stamina_full":false,
        "accepted_expedition_num":3,"total_expedition_num":4,
        "expeditions":[{"status":"Finished","remaining_time":"100"},{"remaining_time":60},{"remaining_time":"120"}]}}
    """)

    @Test fun timestamps() {
        assertEquals(1793750400000L, time)
        assertEquals(time + 123, parseRfc3339Ms("2026-11-04T00:00:00.123456+00:00"))
        assertEquals(time, parseRfc3339Ms("2026-11-04T09:00:00+09:00"))
        assertEquals(time, parseRfc3339Ms("2026-11-03t19:00:00-05:00"))
        assertEquals(0L, parseRfc3339Ms("1970-01-01T00:00:00Z"))
        assertNull(parseRfc3339Ms("garbage"))
        assertNull(parseRfc3339Ms("2026-02-29T00:00:00Z"))
        assertNotNull(parseRfc3339Ms("2024-02-29T00:00:00Z"))
        assertNull(parseRfc3339Ms("2026-11-04T24:00:00Z"))
        assertNull(parseRfc3339Ms("2026-11-04T00:00:00+24:00"))
    }

    @Test fun recoveringStamina() {
        val gauge = Gauge(100, 200, time, time + 100 * 480000L, 480)
        assertEquals(100, gauge.valueAt(time))
        assertEquals(100, gauge.valueAt(time + 479999))
        assertEquals(101, gauge.valueAt(time + 480000))
        assertEquals(101, gauge.valueAt(time + 481000))
        assertEquals(100, gauge.valueAt(time - 1000))
        assertEquals(200, gauge.valueAt(gauge.fullAtMs!!))
        assertEquals(200, gauge.valueAt(gauge.fullAtMs + 1000))
        assertEquals(200, gauge.at(gauge.fullAtMs).current)
        assertEquals(gauge.fullAtMs, gauge.at(time + 480000).fullAtMs)
        assertEquals(48000L, gauge.remainingSecondsAt(time))
        assertEquals(48000L, gauge.remainingSecondsAt(time + 1))
        assertEquals(1L, gauge.remainingSecondsAt(gauge.fullAtMs - 1))
        assertEquals(0L, gauge.remainingSecondsAt(gauge.fullAtMs + 1))
        assertEquals(250, gauge.copy(current = 250).valueAt(time + 100000000))
        assertEquals(0L, gauge.copy(current = 250).remainingSecondsAt(time))
        assertEquals(100, gauge.copy(fullAtMs = null).valueAt(time + 100000000))
        assertNull(gauge.copy(fullAtMs = null).remainingSecondsAt(time))
        val rail = Gauge(100, 300, time, time + 200 * 360000L, 360)
        assertEquals(101, rail.valueAt(time + 360000))
    }

    @Test fun linearCurrency() {
        val gauge = Gauge(1000, 2400, time, time + 14 * 3600000L, null)
        assertEquals(1700, gauge.valueAt(time + 7 * 3600000L))
        assertEquals(1000, gauge.valueAt(time - 1))
        assertEquals(2400, gauge.valueAt(time + 14 * 3600000L))
        assertEquals(2400, gauge.copy(fullAtMs = time).valueAt(time))
    }

    @Test fun expeditionsAndDaily() {
        val ex = WidgetSnapshot.fromStatus(genshin())!!.expeditions!!
        assertEquals(5, ex.total)
        assertEquals(5, ex.max)
        assertEquals(3, ex.finishedAt(time))
        assertEquals(120L, ex.nextRemainingSecondsAt(time))
        assertEquals(120L, ex.nextRemainingSecondsAt(time + 1))
        assertEquals(3600L, ex.allDoneRemainingSecondsAt(time))
        assertEquals(4, ex.finishedAt(time + 120000))
        assertEquals(3480L, ex.nextRemainingSecondsAt(time + 120000))
        assertNull(ex.nextRemainingSecondsAt(time + 3600000))
        assertEquals(0L, ex.allDoneRemainingSecondsAt(time + 3600000))
        assertNull(Expeditions(emptyList(), null).nextRemainingSecondsAt(time))
        assertEquals(0L, Expeditions(emptyList(), null).allDoneRemainingSecondsAt(time))
        assertTrue(Daily(4, 4, true).complete)
        assertFalse(Daily(4, 4, false).complete)
        assertTrue(Daily(500, 500, null).complete)
        assertFalse(Daily(3, 4, true).complete)
    }

    @Test fun snapshotsAndProjection() {
        val g = WidgetSnapshot.fromStatusJson(genshin().toString())!!
        assertEquals(WidgetGame.GENSHIN, g.game)
        assertEquals("123456789", g.gameUid)
        assertFalse(g.stale)
        assertTrue(g.daily!!.complete)
        assertEquals(101, g.at(time + 480000).stamina!!.current)
        assertEquals(1700, g.at(time + 7 * 3600000L).currency!!.current)
        assertEquals(g.expeditions, g.at(time + 480000).expeditions)
        assertEquals(time, g.at(time + 480000).fetchedAtMs)
        val rail = WidgetSnapshot.fromStatus(starrail())!!
        assertEquals(WidgetGame.STARRAIL, rail.game)
        assertNull(rail.currency)
        assertEquals(50, rail.reserveStamina)
        assertTrue(rail.stale)
        assertTrue(rail.daily!!.complete)
        assertEquals(1, rail.expeditions!!.finishedAt(time))
        assertEquals(60L, rail.expeditions.nextRemainingSecondsAt(time))
        assertEquals(120L, rail.expeditions.allDoneRemainingSecondsAt(time))
        assertEquals(3, rail.expeditions.total)
        assertEquals(4, rail.expeditions.max)
        assertEquals(WeeklyProgress(8000, 14000), rail.weeklyPoints)
        assertFalse(rail.weeklyPoints!!.complete)
        assertTrue(WeeklyProgress(14000, 14000).complete)
        assertFalse(WeeklyProgress(0, 0).complete)
        assertFalse(rail.reserveFull)
        val full = starrail()
        full.getJSONObject("data").put("is_reserve_stamina_full", true).put("max_rogue_score", JSONObject.NULL)
        assertTrue(WidgetSnapshot.fromStatus(full)!!.reserveFull)
        assertNull(WidgetSnapshot.fromStatus(full)!!.weeklyPoints)
        assertNull(g.weeklyPoints)
        assertFalse(g.reserveFull)
        assertNull(WidgetSnapshot.fromStatus(genshin().put("game_kind", "other")))
        assertNull(WidgetSnapshot.fromStatus(genshin().put("data", JSONObject.NULL)))
        assertNull(WidgetSnapshot.fromStatus(genshin().put("data", "invalid")))
        assertNull(WidgetSnapshot.fromStatus(genshin().put("fetched_at", "bad")))
        assertNull(WidgetSnapshot.fromStatus(genshin().put("fetched_at", JSONObject.NULL)))
        assertNull(WidgetSnapshot.fromStatusJson("bad"))
    }

    @Test fun nullableFields() {
        val minimal = genshin().put("data", JSONObject("""{"current_resin":1,"expeditions":null}"""))
        val snapshot = WidgetSnapshot.fromStatus(minimal)!!
        assertNull(snapshot.stamina)
        assertNull(snapshot.currency)
        assertNull(snapshot.daily)
        assertNull(snapshot.expeditions)
        val data = minimal.getJSONObject("data").put("max_resin", 200).put("resin_full_at", "bad")
        assertNull(WidgetSnapshot.fromStatus(minimal)!!.stamina!!.fullAtMs)
        data.put("current_resin", JSONObject.NULL)
        assertNull(WidgetSnapshot.fromStatus(minimal)!!.stamina)
    }

    @Test fun storedStatesAndRoundTrip() {
        val game = WidgetGame.GENSHIN
        assertSame(WidgetState.SignedOut, widgetStateFromStored(game, null))
        assertSame(WidgetState.SignedOut, widgetStateFromStored(game, "garbage"))
        assertSame(WidgetState.SignedOut, widgetStateFromStored(game, storedRecord("signed_out", null, null, false)))
        assertSame(WidgetState.NoAccount, widgetStateFromStored(game, storedRecord("no_account", null, null, false)))
        val ready = storedRecord("ready", "synthetic-account", genshin(), true)
        val decoded = widgetStateFromStored(game, ready) as WidgetState.Ready
        assertTrue(decoded.refreshFailed)
        assertEquals(WidgetSnapshot.fromStatus(genshin()), decoded.snapshot)
        assertEquals("synthetic-account", JSONObject(ready).getString("account"))
        assertSame(WidgetState.Unavailable, widgetStateFromStored(WidgetGame.STARRAIL, ready))
        assertSame(WidgetState.Unavailable, widgetStateFromStored(game, storedRecord("ready", "synthetic-account", null, false)))
        assertSame(WidgetState.Unavailable, widgetStateFromStored(game, storedRecord("ready", "synthetic-account", JSONObject(), false)))
    }
}
