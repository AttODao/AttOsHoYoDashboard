package dev.dioxus.main

import android.app.BroadcastOptions
import android.app.PendingIntent
import android.app.Service
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.net.Uri
import android.os.Binder
import android.os.Build
import android.os.IBinder
import android.util.Base64
import android.util.Log
import java.util.UUID
import org.json.JSONObject

/**
 * UnifiedPush connector (spec AND_3.0.0) without the connector library. The distributor app
 * (ntfy, …) gives this installation a push endpoint; it is sent with the app's own Web Push keys
 * to the server (`PUT …/app-devices/{id}/push`), and every push message only wakes the inbox
 * poll (AhdInbox in MainActivity.kt), so a notice is never shown twice. The 15-minute job keeps
 * polling as the fallback. MainActivity.kt must not reference this file (build order).
 */
object AhdPush {
    private const val REGISTER = "org.unifiedpush.android.distributor.REGISTER"
    private const val UNREGISTER = "org.unifiedpush.android.distributor.UNREGISTER"
    private const val ACK = "org.unifiedpush.android.distributor.MESSAGE_ACK"
    const val NEW_ENDPOINT = "org.unifiedpush.android.connector.NEW_ENDPOINT"
    const val REGISTRATION_FAILED = "org.unifiedpush.android.connector.REGISTRATION_FAILED"
    const val MESSAGE = "org.unifiedpush.android.connector.MESSAGE"
    const val UNREGISTERED = "org.unifiedpush.android.connector.UNREGISTERED"
    private const val B64 = Base64.URL_SAFE or Base64.NO_PADDING or Base64.NO_WRAP
    private fun prefs(context: Context) = context.getSharedPreferences("ahd_push", Context.MODE_PRIVATE)
    private inline fun <T> guarded(fallback: T, block: () -> T): T = try { block() } catch (error: Throwable) {
        Log.w("AHD", "UnifiedPush operation failed", error)
        fallback
    }
    private fun changed() { AhdPageEvents.listener?.invoke("push") }

    /** Installed distributors (package names), excluding this app. */
    private fun distributors(context: Context): List<String> {
        val receivers = if (Build.VERSION.SDK_INT >= 33) context.packageManager.queryBroadcastReceivers(Intent(REGISTER), PackageManager.ResolveInfoFlags.of(0))
            else @Suppress("DEPRECATION") context.packageManager.queryBroadcastReceivers(Intent(REGISTER), 0)
        return receivers.map { it.activityInfo.packageName }.filter { it != context.packageName }.distinct().sorted()
    }
    private fun label(context: Context, packageName: String): String = try {
        context.packageManager.getApplicationLabel(context.packageManager.getApplicationInfo(packageName, 0)).toString()
    } catch (error: Throwable) { packageName }

    /** The saved distributor while installed; else the default for unifiedpush://link, else the only/first one. */
    private fun choose(context: Context, installed: List<String>): String? {
        prefs(context).getString("distributor", null)?.let { if (it in installed) return it }
        if (installed.size <= 1) return installed.firstOrNull()
        val default = try {
            @Suppress("DEPRECATION")
            context.packageManager.resolveActivity(Intent(Intent.ACTION_VIEW, Uri.parse("unifiedpush://link")), PackageManager.MATCH_DEFAULT_ONLY)?.activityInfo?.packageName
        } catch (error: Throwable) { null }
        return if (default in installed) default else installed.first()
    }

    /** {"wanted","status","distributor"}: status is off, no_distributor, waiting (no inbox registration), registering, active, failed. */
    @JvmStatic fun state(context: Context): String = guarded("{\"wanted\":false,\"status\":\"off\"}") {
        val prefs = prefs(context)
        val wanted = prefs.getBoolean("wanted", false)
        val installed = distributors(context)
        val status = when {
            !wanted -> "off"
            installed.isEmpty() -> "no_distributor"
            AhdInbox.state(context) == null -> "waiting"
            else -> prefs.getString("status", "registering") ?: "registering"
        }
        val state = JSONObject().put("wanted", wanted).put("status", status)
        if (status == "failed") state.put("reason", prefs.getString("reason", "") ?: "")
        val distributor = prefs.getString("distributor", null)?.takeIf { it in installed } ?: choose(context, installed)
        if (distributor != null) state.put("distributor", label(context, distributor))
        state.toString()
    }

    /** The switch on the Notifications page ("true"/"false"). Off also removes the server's copy. */
    @JvmStatic fun setWanted(context: Context, value: String): Boolean = guarded(false) {
        val wanted = value == "true"
        prefs(context).edit().putBoolean("wanted", wanted).commit()
        if (wanted) sync(context) else {
            forget(context)
            val app = context.applicationContext
            Thread { guarded(Unit) { AhdInbox.deviceRequest(app, "DELETE", "/push", null); Unit } }.start()
        }
        changed()
        true
    }

    /**
     * (Re-)registers with the distributor while wanted and the inbox is registered: after the
     * inbox registration and at every app start (the spec's consistency check). The distributor
     * answers with NEW_ENDPOINT (also for a known token), which sends the endpoint to the server.
     */
    @JvmStatic fun sync(context: Context): Boolean = guarded(false) {
        val prefs = prefs(context)
        if (!prefs.getBoolean("wanted", false) || AhdInbox.state(context) == null) false else {
            val installed = distributors(context)
            val distributor = choose(context, installed)
            if (distributor == null) {
                forget(context)
                false
            } else {
                val app = context.applicationContext
                Thread {
                    guarded(Unit) {
                        val response = AhdInbox.deviceRequest(app, "GET", "/push", null)
                        if (response == null || response.first != 200) fail(app, "server") else {
                            register(app, distributor, JSONObject(response.second).getString("vapid_public_key"))
                        }
                    }
                }.start()
                true
            }
        }
    }

    @Synchronized private fun register(context: Context, distributor: String, vapid: String) {
        val prefs = prefs(context)
        var token = prefs.getString("token", null)
        // A new distributor or server key needs a new registration (and new keys).
        if (token == null || prefs.getString("distributor", null) != distributor || prefs.getString("vapid", null) != vapid) {
            forget(context)
            val keys = AhdPushCrypto.generate()
            token = UUID.randomUUID().toString()
            prefs.edit().putString("token", token).putString("distributor", distributor).putString("vapid", vapid)
                .putString("private", Base64.encodeToString(keys.privatePkcs8, B64))
                .putString("public", Base64.encodeToString(keys.publicKey, B64))
                .putString("auth", Base64.encodeToString(keys.auth, B64))
                .putString("status", "registering").remove("reason").commit()
        } else if (prefs.getString("status", null) != "active") {
            prefs.edit().putString("status", "registering").remove("reason").commit()
        }
        changed()
        send(context, REGISTER, distributor, token) {
            putExtra("vapid", vapid)
            putExtra("message", "AHD")
            // UnifiedPush AND_2 distributors identify the app by this extra.
            putExtra("application", context.packageName)
        }
    }

    private fun send(context: Context, action: String, distributor: String, token: String, fill: Intent.() -> Unit = {}) {
        val intent = Intent(action).setPackage(distributor).putExtra("token", token)
        intent.fill()
        if (Build.VERSION.SDK_INT >= 34) {
            context.sendBroadcast(intent, null, BroadcastOptions.makeBasic().setShareIdentityEnabled(true).toBundle())
        } else {
            intent.putExtra("pi", PendingIntent.getBroadcast(context, 0, Intent("org.unifiedpush.dummy_app"), PendingIntent.FLAG_IMMUTABLE))
            context.sendBroadcast(intent)
        }
    }

    /** Ends the distributor registration and drops the keys; the switch keeps its value. False when they could not be removed. */
    @Synchronized private fun forget(context: Context): Boolean {
        val prefs = prefs(context)
        val token = prefs.getString("token", null)
        val distributor = prefs.getString("distributor", null)
        if (token != null && distributor != null) guarded(Unit) { send(context, UNREGISTER, distributor, token) }
        return prefs.edit().remove("token").remove("vapid").remove("private").remove("public").remove("auth")
            .remove("endpoint").remove("status").remove("reason").commit()
    }

    /** The inbox registration ended (sign-out, notifications off, another user, a server change): stop push too. */
    @JvmStatic fun stop(context: Context): Boolean = guarded(false) {
        val forgotten = forget(context)
        changed()
        forgotten
    }

    private fun fail(context: Context, reason: String) {
        prefs(context).edit().putString("status", "failed").putString("reason", reason).commit()
        changed()
    }

    private fun ack(context: Context, token: String, intent: Intent) {
        val id = intent.getStringExtra("id") ?: return
        val distributor = prefs(context).getString("distributor", null) ?: return
        guarded(Unit) { send(context, ACK, distributor, token) { putExtra("id", id) } }
    }

    /** Handles a distributor broadcast; returns work to finish off the main thread, if any. */
    fun onReceive(context: Context, intent: Intent): (() -> Unit)? = guarded(null) {
        val prefs = prefs(context)
        val token = prefs.getString("token", null)
        // Every connector message carries our random token; anything else is ignored.
        if (token == null || intent.getStringExtra("token") != token) null else when (intent.action) {
            NEW_ENDPOINT -> {
                ack(context, token, intent)
                val endpoint = intent.getStringExtra("endpoint")
                if (endpoint == null) null else {
                    prefs.edit().putString("endpoint", endpoint).commit()
                    val body = JSONObject().put("endpoint", endpoint).put("p256dh", prefs.getString("public", ""))
                        .put("auth", prefs.getString("auth", "")).toString()
                    ({
                        // Skipped when the switch was turned off (and the DELETE sent) since the broadcast.
                        if (prefs.getBoolean("wanted", false) && prefs.getString("token", null) == token) {
                            val response = AhdInbox.deviceRequest(context, "PUT", "/push", body)
                            when (response?.first) {
                                204 -> { prefs.edit().putString("status", "active").remove("reason").commit(); changed() }
                                // The endpoint is not on a public HTTPS host name (e.g. a LAN push server).
                                400 -> fail(context, "rejected")
                                else -> fail(context, "server")
                            }
                        }
                    })
                }
            }
            REGISTRATION_FAILED -> {
                // A failure after an endpoint for the same token is ignored (spec); else use a new token next time.
                if (prefs.getString("endpoint", null) != null) null else {
                    prefs.edit().remove("token").commit()
                    fail(context, intent.getStringExtra("reason") ?: "INTERNAL_ERROR")
                    null
                }
            }
            UNREGISTERED -> {
                prefs.edit().remove("token").remove("endpoint").commit()
                fail(context, "unregistered")
                ({ AhdInbox.deviceRequest(context, "DELETE", "/push", null); Unit })
            }
            MESSAGE -> {
                ack(context, token, intent)
                val message = intent.getByteArrayExtra("bytesMessage")
                val keys = AhdPushCrypto.Keys(Base64.decode(prefs.getString("private", ""), B64), Base64.decode(prefs.getString("public", ""), B64),
                    Base64.decode(prefs.getString("auth", ""), B64))
                val plain = message?.let { AhdPushCrypto.decrypt(it, AhdPushCrypto.privateKey(keys), keys.publicKey, keys.auth) }
                if (plain == null || JSONObject(String(plain, Charsets.UTF_8)).optString("type") != "inbox") {
                    Log.w("AHD", "Ignored a push message that is not an inbox wake-up")
                    null
                } else {
                    Log.i("AHD", "push wake-up: reading the inbox")
                    ({ AhdInbox.poll(context); Unit })
                }
            }
            else -> null
        }
    }
}

/** The distributor's messages (NEW_ENDPOINT, MESSAGE, REGISTRATION_FAILED, UNREGISTERED). */
class AhdPushReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        val app = context.applicationContext
        val work = AhdPush.onReceive(app, intent) ?: return
        val pending = goAsync()
        Thread {
            try { work() } catch (error: Throwable) { Log.w("AHD", "UnifiedPush work failed", error) } finally { pending.finish() }
        }.start()
    }
}

/** The distributor binds to this for a few seconds to raise the app to the foreground (spec). */
class AhdPushForegroundService : Service() {
    private val binder = Binder()
    override fun onBind(intent: Intent?): IBinder = binder
}
