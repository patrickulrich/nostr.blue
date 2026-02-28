package dev.dioxus.main

import android.app.Activity
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.net.Uri
import android.os.Bundle
import android.util.Log
import androidx.activity.result.ActivityResultLauncher
import androidx.activity.result.contract.ActivityResultContracts

/**
 * NIP-55 Android Signer Bridge
 *
 * Provides JNI-callable static methods for Rust to communicate with
 * external Nostr signer apps (e.g. Amber) via Android ContentResolver
 * and Intent-based approval flows.
 *
 * Protocol reference: https://github.com/nostr-protocol/nips/blob/master/55.md
 */
class MainActivity : WryActivity() {

    // Property initializer — registered before STARTED state, safe for AndroidX lifecycle
    private val signerLauncher: ActivityResultLauncher<Intent> = registerForActivityResult(
        ActivityResultContracts.StartActivityForResult()
    ) { result ->
        Log.d(TAG, "Signer activity result: resultCode=${result.resultCode}")
        if (result.resultCode == Activity.RESULT_OK) {
            val pubkey = result.data?.getStringExtra("result")
            val pkg = result.data?.getStringExtra("package")
            Log.d(TAG, "Signer approved: pubkey=$pubkey, package=$pkg")
            synchronized(lock) {
                pendingPubkey = pubkey
                pendingPackage = pkg
                intentError = null
            }
        } else {
            val errorMsg = "User rejected or cancelled (resultCode=${result.resultCode})"
            Log.w(TAG, errorMsg)
            synchronized(lock) {
                pendingPubkey = null
                pendingPackage = null
                intentError = errorMsg
            }
        }
        synchronized(lock) {
            intentInFlight = false
        }
    }

    // Note: `instance` can be briefly null during Activity recreation (e.g., config
    // changes). Callers like launchGetPublicKey() return "no_instance" so Rust callers
    // should retry or handle gracefully.
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        synchronized(lock) {
            instance = this
        }
        Log.d(TAG, "MainActivity created, instance stored")
    }

    override fun onDestroy() {
        synchronized(lock) {
            if (instance === this) {
                instance = null
            }
        }
        Log.d(TAG, "MainActivity destroyed, instance cleared")
        super.onDestroy()
    }

    companion object {
        private const val TAG = "NIP55"
        private val lock = Any()

        @Volatile
        private var instance: MainActivity? = null

        @Volatile
        private var pendingPubkey: String? = null

        @Volatile
        private var pendingPackage: String? = null

        @Volatile
        private var intentError: String? = null

        @Volatile
        private var intentInFlight: Boolean = false

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
            return try {
                val intent = Intent().apply {
                    action = Intent.ACTION_VIEW
                    data = Uri.parse("nostrsigner:")
                }
                val infos = context.packageManager.queryIntentActivities(intent, 0)
                infos.isNotEmpty()
            } catch (e: Exception) {
                Log.e(TAG, "isSignerInstalled failed", e)
                false
            }
        }

        /**
         * Get a comma-separated list of installed NIP-55 signer package names.
         *
         * Queries the package manager for apps that handle the `nostrsigner:` scheme.
         * Returns empty string if none found.
         */
        @JvmStatic
        fun getSignerPackages(context: Context): String {
            return try {
                val intent = Intent().apply {
                    action = Intent.ACTION_VIEW
                    data = Uri.parse("nostrsigner:")
                }
                val infos = context.packageManager.queryIntentActivities(intent, PackageManager.MATCH_DEFAULT_ONLY)
                infos.mapNotNull { it.activityInfo?.packageName }
                    .distinct()
                    .joinToString(",")
            } catch (e: Exception) {
                Log.e(TAG, "getSignerPackages failed", e)
                ""
            }
        }

        /**
         * Get the user's public key from a NIP-55 signer via ContentResolver.
         *
         * Queries `content://{signerPackage}.GET_PUBLIC_KEY` with projection ["login"].
         * Only works if the user has previously approved this app in the signer.
         *
         * @param context Android context
         * @param signerPackage Package name of the signer app
         * @return Hex public key string, or null if not pre-approved or unavailable
         */
        @JvmStatic
        fun getPublicKeyViaContentResolver(context: Context, signerPackage: String): String? {
            return try {
                val uri = Uri.parse("content://$signerPackage.GET_PUBLIC_KEY")
                val cursor = context.contentResolver.query(
                    uri,
                    arrayOf("login"),
                    null,
                    null,
                    null
                ) ?: run {
                    Log.w(TAG, "getPublicKeyViaContentResolver: cursor is null for $signerPackage")
                    return null
                }

                cursor.use {
                    if (it.getColumnIndex("rejected") > -1) {
                        Log.d(TAG, "getPublicKeyViaContentResolver: rejected by $signerPackage")
                        return null
                    }
                    if (it.moveToFirst()) {
                        val resultIndex = it.getColumnIndex("result")
                        if (resultIndex > -1) it.getString(resultIndex) else null
                    } else {
                        null
                    }
                }
            } catch (e: Exception) {
                Log.e(TAG, "getPublicKeyViaContentResolver failed for $signerPackage", e)
                null
            }
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
                ) ?: run {
                    Log.w(TAG, "signEventViaContentResolver: cursor is null for $signerPackage")
                    return null
                }

                cursor.use {
                    if (it.getColumnIndex("rejected") > -1) {
                        Log.d(TAG, "signEventViaContentResolver: rejected by $signerPackage")
                        return null
                    }
                    if (it.moveToFirst()) {
                        val eventIndex = it.getColumnIndex("event")
                        if (eventIndex > -1) it.getString(eventIndex) else null
                    } else {
                        null
                    }
                }
            } catch (e: Exception) {
                Log.e(TAG, "signEventViaContentResolver failed for $signerPackage", e)
                null
            }
        }

        /**
         * NIP-04 encrypt via ContentResolver.
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
         * Launch get_public_key Intent to open the signer app for user approval.
         *
         * This is required for first-time connections — ContentResolver only works
         * after the user has approved via Intent.
         *
         * @param context Android context (unused, kept for JNI consistency)
         * @return "launched" on success, "no_instance" if Activity not available,
         *         "already_in_flight" if an Intent is pending, or "error:..." on failure
         */
        @JvmStatic
        fun launchGetPublicKey(@Suppress("UNUSED_PARAMETER") context: Context): String {
            return try {
                val activity = synchronized(lock) { instance }
                if (activity == null) {
                    Log.e(TAG, "launchGetPublicKey: no Activity instance")
                    return "no_instance"
                }

                synchronized(lock) {
                    if (intentInFlight) {
                        Log.w(TAG, "launchGetPublicKey: Intent already in flight")
                        return "already_in_flight"
                    }
                    intentInFlight = true
                    pendingPubkey = null
                    pendingPackage = null
                    intentError = null
                }

                // NIP-55 permissions: pre-authorize signing for common event kinds.
                // Kinds not listed here prompt the user in the signer each time.
                // Full list of app kinds: 0,1,3,5,6,7,14,1063,1088,1311,1617,1619,
                // 1621,1622,6969,7001,9734,10000-10050,30000-30311,31237,38000+
                val permissions = """[{"type":"sign_event","kind":0},{"type":"sign_event","kind":1},{"type":"sign_event","kind":3},{"type":"sign_event","kind":5},{"type":"sign_event","kind":6},{"type":"sign_event","kind":7},{"type":"sign_event","kind":14},{"type":"sign_event","kind":1063},{"type":"sign_event","kind":1088},{"type":"sign_event","kind":1311},{"type":"sign_event","kind":1617},{"type":"sign_event","kind":1621},{"type":"sign_event","kind":1622},{"type":"sign_event","kind":6969},{"type":"sign_event","kind":7001},{"type":"sign_event","kind":9734},{"type":"sign_event","kind":10000},{"type":"sign_event","kind":10002},{"type":"sign_event","kind":30000},{"type":"sign_event","kind":30001},{"type":"sign_event","kind":30023},{"type":"sign_event","kind":30078},{"type":"sign_event","kind":30311},{"type":"sign_event","kind":31237},{"type":"nip04_encrypt"},{"type":"nip04_decrypt"},{"type":"nip44_encrypt"},{"type":"nip44_decrypt"}]"""

                val intent = Intent(Intent.ACTION_VIEW, Uri.parse("nostrsigner:")).apply {
                    putExtra("type", "get_public_key")
                    putExtra("permissions", permissions)
                }

                Log.d(TAG, "Launching get_public_key Intent")
                activity.signerLauncher.launch(intent)
                "launched"
            } catch (e: Exception) {
                Log.e(TAG, "launchGetPublicKey failed", e)
                synchronized(lock) {
                    intentInFlight = false
                    intentError = e.message ?: "Unknown error"
                }
                "error:${e.message}"
            }
        }

        /**
         * Poll for the public key result from a previous Intent launch.
         *
         * @return The hex public key, or null if not yet available
         */
        @JvmStatic
        fun pollPublicKeyResult(@Suppress("UNUSED_PARAMETER") context: Context): String? {
            return synchronized(lock) { pendingPubkey }
        }

        /**
         * Poll for the package name result from a previous Intent launch.
         *
         * @return The signer package name, or null if not yet available
         */
        @JvmStatic
        fun pollPackageResult(@Suppress("UNUSED_PARAMETER") context: Context): String? {
            return synchronized(lock) { pendingPackage }
        }

        /**
         * Poll for any error from the last Intent launch.
         *
         * @return Error message string, or null if no error
         */
        @JvmStatic
        fun pollIntentError(@Suppress("UNUSED_PARAMETER") context: Context): String? {
            return synchronized(lock) { intentError }
        }

        /**
         * Check if an Intent is currently in flight (user hasn't returned yet).
         *
         * @return true if waiting for signer response
         */
        @JvmStatic
        fun isIntentInFlight(@Suppress("UNUSED_PARAMETER") context: Context): Boolean {
            return synchronized(lock) { intentInFlight }
        }

        /**
         * Clear all pending Intent state.
         */
        @JvmStatic
        fun clearPendingResult(@Suppress("UNUSED_PARAMETER") context: Context) {
            synchronized(lock) {
                pendingPubkey = null
                pendingPackage = null
                intentError = null
                intentInFlight = false
            }
            Log.d(TAG, "Cleared pending Intent state")
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
                ) ?: run {
                    Log.w(TAG, "$method: cursor is null for $signerPackage")
                    return null
                }

                cursor.use {
                    if (it.getColumnIndex("rejected") > -1) {
                        Log.d(TAG, "$method: rejected by $signerPackage")
                        return null
                    }
                    if (it.moveToFirst()) {
                        val resultIndex = it.getColumnIndex("result")
                        if (resultIndex > -1) it.getString(resultIndex) else null
                    } else {
                        null
                    }
                }
            } catch (e: Exception) {
                Log.e(TAG, "$method failed for $signerPackage", e)
                null
            }
        }
    }
}
