package dev.dioxus.main

import android.app.Activity
import android.appwidget.AppWidgetManager
import android.content.Context
import android.content.res.Configuration
import android.graphics.Color
import android.os.Bundle
import android.util.TypedValue
import android.view.View
import android.view.ViewGroup
import android.widget.FrameLayout
import android.widget.HorizontalScrollView
import android.widget.LinearLayout
import android.widget.ScrollView
import android.widget.TextView

// Debug builds only (scripts/build-android.sh adds it to the manifest): both widgets in every
// state (normal, near the cap, at the cap, everything done, failed refresh, no values, signed
// out, no account, no status), regular and compact, in light and dark mode. They are drawn by
// the same code and inflated from the same RemoteViews a placed widget gets (RemoteViews.apply),
// so layouts and drawables are checked without a backend or placing a widget:
//   adb shell am start -n cc.attodao.ahd/dev.dioxus.main.AhdIndicatorSamplesActivity
// The values are made up here; nothing is read from the app's data. Extras: `--ez night true`
// (only one mode, less scrolling), `--es game genshin|starrail`, `--ei first 3 --ei count 3`
// (only some of the states, to fit a screenshot). Try small densities and large fonts with
// `wm density 560` / `settings put system font_scale 2.0`.
class AhdIndicatorSamplesActivity : Activity() {
    private class Sample(val name: String, val genshin: WidgetState, val starRail: WidgetState)

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        title = "AHD widget samples"
        val nights = if (intent.hasExtra("night")) listOf(intent.getBooleanExtra("night", false)) else listOf(false, true)
        val games = WidgetGame.entries.filter { intent.getStringExtra("game") in listOf(null, it.key) }
        val column = LinearLayout(this).apply { orientation = LinearLayout.VERTICAL }
        for (night in nights) column.addView(section(AhdLocale.localized(modeContext(night)), night, games))
        setContentView(ScrollView(this).apply { addView(column) })
    }

    private fun modeContext(night: Boolean): Context {
        val config = Configuration(resources.configuration)
        val mode = if (night) Configuration.UI_MODE_NIGHT_YES else Configuration.UI_MODE_NIGHT_NO
        config.uiMode = (config.uiMode and Configuration.UI_MODE_NIGHT_MASK.inv()) or mode
        return createConfigurationContext(config)
    }

    private fun section(context: Context, night: Boolean, games: List<WidgetGame>): View {
        val section = LinearLayout(context).apply {
            orientation = LinearLayout.VERTICAL
            setBackgroundColor(Color.parseColor(if (night) "#0C0B0E" else "#F5F4F8"))
            setPadding(dp(12), dp(12), dp(12), dp(12))
        }
        val now = System.currentTimeMillis()
        val first = intent.getIntExtra("first", 0)
        for (sample in samples(now).drop(first).take(intent.getIntExtra("count", Int.MAX_VALUE))) {
            section.addView(TextView(context).apply {
                text = "${if (night) "Dark" else "Light"} · ${sample.name}"
                setTextSize(TypedValue.COMPLEX_UNIT_SP, 13f)
                setTextColor(Color.parseColor(if (night) "#F8F7FC" else "#201C29"))
                setPadding(0, dp(12), 0, dp(6))
            })
            val row = LinearLayout(context).apply { orientation = LinearLayout.HORIZONTAL }
            for (game in games) {
                val (widget, state) = if (game == WidgetGame.GENSHIN) Pair(GenshinWidget(), sample.genshin) else Pair(StarRailWidget(), sample.starRail)
                // A 3x3 widget on a 1080px phone at density 420, and the size the compact design was drawn at.
                for ((width, height) in listOf(Pair(270, 343), Pair(196, 208))) {
                    val options = Bundle().apply {
                        putInt(AppWidgetManager.OPTION_APPWIDGET_MIN_WIDTH, width)
                        putInt(AppWidgetManager.OPTION_APPWIDGET_MAX_HEIGHT, height)
                    }
                    val frame = FrameLayout(context)
                    frame.addView(widget.render(context, state, now, options).apply(context, frame))
                    row.addView(frame, LinearLayout.LayoutParams(dp(width), dp(height)).apply { marginEnd = dp(8) })
                }
            }
            section.addView(HorizontalScrollView(context).apply { addView(row) })
        }
        return section
    }

    /** Made-up statuses in the server's shape (AhdWidgetData.kt reads them like a real one). */
    private fun samples(now: Long): List<Sample> {
        fun at(seconds: Long): String {
            val format = java.text.SimpleDateFormat("yyyy-MM-dd'T'HH:mm:ss'Z'", java.util.Locale.US)
            format.timeZone = java.util.TimeZone.getTimeZone("UTC")
            return format.format(java.util.Date(now + seconds * 1000))
        }
        fun state(game: String, data: String, failed: Boolean = false, ago: Long = 240): WidgetState = widgetStateFromStored(
            WidgetGame.fromKey(game)!!,
            """{"kind":"ready","account":"sample","failed":$failed,"status":{"game_kind":"$game","game_uid":"800000000","fetched_at":"${at(-ago)}","stale":false,"data":$data}}""",
        )
        fun genshin(resin: Int, coin: Int, done: Int, received: Boolean, expeditions: String) = """{
            "current_resin":$resin,"max_resin":200,"resin_full_at":"${at((200L - resin) * 480)}",
            "current_home_coin":$coin,"max_home_coin":2400,"home_coin_full_at":"${at((2400L - coin) * 120)}",
            "finished_task_num":$done,"total_task_num":4,"is_extra_task_reward_received":$received,
            "max_expedition_num":5,"expeditions":[$expeditions]}"""
        fun starRail(power: Int, reserve: Int, reserveFull: Boolean, train: Int, rogue: Int, expeditions: String) = """{
            "current_stamina":$power,"max_stamina":240,"stamina_full_at":"${at((240L - power) * 360)}",
            "current_reserve_stamina":$reserve,"is_reserve_stamina_full":$reserveFull,
            "current_train_score":$train,"max_train_score":500,"current_rogue_score":$rogue,"max_rogue_score":14000,
            "total_expedition_num":4,"expeditions":[$expeditions]}"""
        val out = """{"status":"Ongoing","remained_time":"7200","remaining_time":7200},{"status":"Ongoing","remained_time":"30360","remaining_time":20000}"""
        val back = """{"status":"Finished","remained_time":"0","remaining_time":0}"""
        val normalGenshin = genshin(120, 1202, 3, false, "$back,$out")
        val normalStarRail = starRail(150, 1200, false, 300, 8000, "$back,$out")
        return listOf(
            Sample("normal", state("genshin", normalGenshin), state("starrail", normalStarRail)),
            Sample("near the cap", state("genshin", genshin(186, 2300, 4, false, "$back,$back,$out")),
                state("starrail", starRail(230, 2390, false, 400, 13000, out))),
            Sample("at the cap, everything done", state("genshin", genshin(200, 2400, 4, true, "$back,$back,$back,$back,$back")),
                state("starrail", starRail(240, 2400, true, 500, 14000, "$back,$back,$back,$back"))),
            Sample("failed refresh, earlier values", state("genshin", normalGenshin, failed = true, ago = 5400),
                state("starrail", normalStarRail, failed = true, ago = 5400)),
            Sample("nothing sent, nothing done", state("genshin", genshin(0, 0, 0, false, "")), state("starrail", starRail(0, 0, false, 0, 0, ""))),
            Sample("no values returned", state("genshin", "{}"), state("starrail", "{}")),
            Sample("signed out", WidgetState.SignedOut, WidgetState.SignedOut),
            Sample("no game account", WidgetState.NoAccount, WidgetState.NoAccount),
            Sample("no status", WidgetState.Unavailable, WidgetState.Unavailable),
        )
    }

    private fun dp(value: Int) = (value * resources.displayMetrics.density).toInt()
}
