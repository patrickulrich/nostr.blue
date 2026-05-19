use serde_json::Value;

use super::types::GoogleAuthResult;

fn call_drive(method: &str, arg1: Option<&str>, arg2: Option<&str>) -> Result<String, String> {
    let vm = crate::platform::mobile::get_jvm()
        .ok_or("Failed to get JavaVM for cloud backup")?;

    // Import here to avoid polluting namespace on non-android
    use jni::objects::JValue;

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
    .ok_or("Failed to find MainActivity class for cloud backup")?;

    match (arg1, arg2) {
        (None, None) => {
            let result = env
                .call_static_method(
                    &class,
                    method,
                    "(Landroid/content/Context;)Ljava/lang/String;",
                    &[JValue::Object(&context)],
                )
                .map_err(|e| format!("Call {} failed: {}", method, e))?
                .l()
                .map_err(|e| format!("Cast {} result failed: {}", method, e))?;

            let s = env
                .get_string((&result).into())
                .map_err(|e| format!("Get string failed: {}", e))?;
            Ok(s.to_string_lossy().into_owned())
        }
        (Some(a1), None) => {
            let j_arg1 = env.new_string(a1).map_err(|e| e.to_string())?;
            let result = env
                .call_static_method(
                    &class,
                    method,
                    "(Landroid/content/Context;Ljava/lang/String;)Ljava/lang/String;",
                    &[JValue::Object(&context), JValue::Object(&j_arg1)],
                )
                .map_err(|e| format!("Call {} failed: {}", method, e))?
                .l()
                .map_err(|e| format!("Cast {} result failed: {}", method, e))?;

            let s = env
                .get_string((&result).into())
                .map_err(|e| format!("Get string failed: {}", e))?;
            Ok(s.to_string_lossy().into_owned())
        }
        (Some(a1), Some(a2)) => {
            let j_arg1 = env.new_string(a1).map_err(|e| e.to_string())?;
            let j_arg2 = env.new_string(a2).map_err(|e| e.to_string())?;
            let result = env
                .call_static_method(
                    &class,
                    method,
                    "(Landroid/content/Context;Ljava/lang/String;Ljava/lang/String;)Ljava/lang/String;",
                    &[
                        JValue::Object(&context),
                        JValue::Object(&j_arg1),
                        JValue::Object(&j_arg2),
                    ],
                )
                .map_err(|e| format!("Call {} failed: {}", method, e))?
                .l()
                .map_err(|e| format!("Cast {} result failed: {}", method, e))?;

            let s = env
                .get_string((&result).into())
                .map_err(|e| format!("Get string failed: {}", e))?;
            Ok(s.to_string_lossy().into_owned())
        }
        _ => Err("Invalid argument combination".to_string()),
    }
}

pub fn google_sign_in() -> Result<GoogleAuthResult, String> {
    let result_str = call_drive("signInWithGoogle", None, None)?;
    let v: Value = serde_json::from_str(&result_str)
        .map_err(|e| format!("Failed to parse sign-in result: {}", e))?;

    if let Some(err) = v["error"].as_str() {
        return Err(err.to_string());
    }

    let sub = v["sub"]
        .as_str()
        .ok_or("Missing sub in sign-in result")?
        .to_string();
    let access_token = v["accessToken"]
        .as_str()
        .ok_or("Missing accessToken in sign-in result")?
        .to_string();

    Ok(GoogleAuthResult { sub, access_token })
}

pub fn list_backups(access_token: &str) -> Result<Vec<(String, String)>, String> {
    let result_str = call_drive("listDriveBackups", Some(access_token), None)?;
    let v: Value = serde_json::from_str(&result_str)
        .map_err(|e| format!("Failed to parse list result: {}", e))?;

    if let Some(err) = v["error"].as_str() {
        return Err(err.to_string());
    }

    let files = v
        .as_array()
        .ok_or("List result is not an array")?;

    let mut entries = Vec::new();
    for file in files {
        let file_id = file["fileId"].as_str().unwrap_or("").to_string();
        let name = file["name"].as_str().unwrap_or("").to_string();
        if !file_id.is_empty() {
            entries.push((file_id, name));
        }
    }
    Ok(entries)
}

pub fn upload_backup(
    access_token: &str,
    npub: &str,
    payload_b64: &str,
) -> Result<(), String> {
    let result_str = call_drive(
        "uploadDriveBackup",
        Some(access_token),
        Some(&format!("{}|{}", npub, payload_b64)),
    )?;
    let v: Value = serde_json::from_str(&result_str)
        .map_err(|e| format!("Failed to parse upload result: {}", e))?;
    if let Some(err) = v["error"].as_str() {
        return Err(err.to_string());
    }
    Ok(())
}

pub fn download_backup(access_token: &str, file_id: &str) -> Result<String, String> {
    let result_str = call_drive("downloadDriveBackup", Some(access_token), Some(file_id))?;
    let v: Value = serde_json::from_str(&result_str)
        .map_err(|e| format!("Failed to parse download result: {}", e))?;
    if let Some(err) = v["error"].as_str() {
        return Err(err.to_string());
    }
    v["payload"]
        .as_str()
        .ok_or("Missing payload in download result".to_string())
        .map(|s| s.to_string())
}

pub fn delete_backup(access_token: &str, file_id: &str) -> Result<(), String> {
    let result_str = call_drive("deleteDriveBackup", Some(access_token), Some(file_id))?;
    let v: Value = serde_json::from_str(&result_str)
        .map_err(|e| format!("Failed to parse delete result: {}", e))?;
    if let Some(err) = v["error"].as_str() {
        return Err(err.to_string());
    }
    Ok(())
}
