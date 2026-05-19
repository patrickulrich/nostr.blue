use jni::objects::JValue;
use jni::sys::jboolean;
use std::sync::atomic::{AtomicBool, Ordering};

static PIP_MODE: AtomicBool = AtomicBool::new(false);
static PIP_MUTE_PENDING: AtomicBool = AtomicBool::new(false);

pub fn is_pip_mode() -> bool {
    PIP_MODE.load(Ordering::SeqCst)
}

pub fn consume_pip_mute_toggle() -> bool {
    PIP_MUTE_PENDING.swap(false, Ordering::SeqCst)
}

fn call_pip_method(method: &str, arg: Option<&str>) -> Result<String, String> {
    let vm = crate::platform::mobile::get_jvm()
        .ok_or("Failed to get JavaVM for PIP")?;
    let mut env = vm.attach_current_thread().map_err(|e| format!("JNI attach: {}", e))?;
    let ctx = ndk_context::android_context();
    let context = unsafe { jni::objects::JObject::from_raw(ctx.context().cast()) };
    let class = crate::platform::mobile::find_app_class(
        &mut env, &context, "dev/dioxus/main/MainActivity",
    ).ok_or("Failed to find MainActivity class")?;
    match arg {
        None => {
            let result = env
                .call_static_method(
                    &class,
                    method,
                    "(Landroid/content/Context;)Ljava/lang/String;",
                    &[JValue::Object(&context)],
                )
                .map_err(|e| format!("call {} failed: {}", method, e))?
                .l()
                .map_err(|e| format!("result: {}", e))?;
            Ok(env.get_string((&result).into())
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default())
        }
        Some(a) => {
            let j_arg = env.new_string(a).map_err(|e| e.to_string())?;
            let result = env
                .call_static_method(
                    &class,
                    method,
                    "(Landroid/content/Context;Ljava/lang/String;)Ljava/lang/String;",
                    &[JValue::Object(&context), JValue::Object(&j_arg)],
                )
                .map_err(|e| format!("call {} failed: {}", method, e))?
                .l()
                .map_err(|e| format!("result: {}", e))?;
            Ok(env.get_string((&result).into())
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default())
        }
    }
}

pub fn set_nest_active(active: bool) -> Result<(), String> {
    expect_ok(call_pip_method("setNestActive", Some(if active { "true" } else { "false" }))?)
}

pub fn enter_pip() -> Result<(), String> {
    expect_ok(call_pip_method("enterPipMode", None)?)
}

pub fn is_pip_supported() -> bool {
    call_pip_method("isInPip", None).is_ok()
}

fn expect_ok(result: String) -> Result<(), String> {
    if let Some(error) = result.strip_prefix("error:") {
        Err(error.to_string())
    } else if result == "no_instance" {
        Err("no_instance".to_string())
    } else {
        Ok(())
    }
}

#[no_mangle]
pub extern "system" fn Java_dev_dioxus_main_MainActivity_notifyPipModeChanged(
    _env: jni::JNIEnv,
    _class: jni::objects::JClass,
    is_in_pip: jboolean,
) {
    PIP_MODE.store(is_in_pip != 0, Ordering::SeqCst);
}

#[no_mangle]
pub extern "system" fn Java_dev_dioxus_main_MainActivity_notifyPipMuteToggled(
    _env: jni::JNIEnv,
    _class: jni::objects::JClass,
) {
    PIP_MUTE_PENDING.store(true, Ordering::SeqCst);
}
