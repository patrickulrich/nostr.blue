use crate::stores::music_player::MusicTrack;
use jni::objects::{JClass, JObject, JValue};
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

static JNI_VM: OnceLock<Result<jni::JavaVM, jni::errors::Error>> = OnceLock::new();

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AndroidPlaybackSnapshot {
    #[serde(default)]
    pub queue_len: usize,
    #[serde(default)]
    pub current_index: usize,
    #[serde(default)]
    pub is_playing: bool,
    #[serde(default)]
    pub is_buffering: bool,
    #[serde(default)]
    pub current_time: f64,
    #[serde(default)]
    pub duration: f64,
    #[serde(default)]
    pub playback_error: Option<String>,
}

fn get_jvm() -> Option<&'static jni::JavaVM> {
    JNI_VM
        .get_or_init(|| unsafe {
            let ctx = ndk_context::android_context();
            jni::JavaVM::from_raw(ctx.vm().cast() as *mut _)
        })
        .as_ref()
        .ok()
}

fn find_app_class<'a>(
    env: &mut jni::JNIEnv<'a>,
    context: &JObject<'a>,
    class_name: &str,
) -> Option<JClass<'a>> {
    let context_class = env.get_object_class(context).ok()?;
    let class_loader = env
        .call_method(
            &context_class,
            "getClassLoader",
            "()Ljava/lang/ClassLoader;",
            &[],
        )
        .ok()?
        .l()
        .ok()?;
    let j_name = env.new_string(class_name).ok()?;
    let loaded = env
        .call_method(
            &class_loader,
            "loadClass",
            "(Ljava/lang/String;)Ljava/lang/Class;",
            &[JValue::Object(&j_name)],
        )
        .ok()?
        .l()
        .ok()?;
    Some(loaded.into())
}

fn with_activity_class<F, R>(f: F) -> Result<R, String>
where
    F: FnOnce(&mut jni::JNIEnv<'_>, &JObject<'_>, JClass<'_>) -> Result<R, String>,
{
    let vm = get_jvm().ok_or("Failed to get JavaVM")?;
    let mut env = vm.attach_current_thread().map_err(|e| e.to_string())?;
    let ctx = ndk_context::android_context();
    let context = unsafe { JObject::from_raw(ctx.context().cast()) };
    let class = find_app_class(&mut env, &context, "dev.dioxus.main.MainActivity")
        .ok_or("Failed to find MainActivity class")?;
    f(&mut env, &context, class)
}

fn extract_string(env: &mut jni::JNIEnv<'_>, result: JObject<'_>) -> Result<String, String> {
    let result = env
        .get_string((&result).into())
        .map_err(|e| e.to_string())?;
    Ok(result.to_string_lossy().into_owned())
}

fn expect_ok(result: String) -> Result<(), String> {
    if let Some(error) = result.strip_prefix("error:") {
        Err(error.to_string())
    } else {
        Ok(())
    }
}

fn expect_ok_string(result: String) -> Result<String, String> {
    expect_ok(result.clone())?;
    Ok(result)
}

fn call_context_only(method_name: &str) -> Result<String, String> {
    with_activity_class(|env, context, class| {
        let result = env
            .call_static_method(
                class,
                method_name,
                "(Landroid/content/Context;)Ljava/lang/String;",
                &[JValue::Object(context)],
            )
            .map_err(|e| e.to_string())?
            .l()
            .map_err(|e| e.to_string())?;
        extract_string(env, result)
    })
}

pub fn set_queue(
    queue: &[MusicTrack],
    current_index: usize,
    play_when_ready: bool,
) -> Result<(), String> {
    let queue_json = serde_json::to_string(queue).map_err(|e| e.to_string())?;
    let result = with_activity_class(|env, context, class| {
        let j_queue = env.new_string(queue_json).map_err(|e| e.to_string())?;
        let result = env
            .call_static_method(
                class,
                "setPlaybackQueue",
                "(Landroid/content/Context;Ljava/lang/String;IZ)Ljava/lang/String;",
                &[
                    JValue::Object(context),
                    JValue::Object(&j_queue),
                    JValue::Int(current_index as i32),
                    JValue::Bool(if play_when_ready { 1 } else { 0 }),
                ],
            )
            .map_err(|e| e.to_string())?
            .l()
            .map_err(|e| e.to_string())?;
        extract_string(env, result)
    })?;
    expect_ok(result)
}

pub fn play() -> Result<(), String> {
    expect_ok(call_context_only("playNativeAudio")?)
}

pub fn pause() -> Result<(), String> {
    expect_ok(call_context_only("pauseNativeAudio")?)
}

pub fn next_track() -> Result<(), String> {
    expect_ok(call_context_only("nextNativeTrack")?)
}

pub fn previous_track() -> Result<(), String> {
    expect_ok(call_context_only("previousNativeTrack")?)
}

pub fn stop() -> Result<(), String> {
    expect_ok(call_context_only("stopNativeAudio")?)
}

pub fn clear_queue() -> Result<(), String> {
    expect_ok(call_context_only("clearNativeAudioQueue")?)
}

pub fn set_volume(volume: f64) -> Result<(), String> {
    let result = with_activity_class(|env, context, class| {
        let result = env
            .call_static_method(
                class,
                "setNativeVolume",
                "(Landroid/content/Context;F)Ljava/lang/String;",
                &[JValue::Object(context), JValue::Float(volume as f32)],
            )
            .map_err(|e| e.to_string())?
            .l()
            .map_err(|e| e.to_string())?;
        extract_string(env, result)
    })?;
    expect_ok(result)
}

pub fn set_playback_speed(speed: f64) -> Result<(), String> {
    let result = with_activity_class(|env, context, class| {
        let result = env
            .call_static_method(
                class,
                "setNativePlaybackSpeed",
                "(Landroid/content/Context;F)Ljava/lang/String;",
                &[JValue::Object(context), JValue::Float(speed as f32)],
            )
            .map_err(|e| e.to_string())?
            .l()
            .map_err(|e| e.to_string())?;
        extract_string(env, result)
    })?;
    expect_ok(result)
}

pub fn seek_to(position_seconds: f64) -> Result<(), String> {
    let result = with_activity_class(|env, context, class| {
        let result = env
            .call_static_method(
                class,
                "seekNativeAudio",
                "(Landroid/content/Context;J)Ljava/lang/String;",
                &[
                    JValue::Object(context),
                    JValue::Long((position_seconds.max(0.0) * 1000.0) as i64),
                ],
            )
            .map_err(|e| e.to_string())?
            .l()
            .map_err(|e| e.to_string())?;
        extract_string(env, result)
    })?;
    expect_ok(result)
}

pub fn snapshot() -> Result<AndroidPlaybackSnapshot, String> {
    let json = expect_ok_string(call_context_only("getNativePlaybackSnapshot")?)?;
    serde_json::from_str(&json).map_err(|e| e.to_string())
}

pub fn save_browse_cache(key: &str, json: &str) -> Result<(), String> {
    let result = with_activity_class(|env, context, class| {
        let j_key = env.new_string(key).map_err(|e| e.to_string())?;
        let j_json = env.new_string(json).map_err(|e| e.to_string())?;
        let result = env
            .call_static_method(
                class,
                "saveBrowseCache",
                "(Landroid/content/Context;Ljava/lang/String;Ljava/lang/String;)Ljava/lang/String;",
                &[
                    JValue::Object(context),
                    JValue::Object(&j_key),
                    JValue::Object(&j_json),
                ],
            )
            .map_err(|e| e.to_string())?
            .l()
            .map_err(|e| e.to_string())?;
        extract_string(env, result)
    })?;
    expect_ok(result)
}

pub fn save_browse_position(media_id: &str, position_ms: u64) -> Result<(), String> {
    let result = with_activity_class(|env, context, class| {
        let j_id = env.new_string(media_id).map_err(|e| e.to_string())?;
        let result = env
            .call_static_method(
                class,
                "saveBrowsePosition",
                "(Landroid/content/Context;Ljava/lang/String;J)Ljava/lang/String;",
                &[
                    JValue::Object(context),
                    JValue::Object(&j_id),
                    JValue::Long(position_ms as i64),
                ],
            )
            .map_err(|e| e.to_string())?
            .l()
            .map_err(|e| e.to_string())?;
        extract_string(env, result)
    })?;
    expect_ok(result)
}
