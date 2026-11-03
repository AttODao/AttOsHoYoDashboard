package dev.dioxus.main

import org.json.JSONObject

/** Recovery rates supplied by the games. */
enum class WidgetGame(val key: String, val secondsPerStamina: Long) {
    GENSHIN("genshin", 480),
    STARRAIL("starrail", 360);

    companion object {
        fun fromKey(key: String?): WidgetGame? = entries.firstOrNull { it.key == key }
    }
}

data class Gauge(val current: Int, val max: Int, val fetchedAtMs: Long, val fullAtMs: Long?, val secondsPerUnit: Long?) {
    val isFull: Boolean get() = current >= max
    fun valueAt(nowMs: Long): Int {
        if (isFull || fullAtMs == null || nowMs < fetchedAtMs) return current
        val remainingMs = fullAtMs - nowMs
        if (remainingMs <= 0) return max
        val estimate = if (secondsPerUnit != null) {
            val unitMs = secondsPerUnit * 1000
            max.toLong() - (remainingMs + unitMs - 1) / unitMs
        } else {
            val span = fullAtMs - fetchedAtMs
            if (span <= 0) return max
            current.toLong() + (max.toLong() - current) * (nowMs - fetchedAtMs) / span
        }
        return estimate.coerceIn(current.toLong(), max.toLong()).toInt()
    }
    fun remainingSecondsAt(nowMs: Long): Long? = when {
        isFull -> 0
        fullAtMs == null -> null
        else -> ((fullAtMs - nowMs).coerceAtLeast(0) + 999) / 1000
    }
    fun at(nowMs: Long): Gauge = copy(current = valueAt(nowMs))
}

data class Expeditions(val finishAtMs: List<Long>, val max: Int?) {
    val total: Int get() = finishAtMs.size
    fun finishedAt(nowMs: Long): Int = finishAtMs.count { it <= nowMs }
    fun nextRemainingSecondsAt(nowMs: Long): Long? = finishAtMs.filter { it > nowMs }.minOrNull()?.let { (it - nowMs + 999) / 1000 }
    fun allDoneRemainingSecondsAt(nowMs: Long): Long = ((finishAtMs.maxOrNull()?.minus(nowMs) ?: 0).coerceAtLeast(0) + 999) / 1000
}

data class Daily(val current: Int, val max: Int, val extraRewardReceived: Boolean?) {
    val complete: Boolean get() = current >= max && extraRewardReceived != false
}

/** A capped weekly counter that does not recover over time (Star Rail Simulated Universe points). */
data class WeeklyProgress(val current: Int, val max: Int) {
    val complete: Boolean get() = max > 0 && current >= max
}

data class WidgetSnapshot(
    val game: WidgetGame,
    val gameUid: String,
    val fetchedAtMs: Long,
    val stale: Boolean,
    val stamina: Gauge?,
    val reserveStamina: Int?,
    val currency: Gauge?,
    val expeditions: Expeditions?,
    val daily: Daily?,
    /** Star Rail: Simulated Universe weekly points (the widget's realm-currency slot). */
    val weeklyPoints: WeeklyProgress? = null,
    /** Star Rail: the reserve is at its cap (is_reserve_stamina_full). */
    val reserveFull: Boolean = false,
) {
    fun at(nowMs: Long): WidgetSnapshot = copy(stamina = stamina?.at(nowMs), currency = currency?.at(nowMs))
    companion object {
        fun fromStatusJson(text: String): WidgetSnapshot? = try { fromStatus(JSONObject(text)) } catch (_: Exception) { null }
        fun fromStatus(status: JSONObject): WidgetSnapshot? {
            val game = WidgetGame.fromKey(status.optString("game_kind")) ?: return null
            val data = status.optJSONObject("data") ?: return null
            val fetched = parseRfc3339Ms(status.optString("fetched_at")) ?: return null
            fun gauge(current: String, max: String, full: String, rate: Long?): Gauge? {
                val c = data.nullableInt(current) ?: return null
                val m = data.nullableInt(max) ?: return null
                return Gauge(c, m, fetched, parseRfc3339Ms(data.optString(full)), rate)
            }
            val genshin = game == WidgetGame.GENSHIN
            val currentDaily = data.nullableInt(if (genshin) "finished_task_num" else "current_train_score")
            val maxDaily = data.nullableInt(if (genshin) "total_task_num" else "max_train_score")
            val expeditions = data.optJSONArray("expeditions")?.let { array ->
                Expeditions((0 until array.length()).mapNotNull { index ->
                    array.optJSONObject(index)?.let { expedition ->
                        val seconds = expedition.optString(if (genshin) "remained_time" else "remaining_time").toLongOrNull() ?: 0
                        fetched + if (expedition.optString("status") == "Finished" || seconds <= 0) 0 else seconds * 1000
                    }
                }, data.nullableInt(if (genshin) "max_expedition_num" else "total_expedition_num"))
            }
            return WidgetSnapshot(
                game, status.optString("game_uid"), fetched, status.optBoolean("stale"),
                if (genshin) gauge("current_resin", "max_resin", "resin_full_at", game.secondsPerStamina)
                else gauge("current_stamina", "max_stamina", "stamina_full_at", game.secondsPerStamina),
                if (genshin) null else data.nullableInt("current_reserve_stamina"),
                if (genshin) gauge("current_home_coin", "max_home_coin", "home_coin_full_at", null) else null,
                expeditions,
                if (currentDaily != null && maxDaily != null) Daily(currentDaily, maxDaily,
                    if (genshin && !data.isNull("is_extra_task_reward_received")) data.optBoolean("is_extra_task_reward_received") else null) else null,
                if (genshin) null else data.nullableInt("current_rogue_score")?.let { current ->
                    data.nullableInt("max_rogue_score")?.let { WeeklyProgress(current, it) }
                },
                !genshin && data.optBoolean("is_reserve_stamina_full"),
            )
        }
    }
}

private fun JSONObject.nullableInt(key: String): Int? = if (isNull(key)) null else optInt(key)

/** RFC 3339 without java.time (Android API 24). Fractional seconds are truncated. */
internal fun parseRfc3339Ms(text: String): Long? {
    val match = Regex("^(\\d{4})-(\\d{2})-(\\d{2})[Tt ](\\d{2}):(\\d{2}):(\\d{2})(\\.\\d+)?([Zz]|[+-]\\d{2}:\\d{2})$").matchEntire(text) ?: return null
    val g = match.groupValues
    var year = g[1].toLong()
    val month = g[2].toInt()
    val day = g[3].toInt()
    val hour = g[4].toInt()
    val minute = g[5].toInt()
    val second = g[6].toInt()
    val leap = year % 4 == 0L && (year % 100 != 0L || year % 400 == 0L)
    val lengths = intArrayOf(31, if (leap) 29 else 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31)
    if (month !in 1..12 || day !in 1..lengths[month - 1] || hour !in 0..23 || minute !in 0..59 || second !in 0..59) return null
    val zone = g[8]
    var offset = 0L
    if (zone != "Z" && zone != "z") {
        val hours = zone.substring(1, 3).toInt()
        val minutes = zone.substring(4, 6).toInt()
        if (hours > 23 || minutes > 59) return null
        offset = (hours * 60L + minutes) * 60 * if (zone[0] == '-') -1 else 1
    }
    year -= if (month <= 2) 1 else 0
    val era = (if (year >= 0) year else year - 399) / 400
    val yoe = year - era * 400
    val doy = (153 * (month + if (month > 2) -3 else 9) + 2) / 5 + day - 1
    val doe = yoe * 365 + yoe / 4 - yoe / 100 + doy
    val days = era * 146097 + doe - 719468
    val fraction = g[7].removePrefix(".").padEnd(3, '0').take(3).toLong()
    return (days * 86400 + hour * 3600 + minute * 60 + second - offset) * 1000 + fraction
}

/** The app page a widget tap opens: the dashboard on [game]'s tab (app/navigation.rs target_game). */
internal fun widgetOpenTarget(game: WidgetGame): String = "/dashboard?game=${game.key}"

sealed class WidgetState {
    object SignedOut : WidgetState()
    object NoAccount : WidgetState()
    object Unavailable : WidgetState()
    data class Ready(val snapshot: WidgetSnapshot, val refreshFailed: Boolean) : WidgetState()
}

internal fun widgetStateFromStored(game: WidgetGame, json: String?): WidgetState {
    val record = try { JSONObject(json ?: return WidgetState.SignedOut) } catch (_: Exception) { return WidgetState.SignedOut }
    return when (record.optString("kind")) {
        "no_account" -> WidgetState.NoAccount
        "ready" -> record.optJSONObject("status")?.let { WidgetSnapshot.fromStatus(it) }?.takeIf { it.game == game }
            ?.let { WidgetState.Ready(it, record.optBoolean("failed")) } ?: WidgetState.Unavailable
        else -> WidgetState.SignedOut
    }
}

internal fun storedRecord(kind: String, account: String?, status: JSONObject?, failed: Boolean): String = JSONObject()
    .put("kind", kind).put("account", account ?: JSONObject.NULL).put("status", status ?: JSONObject.NULL).put("failed", failed).toString()
