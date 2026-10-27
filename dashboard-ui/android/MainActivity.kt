package dev.dioxus.main

import android.Manifest
import android.app.Activity
import android.app.AlarmManager
import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.provider.Settings
import android.util.Log
import android.webkit.WebView
import org.json.JSONArray
import org.json.JSONObject

typealias BuildConfig = cc.attodao.ahd.BuildConfig

class MainActivity : WryActivity() {
    private var notificationWebView: WebView? = null
    override fun onCreate(savedInstanceState: Bundle?) {
        if (savedInstanceState == null) AhdNotifications.rememberOpenTarget(intent)
        super.onCreate(savedInstanceState)
    }
    override fun onWebViewCreate(webView: WebView) { notificationWebView = webView }
    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        setIntent(intent)
        if (AhdNotifications.rememberOpenTarget(intent)) notifyPage("open")
    }
    override fun onRequestPermissionsResult(requestCode: Int, permissions: Array<out String>, grantResults: IntArray) {
        super.onRequestPermissionsResult(requestCode, permissions, grantResults)
        if (requestCode == AhdNotifications.PERMISSION_REQUEST) notifyPage("permission")
    }
    private fun notifyPage(kind: String) {
        notificationWebView?.post {
            notificationWebView?.evaluateJavascript("window.__ahdNotificationEvent && window.__ahdNotificationEvent(\"$kind\")", null)
        }
    }
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
    private fun openIntent(context: Context, id: Int): PendingIntent = PendingIntent.getActivity(context, id,
        Intent(context, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_CLEAR_TOP or Intent.FLAG_ACTIVITY_SINGLE_TOP)
            .putExtra(EXTRA_OPEN, "dashboard"), PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE)
    @JvmStatic fun deliver(context: Context, id: Int, title: String, body: String): Unit = guarded(Unit) {
        val entries = JSONArray(prefs(context).getString("entries", "[]"))
        val remaining = JSONArray()
        for (i in 0 until entries.length()) { val entry = entries.getJSONObject(i); if (entry.getInt("id") != id) remaining.put(entry) }
        prefs(context).edit().putString("entries", remaining.toString()).apply()
        post(context, id, title, body)
        Unit
    }
    @Suppress("DEPRECATION")
    private fun post(context: Context, id: Int, title: String, body: String): Boolean = guarded(false) {
        if (!permissionGranted(context)) false else {
            ensureChannel(context, prefs(context).getString("channel", "AHD") ?: "AHD")
            val builder = if (Build.VERSION.SDK_INT >= 26) Notification.Builder(context, CHANNEL_ID)
                else Notification.Builder(context).setPriority(Notification.PRIORITY_DEFAULT)
            val icon = context.resources.getIdentifier("ic_stat_ahd", "drawable", context.packageName)
            val notification = builder.setSmallIcon(if (icon == 0) android.R.drawable.ic_popup_reminder else icon)
                .setContentTitle(title).setContentText(body).setStyle(Notification.BigTextStyle().bigText(body))
                .setAutoCancel(true).setContentIntent(openIntent(context, id)).setShowWhen(true).build()
            context.getSystemService(NotificationManager::class.java).notify(id, notification)
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
            else deliver(context, entry.getInt("id"), entry.getString("title"), entry.getString("body"))
        }
    }
}
class AhdAlarmReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        if (intent.action == AhdNotifications.ACTION_ALARM) AhdNotifications.deliver(context, intent.getIntExtra("id", 0), intent.getStringExtra("title") ?: "", intent.getStringExtra("body") ?: "")
    }
}
class AhdBootReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        if (intent.action == Intent.ACTION_BOOT_COMPLETED || intent.action == Intent.ACTION_MY_PACKAGE_REPLACED) AhdNotifications.restore(context)
    }
}
