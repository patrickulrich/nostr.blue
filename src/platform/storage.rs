use serde::de::DeserializeOwned;
use serde::Serialize;

#[cfg(all(feature = "web", feature = "native"))]
compile_error!("Cannot enable both 'web' and 'native' features simultaneously");

#[cfg(not(any(feature = "web", feature = "native")))]
compile_error!("Must enable either 'web' or 'native' feature");

#[cfg(feature = "native")]
static STORAGE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Get a value from persistent key-value storage.
pub fn get<T: DeserializeOwned>(key: &str) -> Result<T, String> {
    #[cfg(feature = "web")]
    {
        use gloo_storage::Storage;
        gloo_storage::LocalStorage::get::<T>(key).map_err(|e| e.to_string())
    }
    #[cfg(feature = "native")]
    {
        let store = load_store()?;
        let value = store
            .get(key)
            .ok_or_else(|| format!("key not found: {key}"))?;
        serde_json::from_value(value.clone()).map_err(|e| e.to_string())
    }
}

/// Set a value in persistent key-value storage.
pub fn set<T: Serialize + ?Sized>(key: &str, value: &T) -> Result<(), String> {
    #[cfg(feature = "web")]
    {
        use gloo_storage::Storage;
        gloo_storage::LocalStorage::set(key, value).map_err(|e| e.to_string())
    }
    #[cfg(feature = "native")]
    {
        let _guard = STORAGE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let mut store = load_store()?;
        let json_value = serde_json::to_value(value).map_err(|e| e.to_string())?;
        store.insert(key.to_string(), json_value);
        save_store(&store)
    }
}

/// Delete a key from persistent storage.
#[allow(clippy::result_unit_err)]
pub fn delete(key: &str) -> Result<(), String> {
    #[cfg(feature = "web")]
    {
        use gloo_storage::Storage;
        gloo_storage::LocalStorage::delete(key);
        Ok(())
    }
    #[cfg(feature = "native")]
    {
        let _guard = STORAGE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let mut store = load_store()?;
        store.remove(key);
        save_store(&store)
    }
}

/// Get a raw string value from storage (convenience for String values).
pub fn get_string(key: &str) -> Option<String> {
    get::<String>(key).ok()
}

/// Set a raw string value in storage (convenience for String values).
pub fn set_string(key: &str, value: &str) -> Result<(), String> {
    set(key, value)
}

// --- Native file-based storage backend ---

/// Get the platform-appropriate data directory.
///
/// On Android, uses JNI to call `context.getFilesDir()` since `dirs::data_dir()` returns `None`.
/// On desktop, delegates to `dirs::data_dir()` with a fallback to the current directory.
#[cfg(feature = "native")]
pub fn data_dir() -> std::path::PathBuf {
    #[cfg(all(target_os = "android", feature = "mobile_platform"))]
    {
        get_android_files_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("/data/data/dev.dioxus.main/files"))
    }
    #[cfg(not(target_os = "android"))]
    {
        dirs::data_dir().unwrap_or_else(|| std::path::PathBuf::from("."))
    }
}

#[cfg(all(target_os = "android", feature = "mobile_platform"))]
fn get_android_files_dir() -> Option<std::path::PathBuf> {
    let ctx = ndk_context::android_context();
    let vm = unsafe { jni::JavaVM::from_raw(ctx.vm().cast()) }.ok()?;
    let mut env = vm.attach_current_thread().ok()?;
    let context = unsafe { jni::objects::JObject::from_raw(ctx.context().cast()) };

    // Call context.getFilesDir().getAbsolutePath() directly via JNI.
    // Avoids find_class("dev/dioxus/main/MainActivity") which fails on native
    // threads because they use the system classloader (no access to app classes).
    let files_dir = env
        .call_method(&context, "getFilesDir", "()Ljava/io/File;", &[])
        .ok()?
        .l()
        .ok()?;
    if files_dir.is_null() {
        return None;
    }

    let abs_path = env
        .call_method(&files_dir, "getAbsolutePath", "()Ljava/lang/String;", &[])
        .ok()?
        .l()
        .ok()?;
    if abs_path.is_null() {
        return None;
    }

    let jstr = env.get_string((&abs_path).into()).ok()?;
    Some(std::path::PathBuf::from(
        jstr.to_string_lossy().into_owned(),
    ))
}

#[cfg(feature = "native")]
fn storage_path() -> std::path::PathBuf {
    data_dir().join("nostr-blue").join("settings.json")
}

#[cfg(feature = "native")]
fn load_store() -> Result<std::collections::HashMap<String, serde_json::Value>, String> {
    let path = storage_path();
    match std::fs::read_to_string(&path) {
        Ok(contents) => serde_json::from_str(&contents).map_err(|e| e.to_string()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(std::collections::HashMap::new()),
        Err(e) => Err(e.to_string()),
    }
}

#[cfg(feature = "native")]
fn save_store(store: &std::collections::HashMap<String, serde_json::Value>) -> Result<(), String> {
    let path = storage_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    // Atomic write: write to temp file then rename
    let tmp_path = path.with_extension("json.tmp");
    let contents = serde_json::to_string_pretty(store).map_err(|e| e.to_string())?;
    std::fs::write(&tmp_path, contents).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp_path, &path).map_err(|e| e.to_string())?;
    Ok(())
}
