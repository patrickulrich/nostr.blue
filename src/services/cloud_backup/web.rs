use serde_json::Value;
use wasm_bindgen::JsValue;

use super::types::GoogleAuthResult;

fn js_error(prefix: &str, js_val: &JsValue) -> String {
    format!("{}: {:?}", prefix, js_val)
}

pub async fn google_sign_in() -> Result<GoogleAuthResult, String> {
    let result_js = nostr_drive_sign_in().await;

    let result_str = result_js
        .as_string()
        .ok_or_else(|| js_error("Google sign-in returned non-string", &result_js))?;

    let v: Value = serde_json::from_str(&result_str)
        .map_err(|e| format!("Failed to parse sign-in result: {}", e))?;

    if let Some(err) = v.get("error").and_then(|e| e.as_str()) {
        return Err(format!("Google sign-in error: {}", err));
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

pub async fn list_backups(access_token: &str) -> Result<Vec<(String, String)>, String> {
    let result_js = nostr_drive_list(access_token).await;

    let result_str = result_js
        .as_string()
        .ok_or_else(|| js_error("List backups returned non-string", &result_js))?;

    let arr: Value = serde_json::from_str(&result_str)
        .map_err(|e| format!("Failed to parse list result: {}", e))?;

    if let Some(err) = arr.get("error").and_then(|e| e.as_str()) {
        return Err(format!("List backups error: {}", err));
    }

    let files = arr.as_array().ok_or("List result is not an array")?;

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

pub async fn upload_backup(
    access_token: &str,
    npub: &str,
    payload_b64: &str,
) -> Result<(), String> {
    let result_js = nostr_drive_upload(access_token, npub, payload_b64).await;
    if result_js.is_null() || result_js.is_undefined() {
        return Ok(());
    }
    if let Some(s) = result_js.as_string() {
        if let Ok(v) = serde_json::from_str::<Value>(&s) {
            if let Some(err) = v.get("error").and_then(|e| e.as_str()) {
                return Err(format!("Upload error: {}", err));
            }
        }
    }
    Ok(())
}

pub async fn download_backup(access_token: &str, file_id: &str) -> Result<String, String> {
    let result_js = nostr_drive_download(access_token, file_id).await;

    let result_str = result_js
        .as_string()
        .ok_or_else(|| js_error("Download returned non-string", &result_js))?;

    if let Ok(v) = serde_json::from_str::<Value>(&result_str) {
        if let Some(err) = v.get("error").and_then(|e| e.as_str()) {
            return Err(format!("Download error: {}", err));
        }
    }

    Ok(result_str)
}

#[allow(dead_code)]
pub async fn delete_backup(access_token: &str, file_id: &str) -> Result<(), String> {
    let result_js = nostr_drive_delete(access_token, file_id).await;
    if result_js.is_null() || result_js.is_undefined() {
        return Ok(());
    }
    if let Some(s) = result_js.as_string() {
        if let Ok(v) = serde_json::from_str::<Value>(&s) {
            if let Some(err) = v.get("error").and_then(|e| e.as_str()) {
                return Err(format!("Delete error: {}", err));
            }
        }
    }
    Ok(())
}

#[wasm_bindgen::prelude::wasm_bindgen(inline_js = r#"
export async function nostr_drive_sign_in() {
    try {
        return JSON.stringify(await window.nostrBlueDrive.signIn());
    } catch (e) {
        return JSON.stringify({ error: e.message || String(e) });
    }
}
export async function nostr_drive_list(accessToken) {
    try {
        return JSON.stringify(await window.nostrBlueDrive.list(accessToken));
    } catch (e) {
        return JSON.stringify({ error: e.message || String(e) });
    }
}
export async function nostr_drive_upload(accessToken, npub, payload) {
    try {
        await window.nostrBlueDrive.upload(accessToken, npub, payload);
        return null;
    } catch (e) {
        return JSON.stringify({ error: e.message || String(e) });
    }
}
export async function nostr_drive_download(accessToken, fileId) {
    try {
        return await window.nostrBlueDrive.download(accessToken, fileId);
    } catch (e) {
        return JSON.stringify({ error: e.message || String(e) });
    }
}
export async function nostr_drive_delete(accessToken, fileId) {
    try {
        await window.nostrBlueDrive.delete(accessToken, fileId);
        return null;
    } catch (e) {
        return JSON.stringify({ error: e.message || String(e) });
    }
}
"#)]
extern "C" {
    pub async fn nostr_drive_sign_in() -> JsValue;
    pub async fn nostr_drive_list(access_token: &str) -> JsValue;
    pub async fn nostr_drive_upload(access_token: &str, npub: &str, payload: &str) -> JsValue;
    pub async fn nostr_drive_download(access_token: &str, file_id: &str) -> JsValue;
    pub async fn nostr_drive_delete(access_token: &str, file_id: &str) -> JsValue;
}
