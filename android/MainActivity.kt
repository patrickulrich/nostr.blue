package dev.dioxus.main

import android.content.Context
import android.content.Intent
import android.net.Uri

/**
 * NIP-55 Android Signer Bridge
 *
 * Provides JNI-callable static methods for Rust to communicate with
 * external Nostr signer apps (e.g. Amber) via Android ContentResolver.
 *
 * Protocol reference: https://github.com/nostr-protocol/nips/blob/master/55.md
 */
class MainActivity : WryActivity() {

    companion object {
        /**
         * Get the app-private files directory for persistent storage.
         * Called from Rust via JNI since dirs::data_dir() returns None on Android.
         */
        @JvmStatic
        fun getDataDir(context: Context): String {
            return context.filesDir.absolutePath
        }

        /**
         * Check if any NIP-55 signer application is installed on the device.
         */
        @JvmStatic
        fun isSignerInstalled(context: Context): Boolean {
            val intent = Intent().apply {
                action = Intent.ACTION_VIEW
                data = Uri.parse("nostrsigner:")
            }
            val infos = context.packageManager.queryIntentActivities(intent, 0)
            return infos.size > 0
        }

        /**
         * Sign an event via ContentResolver (background, requires prior user approval).
         *
         * @param context Android context
         * @param signerPackage Package name of the signer app (e.g. "com.greenart7c3.nostrsigner")
         * @param eventJson JSON string of the unsigned event
         * @param currentUser Hex public key of the currently logged-in user
         * @return Signed event JSON string, or null if not pre-approved or rejected
         */
        @JvmStatic
        fun signEventViaContentResolver(
            context: Context,
            signerPackage: String,
            eventJson: String,
            currentUser: String
        ): String? {
            return try {
                val uri = Uri.parse("content://$signerPackage.SIGN_EVENT")
                val cursor = context.contentResolver.query(
                    uri,
                    arrayOf(eventJson, "", currentUser),
                    null,
                    null,
                    null
                ) ?: return null

                cursor.use {
                    if (it.getColumnIndex("rejected") > -1) return null
                    if (it.moveToFirst()) {
                        val eventIndex = it.getColumnIndex("event")
                        if (eventIndex > -1) it.getString(eventIndex) else null
                    } else {
                        null
                    }
                }
            } catch (e: Exception) {
                null
            }
        }

        /**
         * NIP-04 encrypt via ContentResolver.
         *
         * @param context Android context
         * @param signerPackage Package name of the signer app
         * @param plaintext Text to encrypt
         * @param pubkey Hex public key of the recipient
         * @param currentUser Hex public key of the currently logged-in user
         * @return Encrypted text, or null if not pre-approved or rejected
         */
        @JvmStatic
        fun nip04EncryptViaContentResolver(
            context: Context,
            signerPackage: String,
            plaintext: String,
            pubkey: String,
            currentUser: String
        ): String? {
            return queryContentResolver(context, signerPackage, "NIP04_ENCRYPT", plaintext, pubkey, currentUser)
        }

        /**
         * NIP-04 decrypt via ContentResolver.
         *
         * @param context Android context
         * @param signerPackage Package name of the signer app
         * @param ciphertext Encrypted text to decrypt
         * @param pubkey Hex public key of the sender
         * @param currentUser Hex public key of the currently logged-in user
         * @return Decrypted text, or null if not pre-approved or rejected
         */
        @JvmStatic
        fun nip04DecryptViaContentResolver(
            context: Context,
            signerPackage: String,
            ciphertext: String,
            pubkey: String,
            currentUser: String
        ): String? {
            return queryContentResolver(context, signerPackage, "NIP04_DECRYPT", ciphertext, pubkey, currentUser)
        }

        /**
         * NIP-44 encrypt via ContentResolver.
         *
         * @param context Android context
         * @param signerPackage Package name of the signer app
         * @param plaintext Text to encrypt
         * @param pubkey Hex public key of the recipient
         * @param currentUser Hex public key of the currently logged-in user
         * @return Encrypted text, or null if not pre-approved or rejected
         */
        @JvmStatic
        fun nip44EncryptViaContentResolver(
            context: Context,
            signerPackage: String,
            plaintext: String,
            pubkey: String,
            currentUser: String
        ): String? {
            return queryContentResolver(context, signerPackage, "NIP44_ENCRYPT", plaintext, pubkey, currentUser)
        }

        /**
         * NIP-44 decrypt via ContentResolver.
         *
         * @param context Android context
         * @param signerPackage Package name of the signer app
         * @param ciphertext Encrypted text to decrypt
         * @param pubkey Hex public key of the sender
         * @param currentUser Hex public key of the currently logged-in user
         * @return Decrypted text, or null if not pre-approved or rejected
         */
        @JvmStatic
        fun nip44DecryptViaContentResolver(
            context: Context,
            signerPackage: String,
            ciphertext: String,
            pubkey: String,
            currentUser: String
        ): String? {
            return queryContentResolver(context, signerPackage, "NIP44_DECRYPT", ciphertext, pubkey, currentUser)
        }

        /**
         * Generic ContentResolver query for encrypt/decrypt operations.
         * All NIP-04/NIP-44 operations use the same query pattern with
         * columns: [content, pubkey, current_user].
         */
        private fun queryContentResolver(
            context: Context,
            signerPackage: String,
            method: String,
            content: String,
            pubkey: String,
            currentUser: String
        ): String? {
            return try {
                val uri = Uri.parse("content://$signerPackage.$method")
                val cursor = context.contentResolver.query(
                    uri,
                    arrayOf(content, pubkey, currentUser),
                    null,
                    null,
                    null
                ) ?: return null

                cursor.use {
                    if (it.getColumnIndex("rejected") > -1) return null
                    if (it.moveToFirst()) {
                        val resultIndex = it.getColumnIndex("result")
                        if (resultIndex > -1) it.getString(resultIndex) else null
                    } else {
                        null
                    }
                }
            } catch (e: Exception) {
                null
            }
        }
    }
}
