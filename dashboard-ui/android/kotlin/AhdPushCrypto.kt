package dev.dioxus.main

import java.math.BigInteger
import java.security.KeyFactory
import java.security.KeyPairGenerator
import java.security.PrivateKey
import java.security.SecureRandom
import java.security.interfaces.ECPublicKey
import java.security.spec.ECGenParameterSpec
import java.security.spec.ECParameterSpec
import java.security.spec.ECPoint
import java.security.spec.ECPublicKeySpec
import java.security.spec.PKCS8EncodedKeySpec
import javax.crypto.Cipher
import javax.crypto.KeyAgreement
import javax.crypto.Mac
import javax.crypto.spec.GCMParameterSpec
import javax.crypto.spec.SecretKeySpec

/**
 * The receiving side of Web Push message encryption (RFC 8291, aes128gcm) for UnifiedPush.
 * The app creates its own P-256 key pair and auth secret, hands the public parts to the server
 * with its push endpoint, and only it can read a push. Plain JCA, no Android API, so the JVM
 * tests (android/test) run it against the RFC's example.
 */
object AhdPushCrypto {
    /** Private key (PKCS#8), uncompressed public point (65 bytes) and auth secret (16 bytes). */
    class Keys(val privatePkcs8: ByteArray, val publicKey: ByteArray, val auth: ByteArray)

    private fun curve(): ECParameterSpec {
        val generator = KeyPairGenerator.getInstance("EC")
        generator.initialize(ECGenParameterSpec("secp256r1"))
        return (generator.generateKeyPair().public as ECPublicKey).params
    }

    private fun fixed(value: BigInteger): ByteArray {
        val bytes = value.toByteArray()
        return when {
            bytes.size == 32 -> bytes
            bytes.size > 32 -> bytes.copyOfRange(bytes.size - 32, bytes.size)
            else -> ByteArray(32 - bytes.size) + bytes
        }
    }

    fun uncompressed(key: ECPublicKey): ByteArray = byteArrayOf(4) + fixed(key.w.affineX) + fixed(key.w.affineY)

    fun generate(): Keys {
        val generator = KeyPairGenerator.getInstance("EC")
        generator.initialize(ECGenParameterSpec("secp256r1"))
        val pair = generator.generateKeyPair()
        val auth = ByteArray(16).also { SecureRandom().nextBytes(it) }
        return Keys(pair.private.encoded, uncompressed(pair.public as ECPublicKey), auth)
    }

    fun privateKey(keys: Keys): PrivateKey = KeyFactory.getInstance("EC").generatePrivate(PKCS8EncodedKeySpec(keys.privatePkcs8))

    private fun hmac(key: ByteArray, vararg parts: ByteArray): ByteArray {
        val mac = Mac.getInstance("HmacSHA256")
        mac.init(SecretKeySpec(key, "HmacSHA256"))
        for (part in parts) mac.update(part)
        return mac.doFinal()
    }

    /** HKDF-SHA256 for outputs of at most one hash length. */
    private fun hkdf(salt: ByteArray, ikm: ByteArray, info: ByteArray, length: Int): ByteArray =
        hmac(hmac(salt, ikm), info, byteArrayOf(1)).copyOf(length)

    /** The plaintext of a single-record aes128gcm body, or null when it is not for these keys. */
    fun decrypt(body: ByteArray, privateKey: PrivateKey, publicKey: ByteArray, auth: ByteArray): ByteArray? = try {
        val idLength = body[20].toInt() and 0xff
        if (body.size <= 21 + idLength + 16 || idLength != 65 || body[21].toInt() != 4) null else {
            val salt = body.copyOfRange(0, 16)
            val sender = body.copyOfRange(21, 21 + idLength)
            val point = ECPoint(BigInteger(1, sender.copyOfRange(1, 33)), BigInteger(1, sender.copyOfRange(33, 65)))
            val senderKey = KeyFactory.getInstance("EC").generatePublic(ECPublicKeySpec(point, curve()))
            val agreement = KeyAgreement.getInstance("ECDH")
            agreement.init(privateKey)
            agreement.doPhase(senderKey, true)
            val shared = agreement.generateSecret()
            val ikm = hkdf(auth, shared, "WebPush: info\u0000".toByteArray() + publicKey + sender, 32)
            val cek = hkdf(salt, ikm, "Content-Encoding: aes128gcm\u0000".toByteArray(), 16)
            val nonce = hkdf(salt, ikm, "Content-Encoding: nonce\u0000".toByteArray(), 12)
            val cipher = Cipher.getInstance("AES/GCM/NoPadding")
            cipher.init(Cipher.DECRYPT_MODE, SecretKeySpec(cek, "AES"), GCMParameterSpec(128, nonce))
            val padded = cipher.doFinal(body, 21 + idLength, body.size - 21 - idLength)
            // The last record ends with the delimiter 0x02, followed only by zero padding.
            var end = padded.size - 1
            while (end >= 0 && padded[end].toInt() == 0) end--
            if (end < 0 || padded[end].toInt() != 2) null else padded.copyOf(end)
        }
    } catch (error: Exception) {
        null
    }
}
