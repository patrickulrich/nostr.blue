package dev.dioxus.main

import android.app.Activity
import android.content.ContentResolver
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.util.Log
import android.util.Base64InputStream
import androidx.activity.OnBackPressedCallback
import androidx.activity.result.ActivityResultLauncher
import androidx.activity.result.contract.ActivityResultContracts
import androidx.core.content.ContextCompat
import java.io.File
import java.io.ByteArrayInputStream
import java.io.IOException
import android.app.PictureInPictureParams
import android.app.RemoteAction
import android.app.PendingIntent
import android.content.BroadcastReceiver
import android.content.IntentFilter
import android.graphics.drawable.Icon
import android.util.Rational

import java.util.Locale
import java.util.UUID
import com.nostr.blue.R
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.MultipartBody
import okhttp3.RequestBody.Companion.toRequestBody

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

    private val backPressedCallback = object : OnBackPressedCallback(true) {
        override fun handleOnBackPressed() {
            handleAndroidBackPressed()
        }
    }

    // Property initializer — registered before STARTED state, safe for AndroidX lifecycle
    private val signerLauncher: ActivityResultLauncher<Intent> = registerForActivityResult(
        ActivityResultContracts.StartActivityForResult()
    ) { result ->
        synchronized(lock) {
            try {
                Log.d(TAG, "Signer activity result: resultCode=${result.resultCode}")
                val requestType = activeSignerRequest
                if (result.resultCode == Activity.RESULT_OK) {
                    if (requestType == REQUEST_LOGIN) {
                        val pubkey = result.data?.getStringExtra("result")
                        val pkg = result.data?.getStringExtra("package")
                        val maskedPubkey =
                            pubkey?.let { if (it.length > 8) "...${it.takeLast(4)}" else it }
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
                                try {
                                    val validationError = validateSignerPackage(this@MainActivity, pkg)
                                    if (validationError != null) {
                                        intentError = validationError
                                        Log.e(
                                            TAG,
                                            "Signer approval package validation failed for package=$pkg: $validationError"
                                        )
                                    } else if (launchedSignerPackage != null && launchedSignerPackage != pkg) {
                                        intentError = "Signer approval returned mismatched package"
                                        Log.e(
                                            TAG,
                                            "Signer approval package mismatch: launched=$launchedSignerPackage returned=$pkg"
                                        )
                                    } else {
                                        pendingPubkey = pubkey
                                        pendingPackage = pkg
                                        intentError = null
                                    }
                                } catch (e: Exception) {
                                    val validationError =
                                        e.message ?: "Signer approval package validation failed"
                                    intentError = validationError
                                    Log.e(
                                        TAG,
                                        "Signer approval package validation threw for package=$pkg: $validationError",
                                        e
                                    )
                                }
                            }
                        }
                    } else {
                        val returnedPackage = result.data?.getStringExtra("package")
                        if (launchedSignerPackage != null && returnedPackage != null && launchedSignerPackage != returnedPackage) {
                            pendingOperationResult = null
                            pendingOperationEvent = null
                            pendingOperationPackage = null
                            pendingOperationRejected = null
                            intentError = "Signer operation returned mismatched package"
                            Log.e(
                                TAG,
                                "Signer operation package mismatch: launched=$launchedSignerPackage returned=$returnedPackage"
                            )
                        } else {
                            pendingOperationResult = result.data?.getStringExtra("result")
                            pendingOperationEvent = result.data?.getStringExtra("event")
                            pendingOperationPackage = returnedPackage
                            pendingOperationRejected = if (result.data?.extras?.containsKey("rejected") == true) {
                                result.data?.getBooleanExtra("rejected", false)
                            } else {
                                null
                            }
                            intentError = null
                            Log.d(
                                TAG,
                                "Signer operation completed: request=$requestType package=${pendingOperationPackage} hasEvent=${pendingOperationEvent != null} hasResult=${pendingOperationResult != null} rejected=$pendingOperationRejected"
                            )
                        }
                    }
                } else {
                    val errorMsg = "User rejected or cancelled (resultCode=${result.resultCode})"
                    Log.w(TAG, errorMsg)
                    pendingPubkey = null
                    pendingPackage = null
                    pendingOperationResult = null
                    pendingOperationEvent = null
                    pendingOperationPackage = null
                    pendingOperationRejected = null
                    intentError = errorMsg
                }
            } finally {
                activeSignerRequest = null
                intentInFlight = false
            }
        }
    }

    // File picker launcher - must be instance property registered before STARTED
    private val filePickerLauncher = registerForActivityResult(
        ActivityResultContracts.OpenDocument()
    ) { uri ->
        Log.d(TAG, "File picker result: uri=$uri")
        if (uri == null) {
            synchronized(lock) {
                filePickError = "No file selected"
                filePickInFlight = false
            }
            return@registerForActivityResult
        }
        val mimeType = contentResolver.getType(uri) ?: "application/octet-stream"
        val resolver = contentResolver
        try {
            Thread {
                processPickedContent(uri, mimeType, "file", resolver)
            }.start()
        } catch (e: Exception) {
            synchronized(lock) {
                filePickError = e.message ?: "Failed to process selected file"
                filePickInFlight = false
            }
            Log.e(TAG, "Failed to start file processing thread", e)
        }
    }

    // Image picker launcher - must be instance property registered before STARTED
    private val imagePickerLauncher = registerForActivityResult(
        ActivityResultContracts.GetContent()
    ) { uri ->
        Log.d(TAG, "Image picker result: uri=$uri")
        if (uri == null) {
            synchronized(lock) {
                filePickError = "No image selected"
                filePickInFlight = false
            }
            return@registerForActivityResult
        }
        val mimeType = contentResolver.getType(uri) ?: "image/*"
        val resolver = contentResolver
        try {
            Thread {
                processPickedContent(uri, mimeType, "image", resolver)
            }.start()
        } catch (e: Exception) {
            synchronized(lock) {
                filePickError = e.message ?: "Failed to process selected image"
                filePickInFlight = false
            }
            Log.e(TAG, "Failed to start image processing thread", e)
        }
    }

    // Note: `instance` can be briefly null during Activity recreation (e.g., config
    // changes). Callers like launchGetPublicKey() return "no_instance" so Rust callers
    // should retry or handle gracefully.
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        if (savedInstanceState == null) {
            cleanupTempShareFiles(cacheDir, TEMP_FILE_PRESERVE_MILLIS)
        }
        onBackPressedDispatcher.addCallback(this, backPressedCallback)
        val filter = IntentFilter().apply {
            addAction(ACTION_PIP_TOGGLE_MUTE)
            addAction(ACTION_PIP_LEAVE)
        }
        ContextCompat.registerReceiver(this, pipReceiver, filter, ContextCompat.RECEIVER_NOT_EXPORTED)
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
        try {
            unregisterReceiver(pipReceiver)
        } catch (_: Exception) {}
        Log.d(TAG, "MainActivity destroyed, instance cleared")
        super.onDestroy()
    }

    // region PIP (Picture-in-Picture) for Nests

    @Volatile
    var isNestActive: Boolean = false
        private set

    @Volatile
    var isInPipMode: Boolean = false
        private set

    private val pipLock = Any()

    private fun setNestActive(active: Boolean) {
        synchronized(pipLock) { isNestActive = active }
    }

    private val ACTION_PIP_TOGGLE_MUTE = "dev.dioxus.main.PIP_TOGGLE_MUTE"
    private val ACTION_PIP_LEAVE = "dev.dioxus.main.PIP_LEAVE"
    private val PIP_MUTE_REQ = 1001
    private val PIP_LEAVE_REQ = 1002

    private val pipReceiver = object : BroadcastReceiver() {
        override fun onReceive(context: Context, intent: Intent) {
            when (intent.action) {
                ACTION_PIP_TOGGLE_MUTE -> {
                    notifyPipMuteToggled()
                }
                ACTION_PIP_LEAVE -> {
                    val activity = synchronized(lock) { instance }
                    activity?.let {
                        it.setNestActive(false)
                        it.finish()
                    }
                }
            }
        }
    }

    override fun onUserLeaveHint() {
        super.onUserLeaveHint()
        val active = synchronized(pipLock) { isNestActive }
        if (active) {
            enterPip()
        }
    }

    override fun onPictureInPictureModeChanged(
        isInPictureInPictureMode: Boolean,
        newConfig: android.content.res.Configuration,
    ) {
        super.onPictureInPictureModeChanged(isInPictureInPictureMode, newConfig)
        synchronized(pipLock) { isInPipMode = isInPictureInPictureMode }
        notifyPipModeChanged(isInPictureInPictureMode)
    }

    private fun enterPip() {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.O) return
        if (!packageManager.hasSystemFeature(PackageManager.FEATURE_PICTURE_IN_PICTURE)) return
        try {
            val params = buildPipParams()
            enterPictureInPictureMode(params)
        } catch (e: Exception) {
            Log.e(TAG, "Failed to enter PIP mode", e)
        }
    }

    private fun buildPipParams(): PictureInPictureParams {
        val builder = PictureInPictureParams.Builder()
            .setAspectRatio(Rational(16, 9))

        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            builder.setActions(buildPipActions())
        }

        return builder.build()
    }

    private fun buildPipActions(): List<RemoteAction> {
        val muteIntent = PendingIntent.getBroadcast(
            this, PIP_MUTE_REQ,
            Intent(ACTION_PIP_TOGGLE_MUTE).setPackage(packageName),
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
        )
        val leaveIntent = PendingIntent.getBroadcast(
            this, PIP_LEAVE_REQ,
            Intent(ACTION_PIP_LEAVE).setPackage(packageName),
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
        )
        val muteIcon = Icon.createWithResource(this, R.drawable.ic_mic_off)
        val muteLabel = "Mute"
        val leaveIcon = Icon.createWithResource(this, R.drawable.ic_call_end)
        val leaveLabel = "Leave"
        return listOf(
            RemoteAction(muteIcon, muteLabel, muteLabel, muteIntent),
            RemoteAction(leaveIcon, leaveLabel, leaveLabel, leaveIntent),
        )
    }

    // endregion

    companion object {
        @JvmStatic
        external fun handleAndroidBackPressed()

        @JvmStatic
        external fun notifyPipModeChanged(isInPip: Boolean)

        @JvmStatic
        external fun notifyPipMuteToggled()

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
        private var pendingOperationResult: String? = null

        @Volatile
        private var pendingOperationEvent: String? = null

        @Volatile
        private var pendingOperationPackage: String? = null

        @Volatile
        private var pendingOperationRejected: Boolean? = null

        @Volatile
        private var intentError: String? = null

        @Volatile
        private var intentInFlight: Boolean = false

        @Volatile
        private var activeSignerRequest: String? = null

        @Volatile
        private var launchedSignerPackage: String? = null

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
        private const val REQUEST_LOGIN = "get_public_key"
        private const val REQUEST_SIGN_EVENT = "sign_event"
        private const val REQUEST_NIP04_ENCRYPT = "nip04_encrypt"
        private const val REQUEST_NIP04_DECRYPT = "nip04_decrypt"
        private const val REQUEST_NIP44_ENCRYPT = "nip44_encrypt"
        private const val REQUEST_NIP44_DECRYPT = "nip44_decrypt"

        // NIP-55 permissions requested up front for first-party publish flows.
        // Anything still outside this set can be approved later through the
        // same get_public_key intent flow when a sign_event call is rejected.
        private val NIP55_EVENT_KINDS = intArrayOf(
            0, 1, 3, 5, 6, 7, 8, 14, 16, 17, 20,
            30, 31, 32, 33, 52, 818,
            1063, 1068, 1111, 1301, 1311,
            1617, 1618, 1619, 1620, 1621, 1622,
            1984, 1985, 1987,
            4550, 4551, 4552, 4553, 4554,
            5300, 6969, 7001,
            7374, 7375, 7376, 9321,
            9734, 9802, 9805, 9806, 9807,
            10000, 10002, 10013, 10019, 10030, 10050, 10063, 10073, 10312,
            30000, 30001, 30004, 30008, 30009, 30023, 30030, 30040, 30041,
            30042, 30044, 30045, 30054, 30067, 30078,
            30311, 30312, 30313,
            30402, 30405,
            30817, 30818, 30819,
            31234, 31237, 31555,
            31922, 31923, 31924, 31925, 31926, 31927,
            33169, 34139, 34550, 34551, 36787, 38383, 39067
        )

        private val NIP55_PERMISSIONS: String by lazy {
            val signPermissions = NIP55_EVENT_KINDS.joinToString(",") {
                """{"type":"sign_event","kind":$it}"""
            }
            val cryptoPermissions = listOf(
                """{"type":"nip04_encrypt"}""",
                """{"type":"nip04_decrypt"}""",
                """{"type":"nip44_encrypt"}""",
                """{"type":"nip44_decrypt"}"""
            ).joinToString(",")
            "[$signPermissions,$cryptoPermissions]"
        }

        private fun maxUploadError(): String = "File too large (max 10MB)"

        @JvmStatic
        fun finishApp(context: Context) {
            val activity = instance
            if (activity == null) {
                Log.w(TAG, "finishApp called with no active MainActivity")
                return
            }

            activity.runOnUiThread {
                activity.finish()
            }
        }

        @JvmStatic
        fun setNestActive(context: Context, active: String) {
            val activity = instance ?: return
            activity.setNestActive(active == "true")
        }

        @JvmStatic
        fun enterPipMode(context: Context): String {
            val activity = instance ?: return "no_instance"
            if (!activity.packageManager.hasSystemFeature(PackageManager.FEATURE_PICTURE_IN_PICTURE)) {
                return "error:not_supported"
            }
            activity.runOnUiThread { activity.enterPip() }
            return "ok"
        }

        @JvmStatic
        fun isInPip(context: Context): String {
            val activity = instance ?: return "false"
            return activity.isInPipMode.toString()
        }

        @JvmStatic
        fun openUri(@Suppress("UNUSED_PARAMETER") context: Context, uri: String): String {
            return try {
                val activity = synchronized(lock) { instance }
                if (activity == null) {
                    Log.e(TAG, "openUri: no Activity instance")
                    return "error:no_instance"
                }

                val paymentUri = Uri.parse(uri)
                val intent = Intent(Intent.ACTION_VIEW, paymentUri).apply {
                    addCategory(Intent.CATEGORY_BROWSABLE)
                }

                val hasHandler = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
                    context.packageManager.queryIntentActivities(
                        intent,
                        PackageManager.ResolveInfoFlags.of(PackageManager.MATCH_DEFAULT_ONLY.toLong())
                    )
                } else {
                    @Suppress("DEPRECATION")
                    context.packageManager.queryIntentActivities(intent, PackageManager.MATCH_DEFAULT_ONLY)
                }.isNotEmpty()

                if (!hasHandler) {
                    Log.w(TAG, "openUri: no handler for $uri")
                    return "error:no_handler"
                }

                activity.runOnUiThread {
                    val chooser = Intent.createChooser(intent, "Open payment")
                    activity.startActivity(chooser)
                }
                "launched"
            } catch (e: Exception) {
                Log.e(TAG, "openUri failed for $uri", e)
                "error:${e.message}"
            }
        }

        @JvmStatic
        fun openLightningUri(@Suppress("UNUSED_PARAMETER") context: Context, uri: String): String {
            return try {
                val activity = synchronized(lock) { instance }
                if (activity == null) {
                    Log.e(TAG, "openLightningUri: no Activity instance")
                    return "error:no_instance"
                }

                val lightningUri = Uri.parse(uri)
                val intent = Intent(Intent.ACTION_VIEW, lightningUri).apply {
                    addCategory(Intent.CATEGORY_BROWSABLE)
                }

                val hasHandler = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
                    context.packageManager.queryIntentActivities(
                        intent,
                        PackageManager.ResolveInfoFlags.of(PackageManager.MATCH_DEFAULT_ONLY.toLong())
                    )
                } else {
                    @Suppress("DEPRECATION")
                    context.packageManager.queryIntentActivities(intent, PackageManager.MATCH_DEFAULT_ONLY)
                }.isNotEmpty()

                if (!hasHandler) {
                    Log.w(TAG, "openLightningUri: no handler for $uri")
                    return "error:no_handler"
                }

                activity.runOnUiThread {
                    val chooser = Intent.createChooser(intent, "Open Lightning payment")
                    activity.startActivity(chooser)
                }
                "launched"
            } catch (e: Exception) {
                Log.e(TAG, "openLightningUri failed for $uri", e)
                "error:${e.message}"
            }
        }

        private fun querySignerPackages(context: Context): Set<String> {
            val intent = Intent().apply {
                action = Intent.ACTION_VIEW
                data = Uri.parse("nostrsigner:")
            }
            val resolvedActivities = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
                context.packageManager.queryIntentActivities(
                    intent,
                    PackageManager.ResolveInfoFlags.of(PackageManager.MATCH_DEFAULT_ONLY.toLong())
                )
            } else {
                @Suppress("DEPRECATION")
                context.packageManager.queryIntentActivities(intent, PackageManager.MATCH_DEFAULT_ONLY)
            }

            return resolvedActivities
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
        private fun validateSignerPackage(context: Context, signerPackage: String): String? {
            if (!PACKAGE_NAME_REGEX.matches(signerPackage)) {
                Log.w(TAG, "Invalid package name format: $signerPackage")
                return "Invalid signer package name format"
            }
            return try {
                if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
                    context.packageManager.getPackageInfo(
                        signerPackage,
                        PackageManager.PackageInfoFlags.of(0)
                    )
                } else {
                    @Suppress("DEPRECATION")
                    context.packageManager.getPackageInfo(signerPackage, 0)
                }
                val validSignerPackages = querySignerPackages(context)
                if (signerPackage !in validSignerPackages) {
                    Log.w(TAG, "Package does not handle nostrsigner scheme: $signerPackage")
                    synchronized(lock) {
                        if (pendingPackage == signerPackage) {
                            pendingPackage = null
                        }
                    }
                    "Selected signer package cannot handle nostrsigner requests"
                } else {
                    null
                }
            } catch (e: PackageManager.NameNotFoundException) {
                Log.w(TAG, "Package not found: $signerPackage")
                "Selected signer package is not installed"
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

        // region Health Connect (workout suggestions)

        /**
         * True when a Health Connect provider exists on the device.
         * Called from Rust (platform/android_health.rs) via JNI.
         */
        @JvmStatic
        fun isHealthConnectAvailable(context: Context): String =
            if (HealthConnectBridge.isAvailable(context)) "true" else "false"

        /**
         * True when every Health Connect read permission is granted.
         * Returns "false" on OEM service-bind failures (never throws).
         */
        @JvmStatic
        fun hasHealthConnectPermissions(context: Context): String =
            if (HealthConnectBridge.hasAllPermissions(context)) "true" else "false"

        /**
         * Fire the Health Connect permission activity; the grant result is
         * polled afterwards via [hasHealthConnectPermissions].
         */
        @JvmStatic
        fun requestHealthConnectPermissions(context: Context): String = try {
            HealthConnectBridge.requestPermissions(context)
            "ok"
        } catch (e: Throwable) {
            Log.e(TAG, "requestHealthConnectPermissions failed", e)
            "error:${e.message ?: "request failed"}"
        }

        /**
         * Read finished Health Connect workouts since the given epoch
         * seconds; returns a JSON array (or an "error:..." string).
         */
        @JvmStatic
        fun readHealthConnectWorkouts(context: Context, sinceEpochSeconds: String): String = try {
            HealthConnectBridge.readWorkouts(context, sinceEpochSeconds.toLong())
        } catch (e: Throwable) {
            Log.e(TAG, "readHealthConnectWorkouts failed", e)
            "error:${e.message ?: "read failed"}"
        }

        // endregion

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
            if (validateSignerPackage(context, signerPackage) != null) {
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
                    if (it.moveToFirst() && isRejectedResponse(it)) {
                        Log.d(TAG, "getPublicKeyViaContentResolver: rejected by $signerPackage")
                        return null
                    }
                    val resultIndex = it.getColumnIndex("result")
                    if (resultIndex > -1) it.getString(resultIndex) else null
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
            if (validateSignerPackage(context, signerPackage) != null) {
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
                    if (it.moveToFirst() && isRejectedResponse(it)) {
                        Log.d(TAG, "signEventViaContentResolver: rejected by $signerPackage")
                        return null
                    }
                    val eventIndex = it.getColumnIndex("event")
                    if (eventIndex > -1) it.getString(eventIndex) else null
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
                    activeSignerRequest = REQUEST_LOGIN
                    pendingPubkey = null
                    pendingPackage = null
                    pendingOperationResult = null
                    pendingOperationEvent = null
                    pendingOperationPackage = null
                    pendingOperationRejected = null
                    intentError = null
                    launchedSignerPackage = null
                }

                val intent = Intent(Intent.ACTION_VIEW, Uri.parse("nostrsigner:")).apply {
                    putExtra("type", "get_public_key")
                    putExtra("permissions", NIP55_PERMISSIONS)
                }

                Log.d(TAG, "Launching get_public_key Intent")
                activity.runOnUiThread {
                    activity.signerLauncher.launch(intent)
                }
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
         * Launch the get_public_key Intent for a specific signer package so an
         * already connected signer can refresh or expand permissions in place.
         */
        @JvmStatic
        fun launchGetPublicKeyForPackage(context: Context, signerPackage: String): String {
            validateSignerPackage(context, signerPackage)?.let { return "error:$it" }

            return try {
                val activity = synchronized(lock) { instance }
                if (activity == null) {
                    Log.e(TAG, "launchGetPublicKeyForPackage: no Activity instance")
                    return "no_instance"
                }

                synchronized(lock) {
                    if (intentInFlight) {
                        Log.w(TAG, "launchGetPublicKeyForPackage: Intent already in flight")
                        return "already_in_flight"
                    }
                    intentInFlight = true
                    activeSignerRequest = REQUEST_LOGIN
                    pendingPubkey = null
                    pendingPackage = null
                    pendingOperationResult = null
                    pendingOperationEvent = null
                    pendingOperationPackage = null
                    pendingOperationRejected = null
                    intentError = null
                    launchedSignerPackage = signerPackage
                }

                val intent = Intent(Intent.ACTION_VIEW, Uri.parse("nostrsigner:")).apply {
                    `package` = signerPackage
                    putExtra("type", "get_public_key")
                    putExtra("permissions", NIP55_PERMISSIONS)
                }

                Log.d(TAG, "Launching package-scoped get_public_key Intent for $signerPackage")
                activity.runOnUiThread {
                    activity.signerLauncher.launch(intent)
                }
                "launched"
            } catch (e: Exception) {
                Log.e(TAG, "launchGetPublicKeyForPackage failed for $signerPackage", e)
                synchronized(lock) {
                    intentInFlight = false
                    intentError = e.message ?: "Unknown error"
                }
                "error:${e.message}"
            }
        }

        private fun launchSignerOperationIntent(
            context: Context,
            signerPackage: String,
            requestType: String,
            uriPayload: String,
            extras: Map<String, String>
        ): String {
            validateSignerPackage(context, signerPackage)?.let { return "error:$it" }

            return try {
                val activity = synchronized(lock) { instance }
                if (activity == null) {
                    Log.e(TAG, "launchSignerOperationIntent: no Activity instance")
                    return "no_instance"
                }

                synchronized(lock) {
                    if (intentInFlight) {
                        Log.w(TAG, "launchSignerOperationIntent: Intent already in flight")
                        return "already_in_flight"
                    }
                    intentInFlight = true
                    activeSignerRequest = requestType
                    pendingPubkey = null
                    pendingPackage = null
                    pendingOperationResult = null
                    pendingOperationEvent = null
                    pendingOperationPackage = null
                    pendingOperationRejected = null
                    intentError = null
                    launchedSignerPackage = signerPackage
                }

                val intent = Intent(
                    Intent.ACTION_VIEW,
                    Uri.fromParts("nostrsigner", uriPayload, null)
                ).apply {
                    `package` = signerPackage
                    putExtra("type", requestType)
                    extras.forEach { (key, value) -> putExtra(key, value) }
                }

                Log.d(TAG, "Launching signer operation request=$requestType for $signerPackage")
                activity.runOnUiThread {
                    activity.signerLauncher.launch(intent)
                }
                "launched"
            } catch (e: Exception) {
                Log.e(TAG, "launchSignerOperationIntent failed for $requestType/$signerPackage", e)
                synchronized(lock) {
                    intentInFlight = false
                    activeSignerRequest = null
                    intentError = e.message ?: "Unknown error"
                }
                "error:${e.message}"
            }
        }

        @JvmStatic
        fun launchSignEventIntent(
            context: Context,
            signerPackage: String,
            eventJson: String,
            currentUser: String
        ): String = launchSignerOperationIntent(
            context,
            signerPackage,
            REQUEST_SIGN_EVENT,
            eventJson,
            mapOf("current_user" to currentUser)
        )

        @JvmStatic
        fun launchNip04EncryptIntent(
            context: Context,
            signerPackage: String,
            plaintext: String,
            pubkey: String,
            currentUser: String
        ): String = launchSignerOperationIntent(
            context,
            signerPackage,
            REQUEST_NIP04_ENCRYPT,
            plaintext,
            mapOf("pubKey" to pubkey, "current_user" to currentUser)
        )

        @JvmStatic
        fun launchNip04DecryptIntent(
            context: Context,
            signerPackage: String,
            ciphertext: String,
            pubkey: String,
            currentUser: String
        ): String = launchSignerOperationIntent(
            context,
            signerPackage,
            REQUEST_NIP04_DECRYPT,
            ciphertext,
            mapOf("pubKey" to pubkey, "current_user" to currentUser)
        )

        @JvmStatic
        fun launchNip44EncryptIntent(
            context: Context,
            signerPackage: String,
            plaintext: String,
            pubkey: String,
            currentUser: String
        ): String = launchSignerOperationIntent(
            context,
            signerPackage,
            REQUEST_NIP44_ENCRYPT,
            plaintext,
            mapOf("pubKey" to pubkey, "current_user" to currentUser)
        )

        @JvmStatic
        fun launchNip44DecryptIntent(
            context: Context,
            signerPackage: String,
            ciphertext: String,
            pubkey: String,
            currentUser: String
        ): String = launchSignerOperationIntent(
            context,
            signerPackage,
            REQUEST_NIP44_DECRYPT,
            ciphertext,
            mapOf("pubKey" to pubkey, "current_user" to currentUser)
        )

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

        @JvmStatic
        fun pollOperationResult(@Suppress("UNUSED_PARAMETER") context: Context): String? {
            return synchronized(lock) { pendingOperationResult }
        }

        @JvmStatic
        fun pollOperationEvent(@Suppress("UNUSED_PARAMETER") context: Context): String? {
            return synchronized(lock) { pendingOperationEvent }
        }

        @JvmStatic
        fun pollOperationPackage(@Suppress("UNUSED_PARAMETER") context: Context): String? {
            return synchronized(lock) { pendingOperationPackage }
        }

        @JvmStatic
        fun pollOperationRejected(@Suppress("UNUSED_PARAMETER") context: Context): String? {
            return synchronized(lock) { pendingOperationRejected?.toString() }
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
                pendingOperationResult = null
                pendingOperationEvent = null
                pendingOperationPackage = null
                pendingOperationRejected = null
                intentError = null
                intentInFlight = false
                activeSignerRequest = null
                launchedSignerPackage = null
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

                ByteArrayInputStream(contentBase64.toByteArray(Charsets.US_ASCII)).use { encoded ->
                    Base64InputStream(encoded, android.util.Base64.NO_WRAP).use { decoded ->
                        file.outputStream().use { output ->
                            decoded.copyTo(output)
                        }
                    }
                }

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
            if (validateSignerPackage(context, signerPackage) != null) {
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
                    if (it.moveToFirst() && isRejectedResponse(it)) {
                        Log.d(TAG, "$method: rejected by $signerPackage")
                        return null
                    }
                    val resultIndex = it.getColumnIndex("result")
                    if (resultIndex > -1) it.getString(resultIndex) else null
                }
            } catch (e: Exception) {
                Log.e(TAG, "$method failed for $signerPackage", e)
                null
            }
        }

        private fun isRejectedResponse(cursor: android.database.Cursor): Boolean {
            val rejectedIndex = cursor.getColumnIndex("rejected")
            if (rejectedIndex < 0) {
                return false
            }
            return try {
                when (cursor.getType(rejectedIndex)) {
                    android.database.Cursor.FIELD_TYPE_INTEGER -> cursor.getInt(rejectedIndex) != 0
                    android.database.Cursor.FIELD_TYPE_STRING -> {
                        val value = cursor.getString(rejectedIndex)?.trim()?.lowercase(Locale.ROOT)
                        value == "1" || value == "true" || value == "yes"
                    }
                    else -> false
                }
            } catch (e: Exception) {
                Log.w(TAG, "Failed to read rejected column", e)
                false
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
                activity.runOnUiThread {
                    activity.filePickerLauncher.launch(arrayOf("*/*"))
                }
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

                activity.runOnUiThread {
                    activity.imagePickerLauncher.launch("image/*")
                }
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
            // NativeAudioBridge.setQueue already calls ensureServiceStarted()
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

        @JvmStatic
        fun saveBrowseCache(context: Context, key: String, json: String): String {
            return try {
                BrowseCache.save(context, key, json)
                "ok"
            } catch (e: Exception) {
                Log.e(TAG, "saveBrowseCache failed", e)
                "error:${e.message}"
            }
        }

        @JvmStatic
        fun saveBrowsePosition(context: Context, mediaId: String, positionMs: Long): String {
            return try {
                BrowseCache.savePosition(context, mediaId, positionMs)
                "ok"
            } catch (e: Exception) {
                Log.e(TAG, "saveBrowsePosition failed", e)
                "error:${e.message}"
            }
        }

        @JvmStatic
        fun copyToClipboard(context: Context, text: String): String {
            return try {
                val clipboard = context.getSystemService(Context.CLIPBOARD_SERVICE)
                    as android.content.ClipboardManager
                val clip = android.content.ClipData.newPlainText("text", text)
                clipboard.setPrimaryClip(clip)
                "success"
            } catch (e: Exception) {
                Log.e(TAG, "copyToClipboard failed", e)
                "error:${e.message}"
            }
        }

        @JvmStatic
        fun readFromClipboard(context: Context): String {
            return try {
                val clipboard = context.getSystemService(Context.CLIPBOARD_SERVICE)
                    as android.content.ClipboardManager
                if (!clipboard.hasPrimaryClip()) return "empty"
                val item = clipboard.primaryClip?.getItemAt(0) ?: return "empty"
                val text = item.text?.toString() ?: return "empty"
                text
            } catch (e: Exception) {
                Log.e(TAG, "readFromClipboard failed", e)
                "error:${e.message}"
            }
        }

        private const val GOOGLE_WEB_CLIENT_ID =
            "665414552910-b0b9mu4guac4bk9hdoc751uqqmd6irum.apps.googleusercontent.com"
        private const val DRIVE_APPDATA_SCOPE = "https://www.googleapis.com/auth/drive.appdata"
        private const val DRIVE_FILES_URL = "https://www.googleapis.com/drive/v3/files"
        private const val DRIVE_UPLOAD_URL = "https://www.googleapis.com/upload/drive/v3/files"
        private const val BACKUP_PREFIX = "nostrblue_backup_"
        private const val BACKUP_SUFFIX = ".bin"

        @JvmStatic
        fun signInWithGoogle(context: Context): String {
            return try {
                // This is a synchronous stub that returns pending status.
                // The actual sign-in flow is initiated via signInWithGoogleAsync below.
                // For JNI compatibility we return a JSON error indicating async is needed.
                kotlinx.coroutines.runBlocking {
                    signInWithGoogleInternal(context)
                }
            } catch (e: Exception) {
                Log.e(TAG, "signInWithGoogle failed", e)
                """{"error":"${e.message?.replace("\"", "\\\"")}"}"""
            }
        }

        private suspend fun signInWithGoogleInternal(context: Context): String {
            return kotlinx.coroutines.withContext(kotlinx.coroutines.Dispatchers.IO) {
                try {
                    // Step 1: Get sub via CredentialManager
                    val credentialManager = androidx.credentials.CredentialManager.create(context)

                    // We need an Activity for CredentialManager — use the static instance
                    val activity = instance
                        ?: return@withContext """{"error":"No active MainActivity"}"""

                    val googleIdOption = com.google.android.libraries.identity.googleid.GetGoogleIdOption.Builder()
                        .setServerClientId(GOOGLE_WEB_CLIENT_ID)
                        .setFilterByAuthorizedAccounts(false)
                        .setAutoSelectEnabled(false)
                        .build()

                    val response = try {
                        val request = androidx.credentials.GetCredentialRequest.Builder()
                            .addCredentialOption(googleIdOption)
                            .build()
                        credentialManager.getCredential(activity, request)
                    } catch (e: androidx.credentials.exceptions.NoCredentialException) {
                        val signInOption = com.google.android.libraries.identity.googleid.GetSignInWithGoogleOption.Builder(GOOGLE_WEB_CLIENT_ID)
                            .build()
                        val fallbackRequest = androidx.credentials.GetCredentialRequest.Builder()
                            .addCredentialOption(signInOption)
                            .build()
                        credentialManager.getCredential(activity, fallbackRequest)
                    }
                    val credential = response.credential
                    if (credential !is androidx.credentials.CustomCredential ||
                        credential.type != com.google.android.libraries.identity.googleid.GoogleIdTokenCredential.TYPE_GOOGLE_ID_TOKEN_CREDENTIAL
                    ) {
                        return@withContext """{"error":"Unexpected credential type"}"""
                    }

                    val parsed = com.google.android.libraries.identity.googleid.GoogleIdTokenCredential
                        .createFrom(credential.data)
                    val idToken = parsed.idToken
                    val payloadB64 = idToken.split(".")[1]
                    val decoded = android.util.Base64.decode(
                        payloadB64,
                        android.util.Base64.URL_SAFE or android.util.Base64.NO_WRAP or android.util.Base64.NO_PADDING
                    )
                    val jwtJson = org.json.JSONObject(String(decoded, Charsets.UTF_8))
                    val sub = jwtJson.getString("sub")

                    // Step 2: Get Drive access token via AuthorizationClient
                    val authClient = com.google.android.gms.auth.api.identity.Identity
                        .getAuthorizationClient(activity)
                    val authRequest = com.google.android.gms.auth.api.identity.AuthorizationRequest.Builder()
                        .setRequestedScopes(listOf(com.google.android.gms.common.api.Scope(DRIVE_APPDATA_SCOPE)))
                        .build()

                    val authResult = com.google.android.gms.tasks.Tasks.await(authClient.authorize(authRequest))
                    val accessToken = authResult.accessToken
                        ?: return@withContext """{"error":"No access token returned"}"""

                    """{"sub":"$sub","accessToken":"$accessToken"}"""
                } catch (e: Exception) {
                    Log.e(TAG, "signInWithGoogleInternal failed", e)
                    """{"error":"${e.message?.replace("\"", "\\\"")?.replace("\n", " ")}"}"""
                }
            }
        }

        @JvmStatic
        fun listDriveBackups(context: Context, accessToken: String): String {
            return try {
                val query = java.net.URLEncoder.encode("name contains '$BACKUP_PREFIX'", "UTF-8")
                val url = "$DRIVE_FILES_URL?spaces=appDataFolder&q=$query&fields=files(id,name,modifiedTime)&pageSize=100"
                val client = okhttp3.OkHttpClient()
                val request = okhttp3.Request.Builder()
                    .url(url)
                    .header("Authorization", "Bearer $accessToken")
                    .get()
                    .build()
                val response = client.newCall(request).execute()
                if (!response.isSuccessful) {
                    return """{"error":"Drive list failed: ${response.code}"}"""
                }
                val body = response.body?.string() ?: return """[]"""
                val json = org.json.JSONObject(body)
                val files = json.optJSONArray("files") ?: org.json.JSONArray()
                val result = org.json.JSONArray()
                for (i in 0 until files.length()) {
                    val file = files.getJSONObject(i)
                    val entry = org.json.JSONObject()
                    entry.put("fileId", file.getString("id"))
                    entry.put("name", file.getString("name"))
                    result.put(entry)
                }
                result.toString()
            } catch (e: Exception) {
                Log.e(TAG, "listDriveBackups failed", e)
                """{"error":"${e.message?.replace("\"", "\\\"")}"}"""
            }
        }

        @JvmStatic
        fun uploadDriveBackup(context: Context, accessToken: String, combinedArg: String): String {
            return try {
                val parts = combinedArg.split("|", limit = 2)
                val npub = parts[0]
                val payload = parts.getOrElse(1) { "" }
                val filename = BACKUP_PREFIX + npub + BACKUP_SUFFIX

                val oldFileIds = mutableListOf<String>()
                try {
                    val listResult = listDriveBackups(context, accessToken)
                    val arr = org.json.JSONArray(listResult)
                    for (i in 0 until arr.length()) {
                        val f = arr.getJSONObject(i)
                        if (f.getString("name") == filename) {
                            oldFileIds.add(f.getString("fileId"))
                        }
                    }
                } catch (e: Exception) {
                    Log.w(TAG, "Failed to list existing backups for cleanup", e)
                }

                val metadata = """{"name":"$filename","parents":["appDataFolder"]}"""
                val multipartBody = okhttp3.MultipartBody.Builder()
                    .setType("multipart/related".toMediaType())
                    .addPart(metadata.toRequestBody("application/json; charset=UTF-8".toMediaType()))
                    .addPart(payload.toRequestBody("application/octet-stream".toMediaType()))
                    .build()

                val client = okhttp3.OkHttpClient()
                val request = okhttp3.Request.Builder()
                    .url("$DRIVE_UPLOAD_URL?uploadType=multipart")
                    .header("Authorization", "Bearer $accessToken")
                    .post(multipartBody)
                    .build()
                val response = client.newCall(request).execute()
                if (!response.isSuccessful) {
                    val errorBody = response.body?.string() ?: ""
                    return """{"error":"Upload failed: ${response.code} $errorBody"}"""
                }

                for (oldId in oldFileIds) {
                    try {
                        deleteDriveBackup(context, accessToken, oldId)
                    } catch (e: Exception) {
                        Log.w(TAG, "Failed to delete old backup $oldId", e)
                    }
                }

                """{"success":true}"""
            } catch (e: Exception) {
                Log.e(TAG, "uploadDriveBackup failed", e)
                """{"error":"${e.message?.replace("\"", "\\\"")}"}"""
            }
        }

        @JvmStatic
        fun downloadDriveBackup(context: Context, accessToken: String, fileId: String): String {
            return try {
                val client = okhttp3.OkHttpClient()
                val request = okhttp3.Request.Builder()
                    .url("$DRIVE_FILES_URL/$fileId?alt=media")
                    .header("Authorization", "Bearer $accessToken")
                    .get()
                    .build()
                val response = client.newCall(request).execute()
                if (!response.isSuccessful) {
                    return """{"error":"Download failed: ${response.code}"}"""
                }
                val payload = response.body?.string() ?: ""
                """{"payload":${org.json.JSONObject.quote(payload)}}"""
            } catch (e: Exception) {
                Log.e(TAG, "downloadDriveBackup failed", e)
                """{"error":"${e.message?.replace("\"", "\\\"")}"}"""
            }
        }

        @JvmStatic
        fun deleteDriveBackup(context: Context, accessToken: String, fileId: String): String {
            return try {
                val client = okhttp3.OkHttpClient()
                val request = okhttp3.Request.Builder()
                    .url("$DRIVE_FILES_URL/$fileId")
                    .header("Authorization", "Bearer $accessToken")
                    .delete()
                    .build()
                val response = client.newCall(request).execute()
                response.close()
                if (!response.isSuccessful) {
                    return """{"error":"Delete failed: ${response.code}"}"""
                }
                """{"success":true}"""
            } catch (e: Exception) {
                Log.e(TAG, "deleteDriveBackup failed", e)
                """{"error":"${e.message?.replace("\"", "\\\"")}"}"""
            }
        }
    }
}
