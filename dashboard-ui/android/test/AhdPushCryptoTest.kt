package dev.dioxus.main

import java.math.BigInteger
import java.security.KeyFactory
import java.security.KeyPairGenerator
import java.security.interfaces.ECPublicKey
import java.security.spec.ECGenParameterSpec
import java.security.spec.ECPrivateKeySpec
import java.util.Base64
import org.junit.Assert.*
import org.junit.Test

class AhdPushCryptoTest {
    private fun b64(text: String): ByteArray = Base64.getUrlDecoder().decode(text.replace(Regex("\\s"), ""))

    // RFC 8291 Appendix A: the user agent's keys and the message the application server sent.
    private val uaPublic = b64("BCVxsr7N_eNgVRqvHtD0zTZsEc6-VV-JvLexhqUzORcxaOzi6-AYWXvTBHm4bjyPjs7Vd8pZGH6SRpkNtoIAiw4")
    private val auth = b64("BTBZMqHH6r4Tts7J_aSIgg")
    private val body = b64("DGv6ra1nlYgDCS1FRnbzlwAAEABBBP4z9KsN6nGRTbVYI_c7VJSPQTBtkgcy27mlmlMoZIIgDll6e3vCYLocInmYWAmS6TlzAC8wEqKK6PBru3jl7A8") +
        b64("8pfeW0KbunFT06SuDKoJH9Ql87S1QUrdirN6GcG7sFz1y1sqLgVi1VhjVkHsUoEsbI_0LpXMuGvnzQ")
    private val uaPrivate = run {
        val generator = KeyPairGenerator.getInstance("EC")
        generator.initialize(ECGenParameterSpec("secp256r1"))
        val params = (generator.generateKeyPair().public as ECPublicKey).params
        KeyFactory.getInstance("EC").generatePrivate(ECPrivateKeySpec(BigInteger(1, b64("q1dXpw3UpT5VOmu_cf_v6ih07Aems3njxI-JWgLcM94")), params))
    }

    @Test fun decryptsTheRfcExample() {
        val plain = AhdPushCrypto.decrypt(body, uaPrivate, uaPublic, auth)
        assertEquals("When I grow up, I want to be a watermelon", plain?.toString(Charsets.UTF_8))
    }

    @Test fun rejectsTamperedOrForeignMessages() {
        val tampered = body.copyOf().also { it[it.size - 1] = (it[it.size - 1].toInt() xor 1).toByte() }
        assertNull(AhdPushCrypto.decrypt(tampered, uaPrivate, uaPublic, auth))
        assertNull(AhdPushCrypto.decrypt(body, uaPrivate, uaPublic, ByteArray(16)))
        assertNull(AhdPushCrypto.decrypt(body.copyOf(40), uaPrivate, uaPublic, auth))
        assertNull(AhdPushCrypto.decrypt(ByteArray(0), uaPrivate, uaPublic, auth))
        val other = AhdPushCrypto.generate()
        assertNull(AhdPushCrypto.decrypt(body, AhdPushCrypto.privateKey(other), other.publicKey, other.auth))
    }

    @Test fun generatedKeysHaveTheWebPushShapes() {
        val keys = AhdPushCrypto.generate()
        assertEquals(65, keys.publicKey.size)
        assertEquals(4, keys.publicKey[0].toInt())
        assertEquals(16, keys.auth.size)
        assertNotNull(AhdPushCrypto.privateKey(keys))
    }
}
