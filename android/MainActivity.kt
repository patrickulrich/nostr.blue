package dev.dioxus.main

import android.app.Activity
import android.content.ContentResolver
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.net.Uri
import android.os.Bundle
import android.util.Log
import androidx.activity.result.ActivityResultLauncher
import androidx.activity.result.contract.ActivityResultContracts
import androidx.core.content.ContextCompat
import java.io.File
import java.io.IOException
import java.util.UUID

typealias BuildConfig = com.nostr.blue.BuildConfig

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
        synchronized(lock) {
            try {
                Log.d(TAG, "Signer activity result: resultCode=${result.resultCode}")
                if (result.resultCode == Activity.RESULT_OK) {
                    val pubkey = result.data?.getStringExtra("result")
                    val pkg = result.data?.getStringExtra("package")
                    val maskedPubkey = pubkey?.let { if (it.length > 8) "...${it.takeLast(4)}" else it }
                    Log.d(TAG, "Signer approved: pubkey=$maskedPubkey, package=$pkg")
                    when {
                        pubkey.isNullOrBlank() -> {
                            intentError = "Signer approval returned without a pubkey"
                            Log.e(TAG, "Signer approval missing pubkey for package=$pkg")
                        }
                        pkg.isNullOrBlank() -> {
                            intentError = "Signer approval returned without a package name"
                            Log.e(TAG, "Signer approval missing package for pubkey=$maskedPubkey")
                        }
                        else -> {
                            pendingPubkey = pubkey
                            pendingPackage = pkg
                            intentError = null
                        }
                    }
                } else {
                    val errorMsg = "User rejected or cancelled (resultCode=${result.resultCode})"
                    Log.w(TAG, errorMsg)
                    pendingPubkey = null
                    pendingPackage = null
                    intentError = errorMsg
                }
            } finally {
                intentInFlight = false
            }
        }
    }

    // File picker launcher - must be instance property registered before STARTED
    private val filePickerLauncher = registerForActivityResult(
        ActivityResultContracts.OpenDocument()
    ) { uri ->
        Log.d(TAG, "File picker result: uri=$uri")
        synchronized(lock) {
            filePickInFlight = true
        }
        if (uri == null) {
            synchronized(lock) {
                filePickError = "No file selected"
                filePickInFlight = false
            }
            return@registerForActivityResult
        }
        val mimeType = contentResolver.getType(uri) ?: "application/octet-stream"
        val resolver = contentResolver
        Thread {
            processPickedContent(uri, mimeType, "file", resolver)
        }.start()
    }

    // Image picker launcher - must be instance property registered before STARTED
    private val imagePickerLauncher = registerForActivityResult(
        ActivityResultContracts.GetContent()
    ) { uri ->
        Log.d(TAG, "Image picker result: uri=$uri")
        synchronized(lock) {
            filePickInFlight = true
        }
        if (uri == null) {
            synchronized(lock) {
                filePickError = "No image selected"
                filePickInFlight = false
            }
            return@registerForActivityResult
        }
        val mimeType = contentResolver.getType(uri) ?: "image/*"
        val resolver = contentResolver
        Thread {
            processPickedContent(uri, mimeType, "image", resolver)
        }.start()
    }

    // Note: `instance` can be briefly null during Activity recreation (e.g., config
    // changes). Callers like launchGetPublicKey() return "no_instance" so Rust callers
    // should retry or handle gracefully.
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        if (savedInstanceState == null) {
            cleanupTempShareFiles(cacheDir, TEMP_FILE_PRESERVE_MILLIS)
        }
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

        // Java package identifier regex: segments of [A-Za-z_][A-Za-z0-9_]* separated by dots
        private val PACKAGE_NAME_REGEX = Regex("^[A-Za-z][A-Za-z0-9_]*(\\.[A-Za-z][A-Za-z0-9_]*)+$")

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

        // Storage for file picker results
        @Volatile
        private var pendingFileContent: String? = null
        @Volatile
        private var pendingFileMimeType: String? = null
        @Volatile
        private var filePickError: String? = null
        @Volatile
        private var filePickInFlight: Boolean = false
        private const val MAX_UPLOAD_BYTES = 10 * 1024 * 1024 // 10MB
        private const val SHARE_TEMP_PREFIX = "share_"
        private const val TEMP_FILE_PRESERVE_MILLIS = 5 * 60 * 1000L

        private fun maxUploadError(): String = "File too large (max 10MB)"

        private fun querySignerPackages(context: Context): Set<String> {
            val intent = Intent().apply {
                action = Intent.ACTION_VIEW
                data = Uri.parse("nostrsigner:")
            }
            return context.packageManager
                .queryIntentActivities(intent, PackageManager.MATCH_DEFAULT_ONLY)
                .mapNotNull { it.activityInfo?.packageName }
                .toSet()
        }

        private fun processPickedContent(uri: Uri, fallbackMimeType: String, label: String, contentResolver: ContentResolver) {
            try {
                val result = readPickedContent(uri, fallbackMimeType, contentResolver)
                synchronized(lock) {
                    pendingFileContent = result.first
                    pendingFileMimeType = result.second
                    filePickError = null
                    filePickInFlight = false
                }
                Log.d(TAG, "${label.replaceFirstChar { it.uppercase() }} picked successfully: mime=${result.second}")
            } catch (e: Exception) {
                synchronized(lock) {
                    filePickError = e.message ?: "Could not open $label"
                    filePickInFlight = false
                }
                Log.e(TAG, "${label.replaceFirstChar { it.uppercase() }} pick failed", e)
            }
        }

        @Throws(IOException::class)
        private fun readPickedContent(uri: Uri, fallbackMimeType: String, contentResolver: ContentResolver): Pair<String, String> {
            val mimeType = contentResolver.getType(uri) ?: fallbackMimeType

            var fileSize: Long = -1
            try {
                contentResolver.openAssetFileDescriptor(uri, "r")?.use { afd ->
                    fileSize = afd.length
                }
            } catch (e: Exception) {
                Log.d(TAG, "Could not get AFD size: ${e.message}")
            }

            if (fileSize <= 0) {
                try {
                    contentResolver.query(
                        uri,
                        arrayOf(android.provider.OpenableColumns.SIZE),
                        null,
                        null,
                        null
                    )?.use { cursor ->
                        if (cursor.moveToFirst()) {
                            val sizeIndex = cursor.getColumnIndex(android.provider.OpenableColumns.SIZE)
                            if (sizeIndex >= 0 && !cursor.isNull(sizeIndex)) {
                                fileSize = cursor.getLong(sizeIndex)
                            }
                        }
                    }
                } catch (e: Exception) {
                    Log.d(TAG, "Could not get query size: ${e.message}")
                }
            }

            if (fileSize > 0 && fileSize > MAX_UPLOAD_BYTES) {
                throw IOException(maxUploadError())
            }

            val bytes = contentResolver.openInputStream(uri)?.use { stream ->
                val output = java.io.ByteArrayOutputStream()
                val buffer = ByteArray(8 * 1024)
                var totalBytes = 0
                while (true) {
                    val read = stream.read(buffer)
                    if (read <= 0) break
                    totalBytes += read
                    if (totalBytes > MAX_UPLOAD_BYTES) {
                        throw IOException(maxUploadError())
                    }
                    output.write(buffer, 0, read)
                }
                output.toByteArray()
            } ?: throw IOException("Could not open file")

            return Pair(
                android.util.Base64.encodeToString(bytes, android.util.Base64.NO_WRAP),
                mimeType
            )
        }

        private fun cleanupTempShareFiles(cacheDir: File, preserveDurationMillis: Long) {
            val cutoff = System.currentTimeMillis() - preserveDurationMillis
            cacheDir.listFiles()?.forEach { file ->
                if (file.name.startsWith(SHARE_TEMP_PREFIX)) {
                    if (file.lastModified() >= cutoff) {
                        return@forEach
                    }
                    if (!file.delete()) {
                        Log.d(TAG, "Could not delete stale shared temp file: ${file.name}")
                    }
                }
            }
        }

        /**
         * Validate a signer package name.
         * 1. Must match Java package identifier pattern
         * 2. Must be an installed package on the device
         */
        private fun validateSignerPackage(context: Context, signerPackage: String): Boolean {
            if (!PACKAGE_NAME_REGEX.matches(signerPackage)) {
                Log.w(TAG, "Invalid package name format: $signerPackage")
                return false
            }
            return try {
                context.packageManager.getPackageInfo(signerPackage, 0)
                val validSignerPackages = querySignerPackages(context)
                if (signerPackage !in validSignerPackages) {
                    Log.w(TAG, "Package does not handle nostrsigner scheme: $signerPackage")
                    synchronized(lock) {
                        if (pendingPackage == signerPackage) {
                            pendingPackage = null
                        }
                    }
                    false
                } else {
                    true
                }
            } catch (e: PackageManager.NameNotFoundException) {
                Log.w(TAG, "Package not found: $signerPackage")
                false
            }
        }

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
                querySignerPackages(context).isNotEmpty()
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
                querySignerPackages(context)
                    .toList()
                    .sorted()
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
            if (!validateSignerPackage(context, signerPackage)) {
                return null
            }
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
            if (!validateSignerPackage(context, signerPackage)) {
                return null
            }
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
            }
            Log.d(TAG, "Cleared pending Intent state")
        }

        /**
         * Download file by writing to cache and triggering Share Intent.
         *
         * Writes content to a temporary file in cache directory, then opens
         * the system share dialog so the user can save or share the file.
         *
         * @param context Android context
         * @param filename Name of the file to download
         * @param contentBase64 Base64-encoded file content
         * @param mimeType MIME type of the content (e.g., "text/markdown", "image/png")
         * @return "success" on launch, "error:..." on failure
         */
        @JvmStatic
        fun downloadFile(context: Context, filename: String, contentBase64: String, mimeType: String): String {
            return try {
                // Decode base64 content
                val contentBytes = android.util.Base64.decode(contentBase64, android.util.Base64.NO_WRAP)

                // Sanitize filename to prevent path traversal:
                // - Strip any path separators (both / and \)
                // - Get only the basename (last segment after any separator)
                // - Reject empty or whitespace-only names
                val safeName = filename
                    .replace("\\", "/")
                    .substringAfterLast("/")
                    .trim()
                    .ifEmpty { "download" }

                // Write to cache directory
                val cacheDir = context.cacheDir
                val file = File(cacheDir, "${SHARE_TEMP_PREFIX}${UUID.randomUUID()}_$safeName")

                // Verify the canonical path is within cacheDir (defense in depth)
                if (!file.canonicalPath.startsWith(cacheDir.canonicalPath)) {
                    Log.e(TAG, "downloadFile: path traversal attempt blocked: $filename")
                    return "error:Invalid file path"
                }

                file.writeBytes(contentBytes)

                // Create URI for the file
                val uri = androidx.core.content.FileProvider.getUriForFile(
                    context,
                    "${context.packageName}.fileprovider",
                    file
                )

                // Create Share Intent
                val shareIntent = Intent(Intent.ACTION_SEND).apply {
                    type = mimeType
                    putExtra(Intent.EXTRA_STREAM, uri)
                    putExtra(Intent.EXTRA_TITLE, safeName)
                    addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
                }

                // Launch chooser (share dialog)
                val chooserIntent = Intent.createChooser(shareIntent, "Save or share file")
                chooserIntent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
                context.startActivity(chooserIntent)

                Log.d(TAG, "downloadFile: launched share for $safeName")
                "success"
            } catch (e: Exception) {
                Log.e(TAG, "downloadFile failed for $filename", e)
                "error:${e.message}"
            }
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
            if (!validateSignerPackage(context, signerPackage)) {
                return null
            }
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

        /**
         * Open file picker and return selected file content as base64.
         * Uses ACTION_OPEN_DOCUMENT for broad file selection.
         *
         * @return Base64-encoded file content with mime type, or "error:..." on failure
         */
        @JvmStatic
        fun pickFile(@Suppress("UNUSED_PARAMETER") context: Context): String {
            return try {
                val activity = synchronized(lock) { instance }
                if (activity == null) {
                    Log.e(TAG, "pickFile: no Activity instance")
                    return "error:no_instance"
                }

                synchronized(lock) {
                    if (filePickInFlight) {
                        return "error:already_in_flight"
                    }
                    filePickInFlight = true
                    pendingFileContent = null
                    pendingFileMimeType = null
                    filePickError = null
                }

                // Launch file picker with common file types
                activity.filePickerLauncher.launch(arrayOf("*/*"))
                "picking"
            } catch (e: Exception) {
                Log.e(TAG, "pickFile failed", e)
                synchronized(lock) {
                    filePickInFlight = false
                }
                "error:${e.message}"
            }
        }

        /**
         * Open image picker from gallery.
         *
         * @return Base64-encoded image content with mime type, or "error:..." on failure
         */
        @JvmStatic
        fun pickImage(@Suppress("UNUSED_PARAMETER") context: Context): String {
            return try {
                val activity = synchronized(lock) { instance }
                if (activity == null) {
                    Log.e(TAG, "pickImage: no Activity instance")
                    return "error:no_instance"
                }

                synchronized(lock) {
                    if (filePickInFlight) {
                        return "error:already_in_flight"
                    }
                    filePickInFlight = true
                    pendingFileContent = null
                    pendingFileMimeType = null
                    filePickError = null
                }

                activity.imagePickerLauncher.launch("image/*")
                "picking"
            } catch (e: Exception) {
                Log.e(TAG, "pickImage failed", e)
                synchronized(lock) {
                    filePickInFlight = false
                }
                "error:${e.message}"
            }
        }

        /**
         * Poll for file picker result.
         *
         * @return Base64-encoded content prefixed with mime type "mime|base64", "picking", "error:...", or "none"
         */
        @JvmStatic
        fun pollFileResult(@Suppress("UNUSED_PARAMETER") context: Context): String {
            return synchronized(lock) {
                when {
                    filePickInFlight -> "picking"
                    filePickError != null -> {
                        val error = filePickError
                        filePickError = null
                        "error:$error"
                    }
                    pendingFileContent != null && pendingFileMimeType != null -> {
                        val mimeType = pendingFileMimeType
                        val content = pendingFileContent
                        pendingFileMimeType = null
                        pendingFileContent = null
                        "$mimeType|$content"
                    }
                    else -> "none"
                }
            }
        }

        /**
         * Check if file picker is currently in flight.
         */
        @JvmStatic
        fun isFilePickInFlight(@Suppress("UNUSED_PARAMETER") context: Context): Boolean {
            return synchronized(lock) { filePickInFlight }
        }

        @JvmStatic
        fun setPlaybackQueue(
            context: Context,
            queueJson: String,
            startIndex: Int,
            playWhenReady: Boolean
        ): String {
            ContextCompat.startForegroundService(
                context,
                Intent(context, MediaPlaybackService::class.java)
            )
            return NativeAudioBridge.setQueue(context, queueJson, startIndex, playWhenReady)
        }

        @JvmStatic
        fun playNativeAudio(context: Context): String {
            return NativeAudioBridge.play(context)
        }

        @JvmStatic
        fun pauseNativeAudio(context: Context): String {
            return NativeAudioBridge.pause(context)
        }

        @JvmStatic
        fun nextNativeTrack(context: Context): String {
            return NativeAudioBridge.skipNext(context)
        }

        @JvmStatic
        fun previousNativeTrack(context: Context): String {
            return NativeAudioBridge.skipPrevious(context)
        }

        @JvmStatic
        fun seekNativeAudio(context: Context, positionMs: Long): String {
            return NativeAudioBridge.seekTo(context, positionMs)
        }

        @JvmStatic
        fun setNativePlaybackSpeed(context: Context, speed: Float): String {
            return NativeAudioBridge.setPlaybackSpeed(context, speed)
        }

        @JvmStatic
        fun setNativeVolume(context: Context, volume: Float): String {
            return NativeAudioBridge.setVolume(context, volume)
        }

        @JvmStatic
        fun stopNativeAudio(context: Context): String {
            return NativeAudioBridge.stop(context)
        }

        @JvmStatic
        fun clearNativeAudioQueue(context: Context): String {
            return NativeAudioBridge.clearQueue(context)
        }

        @JvmStatic
        fun getNativePlaybackSnapshot(context: Context): String {
            return NativeAudioBridge.getSnapshot(context)
        }
    }
}
