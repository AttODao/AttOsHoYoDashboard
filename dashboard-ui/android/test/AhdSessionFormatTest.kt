package dev.dioxus.main

import javax.crypto.AEADBadTagException
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import org.junit.Assert.*
import org.junit.Test

// AhdSessionStore's Keystore and files need a device; its format and decisions do not. A software
// AES key stands in for the Keystore key (both go through the same AES/GCM/NoPadding cipher).
class AhdSessionFormatTest {
    private fun key(): SecretKey = KeyGenerator.getInstance("AES").apply { init(256) }.generateKey()
    private val jar = """{"__Host-attos_session":"synthetic-value"}"""

    @Test fun migrationPlan() {
        assertEquals(AhdSessionFormat.Plan.SIGNED_OUT, AhdSessionFormat.plan(sealedExists = false, legacyExists = false))
        assertEquals(AhdSessionFormat.Plan.MIGRATE_LEGACY, AhdSessionFormat.plan(sealedExists = false, legacyExists = true))
        assertEquals(AhdSessionFormat.Plan.OPEN_SEALED, AhdSessionFormat.plan(sealedExists = true, legacyExists = false))
        // A migration that stopped between writing the sealed file and deleting the plain one.
        assertEquals(AhdSessionFormat.Plan.OPEN_SEALED_DROP_LEGACY, AhdSessionFormat.plan(sealedExists = true, legacyExists = true))
    }

    @Test fun sealedJarRoundTripsWithoutPlainText() {
        val key = key()
        val sealed = AhdSessionFormat.seal(key, jar)
        assertEquals(jar, AhdSessionFormat.open(key, sealed))
        assertFalse(String(sealed, Charsets.ISO_8859_1).contains("synthetic-value"))
        assertArrayEquals("AHDS".toByteArray(), sealed.copyOfRange(0, 4))
        assertEquals(1.toByte(), sealed[4])
        assertEquals(12, sealed[5].toInt())
        // A fresh IV every time.
        assertFalse(sealed.contentEquals(AhdSessionFormat.seal(key, jar)))
    }

    @Test fun foreignOrDamagedFilesArePermanentFailures() {
        val sealed = AhdSessionFormat.seal(key(), jar)
        // Another key: a restored backup or a new Keystore key.
        val wrongKey = runCatching { AhdSessionFormat.open(key(), sealed) }.exceptionOrNull()
        assertTrue(wrongKey is AEADBadTagException)
        assertTrue(AhdSessionFormat.isPermanent(wrongKey!!))
        val key = key()
        val tampered = AhdSessionFormat.seal(key, jar).also { it[it.size - 1] = (it[it.size - 1].toInt() xor 1).toByte() }
        assertTrue(AhdSessionFormat.isPermanent(runCatching { AhdSessionFormat.open(key, tampered) }.exceptionOrNull()!!))
        for (bytes in listOf(ByteArray(0), "AHDS".toByteArray(), jar.toByteArray(), sealed.copyOfRange(0, 20),
            sealed.copyOf().also { it[4] = 2 }, sealed.copyOf().also { it[5] = 40 })) {
            val error = runCatching { AhdSessionFormat.open(key, bytes) }.exceptionOrNull()
            assertTrue(error is AhdSessionFormat.Malformed)
            assertTrue(AhdSessionFormat.isPermanent(error!!))
        }
        // The Keystore not answering is retried, not a sign-out (it also reports a busy backend as InvalidKeyException).
        assertFalse(AhdSessionFormat.isPermanent(java.security.InvalidKeyException("Keystore operation failed")))
        assertFalse(AhdSessionFormat.isPermanent(java.security.KeyStoreException("busy")))
        assertFalse(AhdSessionFormat.isPermanent(java.security.ProviderException("Keystore operation failed")))
        assertFalse(AhdSessionFormat.isPermanent(java.io.IOException("read")))
    }

    @Test fun aKeystoreThatKeepsFailingGivesTheSessionUp() {
        val limit = AhdSessionFormat.TRANSIENT_LIMIT
        val period = AhdSessionFormat.TRANSIENT_PERIOD_MS
        for (failures in 0 until limit) assertFalse(AhdSessionFormat.gaveUp(failures, period * 2))
        // A burst during a short outage keeps the session.
        assertFalse(AhdSessionFormat.gaveUp(limit * 10, period - 1))
        assertTrue(AhdSessionFormat.gaveUp(limit, period))
    }

    @Test fun legacyJarsAreValidatedBeforeMigration() {
        assertEquals(jar, AhdSessionFormat.normalizeJar(jar))
        assertEquals("{}", AhdSessionFormat.normalizeJar("{}"))
        assertNull(AhdSessionFormat.normalizeJar(""))
        assertNull(AhdSessionFormat.normalizeJar("{"))
        assertNull(AhdSessionFormat.normalizeJar("[]"))
        assertNull(AhdSessionFormat.normalizeJar("""{"a":1}"""))
    }

    @Test fun cookieHeaderMatchesTheRustJar() {
        assertNull(AhdSessionFormat.header(AhdSessionFormat.EMPTY))
        assertEquals("__Host-attos_session=synthetic-value", AhdSessionFormat.header(jar))
        assertEquals("a=1; b=2", AhdSessionFormat.header("""{"b":"2","a":"1"}"""))
    }
}
