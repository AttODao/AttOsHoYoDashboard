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
import android.content.pm.PackageInstaller
import android.content.res.Configuration
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.provider.Settings
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyPermanentlyInvalidatedException
import android.security.keystore.KeyProperties
import android.util.Log
import android.webkit.WebView
import androidx.activity.OnBackPressedCallback
import androidx.lifecycle.Lifecycle
import kotlin.math.roundToInt
import java.io.File
import java.net.HttpURLConnection
import java.net.URL
import java.net.URLEncoder
import java.security.KeyStore
import java.security.MessageDigest
import javax.crypto.AEADBadTagException
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec
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
        AhdUpdater.clearInstalledNotice(this)
        // WryActivity.onKeyDown still goes back in the WebView history first.
        onBackPressedDispatcher.addCallback(this, object : OnBackPressedCallback(true) {
            override fun handleOnBackPressed() {
                moveTaskToBack(true)
            }
        })
    }

    override fun onWebViewCreate(webView: WebView) {
        this.webView = webView
        AhdUpdater.listener = { notifyUpdate() }
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        setIntent(intent)
        if (AhdNotifications.rememberOpenTarget(intent)) notifyPage("open")
        if (AhdBrowserLogin.remember(intent)) notifyBrowserLogin()
    }
    // Back from the "install unknown apps" settings (or a permission dialog): the update dialog re-checks.
    override fun onResume() {
        super.onResume()
        AhdUpdater.listener?.invoke()
    }
    override fun onRequestPermissionsResult(requestCode: Int, permissions: Array<out String>, grantResults: IntArray) {
        super.onRequestPermissionsResult(requestCode, permissions, grantResults)
        if (requestCode == AhdNotifications.PERMISSION_REQUEST) {
            notifyPage("permission")
            // The update dialog waits for this answer. After a permission dialog it arrives while
            // the activity is still paused, and page updates made then stall until the next resume,
            // so onResume delivers it; only an answer given without a dialog is passed on here.
            if (lifecycle.currentState.isAtLeast(Lifecycle.State.RESUMED)) AhdUpdater.listener?.invoke()
        }
    }
    private fun notifyUpdate() { webView?.post { webView?.evaluateJavascript("window.__ahdUpdateEvent && window.__ahdUpdateEvent()", null) } }
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
            if (intent.action == Intent.ACTION_MY_PACKAGE_REPLACED) AhdUpdater.afterUpdate(context)
        }
    }
}

// The Rust client's session cookie jar ({"name":"value",…}), sealed with an AES-256-GCM key
// that never leaves the Android Keystore. The key needs no user authentication, so the inbox
// and widget jobs read the jar in the background (also while the device is locked). The sealed
// file is excluded from backups (res/xml/ahd_backup_rules.xml, ahd_data_extraction_rules.xml);
// a sealed file that cannot be opened (key gone, data restored on another device) is deleted
// and reads as signed out. Versions up to 1.9.0 kept the jar in plain text: that file is
// sealed and deleted on the first read.
object AhdSessionStore {
    const val SEALED_FILE = "session-cookies.enc"
    const val LEGACY_FILE = "session-cookies.json"
    private const val KEY_ALIAS = "ahd_session_cookies"
    private const val KEYSTORE = "AndroidKeyStore"
    /** The Keystore failed for now; the sealed file is kept and the caller retries later. */
    class Unavailable(cause: Throwable) : Exception("session store unavailable", cause)
    // One lock for every file operation: the Rust client (write/clear/read on its request
    // thread) and the inbox and widget jobs (cookieHeader on their own threads) share the files.
    private val lock = Any()
    // Transient Keystore failures in a row while opening the sealed file, and when the first
    // of them happened (AhdSessionFormat.gaveUp).
    private var openFailures = 0
    private var firstFailureAt = 0L
    // Messages of JSON errors quote the input, so only the type is logged.
    private fun warn(message: String, error: Throwable) = Log.w("AHD", "$message: ${error.javaClass.name}")

    /** The jar's JSON ("{}" when signed out), or null when the Keystore failed for now. */
    @JvmStatic fun read(context: Context): String? = try { load(context) } catch (error: Throwable) {
        warn("Session store unavailable", (error as? Unavailable)?.cause ?: error)
        null
    }
    /** Seals the jar; an empty jar deletes it. */
    @JvmStatic fun write(context: Context, json: String): Boolean = synchronized(lock) {
        try {
            val jar = AhdSessionFormat.normalizeJar(json)
            if (jar == null || AhdSessionFormat.header(jar) == null) clear(context) else {
                seal(context, jar)
                File(context.filesDir, LEGACY_FILE).delete()
                openFailures = 0
                true
            }
        } catch (error: Throwable) {
            warn("Could not save the session", error)
            false
        }
    }
    @JvmStatic fun clear(context: Context): Boolean = synchronized(lock) {
        val sealed = File(context.filesDir, SEALED_FILE)
        val legacy = File(context.filesDir, LEGACY_FILE)
        sealed.delete()
        legacy.delete()
        !sealed.exists() && !legacy.exists()
    }
    /** The Cookie header, null when signed out; throws [Unavailable] when the Keystore failed for now. */
    @JvmStatic fun cookieHeader(context: Context): String? = AhdSessionFormat.header(load(context))

    private fun load(context: Context): String = synchronized(lock) {
        val sealed = File(context.filesDir, SEALED_FILE)
        val legacy = File(context.filesDir, LEGACY_FILE)
        when (AhdSessionFormat.plan(sealed.exists(), legacy.exists())) {
            AhdSessionFormat.Plan.SIGNED_OUT -> AhdSessionFormat.EMPTY
            AhdSessionFormat.Plan.OPEN_SEALED_DROP_LEGACY -> { legacy.delete(); open(sealed) }
            AhdSessionFormat.Plan.OPEN_SEALED -> open(sealed)
            AhdSessionFormat.Plan.MIGRATE_LEGACY -> {
                val jar = AhdSessionFormat.normalizeJar(legacy.readText())
                if (jar == null) { legacy.delete(); AhdSessionFormat.EMPTY } else {
                    // Without a sealed copy the plain file stays, so the user stays signed in
                    // and the next read tries again.
                    try { seal(context, jar); legacy.delete() } catch (error: Throwable) { warn("Could not seal the saved session", error) }
                    jar
                }
            }
        }
    }
    private fun open(sealed: File): String {
        try {
            val key = keyStore().getKey(KEY_ALIAS, null) as SecretKey? ?: return discard(sealed)
            val jar = AhdSessionFormat.open(key, sealed.readBytes())
            openFailures = 0
            return jar
        } catch (error: Throwable) {
            if (AhdSessionFormat.isPermanent(error) || error is KeyPermanentlyInvalidatedException) return discard(sealed)
            // A Keystore that keeps failing would otherwise fail every request for good:
            // after a few attempts the session is given up and the app can sign in again.
            val now = android.os.SystemClock.elapsedRealtime()
            if (openFailures == 0) firstFailureAt = now
            openFailures += 1
            if (AhdSessionFormat.gaveUp(openFailures, now - firstFailureAt)) return discard(sealed)
            throw Unavailable(error)
        }
    }
    private fun discard(sealed: File): String {
        Log.w("AHD", "The saved session cannot be opened on this device; signed out")
        openFailures = 0
        sealed.delete()
        return AhdSessionFormat.EMPTY
    }
    private fun keyStore(): KeyStore = KeyStore.getInstance(KEYSTORE).apply { load(null) }
    private fun newKey(): SecretKey = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, KEYSTORE).run {
        init(KeyGenParameterSpec.Builder(KEY_ALIAS, KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT)
            .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
            .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
            .setKeySize(256)
            .build())
        generateKey()
    }
    private fun seal(context: Context, jar: String) {
        val store = keyStore()
        val existing = store.getKey(KEY_ALIAS, null) as SecretKey?
        val bytes = if (existing == null) AhdSessionFormat.seal(newKey(), jar) else try {
            AhdSessionFormat.seal(existing, jar)
        } catch (error: KeyPermanentlyInvalidatedException) {
            // Only a key the Keystore will never accept again is replaced; a sealed file under it was unreadable anyway.
            store.deleteEntry(KEY_ALIAS)
            AhdSessionFormat.seal(newKey(), jar)
        }
        val temporary = File(context.filesDir, ".$SEALED_FILE.tmp")
        temporary.outputStream().use { out -> out.write(bytes); out.fd.sync() }
        if (!temporary.renameTo(File(context.filesDir, SEALED_FILE))) {
            temporary.delete()
            throw java.io.IOException("could not replace the sealed session")
        }
    }
}

/** Android-free parts of [AhdSessionStore], tested in android/test/AhdSessionFormatTest.kt. */
object AhdSessionFormat {
    const val EMPTY = "{}"
    private val MAGIC = byteArrayOf('A'.code.toByte(), 'H'.code.toByte(), 'D'.code.toByte(), 'S'.code.toByte())
    private const val VERSION: Byte = 1
    private const val TAG_BITS = 128
    private val AAD = "cc.attodao.ahd session-cookies v1".toByteArray(Charsets.UTF_8)
    class Malformed : Exception("malformed sealed session")

    enum class Plan { SIGNED_OUT, OPEN_SEALED, OPEN_SEALED_DROP_LEGACY, MIGRATE_LEGACY }
    /** A sealed file wins (a plain one beside it is a migration that stopped before deleting it). */
    fun plan(sealedExists: Boolean, legacyExists: Boolean): Plan = when {
        sealedExists && legacyExists -> Plan.OPEN_SEALED_DROP_LEGACY
        sealedExists -> Plan.OPEN_SEALED
        legacyExists -> Plan.MIGRATE_LEGACY
        else -> Plan.SIGNED_OUT
    }

    /** "AHDS", version 1, IV length, IV, AES-GCM ciphertext with its tag. */
    fun encode(iv: ByteArray, ciphertext: ByteArray): ByteArray =
        MAGIC + byteArrayOf(VERSION, iv.size.toByte()) + iv + ciphertext
    /** (IV, ciphertext), or null for anything else. */
    fun decode(bytes: ByteArray): Pair<ByteArray, ByteArray>? {
        val header = MAGIC.size + 2
        if (bytes.size < header || !bytes.copyOfRange(0, MAGIC.size).contentEquals(MAGIC) || bytes[MAGIC.size] != VERSION) return null
        val ivLength = bytes[MAGIC.size + 1].toInt()
        if (ivLength !in 12..16 || bytes.size < header + ivLength + TAG_BITS / 8) return null
        return bytes.copyOfRange(header, header + ivLength) to bytes.copyOfRange(header + ivLength, bytes.size)
    }

    /** Encrypts with a fresh IV chosen by the cipher (the Keystore refuses caller IVs). */
    fun seal(key: SecretKey, jar: String): ByteArray {
        val cipher = Cipher.getInstance("AES/GCM/NoPadding")
        cipher.init(Cipher.ENCRYPT_MODE, key)
        cipher.updateAAD(AAD)
        val ciphertext = cipher.doFinal(jar.toByteArray(Charsets.UTF_8))
        return encode(cipher.iv, ciphertext)
    }
    /** The jar's JSON; throws [Malformed] or the cipher's error for anything that is not ours. */
    fun open(key: SecretKey, bytes: ByteArray): String {
        val (iv, ciphertext) = decode(bytes) ?: throw Malformed()
        val cipher = Cipher.getInstance("AES/GCM/NoPadding")
        cipher.init(Cipher.DECRYPT_MODE, key, GCMParameterSpec(TAG_BITS, iv))
        cipher.updateAAD(AAD)
        return normalizeJar(String(cipher.doFinal(ciphertext), Charsets.UTF_8)) ?: throw Malformed()
    }
    /**
     * Failures that stay the same on every attempt: a damaged or foreign file, or a key that
     * does not match it. Anything else may pass: the Keystore also reports a busy or not yet
     * started backend as InvalidKeyException, so that is retried (AhdSessionStore also treats
     * KeyPermanentlyInvalidatedException as permanent).
     */
    fun isPermanent(error: Throwable): Boolean = error is Malformed || error is AEADBadTagException
    /**
     * A sealed session is given up (signed out) after this many transient failures in a row
     * spread over at least this long, so a burst of requests during a short outage (app start,
     * one widget refresh) does not end the session.
     */
    const val TRANSIENT_LIMIT = 5
    const val TRANSIENT_PERIOD_MS = 5 * 60 * 1000L
    fun gaveUp(consecutiveFailures: Int, sinceFirstFailureMs: Long): Boolean =
        consecutiveFailures >= TRANSIENT_LIMIT && sinceFirstFailureMs >= TRANSIENT_PERIOD_MS

    /** A jar is a JSON object of string values; anything else is null. */
    fun normalizeJar(text: String): String? = try {
        val jar = JSONObject(text)
        val names = jar.keys().asSequence().toList()
        if (names.all { jar.opt(it) is String }) {
            JSONObject().apply { for (name in names) put(name, jar.getString(name)) }.toString()
        } else null
    } catch (error: Exception) { null }
    /** "name=value; …" in name order (like the Rust jar), or null for an empty jar. */
    fun header(jar: String): String? {
        val cookies = JSONObject(jar)
        val names = cookies.keys().asSequence().sorted().toList()
        return if (names.isEmpty()) null else names.joinToString("; ") { "$it=${cookies.getString(it)}" }
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
    /** One read pass; returns false when nothing could be fetched. */
    @Synchronized @JvmStatic fun poll(context: Context): Boolean = guarded(false) {
        val prefs = prefs(context)
        val device = prefs.getString("device", null)
        val origin = prefs.getString("origin", null)
        val cookie = AhdSessionStore.cookieHeader(context)
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

// Dioxus has no updater: the APK signature is the update signature. PackageInstaller shows the
// system confirmation (also when this app is the installer of record, measured on Android 14).
object AhdUpdater {
    const val ACTION_STATUS = "cc.attodao.ahd.UPDATE_STATUS"
    private const val NOTICE_ID = 0x41485544
    private const val CHANNEL_ID = "app_updates"
    @Volatile @JvmStatic var listener: (() -> Unit)? = null
    @Volatile private var result: String? = null
    private fun prefs(context: Context) = context.getSharedPreferences("ahd_update", Context.MODE_PRIVATE)
    private inline fun <T> guarded(fallback: T, block: () -> T): T = try { block() } catch (error: Throwable) {
        Log.w("AHD", "App update operation failed", error)
        fallback
    }
    @Suppress("DEPRECATION")
    @JvmStatic fun inspect(context: Context, path: String): String? = guarded(null) {
        val pm = context.packageManager
        val flags = if (Build.VERSION.SDK_INT >= 28) PackageManager.GET_SIGNING_CERTIFICATES else PackageManager.GET_SIGNATURES
        fun info(value: android.content.pm.PackageInfo): JSONObject {
            val signers = if (Build.VERSION.SDK_INT >= 28) value.signingInfo?.apkContentsSigners ?: emptyArray() else value.signatures ?: emptyArray()
            val hashes = JSONArray()
            for (signer in signers) hashes.put(MessageDigest.getInstance("SHA-256").digest(signer.toByteArray()).joinToString("") { "%02x".format(it.toInt() and 0xff) })
            return JSONObject().put("package", value.packageName)
                .put("versionCode", if (Build.VERSION.SDK_INT >= 28) value.longVersionCode else value.versionCode.toLong())
                .put("versionName", value.versionName ?: "").put("signers", hashes)
        }
        val archive = pm.getPackageArchiveInfo(path, flags)
        JSONObject().put("archive", if (archive == null) JSONObject.NULL else info(archive))
            .put("own", info(pm.getPackageInfo(context.packageName, flags))).toString()
    }
    /** Whether this app may start an install session ("install unknown apps"; always true before Android 8). */
    @JvmStatic fun installAllowed(context: Context): Boolean = guarded(true) {
        Build.VERSION.SDK_INT < 26 || context.packageManager.canRequestPackageInstalls()
    }
    @JvmStatic fun openInstallSettings(activity: Activity): Boolean = guarded(false) {
        if (Build.VERSION.SDK_INT < 26) false else {
            activity.runOnUiThread {
                guarded(Unit) {
                    activity.startActivity(Intent(Settings.ACTION_MANAGE_UNKNOWN_APP_SOURCES, Uri.parse("package:${activity.packageName}")))
                }
            }
            true
        }
    }
    /** Asks for POST_NOTIFICATIONS (Android 13+) so the "updated" notice can reopen the app; never opens settings. */
    @JvmStatic fun requestNotificationPermission(activity: Activity): Boolean = guarded(false) {
        if (Build.VERSION.SDK_INT < 33 || AhdNotifications.permissionGranted(activity)) false else {
            activity.runOnUiThread {
                guarded(Unit) { activity.requestPermissions(arrayOf(Manifest.permission.POST_NOTIFICATIONS), AhdNotifications.PERMISSION_REQUEST) }
            }
            true
        }
    }
    @JvmStatic fun install(context: Context, path: String, json: String): Boolean = guarded(false) {
        prefs(context).edit().putString("pending", json).commit()
        result = null
        val installer = context.packageManager.packageInstaller
        val params = PackageInstaller.SessionParams(PackageInstaller.SessionParams.MODE_FULL_INSTALL)
        params.setAppPackageName(context.packageName)
        if (Build.VERSION.SDK_INT >= 31) params.setRequireUserAction(PackageInstaller.SessionParams.USER_ACTION_NOT_REQUIRED)
        val id = installer.createSession(params)
        val session = installer.openSession(id)
        try {
            val file = File(path)
            session.openWrite("base.apk", 0, file.length()).use { out ->
                file.inputStream().use { it.copyTo(out) }
                session.fsync(out)
            }
            val intent = Intent(context, AhdUpdateReceiver::class.java).setAction(ACTION_STATUS).setPackage(context.packageName)
            val pending = PendingIntent.getBroadcast(context, id, intent, PendingIntent.FLAG_UPDATE_CURRENT or (if (Build.VERSION.SDK_INT >= 31) PendingIntent.FLAG_MUTABLE else 0))
            session.commit(pending.intentSender)
            session.close()
            true
        } catch (error: Throwable) {
            session.abandon()
            prefs(context).edit().remove("pending").commit()
            throw error
        }
    }
    @Suppress("DEPRECATION")
    fun onStatus(context: Context, intent: Intent) {
        val status = intent.getIntExtra(PackageInstaller.EXTRA_STATUS, PackageInstaller.STATUS_FAILURE)
        when (status) {
            PackageInstaller.STATUS_PENDING_USER_ACTION -> {
                try {
                    val confirmation = if (Build.VERSION.SDK_INT >= 33) intent.getParcelableExtra(Intent.EXTRA_INTENT, Intent::class.java)
                        else intent.getParcelableExtra<Intent>(Intent.EXTRA_INTENT)
                    if (confirmation == null) report("failed")
                    else context.startActivity(confirmation.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
                } catch (error: Throwable) { report("failed") }
            }
            PackageInstaller.STATUS_SUCCESS -> report("success")
            PackageInstaller.STATUS_FAILURE_ABORTED -> { prefs(context).edit().remove("pending").commit(); report("aborted") }
            else -> { prefs(context).edit().remove("pending").commit(); Log.w("AHD", "update install failed: status $status"); report("failed") }
        }
    }
    @Synchronized private fun report(value: String) { result = value; listener?.invoke() }
    @JvmStatic @Synchronized fun takeResult(): String? { val value = result; result = null; return value }
    @Suppress("DEPRECATION")
    @JvmStatic fun afterUpdate(context: Context): Unit = guarded(Unit) {
        val pending = prefs(context).getString("pending", null)
        prefs(context).edit().remove("pending").commit()
        if (pending != null) {
            val notice = JSONObject(pending)
            if (notice.getString("version") == context.packageManager.getPackageInfo(context.packageName, 0).versionName) {
                if (AhdNotifications.permissionGranted(context)) {
                    val manager = context.getSystemService(NotificationManager::class.java)
                    if (Build.VERSION.SDK_INT >= 26) manager.createNotificationChannel(NotificationChannel(CHANNEL_ID, notice.getString("channel"), NotificationManager.IMPORTANCE_DEFAULT))
                    val builder = if (Build.VERSION.SDK_INT >= 26) Notification.Builder(context, CHANNEL_ID) else Notification.Builder(context).setPriority(Notification.PRIORITY_DEFAULT)
                    val icon = context.resources.getIdentifier("ic_stat_ahd", "drawable", context.packageName)
                    val open = PendingIntent.getActivity(context, NOTICE_ID,
                        Intent(context, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_CLEAR_TOP or Intent.FLAG_ACTIVITY_SINGLE_TOP),
                        PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE)
                    manager.notify(NOTICE_ID, builder.setSmallIcon(if (icon == 0) android.R.drawable.stat_sys_download_done else icon)
                        .setContentTitle(notice.getString("title")).setContentText(notice.getString("body")).setAutoCancel(true).setContentIntent(open).build())
                }
                try { context.startActivity(Intent(context, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)) }
                catch (error: Throwable) { Log.w("AHD", "Could not reopen after update", error) }
            }
        }
    }
    @JvmStatic fun clearInstalledNotice(context: Context): Unit = guarded(Unit) { context.getSystemService(NotificationManager::class.java).cancel(NOTICE_ID) }
}
class AhdUpdateReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        if (intent.action == AhdUpdater.ACTION_STATUS) AhdUpdater.onStatus(context, intent)
    }
}
