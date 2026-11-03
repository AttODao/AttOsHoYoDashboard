package dev.dioxus.main

import android.app.PendingIntent
import android.app.job.JobInfo
import android.app.job.JobParameters
import android.app.job.JobScheduler
import android.app.job.JobService
import android.appwidget.AppWidgetManager
import android.appwidget.AppWidgetProvider
import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.os.Bundle
import android.util.Log
import android.widget.RemoteViews
import org.json.JSONObject
import java.net.HttpURLConnection
import java.net.URL
import java.net.URLEncoder
import java.text.DateFormat
import java.util.Date

object AhdWidgets {
    const val JOB_ID = 0x41484432
    private fun prefs(context: Context) = context.getSharedPreferences("ahd_widgets", Context.MODE_PRIVATE)
    private inline fun <T> guarded(fallback: T, operation: () -> T): T = try { operation() } catch (error: Exception) {
        Log.w("AHD", "Home widget operation failed", error)
        fallback
    }

    @Synchronized @JvmStatic fun publish(context: Context, origin: String, json: String): Boolean = guarded(false) {
        val payload = JSONObject(json)
        val preferences = prefs(context)
        val edit = preferences.edit().putString("origin", origin).putString("agent", payload.optString("agent"))
        var needsRefresh = false
        for (game in WidgetGame.entries) {
            val entry = payload.optJSONObject("games")?.optJSONObject(game.key)
            val account = entry?.takeUnless { it.isNull("account") }?.optString("account")
            val status = entry?.optJSONObject("status")
            val key = "game.${game.key}"
            val previous = guarded(JSONObject()) { JSONObject(preferences.getString(key, null) ?: "{}") }
            val record = if (account == null) storedRecord("no_account", null, null, false)
            else if (status != null) storedRecord("ready", account, status, false)
            else {
                needsRefresh = true
                if (previous.optString("account") == account) previous.toString()
                else storedRecord("ready", account, null, false)
            }
            edit.putString(key, record)
        }
        if (!edit.commit()) return@guarded false
        schedule(context)
        renderAll(context)
        if (needsRefresh) {
            val app = context.applicationContext
            Thread { refresh(app) }.start()
        }
        true
    }

    @Synchronized @JvmStatic fun signOut(context: Context): Boolean = guarded(false) {
        val committed = prefs(context).edit().clear().commit()
        context.getSystemService(JobScheduler::class.java).cancel(JOB_ID)
        renderAll(context)
        committed
    }

    @JvmStatic fun state(context: Context, game: WidgetGame): WidgetState = guarded(WidgetState.SignedOut as WidgetState) {
        widgetStateFromStored(game, prefs(context).getString("game.${game.key}", null))
    }

    @Synchronized @JvmStatic fun refresh(context: Context): Boolean = guarded(false) {
        val preferences = prefs(context)
        val origin = preferences.getString("origin", null) ?: return@guarded true
        var ok = true
        for (game in WidgetGame.entries) {
            val key = "game.${game.key}"
            val record = JSONObject(preferences.getString(key, null) ?: "{}")
            if (record.isNull("account")) continue
            val account = record.getString("account")
            var connection: HttpURLConnection? = null
            val fetched = guarded(false) {
                val cookie = AhdInbox.cookieHeader(context)
                if (cookie == null) {
                    signOut(context)
                    return@guarded false
                }
                val request = URL("$origin/api/v1/game-accounts/${URLEncoder.encode(account, "UTF-8")}/status").openConnection() as HttpURLConnection
                connection = request
                request.instanceFollowRedirects = false
                request.connectTimeout = 15000
                request.readTimeout = 15000
                request.setRequestProperty("Accept", "application/json")
                request.setRequestProperty("Origin", origin)
                request.setRequestProperty("Cookie", cookie)
                request.setRequestProperty("User-Agent", preferences.getString("agent", ""))
                when (request.responseCode) {
                    200 -> {
                        val status = request.inputStream.bufferedReader().use { JSONObject(it.readText()) }
                        if (status.optString("game_kind") == game.key) {
                            preferences.edit().putString(key, storedRecord("ready", account, status, false)).commit()
                        } else false
                    }
                    401 -> { signOut(context); false }
                    else -> false
                }
            }
            connection?.disconnect()
            // A 401/no cookie cleared the complete store. Never resurrect that account.
            if (!preferences.contains("origin")) return@guarded false
            if (!fetched) {
                ok = false
                record.put("failed", true)
                preferences.edit().putString(key, record.toString()).commit()
            }
        }
        renderAll(context)
        ok
    }

    @JvmStatic fun schedule(context: Context) { guarded(Unit) {
        if (!prefs(context).contains("origin")) return@guarded
        val scheduler = context.getSystemService(JobScheduler::class.java)
        if (scheduler.getPendingJob(JOB_ID) == null) scheduler.schedule(
            JobInfo.Builder(JOB_ID, ComponentName(context, AhdWidgetJob::class.java))
                .setPeriodic(30 * 60 * 1000L).setRequiredNetworkType(JobInfo.NETWORK_TYPE_ANY).setPersisted(true).build()
        )
    } }

    @JvmStatic fun renderAll(context: Context) { guarded(Unit) {
        val manager = AppWidgetManager.getInstance(context)
        for (provider in listOf(GenshinWidget(), StarRailWidget())) {
            val ids = manager.getAppWidgetIds(ComponentName(context, provider.javaClass))
            if (ids.isNotEmpty()) provider.updateWidgets(context, manager, ids)
        }
    } }
}

class AhdWidgetJob : JobService() {
    override fun onStartJob(params: JobParameters): Boolean {
        Thread { jobFinished(params, !AhdWidgets.refresh(applicationContext)) }.start()
        return true
    }
    override fun onStopJob(params: JobParameters): Boolean = true
}

/** Shared state, tap target and periodic refresh; game widgets override render only. */
abstract class AhdGameWidget(val game: WidgetGame) : AppWidgetProvider() {
    override fun onUpdate(context: Context, manager: AppWidgetManager, ids: IntArray) = updateWidgets(context, manager, ids)
    // Resizing changes the size the layout is chosen for (render with options).
    override fun onAppWidgetOptionsChanged(context: Context, manager: AppWidgetManager, id: Int, options: Bundle) =
        updateWidgets(context, manager, intArrayOf(id))
    // Texts set from code (values, times) follow the locale only when drawn again; the launcher
    // re-inflates the layout itself (receiver intent-filter in AndroidManifest.application.xml).
    override fun onReceive(context: Context, intent: Intent) {
        if (intent.action == Intent.ACTION_LOCALE_CHANGED) AhdWidgets.renderAll(context) else super.onReceive(context, intent)
    }
    override fun onEnabled(context: Context) {
        AhdWidgets.schedule(context)
        val app = context.applicationContext
        Thread { AhdWidgets.refresh(app) }.start()
    }
    fun updateWidgets(context: Context, manager: AppWidgetManager, ids: IntArray) {
        val now = System.currentTimeMillis()
        val state = AhdWidgets.state(context, game)
        for (id in ids) {
            val views = render(context, state, now, manager.getAppWidgetOptions(id))
            views.setOnClickPendingIntent(rootId(context), openApp(context))
            manager.updateAppWidget(id, views)
        }
    }
    open fun render(context: Context, state: WidgetState, nowMs: Long): RemoteViews = placeholder(context, state)
    /** Size-aware variant: [options] holds the AppWidgetManager.OPTION_APPWIDGET_* sizes in dp. */
    open fun render(context: Context, state: WidgetState, nowMs: Long, options: Bundle): RemoteViews = render(context, state, nowMs)
    protected fun rootId(context: Context): Int = context.resources.getIdentifier("ahd_widget_root", "id", context.packageName)
    protected fun placeholder(context: Context, state: WidgetState): RemoteViews {
        fun id(name: String, type: String) = context.resources.getIdentifier(name, type, context.packageName)
        fun text(name: String, fallback: String): String = id(name, "string").let { if (it == 0) fallback else context.getString(it) }
        val views = RemoteViews(context.packageName, id("ahd_widget_${game.key}", "layout"))
        views.setTextViewText(id("ahd_widget_title", "id"), text("ahd_widget_${game.key}_label", if (game == WidgetGame.GENSHIN) "Genshin Impact" else "Honkai: Star Rail"))
        val status = when (state) {
            WidgetState.SignedOut -> text("ahd_widget_sign_in", "Open AHD and sign in")
            WidgetState.NoAccount -> text("ahd_widget_no_account", "Open AHD and link a game account")
            WidgetState.Unavailable -> text("ahd_widget_unavailable", "Could not load the status. Open AHD to retry.")
            is WidgetState.Ready -> String.format(text("ahd_widget_placeholder_ready", "Updated %1\$s"), DateFormat.getTimeInstance(DateFormat.SHORT).format(Date(state.snapshot.fetchedAtMs)))
        }
        views.setTextViewText(id("ahd_widget_status", "id"), status)
        return views
    }
    // Opens the dashboard on this game's tab (Rust: app/navigation.rs target_game).
    protected fun openApp(context: Context): PendingIntent = PendingIntent.getActivity(context, 0x4157 + game.ordinal,
        Intent(context, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_CLEAR_TOP or Intent.FLAG_ACTIVITY_SINGLE_TOP)
            .putExtra(AhdNotifications.EXTRA_OPEN, widgetOpenTarget(game)), PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE)
}
