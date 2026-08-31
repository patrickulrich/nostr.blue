//! Mobile-specific platform utilities (Android).

use base64::Engine;
use std::sync::OnceLock;

static JNI_VM: OnceLock<Result<jni::JavaVM, jni::errors::Error>> = OnceLock::new();

pub fn get_jvm() -> Option<&'static jni::JavaVM> {
    JNI_VM
        .get_or_init(|| unsafe {
            let ctx = ndk_context::android_context();
            jni::JavaVM::from_raw(ctx.vm().cast() as *mut _)
        })
        .as_ref()
        .ok()
}

pub fn find_app_class<'a>(
    env: &mut jni::JNIEnv<'a>,
    context: &jni::objects::JObject<'a>,
    class_name: &str,
) -> Option<jni::objects::JClass<'a>> {
    use jni::objects::JValue;

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

pub fn download_file(filename: &str, content: &[u8], mime_type: &str) -> Result<(), String> {
    use jni::objects::JValue;

    let vm = get_jvm().ok_or("Failed to get JavaVM")?;
    let mut env = vm.attach_current_thread().map_err(|e| e.to_string())?;

    // SAFETY: The pointer returned by ndk_context::android_context().context() is valid
    // for the entire lifetime of the Android application. This JObject is used immediately
    // to call Java methods and is not stored beyond this function's scope, so there are
    // no thread-safety or ownership issues.
    let ctx = ndk_context::android_context();
    let context = unsafe { jni::objects::JObject::from_raw(ctx.context().cast()) };

    let class = find_app_class(&mut env, &context, "dev.dioxus.main.MainActivity")
        .ok_or("Failed to find MainActivity class")?;

    let content_base64 = base64::engine::general_purpose::STANDARD.encode(content);

    let j_filename = env.new_string(filename).map_err(|e| e.to_string())?;
    let j_content_base64 = env.new_string(&content_base64).map_err(|e| e.to_string())?;
    let j_mime_type = env.new_string(mime_type).map_err(|e| e.to_string())?;

    let result = env
        .call_static_method(
            class,
            "downloadFile",
            "(Landroid/content/Context;Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;)Ljava/lang/String;",
            &[
                JValue::Object(&context),
                JValue::Object(&j_filename),
                JValue::Object(&j_content_base64),
                JValue::Object(&j_mime_type),
            ],
        )
        .map_err(|e| e.to_string())?
        .l()
        .map_err(|e| e.to_string())?;

    let result_str = env
        .get_string((&result).into())
        .map_err(|e| e.to_string())?;
    let result_str = result_str.to_string_lossy();

    if result_str.starts_with("error:") {
        return Err(result_str.into_owned());
    }

    Ok(())
}

fn call_static_string_method(
    env: &mut jni::JNIEnv,
    class: &jni::objects::JClass,
    method_name: &str,
) -> Result<String, String> {
    use jni::objects::JValue;

    let context =
        unsafe { jni::objects::JObject::from_raw(ndk_context::android_context().context().cast()) };

    let result = env
        .call_static_method(
            class,
            method_name,
            "(Landroid/content/Context;)Ljava/lang/String;",
            &[JValue::Object(&context)],
        )
        .map_err(|e| e.to_string())?
        .l()
        .map_err(|e| e.to_string())?;

    let result_str = env
        .get_string((&result).into())
        .map_err(|e| e.to_string())?;
    Ok(result_str.to_string_lossy().into_owned())
}

fn call_static_string_method_with_string_arg(
    env: &mut jni::JNIEnv,
    class: &jni::objects::JClass,
    method_name: &str,
    arg: &str,
) -> Result<String, String> {
    use jni::objects::JValue;

    let context =
        unsafe { jni::objects::JObject::from_raw(ndk_context::android_context().context().cast()) };
    let j_arg = env.new_string(arg).map_err(|e| e.to_string())?;

    let result = env
        .call_static_method(
            class,
            method_name,
            "(Landroid/content/Context;Ljava/lang/String;)Ljava/lang/String;",
            &[JValue::Object(&context), JValue::Object(&j_arg)],
        )
        .map_err(|e| e.to_string())?
        .l()
        .map_err(|e| e.to_string())?;

    let result_str = env
        .get_string((&result).into())
        .map_err(|e| e.to_string())?;
    Ok(result_str.to_string_lossy().into_owned())
}

fn call_static_void_method(
    env: &mut jni::JNIEnv,
    class: &jni::objects::JClass,
    method_name: &str,
) -> Result<(), String> {
    use jni::objects::JValue;

    let context =
        unsafe { jni::objects::JObject::from_raw(ndk_context::android_context().context().cast()) };

    env.call_static_method(
        class,
        method_name,
        "(Landroid/content/Context;)V",
        &[JValue::Object(&context)],
    )
    .map_err(|e| e.to_string())?;

    Ok(())
}

/// Whether the current Android network is metered (e.g. cellular).
/// Used by the downloads engine to honor the "Wi-Fi only" setting.
pub fn is_network_metered() -> bool {
    use jni::objects::JValue;

    let Some(vm) = get_jvm() else {
        return false;
    };
    let Ok(mut env) = vm.attach_current_thread() else {
        return false;
    };
    let ctx = ndk_context::android_context();
    // SAFETY: the context pointer from ndk-context is valid for the
    // process lifetime and only used for the duration of this call.
    let context = unsafe { jni::objects::JObject::from_raw(ctx.context().cast()) };
    let service_name = match env.new_string("connectivity") {
        Ok(name) => name,
        Err(_) => return false,
    };
    let connectivity = match env.call_method(
        &context,
        "getSystemService",
        "(Ljava/lang/String;)Ljava/lang/Object;",
        &[JValue::Object(&service_name)],
    ) {
        Ok(service) => match service.l() {
            Ok(obj) if !obj.is_null() => obj,
            _ => return false,
        },
        Err(_) => return false,
    };
    matches!(
        env.call_method(&connectivity, "isActiveNetworkMetered", "()Z", &[]),
        Ok(jni::objects::JValueGen::Bool(metered)) if metered != 0
    )
}

pub fn finish_app() -> Result<(), String> {
    let vm = get_jvm().ok_or("Failed to get JavaVM")?;
    let mut env = vm.attach_current_thread().map_err(|e| e.to_string())?;

    let ctx = ndk_context::android_context();
    let context = unsafe { jni::objects::JObject::from_raw(ctx.context().cast()) };

    let class = find_app_class(&mut env, &context, "dev.dioxus.main.MainActivity")
        .ok_or("Failed to find MainActivity class")?;

    call_static_void_method(&mut env, &class, "finishApp")
}

pub fn open_uri(uri: &str) -> Result<(), String> {
    let vm = get_jvm().ok_or("Failed to get JavaVM")?;
    let mut env = vm.attach_current_thread().map_err(|e| e.to_string())?;

    let ctx = ndk_context::android_context();
    let context = unsafe { jni::objects::JObject::from_raw(ctx.context().cast()) };

    let class = find_app_class(&mut env, &context, "dev.dioxus.main.MainActivity")
        .ok_or("Failed to find MainActivity class")?;

    let result =
        call_static_string_method_with_string_arg(&mut env, &class, "openUri", uri)?;

    if result.starts_with("error:") {
        return Err(result);
    }

    if result != "launched" {
        return Err(format!("Unexpected payment open result: {result}"));
    }

    Ok(())
}

pub fn open_lightning_uri(uri: &str) -> Result<(), String> {
    let vm = get_jvm().ok_or("Failed to get JavaVM")?;
    let mut env = vm.attach_current_thread().map_err(|e| e.to_string())?;

    let ctx = ndk_context::android_context();
    let context = unsafe { jni::objects::JObject::from_raw(ctx.context().cast()) };

    let class = find_app_class(&mut env, &context, "dev.dioxus.main.MainActivity")
        .ok_or("Failed to find MainActivity class")?;

    let result =
        call_static_string_method_with_string_arg(&mut env, &class, "openLightningUri", uri)?;

    if result.starts_with("error:") {
        return Err(result);
    }

    if result != "launched" {
        return Err(format!("Unexpected lightning open result: {result}"));
    }

    Ok(())
}

pub fn copy_to_clipboard_native(text: &str) -> Result<(), String> {
    let vm = get_jvm().ok_or("Failed to get JavaVM")?;
    let mut env = vm.attach_current_thread().map_err(|e| e.to_string())?;

    let ctx = ndk_context::android_context();
    let context = unsafe { jni::objects::JObject::from_raw(ctx.context().cast()) };

    let class = find_app_class(&mut env, &context, "dev.dioxus.main.MainActivity")
        .ok_or("Failed to find MainActivity class")?;

    let result =
        call_static_string_method_with_string_arg(&mut env, &class, "copyToClipboard", text)?;

    if result == "success" {
        Ok(())
    } else if result.starts_with("error:") {
        Err(result)
    } else {
        Err(format!("Unexpected clipboard result: {result}"))
    }
}

pub fn read_text_from_clipboard_native() -> Result<String, String> {
    let vm = get_jvm().ok_or("Failed to get JavaVM")?;
    let mut env = vm.attach_current_thread().map_err(|e| e.to_string())?;

    let ctx = ndk_context::android_context();
    let context = unsafe { jni::objects::JObject::from_raw(ctx.context().cast()) };

    let class = find_app_class(&mut env, &context, "dev.dioxus.main.MainActivity")
        .ok_or("Failed to find MainActivity class")?;

    let result = call_static_string_method(&mut env, &class, "readFromClipboard")?;

    if result == "empty" {
        Err("No text in clipboard".to_string())
    } else if result.starts_with("error:") {
        Err(result)
    } else {
        Ok(result)
    }
}

pub async fn pick_file() -> Result<(Vec<u8>, String), String> {
    pick_from_android("pickFile", "pollFileResult").await
}

pub async fn pick_image() -> Result<(Vec<u8>, String), String> {
    pick_from_android("pickImage", "pollFileResult").await
}

async fn pick_from_android(
    pick_method: &str,
    poll_method: &str,
) -> Result<(Vec<u8>, String), String> {
    let vm = get_jvm().ok_or("Failed to get JavaVM")?;
    let mut env = vm.attach_current_thread().map_err(|e| e.to_string())?;

    let ctx = ndk_context::android_context();
    let context = unsafe { jni::objects::JObject::from_raw(ctx.context().cast()) };

    let class = find_app_class(&mut env, &context, "dev.dioxus.main.MainActivity")
        .ok_or("Failed to find MainActivity class")?;

    // Launch the picker
    let result = call_static_string_method(&mut env, &class, pick_method)?;

    if result.starts_with("error:") {
        return Err(result);
    }

    if result != "picking" {
        return Err(format!("Unexpected pick result: {}", result));
    }

    // Poll for result with timeout
    let max_attempts = 300; // 30 seconds at 100ms intervals
    for _ in 0..max_attempts {
        crate::platform::timer::sleep_ms(100).await;

        let poll_result = call_static_string_method(&mut env, &class, poll_method)?;

        if poll_result == "none" {
            continue;
        }
        if poll_result == "picking" {
            continue;
        }
        if poll_result.starts_with("error:") {
            return Err(poll_result);
        }

        // Parse "mime|base64"
        if let Some((mime_type, base64_content)) = poll_result.split_once('|') {
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(base64_content)
                .map_err(|e| format!("Failed to decode base64: {}", e))?;
            return Ok((bytes, mime_type.to_string()));
        }

        return Err(format!("Unexpected poll result: {}", poll_result));
    }

    Err("File pick timed out".to_string())
}
