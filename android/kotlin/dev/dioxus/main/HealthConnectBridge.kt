package dev.dioxus.main

import android.content.Context
import android.content.Intent
import androidx.health.connect.client.HealthConnectClient
import androidx.health.connect.client.PermissionController
import androidx.health.connect.client.aggregate.AggregateMetric
import androidx.health.connect.client.aggregate.AggregationResult
import androidx.health.connect.client.records.ActiveCaloriesBurnedRecord
import androidx.health.connect.client.records.DistanceRecord
import androidx.health.connect.client.records.ElevationGainedRecord
import androidx.health.connect.client.records.ExerciseSessionRecord
import androidx.health.connect.client.records.HeartRateRecord
import androidx.health.connect.client.records.StepsRecord
import androidx.health.connect.client.records.TotalCaloriesBurnedRecord
import androidx.health.connect.client.request.AggregateRequest
import androidx.health.connect.client.request.ReadRecordsRequest
import androidx.health.connect.client.time.TimeRangeFilter
import androidx.health.connect.client.units.Energy
import androidx.health.connect.client.units.Length
import kotlinx.coroutines.runBlocking
import org.json.JSONArray
import org.json.JSONObject
import java.time.Instant

/**
 * Reads finished workouts from Android Health Connect - the single aggregator
 * every Android health source funnels into (Samsung Health/Galaxy Watch,
 * Google Fit, Fitbit, Garmin Connect, Strava, ...) - and maps each session to
 * a JSON object ready to become a NIP-101e kind 1301 event on the Rust side.
 *
 * Read-only. The caller (Rust) decides when to prompt for permission via
 * [requestPermissions]; every entry point is defensive and never throws.
 */
object HealthConnectBridge {
    private const val TAG = "HealthConnectBridge"

    /** Platform requestPermissions code (Android 14+ path). */
    private const val PERMISSIONS_REQUEST_CODE = 4711

    /** Read permissions for the seven record types the suggestion needs. */
    val PERMISSIONS = setOf(
        androidx.health.connect.client.permission.HealthPermission
            .getReadPermission(ExerciseSessionRecord::class),
        androidx.health.connect.client.permission.HealthPermission
            .getReadPermission(DistanceRecord::class),
        androidx.health.connect.client.permission.HealthPermission
            .getReadPermission(ActiveCaloriesBurnedRecord::class),
        androidx.health.connect.client.permission.HealthPermission
            .getReadPermission(TotalCaloriesBurnedRecord::class),
        androidx.health.connect.client.permission.HealthPermission
            .getReadPermission(HeartRateRecord::class),
        androidx.health.connect.client.permission.HealthPermission
            .getReadPermission(StepsRecord::class),
        androidx.health.connect.client.permission.HealthPermission
            .getReadPermission(ElevationGainedRecord::class),
    )

    /** Friendly names for well-known writers when their app isn't installed. */
    private val KNOWN_SOURCES = mapOf(
        "com.sec.android.app.shealth" to "Samsung Health",
        "com.google.android.apps.fitness" to "Google Fit",
        "com.google.android.apps.healthdata" to "Health Connect",
        "com.fitbit.FitbitMobile" to "Fitbit",
        "com.garmin.android.apps.connectmobile" to "Garmin Connect",
        "com.strava" to "Strava",
        "com.nike.plusgps" to "Nike Run Club",
    )

    fun isAvailable(context: Context): Boolean =
        HealthConnectClient.getSdkStatus(context) == HealthConnectClient.SDK_AVAILABLE

    /**
     * True when every permission in [PERMISSIONS] is granted. Also returns
     * false on OEM service-bind failures: some builds report the SDK as
     * available yet fail to bind the Health Connect service
     * ("Binding to service failed" RemoteException on ITEL/low-end devices),
     * so the permission check itself must be guarded.
     */
    fun hasAllPermissions(context: Context): Boolean = try {
        val client = HealthConnectClient.getOrCreate(context)
        // getGrantedPermissions() became suspend in connect-client 1.1.0.
        runBlocking {
            client.permissionController.getGrantedPermissions().containsAll(PERMISSIONS)
        }
    } catch (e: Exception) {
        android.util.Log.w(TAG, "Health Connect permission check failed", e)
        false
    } catch (e: Throwable) {
        android.util.Log.w(TAG, "Health Connect permission check failed", e)
        false
    }

    /**
     * Fire the Health Connect permission flow. Result delivery is polled
     * from Rust via [hasAllPermissions] afterwards - the system sheet runs
     * outside the app so there is no callback into native code.
     */
    fun requestPermissions(context: Context) {
        if (android.os.Build.VERSION.SDK_INT >= 34) {
            // On Android 14+ health permissions are platform runtime
            // permissions: the PermissionController contract extends the
            // androidx RequestMultiplePermissions contract here, whose
            // intent is only valid through registerForActivityResult —
            // startActivity() on it throws ActivityNotFoundException.
            // Use the platform permission dialog instead.
            val activity = context as? android.app.Activity
            if (activity != null) {
                val perms = PERMISSIONS.map { it.toString() }.toTypedArray()
                activity.requestPermissions(perms, PERMISSIONS_REQUEST_CODE)
            } else {
                android.util.Log.e(
                    TAG,
                    "requestPermissions: context is not an Activity (${context.javaClass.name})",
                )
            }
            return
        }
        // Android 8-13: Health Connect ships as a separate app; its
        // permission-activity intent resolves via the manifest <queries>.
        val intent: Intent = PermissionController
            .createRequestPermissionResultContract()
            .createIntent(context, PERMISSIONS)
        intent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
        context.startActivity(intent)
    }

    /**
     * Read sessions that ended within `sinceEpochSeconds..now` and return
     * them as a JSON array. Sessions whose type Amethyst-style vocabulary
     * cannot represent, or with non-positive duration, are skipped.
     */
    fun readWorkouts(context: Context, sinceEpochSeconds: Long): String {
        if (!isAvailable(context)) return "[]"
        val client = try {
            HealthConnectClient.getOrCreate(context)
        } catch (e: Throwable) {
            android.util.Log.w(TAG, "Health Connect client creation failed", e)
            return "error:${e.message ?: "client unavailable"}"
        }
        return try {
            runBlocking {
                val since = Instant.ofEpochSecond(sinceEpochSeconds)
                val now = Instant.now()
                val response = client.readRecords(
                    ReadRecordsRequest(
                        ExerciseSessionRecord::class,
                        timeRangeFilter = TimeRangeFilter.between(since, now),
                    )
                )
                val array = JSONArray()
                for (session in response.records) {
                    mapSession(context, client, session)?.let { array.put(it) }
                }
                array.toString()
            }
        } catch (e: Throwable) {
            android.util.Log.w(TAG, "Failed to read workouts from Health Connect", e)
            "error:${e.message ?: "read failed"}"
        }
    }

    /** Returns null for unmapped types / non-positive durations. */
    private fun mapSession(
        context: Context,
        client: HealthConnectClient,
        session: ExerciseSessionRecord,
    ): JSONObject? {
        val code = exerciseCode(session.exerciseType) ?: run {
            android.util.Log.i(
                TAG,
                "Skipping session: unmapped exerciseType=${session.exerciseType} " +
                    "title=${session.title} from ${session.metadata.dataOrigin.packageName}",
            )
            return null
        }
        val durationSeconds = java.time.Duration
            .between(session.startTime, session.endTime).seconds
        if (durationSeconds <= 0) {
            android.util.Log.i(TAG, "Skipping session: non-positive duration ($durationSeconds s)")
            return null
        }
        val totals = aggregate(client, session)
        val obj = JSONObject()
        obj.put("id", session.metadata.id)
        obj.put("exercise", code)
        session.title?.takeIf { it.isNotBlank() }?.let { obj.put("title", it) }
        obj.put("start", session.startTime.epochSecond)
        obj.put("end", session.endTime.epochSecond)
        lengthMeters(totals, DistanceRecord.DISTANCE_TOTAL)?.let { obj.put("distance", it) }
        // Active calories match what RUNSTR publishes; total includes basal
        // burn, so it over-reports the workout if used as the primary figure.
        energyKcal(totals, ActiveCaloriesBurnedRecord.ACTIVE_CALORIES_TOTAL)
            ?.let { obj.put("activeCalories", it) }
        energyKcal(totals, TotalCaloriesBurnedRecord.ENERGY_TOTAL)
            ?.let { obj.put("totalCalories", it) }
        longValue(totals, HeartRateRecord.BPM_AVG)?.let { obj.put("avgHr", it.toDouble()) }
        longValue(totals, HeartRateRecord.BPM_MAX)?.let { obj.put("maxHr", it.toDouble()) }
        longValue(totals, StepsRecord.COUNT_TOTAL)?.let { obj.put("steps", it.toDouble()) }
        lengthMeters(totals, ElevationGainedRecord.ELEVATION_GAINED_TOTAL)
            ?.let { obj.put("elevation", it) }
        obj.put("source", resolveSourceName(context, session.metadata.dataOrigin.packageName))
        return obj
    }

    /**
     * connect-client 1.1.0 renamed `AggregateResponse` to
     * `AggregationResult`; `get(metric)` is nullable there.
     */
    private fun lengthMeters(totals: AggregationResult?, metric: AggregateMetric<Length>): Double? =
        totals?.get(metric)?.inMeters

    private fun energyKcal(totals: AggregationResult?, metric: AggregateMetric<Energy>): Double? =
        totals?.get(metric)?.inKilocalories

    private fun longValue(totals: AggregationResult?, metric: AggregateMetric<Long>): Long? =
        totals?.get(metric)

    /** One aggregate request over the exact session window, all 7 metrics. */
    private fun aggregate(
        client: HealthConnectClient,
        session: ExerciseSessionRecord,
    ): AggregationResult? = try {
        runBlocking {
            client.aggregate(
                AggregateRequest(
                    metrics = setOf(
                        DistanceRecord.DISTANCE_TOTAL,
                        ActiveCaloriesBurnedRecord.ACTIVE_CALORIES_TOTAL,
                        TotalCaloriesBurnedRecord.ENERGY_TOTAL,
                        HeartRateRecord.BPM_AVG,
                        HeartRateRecord.BPM_MAX,
                        StepsRecord.COUNT_TOTAL,
                        ElevationGainedRecord.ELEVATION_GAINED_TOTAL,
                    ),
                    timeRangeFilter = TimeRangeFilter.between(session.startTime, session.endTime),
                )
            )
        }
    } catch (e: Throwable) {
        android.util.Log.w(TAG, "Failed to aggregate workout metrics", e)
        null
    }

    /**
     * Health Connect exercise-type constants mapped to the NIP-101e wire
     * vocabulary; anything else is skipped rather than mislabeled.
     */
    private fun exerciseCode(type: Int): String? = when (type) {
        ExerciseSessionRecord.EXERCISE_TYPE_RUNNING,
        ExerciseSessionRecord.EXERCISE_TYPE_RUNNING_TREADMILL,
            -> "running"
        ExerciseSessionRecord.EXERCISE_TYPE_WALKING -> "walking"
        ExerciseSessionRecord.EXERCISE_TYPE_BIKING,
        ExerciseSessionRecord.EXERCISE_TYPE_BIKING_STATIONARY,
            -> "cycling"
        ExerciseSessionRecord.EXERCISE_TYPE_HIKING -> "hiking"
        ExerciseSessionRecord.EXERCISE_TYPE_SWIMMING_POOL,
        ExerciseSessionRecord.EXERCISE_TYPE_SWIMMING_OPEN_WATER,
            -> "swimming"
        ExerciseSessionRecord.EXERCISE_TYPE_ROWING,
        ExerciseSessionRecord.EXERCISE_TYPE_ROWING_MACHINE,
            -> "rowing"
        ExerciseSessionRecord.EXERCISE_TYPE_STRENGTH_TRAINING,
        ExerciseSessionRecord.EXERCISE_TYPE_WEIGHTLIFTING,
        ExerciseSessionRecord.EXERCISE_TYPE_CALISTHENICS,
            -> "strength"
        ExerciseSessionRecord.EXERCISE_TYPE_YOGA -> "yoga"
        else -> null
    }

    /** Installed app label, else the well-known map, else the raw package. */
    private fun resolveSourceName(context: Context, packageName: String): String {
        if (packageName.isBlank()) return "Health Connect"
        try {
            val info = context.packageManager.getApplicationInfo(packageName, 0)
            val label = context.packageManager.getApplicationLabel(info).toString()
            if (label.isNotBlank()) return label
        } catch (e: Throwable) {
            // Not installed / uninstalled; fall through to the known map.
        }
        return KNOWN_SOURCES[packageName] ?: packageName
    }
}
