use jni::objects::JValue;

fn call_nest_method(method: &str, arg: Option<&str>) -> Result<String, String> {
    let vm = crate::platform::mobile::get_jvm()
        .ok_or("Failed to get JavaVM for nest notification")?;

    let mut env = vm
        .attach_current_thread()
        .map_err(|e| format!("JNI attach failed: {}", e))?;

    let ctx = ndk_context::android_context();
    let context = unsafe { jni::objects::JObject::from_raw(ctx.context().cast()) };

    let class = crate::platform::mobile::find_app_class(
        &mut env,
        &context,
        "dev/dioxus/main/MainActivity",
    )
    .ok_or("Failed to find MainActivity class for nest notification")?;

    let context_obj = unsafe {
        jni::objects::JObject::from_raw(ndk_context::android_context().context().cast())
    };

    match arg {
        None => {
            let result = env
                .call_static_method(
                    &class,
                    method,
                    "(Landroid/content/Context;)Ljava/lang/String;",
                    &[JValue::Object(&context_obj)],
                )
                .map_err(|e| format!("Call {} failed: {}", method, e))?
                .l()
                .map_err(|e| format!("Cast {} result failed: {}", method, e))?;

            let s = env
                .get_string((&result).into())
                .map_err(|e| format!("Get string failed: {}", e))?;
            Ok(s.to_string_lossy().into_owned())
        }
        Some(a) => {
            let j_arg = env.new_string(a).map_err(|e| e.to_string())?;
            let result = env
                .call_static_method(
                    &class,
                    method,
                    "(Landroid/content/Context;Ljava/lang/String;)Ljava/lang/String;",
                    &[JValue::Object(&context_obj), JValue::Object(&j_arg)],
                )
                .map_err(|e| format!("Call {} failed: {}", method, e))?
                .l()
                .map_err(|e| format!("Cast {} result failed: {}", method, e))?;

            let s = env
                .get_string((&result).into())
                .map_err(|e| format!("Get string failed: {}", e))?;
            Ok(s.to_string_lossy().into_owned())
        }
    }
}

fn expect_ok(result: String) -> Result<(), String> {
    if let Some(error) = result.strip_prefix("error:") {
        Err(error.to_string())
    } else {
        Ok(())
    }
}

pub fn start_nest_notification(title: &str) -> Result<(), String> {
    expect_ok(call_nest_method("startNestNotification", Some(title))?)
}

pub fn stop_nest_notification() -> Result<(), String> {
    expect_ok(call_nest_method("stopNestNotification", None)?)
}
