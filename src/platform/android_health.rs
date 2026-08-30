//! Android Health Connect bridge (JNI, mobile only).
//!
//! Reads finished workouts from Android Health Connect — the single
//! aggregator every Android health source funnels into (Samsung
//! Health/Galaxy Watch, Google Fit, Fitbit, Garmin Connect, Strava, …) —
//! via static methods on `dev.dioxus.main.MainActivity` that delegate to
//! the Kotlin `HealthConnectBridge`. Read-only; the caller decides when
//! to prompt for permission.
use crate::utils::nips::nip101e::ExerciseType;
use crate::utils::workout_merger::DetectedWorkout;
use serde::Deserialize;

/// How far back the composer carousel looks for workouts to offer.
pub const LOOKBACK_DAYS: u64 = 7;

#[derive(Deserialize)]
struct RawWorkout {
    id: String,
    /// Exercise code already mapped by the Kotlin side (e.g. "running");
    /// sessions with unmapped types are skipped there.
    exercise: String,
    title: Option<String>,
    start: u64,
    end: u64,
    distance: Option<f64>,
    #[serde(rename = "activeCalories")]
    active_calories: Option<f64>,
    #[serde(rename = "totalCalories")]
    total_calories: Option<f64>,
    #[serde(rename = "avgHr")]
    avg_hr: Option<f64>,
    #[serde(rename = "maxHr")]
    max_hr: Option<f64>,
    steps: Option<f64>,
    elevation: Option<f64>,
    source: String,
}

fn call_static_string(method_name: &str) -> Option<String> {
    use jni::objects::JValue;

    let ctx = ndk_context::android_context();
    let vm = match unsafe { jni::JavaVM::from_raw(ctx.vm().cast()) } {
        Ok(v) => v,
        Err(e) => {
            log::error!("HC JNI: JavaVM::from_raw failed in {}: {}", method_name, e);
            return None;
        }
    };
    let mut env = match vm.attach_current_thread() {
        Ok(e) => e,
        Err(e) => {
            log::error!("HC JNI: attach_current_thread failed in {}: {}", method_name, e);
            return None;
        }
    };
    let context = unsafe { jni::objects::JObject::from_raw(ctx.context().cast()) };
    let class =
        crate::platform::mobile::find_app_class(&mut env, &context, "dev.dioxus.main.MainActivity")?;
    let result = match env.call_static_method(
        class,
        method_name,
        "(Landroid/content/Context;)Ljava/lang/String;",
        &[JValue::Object(&context)],
    ) {
        Ok(v) => match v.l() {
            Ok(l) => l,
            Err(e) => {
                log::error!("HC JNI: {}.l() failed: {}", method_name, e);
                return None;
            }
        },
        Err(e) => {
            log::error!("HC JNI: {} call failed: {}", method_name, e);
            return None;
        }
    };
    if result.is_null() {
        return None;
    }
    let ret = match env.get_string((&result).into()) {
        Ok(jstr) => Some(jstr.to_string_lossy().into_owned()),
        Err(e) => {
            log::error!("HC JNI: get_string failed in {}: {}", method_name, e);
            None
        }
    };
    ret
}

fn call_static_string_with_arg(method_name: &str, arg: &str) -> Option<String> {
    use jni::objects::JValue;

    let ctx = ndk_context::android_context();
    let vm = match unsafe { jni::JavaVM::from_raw(ctx.vm().cast()) } {
        Ok(v) => v,
        Err(e) => {
            log::error!("HC JNI: JavaVM::from_raw failed in {}: {}", method_name, e);
            return None;
        }
    };
    let mut env = match vm.attach_current_thread() {
        Ok(e) => e,
        Err(e) => {
            log::error!("HC JNI: attach_current_thread failed in {}: {}", method_name, e);
            return None;
        }
    };
    let context = unsafe { jni::objects::JObject::from_raw(ctx.context().cast()) };
    let j_arg = match env.new_string(arg) {
        Ok(s) => s,
        Err(e) => {
            log::error!("HC JNI: new_string failed in {}: {}", method_name, e);
            return None;
        }
    };
    let class =
        crate::platform::mobile::find_app_class(&mut env, &context, "dev.dioxus.main.MainActivity")?;
    let result = match env.call_static_method(
        class,
        method_name,
        "(Landroid/content/Context;Ljava/lang/String;)Ljava/lang/String;",
        &[JValue::Object(&context), JValue::Object(&j_arg)],
    ) {
        Ok(v) => match v.l() {
            Ok(l) => l,
            Err(e) => {
                log::error!("HC JNI: {}.l() failed: {}", method_name, e);
                return None;
            }
        },
        Err(e) => {
            log::error!("HC JNI: {} call failed: {}", method_name, e);
            return None;
        }
    };
    if result.is_null() {
        return None;
    }
    let ret = match env.get_string((&result).into()) {
        Ok(jstr) => Some(jstr.to_string_lossy().into_owned()),
        Err(e) => {
            log::error!("HC JNI: get_string failed in {}: {}", method_name, e);
            None
        }
    };
    ret
}

/// True when a Health Connect provider exists on the device.
pub fn is_health_connect_available() -> bool {
    call_static_string("isHealthConnectAvailable").as_deref() == Some("true")
}

/// True when every read permission we need is granted. Also returns
/// false on OEM service-bind failures (some builds report the SDK as
/// available yet fail to bind), matching Amethyst's guard.
pub fn has_all_health_permissions() -> bool {
    call_static_string("hasHealthConnectPermissions").as_deref() == Some("true")
}

/// Fire the Health Connect permission activity. Result delivery is
/// polled afterwards via [has_all_health_permissions].
pub fn request_health_permissions() -> bool {
    call_static_string("requestHealthConnectPermissions").is_some_and(|r| r == "ok")
}

/// Read finished workouts that ended within `since_epoch_seconds..now`,
/// already merged into suggestions. Never panics; errors degrade to an
/// empty list.
pub fn read_health_workouts(since_epoch_seconds: u64) -> Vec<DetectedWorkout> {
    let json = match call_static_string_with_arg(
        "readHealthConnectWorkouts",
        &since_epoch_seconds.to_string(),
    ) {
        Some(j) => j,
        None => return Vec::new(),
    };
    if let Some(err) = json.strip_prefix("error:") {
        log::warn!("Health Connect read failed: {}", err);
        return Vec::new();
    }
    let raw: Vec<RawWorkout> = match serde_json::from_str(&json) {
        Ok(r) => r,
        Err(e) => {
            log::warn!("Health Connect JSON parse failed: {}", e);
            return Vec::new();
        }
    };
    let workouts: Vec<DetectedWorkout> = raw
        .into_iter()
        .filter_map(|r| {
            let exercise = ExerciseType::parse(&r.exercise)?;
            if r.end <= r.start {
                return None;
            }
            Some(DetectedWorkout {
                id: r.id,
                exercise,
                title: r.title.filter(|t| !t.trim().is_empty()),
                start_time_epoch_seconds: r.start,
                duration_seconds: r.end - r.start,
                distance_meters: r.distance,
                // Active calories match what RUNSTR publishes; total
                // includes basal burn and over-reports the workout.
                calories: r
                    .active_calories
                    .or(r.total_calories)
                    .map(|kcal| kcal.round() as u32),
                avg_heart_rate: r.avg_hr.map(|bpm| bpm.round() as u32),
                max_heart_rate: r.max_hr.map(|bpm| bpm.round() as u32),
                steps: r.steps.map(|s| s.round() as u32),
                elevation_gain_meters: r.elevation,
                source: r.source,
                session_count: 1,
            })
        })
        .collect();
    crate::utils::workout_merger::merge_close_workouts_default(workouts)
}

/// Current time in seconds since the epoch (platform timestamp helper).
pub fn now_secs() -> u64 {
    crate::platform::timestamp::now_secs()
}
