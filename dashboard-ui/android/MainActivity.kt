package dev.dioxus.main

import android.Manifest
import android.app.Activity
import android.app.AlarmManager
import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.job.JobInfo
import android.app.job.JobParameters
import android.app.job.JobScheduler
import android.app.job.JobService
import android.content.ComponentName
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.content.res.Configuration
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.provider.Settings
import android.util.Log
import android.webkit.WebView
import androidx.activity.OnBackPressedCallback
import kotlin.math.roundToInt
import java.io.File
import java.net.HttpURLConnection
import java.net.URL
import java.net.URLEncoder
import org.json.JSONArray
import org.json.JSONObject

typealias BuildConfig = cc.attodao.ahd.BuildConfig

// tao/wry cannot survive a destroyed Activity: the Rust event loop ends and the process
// aborts ("pthread_mutex_lock called on a destroyed mutex"), and a second instance aborts
// in create(). So there is only ever one instance (launchMode singleTask, set with the
// manifest's configChanges by scripts/build-android.sh), it is never finished or recreated
// on purpose (Back on the first history entry moves the app to the background like Home,
// configuration changes are applied in place), and when the system destroys it anyway
// (task swiped away, unhandled change) the process ends without running native teardown.
class MainActivity : WryActivity() {
    private var webView: WebView? = null

    override fun onCreate(savedInstanceState: Bundle?) {
        if (savedInstanceState == null) {
            AhdNotifications.rememberOpenTarget(intent)
            AhdBrowserLogin.remember(intent)
        }
        super.onCreate(savedInstanceState)
        // WryActivity.onKeyDown still goes back in the WebView history first.
        onBackPressedDispatcher.addCallback(this, object : OnBackPressedCallback(true) {
            override fun handleOnBackPressed() {
                moveTaskToBack(true)
            }
        })
    }

    override fun onWebViewCreate(webView: WebView) {
        this.webView = webView
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        setIntent(intent)
        if (AhdNotifications.rememberOpenTarget(intent)) notifyPage("open")
        if (AhdBrowserLogin.remember(intent)) notifyBrowserLogin()
    }
    override fun onRequestPermissionsResult(requestCode: Int, permissions: Array<out String>, grantResults: IntArray) {
        super.onRequestPermissionsResult(requestCode, permissions, grantResults)
        if (requestCode == AhdNotifications.PERMISSION_REQUEST) notifyPage("permission")
    }
    private fun notifyBrowserLogin() {
        webView?.post {
            webView?.evaluateJavascript("window.__ahdBrowserLoginEvent && window.__ahdBrowserLoginEvent()", null)
        }
    }
    private fun notifyPage(kind: String) {
        webView?.post {
            webView?.evaluateJavascript("window.__ahdNotificationEvent && window.__ahdNotificationEvent(\"$kind\")", null)
        }
    }

    override fun onDestroy() {
        // SIGKILL instead of tao's exit(): Android starts a fresh process when the app is
        // opened again.
        android.os.Process.killProcess(android.os.Process.myPid())
        super.onDestroy()
    }

    override fun onConfigurationChanged(newConfig: Configuration) {
        super.onConfigurationChanged(newConfig)
        // The WebView takes the system font size only when it is created.
        webView?.settings?.textZoom = (newConfig.fontScale * 100).roundToInt()
    }
}

// Browser sign-in callback (cc.attodao.ahd://login?code=…); Rust takes it once and validates it.
object AhdBrowserLogin {
    private var callback: String? = null
    @JvmStatic @Synchronized fun remember(intent: Intent?): Boolean = try {
        val data = intent?.data
        if (intent?.action == Intent.ACTION_VIEW && data?.scheme == "cc.attodao.ahd" && data.host == "login") { callback = data.toString(); true } else false
    } catch (error: Throwable) { false }
    @JvmStatic @Synchronized fun takeCallback(): String? { val value = callback; callback = null; return value }
}

object AhdNotifications {
    const val CHANNEL_ID = "game_status"
    const val PERMISSION_REQUEST = 0x4148
    const val EXTRA_OPEN = "cc.attodao.ahd.OPEN"
    const val ACTION_ALARM = "cc.attodao.ahd.NOTIFY"
    @Volatile private var openTarget: String? = null
    private fun prefs(context: Context) = context.getSharedPreferences("ahd_notifications", Context.MODE_PRIVATE)
    private inline fun <T> guarded(fallback: T, block: () -> T): T = try { block() } catch (error: Throwable) {
        Log.w("AHD", "Device notification operation failed", error)
        fallback
    }
    @JvmStatic fun rememberOpenTarget(intent: Intent?): Boolean = guarded(false) {
        val target = intent?.getStringExtra(EXTRA_OPEN)
        if (target == null) false else { intent.removeExtra(EXTRA_OPEN); openTarget = target; true }
    }
    @JvmStatic fun takeOpenTarget(): String? = guarded(null) { val target = openTarget; openTarget = null; target }
    @JvmStatic fun permissionGranted(context: Context): Boolean = guarded(false) {
        (Build.VERSION.SDK_INT < 33 || context.checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) == PackageManager.PERMISSION_GRANTED) &&
            context.getSystemService(NotificationManager::class.java).areNotificationsEnabled()
    }
    @JvmStatic fun requestPermission(activity: Activity): Unit = guarded(Unit) {
        activity.runOnUiThread {
            guarded(Unit) {
                if (Build.VERSION.SDK_INT >= 33) activity.requestPermissions(arrayOf(Manifest.permission.POST_NOTIFICATIONS), PERMISSION_REQUEST)
                else openSettings(activity)
            }
        }
    }
    @JvmStatic fun openSettings(activity: Activity): Unit = guarded(Unit) {
        activity.runOnUiThread {
            guarded(Unit) {
                val intent = if (Build.VERSION.SDK_INT >= 26) Intent(Settings.ACTION_APP_NOTIFICATION_SETTINGS).putExtra(Settings.EXTRA_APP_PACKAGE, activity.packageName)
                    else Intent(Settings.ACTION_APPLICATION_DETAILS_SETTINGS, Uri.parse("package:${activity.packageName}"))
                activity.startActivity(intent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
            }
        }
    }
    private fun ensureChannel(context: Context, name: String) {
        if (Build.VERSION.SDK_INT >= 26) context.getSystemService(NotificationManager::class.java)
            .createNotificationChannel(NotificationChannel(CHANNEL_ID, name, NotificationManager.IMPORTANCE_DEFAULT))
    }
    private fun alarmIntent(context: Context, entry: JSONObject, cancel: Boolean = false): PendingIntent? {
        val intent = Intent(context, AhdAlarmReceiver::class.java).setAction(ACTION_ALARM)
            .putExtra("id", entry.getInt("id")).putExtra("title", entry.getString("title")).putExtra("body", entry.getString("body"))
            .putExtra("tag", entry.optString("tag", ""))
        return PendingIntent.getBroadcast(context, entry.getInt("id"), intent,
            (if (cancel) PendingIntent.FLAG_NO_CREATE else PendingIntent.FLAG_UPDATE_CURRENT) or PendingIntent.FLAG_IMMUTABLE)
    }
    @JvmStatic fun schedule(context: Context, channelName: String, json: String): Boolean = guarded(false) {
        val entries = JSONArray(json)
        val old = JSONArray(prefs(context).getString("entries", "[]"))
        val oldKeys = (0 until old.length()).map { i ->
            val entry = old.getJSONObject(i)
            "${entry.getInt("id")}:${entry.getLong("at")}"
        }.toSet()
        val alarms = context.getSystemService(AlarmManager::class.java)
        for (i in 0 until old.length()) alarmIntent(context, old.getJSONObject(i), true)?.let { alarms.cancel(it); it.cancel() }
        val now = System.currentTimeMillis()
        val scheduled = JSONArray()
        ensureChannel(context, channelName)
        for (i in 0 until entries.length()) {
            val entry = entries.getJSONObject(i)
            val at = entry.getLong("at")
            val key = "${entry.getInt("id")}:$at"
            if (at > now || key in oldKeys) {
                scheduled.put(entry)
                alarms.setAndAllowWhileIdle(AlarmManager.RTC_WAKEUP, at, alarmIntent(context, entry)!!)
            }
        }
        prefs(context).edit().putString("entries", scheduled.toString()).putString("channel", channelName).apply()
        true
    }
    // The page to open is a dashboard path ("/settings/bonus"); MainActivity hands it to the page.
    private fun openIntent(context: Context, request: Int, target: String): PendingIntent = PendingIntent.getActivity(context, request,
        Intent(context, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_CLEAR_TOP or Intent.FLAG_ACTIVITY_SINGLE_TOP)
            .putExtra(EXTRA_OPEN, target), PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE)
    @JvmStatic fun deliver(context: Context, id: Int, title: String, body: String, tag: String): Unit = guarded(Unit) {
        val entries = JSONArray(prefs(context).getString("entries", "[]"))
        val remaining = JSONArray()
        for (i in 0 until entries.length()) { val entry = entries.getJSONObject(i); if (entry.getInt("id") != id) remaining.put(entry) }
        prefs(context).edit().putString("entries", remaining.toString()).apply()
        if (tag.isEmpty()) post(context, id, title, body) else postTagged(context, tag, title, body, "/dashboard", "local")
        Unit
    }
    // A predicted (local) and a server notice for the same condition share the tag
    // "<game account>:<kind>"; the second one within this window is not shown again.
    private const val SAME_TAG_WINDOW_MS = 2 * 60 * 60 * 1000L
    private const val TAGGED_ID = 0x4148
    /**
     * Shows a notice unless the other source ("local" prediction or "server") showed the same
     * tag recently; a still-visible one with the same tag is replaced. Each source already
     * reports a condition once, so a repeat from the same source is a new occurrence.
     */
    @Synchronized @JvmStatic fun postTagged(context: Context, tag: String, title: String, body: String, target: String, source: String): Boolean = guarded(false) {
        val now = System.currentTimeMillis()
        val recent = JSONObject(prefs(context).getString("recent", "{}"))
        for (key in recent.keys().asSequence().toList()) {
            if (recent.getJSONObject(key).optLong("at", 0L) <= now - SAME_TAG_WINDOW_MS) recent.remove(key)
        }
        val previous = recent.optJSONObject(tag)
        if (previous != null && previous.optString("source") != source) true else {
            val posted = post(context, TAGGED_ID, title, body, tag, target)
            if (posted) recent.put(tag, JSONObject().put("at", now).put("source", source))
            prefs(context).edit().putString("recent", recent.toString()).apply()
            posted
        }
    }
    @JvmStatic fun rememberChannel(context: Context, name: String): Unit = guarded(Unit) {
        prefs(context).edit().putString("channel", name).apply()
    }
    @Suppress("DEPRECATION")
    private fun post(context: Context, id: Int, title: String, body: String, tag: String? = null, target: String = "/dashboard"): Boolean = guarded(false) {
        if (!permissionGranted(context)) false else {
            ensureChannel(context, prefs(context).getString("channel", "AHD") ?: "AHD")
            val builder = if (Build.VERSION.SDK_INT >= 26) Notification.Builder(context, CHANNEL_ID)
                else Notification.Builder(context).setPriority(Notification.PRIORITY_DEFAULT)
            val icon = context.resources.getIdentifier("ic_stat_ahd", "drawable", context.packageName)
            val notification = builder.setSmallIcon(if (icon == 0) android.R.drawable.ic_popup_reminder else icon)
                .setContentTitle(title).setContentText(body).setStyle(Notification.BigTextStyle().bigText(body))
                .setAutoCancel(true).setContentIntent(openIntent(context, tag?.hashCode() ?: id, target)).setShowWhen(true).build()
            context.getSystemService(NotificationManager::class.java).notify(tag, id, notification)
            true
        }
    }
    @JvmStatic fun showNow(context: Context, channelName: String, json: String): Boolean = guarded(false) {
        val entry = JSONObject(json)
        prefs(context).edit().putString("channel", channelName).apply()
        post(context, entry.getInt("id"), entry.getString("title"), entry.getString("body"))
    }
    @JvmStatic fun restore(context: Context): Unit = guarded(Unit) {
        val entries = JSONArray(prefs(context).getString("entries", "[]"))
        val now = System.currentTimeMillis()
        val alarms = context.getSystemService(AlarmManager::class.java)
        for (i in 0 until entries.length()) {
            val entry = entries.getJSONObject(i)
            if (entry.getLong("at") > now) alarms.setAndAllowWhileIdle(AlarmManager.RTC_WAKEUP, entry.getLong("at"), alarmIntent(context, entry)!!)
            else deliver(context, entry.getInt("id"), entry.getString("title"), entry.getString("body"), entry.optString("tag", ""))
        }
    }
}
class AhdAlarmReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        if (intent.action == AhdNotifications.ACTION_ALARM) AhdNotifications.deliver(context, intent.getIntExtra("id", 0), intent.getStringExtra("title") ?: "", intent.getStringExtra("body") ?: "", intent.getStringExtra("tag") ?: "")
    }
}
class AhdBootReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        if (intent.action == Intent.ACTION_BOOT_COMPLETED || intent.action == Intent.ACTION_MY_PACKAGE_REPLACED) {
            AhdNotifications.restore(context)
            AhdInbox.restore(context)
        }
    }
}

// Server notices (the same ones the web's Webhook and browser push get): the app registers
// this installation with the server, then this polls its inbox with the stored session
// cookie, in the foreground (pollNow) and every 15 minutes through JobScheduler.
object AhdInbox {
    private const val JOB_ID = 0x41484431
    private const val PAGES = 5
    private fun prefs(context: Context) = context.getSharedPreferences("ahd_inbox", Context.MODE_PRIVATE)
    private inline fun <T> guarded(fallback: T, block: () -> T): T = try { block() } catch (error: Throwable) {
        Log.w("AHD", "App inbox operation failed", error)
        fallback
    }
    /** {"device","owner"} of the registered installation, or null. */
    @JvmStatic fun state(context: Context): String? = guarded(null) {
        val prefs = prefs(context)
        val device = prefs.getString("device", null)
        if (device == null) null else JSONObject().put("device", device).put("owner", prefs.getString("owner", "")).toString()
    }
    /** {"origin","device","owner","cursor","agent"}; a new device starts at its cursor. */
    @JvmStatic fun configure(context: Context, channelName: String, json: String): Boolean = guarded(false) {
        val config = JSONObject(json)
        val prefs = prefs(context)
        // Schedule first: a job the system refuses leaves no half-configured registration.
        schedule(context)
        val edit = prefs.edit().putString("origin", config.getString("origin")).putString("owner", config.getString("owner"))
            .putString("agent", config.getString("agent"))
        if (prefs.getString("device", null) != config.getString("device")) {
            edit.putString("device", config.getString("device")).putLong("cursor", config.getLong("cursor"))
        }
        edit.commit()
        AhdNotifications.rememberChannel(context, channelName)
        pollNow(context)
        true
    }
    @JvmStatic fun stop(context: Context): Boolean = guarded(false) {
        prefs(context).edit().clear().commit()
        context.getSystemService(JobScheduler::class.java).cancel(JOB_ID)
        true
    }
    @JvmStatic fun pollNow(context: Context): Boolean = guarded(false) {
        restore(context)
        val app = context.applicationContext
        Thread { poll(app) }.start()
        true
    }
    @JvmStatic fun restore(context: Context): Unit = guarded(Unit) {
        if (prefs(context).getString("device", null) != null) schedule(context)
    }
    private fun schedule(context: Context) {
        val scheduler = context.getSystemService(JobScheduler::class.java)
        if (scheduler.getPendingJob(JOB_ID) != null) return
        val job = JobInfo.Builder(JOB_ID, ComponentName(context, AhdInboxJob::class.java))
            .setRequiredNetworkType(JobInfo.NETWORK_TYPE_ANY)
            .setPeriodic(15 * 60 * 1000L)
            .setPersisted(true)
            .build()
        scheduler.schedule(job)
    }
    private fun cookieHeader(context: Context): String? {
        val file = File(context.filesDir, "session-cookies.json")
        if (!file.exists()) return null
        val cookies = JSONObject(file.readText())
        val names = cookies.keys().asSequence().toList()
        return if (names.isEmpty()) null else names.joinToString("; ") { "$it=${cookies.getString(it)}" }
    }
    /** One read pass; returns false when nothing could be fetched. */
    @Synchronized @JvmStatic fun poll(context: Context): Boolean = guarded(false) {
        val prefs = prefs(context)
        val device = prefs.getString("device", null)
        val origin = prefs.getString("origin", null)
        val cookie = cookieHeader(context)
        if (device == null || origin == null || cookie == null) false else {
            var cursor = prefs.getLong("cursor", 0L)
            var ok = true
            for (page in 0 until PAGES) {
                val url = URL("$origin/api/v1/me/notifications/app-devices/${URLEncoder.encode(device, "UTF-8")}/inbox?after=$cursor")
                val connection = url.openConnection() as HttpURLConnection
                connection.instanceFollowRedirects = false
                connection.connectTimeout = 15000
                connection.readTimeout = 15000
                connection.setRequestProperty("Accept", "application/json")
                connection.setRequestProperty("Origin", origin)
                connection.setRequestProperty("Cookie", cookie)
                connection.setRequestProperty("User-Agent", prefs.getString("agent", "AHD-Android"))
                val status = try { connection.responseCode } catch (error: Throwable) { connection.disconnect(); throw error }
                if (status == 404) {
                    // The server forgot this installation (unregistered or unseen for 30 days):
                    // the app registers again the next time it opens.
                    connection.disconnect()
                    stop(context)
                    ok = false
                    break
                }
                if (status != 200) { connection.disconnect(); ok = false; break }
                val text = connection.inputStream.bufferedReader().use { it.readText() }
                connection.disconnect()
                val inbox = JSONObject(text)
                val items = inbox.getJSONArray("items")
                for (i in 0 until items.length()) {
                    val item = items.getJSONObject(i)
                    AhdNotifications.postTagged(context, item.getString("tag"), item.getString("title"), item.getString("body"), item.getString("url"), "server")
                }
                cursor = inbox.getLong("cursor")
                prefs.edit().putLong("cursor", cursor).commit()
                if (!inbox.optBoolean("more", false)) break
            }
            ok
        }
    }
}
class AhdInboxJob : JobService() {
    override fun onStartJob(params: JobParameters): Boolean {
        Thread {
            val ok = AhdInbox.poll(applicationContext)
            jobFinished(params, !ok)
        }.start()
        return true
    }
    override fun onStopJob(params: JobParameters): Boolean = true
}
