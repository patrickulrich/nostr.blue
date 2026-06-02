#![allow(dead_code)]
use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

#[cfg(feature = "desktop")]
mod native;

#[cfg(all(target_os = "android", feature = "mobile_platform"))]
mod android;

#[derive(Clone, Debug, PartialEq)]
pub enum ConnectionState {
    Disconnected,
    Connecting,
    Authenticating,
    Connected,
    Publishing,
    Error(String),
}

#[derive(Clone, Debug)]
pub enum AudioCommand {
    Connect {
        auth_url: String,
        relay_url: String,
        namespace: String,
    },
    StartPublishing,
    StopPublishing,
    Mute,
    Unmute,
    SubscribeToParticipant {
        pubkey: String,
    },
    UnsubscribeFromParticipant {
        pubkey: String,
    },
    Disconnect,
}

#[derive(Clone, Debug)]
pub enum AudioEvent {
    ConnectionStateChanged(ConnectionState),
    ParticipantTracksChanged(Vec<String>),
    AudioLevel {
        pubkey: String,
        level: f32,
    },
    Error(String),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct AuthResponse {
    #[serde(alias = "token")]
    jwt: String,
}

#[cfg(any(feature = "web", feature = "mobile_platform"))]
async fn eval_nest_js(expr: &str, error_context: &str) -> Result<(), String> {
    let result = document::eval(expr)
        .await
        .map_err(|e| format!("JS eval error: {}", e))?;
    let obj = result.as_object().ok_or("Invalid JS return")?;
    if obj
        .get("type")
        .and_then(|v| v.as_str())
        .map(|s| s == "success")
        .unwrap_or(false)
    {
        Ok(())
    } else {
        Err(obj
            .get("error")
            .and_then(|v| v.as_str())
            .unwrap_or(error_context)
            .to_string())
    }
}

#[cfg(any(feature = "web", feature = "mobile_platform"))]
pub async fn js_init(publisher_id: &str) -> Result<(), String> {
    let pid = serde_json::to_string(publisher_id).map_err(|e| e.to_string())?;
    eval_nest_js(
        &format!("return window.nestAudioManager.init({pid});"),
        "Unknown init error",
    )
    .await
}

#[cfg(any(feature = "web", feature = "mobile_platform"))]
pub async fn js_connect(
    publisher_id: &str,
    auth_url: &str,
    relay_url: &str,
    namespace: &str,
    jwt: &str,
    my_pubkey: &str,
) -> Result<(), String> {
    let pid = serde_json::to_string(publisher_id).map_err(|e| e.to_string())?;
    let aurl = serde_json::to_string(auth_url).map_err(|e| e.to_string())?;
    let rurl = serde_json::to_string(relay_url).map_err(|e| e.to_string())?;
    let ns = serde_json::to_string(namespace).map_err(|e| e.to_string())?;
    let j = serde_json::to_string(jwt).map_err(|e| e.to_string())?;
    let mpk = serde_json::to_string(my_pubkey).map_err(|e| e.to_string())?;
    eval_nest_js(
        &format!("return window.nestAudioManager.connect({pid}, {aurl}, {rurl}, {ns}, {j}, {mpk});"),
        "Unknown connect error",
    )
    .await
}

#[cfg(any(feature = "web", feature = "mobile_platform"))]
pub async fn js_publish_audio(publisher_id: &str) -> Result<(), String> {
    let pid = serde_json::to_string(publisher_id).map_err(|e| e.to_string())?;
    eval_nest_js(
        &format!("return window.nestAudioManager.publishAudio({pid});"),
        "Unknown publish error",
    )
    .await
}

#[cfg(any(feature = "web", feature = "mobile_platform"))]
pub async fn js_subscribe_audio(publisher_id: &str, participant_pubkey: &str) -> Result<(), String> {
    let pid = serde_json::to_string(publisher_id).map_err(|e| e.to_string())?;
    let ppk = serde_json::to_string(participant_pubkey).map_err(|e| e.to_string())?;
    eval_nest_js(
        &format!("return window.nestAudioManager.subscribeAudio({pid}, {ppk});"),
        "Unknown subscribe error",
    )
    .await
}

#[cfg(any(feature = "web", feature = "mobile_platform"))]
pub async fn js_unsubscribe_audio(publisher_id: &str, participant_pubkey: &str) -> Result<(), String> {
    let pid = serde_json::to_string(publisher_id).map_err(|e| e.to_string())?;
    let ppk = serde_json::to_string(participant_pubkey).map_err(|e| e.to_string())?;
    eval_nest_js(
        &format!("return window.nestAudioManager.unsubscribeAudio({pid}, {ppk});"),
        "Unknown unsubscribe error",
    )
    .await
}

#[cfg(any(feature = "web", feature = "mobile_platform"))]
pub async fn js_mute(publisher_id: &str) -> Result<(), String> {
    let pid = serde_json::to_string(publisher_id).map_err(|e| e.to_string())?;
    eval_nest_js(
        &format!("return window.nestAudioManager.mute({pid});"),
        "Unknown mute error",
    )
    .await
}

#[cfg(any(feature = "web", feature = "mobile_platform"))]
pub async fn js_unmute(publisher_id: &str) -> Result<(), String> {
    let pid = serde_json::to_string(publisher_id).map_err(|e| e.to_string())?;
    eval_nest_js(
        &format!("return window.nestAudioManager.unmute({pid});"),
        "Unknown unmute error",
    )
    .await
}

#[cfg(any(feature = "web", feature = "mobile_platform"))]
pub async fn js_disconnect(publisher_id: &str) -> Result<(), String> {
    let pid = serde_json::to_string(publisher_id).map_err(|e| e.to_string())?;
    eval_nest_js(
        &format!("return window.nestAudioManager.disconnect({pid});"),
        "Unknown disconnect error",
    )
    .await
}

#[cfg(any(feature = "web", feature = "mobile_platform"))]
pub async fn js_get_connection_state(publisher_id: &str) -> ConnectionState {
    let pid = match serde_json::to_string(publisher_id) {
        Ok(p) => p,
        Err(e) => return ConnectionState::Error(e.to_string()),
    };
    let result = document::eval(&format!(
        "return window.nestAudioManager.getConnectionState({pid});"
    ))
    .await;
    match result {
        Ok(val) => match val.as_str() {
            Some("connecting") => ConnectionState::Connecting,
            Some("authenticating") => ConnectionState::Authenticating,
            Some("connected") => ConnectionState::Connected,
            Some("publishing") => ConnectionState::Publishing,
            Some(s) if s.starts_with("error") => {
                ConnectionState::Error(s.to_string())
            }
            _ => ConnectionState::Disconnected,
        },
        Err(e) => ConnectionState::Error(e.to_string()),
    }
}

#[cfg(any(feature = "web", feature = "mobile_platform"))]
pub async fn js_get_participant_tracks(publisher_id: &str) -> Vec<String> {
    let pid = match serde_json::to_string(publisher_id) {
        Ok(p) => p,
        Err(e) => {
            log::warn!("Failed to serialize publisher_id: {}", e);
            return Vec::new();
        }
    };
    let result = document::eval(&format!(
        "return JSON.stringify(window.nestAudioManager.getParticipantTracks({pid}));"
    ))
    .await;
    match result {
        Ok(val) => val
            .as_str()
            .and_then(|s| serde_json::from_str::<Vec<String>>(s).ok())
            .unwrap_or_default(),
        Err(e) => {
            log::warn!("Failed to get participant tracks: {}", e);
            Vec::new()
        }
    }
}

pub async fn authenticate_with_nest(
    auth_url: &str,
    namespace: &str,
    publish: bool,
) -> Result<String, String> {
    use crate::platform::http::http_client;
    use crate::utils::nips::nip98::{create_auth_header_with_payload, AuthResult};
    use bitcoin_hashes::Hash as _;
    use nostr_sdk::nips::nip98;

    let full_url = format!("{}/auth", auth_url.trim_end_matches('/'));

    let body = serde_json::json!({
        "namespace": namespace,
        "publish": publish
    }).to_string();
    let body_bytes = body.as_bytes();
    let payload_hash = bitcoin_hashes::sha256::Hash::hash(body_bytes);

    let auth: AuthResult =
        create_auth_header_with_payload(&full_url, nip98::HttpMethod::POST, payload_hash).await?;

    let response = http_client()
        .map_err(|e| format!("HTTP client init failed: {}", e))?
        .post(&auth.signed_url)
        .header("Authorization", &auth.header)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json")
        .body(body)
        .send()
        .await
        .map_err(|e| format!("Auth request failed: {}", e))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!("Auth failed {}: {}", status, body));
    }

    let auth_resp: AuthResponse = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse auth response: {}", e))?;

    Ok(auth_resp.jwt)
}

#[cfg(feature = "desktop")]
mod desktop_bridges {
    use super::native::NativeBridge;
    use once_cell::sync::Lazy;
    use std::collections::HashMap;
    use std::sync::Mutex;

    static BRIDGES: Lazy<Mutex<HashMap<String, NativeBridge>>> =
        Lazy::new(|| Mutex::new(HashMap::new()));

    pub fn get(publisher_id: &str) -> NativeBridge {
        let mut map = BRIDGES.lock().unwrap_or_else(|e| e.into_inner());
        map.entry(publisher_id.to_string())
            .or_insert_with(NativeBridge::new)
            .clone()
    }

    pub fn remove(publisher_id: &str) {
        let mut map = BRIDGES.lock().unwrap_or_else(|e| e.into_inner());
        map.remove(publisher_id);
    }
}

#[cfg(feature = "desktop")]
fn get_bridge(publisher_id: &str) -> native::NativeBridge {
    desktop_bridges::get(publisher_id)
}

#[cfg(feature = "desktop")]
fn remove_bridge(publisher_id: &str) {
    desktop_bridges::remove(publisher_id)
}

#[cfg(feature = "desktop")]
pub async fn js_init(_publisher_id: &str) -> Result<(), String> {
    Ok(())
}

#[cfg(feature = "desktop")]
pub async fn js_connect(
    publisher_id: &str,
    _auth_url: &str,
    relay_url: &str,
    namespace: &str,
    jwt: &str,
    _my_pubkey: &str,
) -> Result<(), String> {
    let bridge = get_bridge(publisher_id);
    bridge.connect(relay_url, namespace, jwt).await
}

#[cfg(feature = "desktop")]
pub async fn js_publish_audio(publisher_id: &str) -> Result<(), String> {
    let bridge = get_bridge(publisher_id);
    bridge.start_publishing().await
}

#[cfg(feature = "desktop")]
pub async fn js_subscribe_audio(
    publisher_id: &str,
    participant_pubkey: &str,
) -> Result<(), String> {
    let bridge = get_bridge(publisher_id);
    bridge.subscribe(participant_pubkey).await
}

#[cfg(feature = "desktop")]
pub async fn js_unsubscribe_audio(
    publisher_id: &str,
    participant_pubkey: &str,
) -> Result<(), String> {
    let bridge = get_bridge(publisher_id);
    bridge.unsubscribe(participant_pubkey).await
}

#[cfg(feature = "desktop")]
pub async fn js_mute(publisher_id: &str) -> Result<(), String> {
    let bridge = get_bridge(publisher_id);
    bridge.set_muted(true).await
}

#[cfg(feature = "desktop")]
pub async fn js_unmute(publisher_id: &str) -> Result<(), String> {
    let bridge = get_bridge(publisher_id);
    bridge.set_muted(false).await
}

#[cfg(feature = "desktop")]
pub async fn js_disconnect(publisher_id: &str) -> Result<(), String> {
    let bridge = get_bridge(publisher_id);
    let result = bridge.disconnect().await;
    remove_bridge(publisher_id);
    result
}

#[cfg(feature = "desktop")]
pub async fn js_get_connection_state(publisher_id: &str) -> ConnectionState {
    let bridge = get_bridge(publisher_id);
    bridge.connection_state()
}

#[cfg(feature = "desktop")]
pub async fn js_get_participant_tracks(publisher_id: &str) -> Vec<String> {
    let bridge = get_bridge(publisher_id);
    bridge.participant_tracks()
}
