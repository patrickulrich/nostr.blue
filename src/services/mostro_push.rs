//! Mostro push notification registration and peer wake.
//!
//! Phase 10: client-side push notification integration with the Mostro
//! push server. The push server sends Web Push (web) or FCM (mobile)
//! notifications to wake the user's device when trade events arrive
//! while the app is closed.
//!
//! Flow:
//! 1. On login, the client obtains a platform-specific push token
//!    (Web Push subscription on web, FCM token on Android).
//! 2. The client POSTs `{trade_pubkey, token, platform, mostro_pubkey}`
//!    to the daemon's push server `/api/register` endpoint.
//! 3. When a P2P chat message is sent, the client POSTs the peer's
//!    trade pubkey to `/api/notify` so the peer's device wakes.
//!
//! Privacy note (v1): tokens are sent as plaintext over HTTPS. Phase 5
//! of the reference (mostro/mobile) will add ECDH-encrypted tokens.
//!
//! DEFERRED: Android/FCM push is not yet implemented — see
//! `docs/MOSTRO_MOBILE_PUSH.md` for status, impact, and alternatives.

use crate::platform::storage;
use crate::stores::mostro::node_config;

const PUSH_TOKEN_KEY: &str = "mostro_push_token";
const PUSH_REGISTERED_TRADE_KEYS: &str = "mostro_push_registered_keys";

/// Get the push server URL from the current daemon's info event.
/// Falls back to a default if the daemon doesn't advertise one.
fn push_server_url() -> Option<String> {
    // Check the parsed MostroNodeInfo first (live from kind 38385).
    if let Some(info) = node_config::MOSTRO_NODE_INFO().as_ref() {
        if let Some(ref url) = info.push_server_url {
            return Some(url.clone());
        }
    }
    // Default push server (if the daemon doesn't advertise one).
    // This is best-effort; if the server doesn't exist, registration
    // silently fails and the client falls back to Phase 9 local
    // notifications.
    Some("https://push.mostro.network".to_string())
}

/// Phase 10.3: obtain a platform-specific push token.
///
/// - **Web**: subscribes via the Web Push API
///   (`navigator.serviceWorker.pushManager.subscribe`). Returns the
///   endpoint URL as the token.
/// - **Desktop**: returns `None` (push not supported).
/// - **Mobile (Android)**: returns the FCM token (stub — requires
///   Firebase integration in the Android shell).
pub async fn acquire_push_token() -> Option<String> {
    // Check if we already have a cached token.
    if let Ok(Some(token)) = storage::get::<Option<String>>(PUSH_TOKEN_KEY) {
        if !token.is_empty() {
            return Some(token);
        }
    }

    #[cfg(feature = "web")]
    {
        acquire_web_push_token().await
    }
    #[cfg(not(feature = "web"))]
    {
        // DEFERRED: Android FCM integration is not yet implemented.
        // See `docs/MOSTRO_MOBILE_PUSH.md` for status, impact, and the
        // alternatives considered (FCM, UnifiedPush, ntfy, aggressive
        // polling). The 60s visibility-backfill poll in
        // `mostro_toast_drainer.rs` is the current fallback — it
        // covers the case where the user returns to the app within a
        // minute of an event arriving.
        None
    }
}

#[cfg(feature = "web")]
async fn acquire_web_push_token() -> Option<String> {
    // Web Push requires:
    // 1. A service worker registration
    // 2. A VAPID public key (fetched from the push server's /api/vapid)
    // 3. pushManager.subscribe({ userVisibleOnly: true, applicationServerKey })
    //
    // This is a best-effort implementation — if the VAPID key fetch fails
    // or the SW isn't registered, returns None and falls back to Phase 9
    // local notifications.

    let server_url = push_server_url()?;

    // Fetch VAPID public key from the push server.
    let vapid_key = match fetch_vapid_key(&server_url).await {
        Some(k) => k,
        None => {
            log::debug!("Push server VAPID key fetch failed; skipping push");
            return None;
        }
    };

    // Subscribe via the Web Push API.
    // We use JS interop to call pushManager.subscribe since web_sys
    // doesn't enable Push API features by default.
    subscribe_web_push(&vapid_key).await
}

#[cfg(feature = "web")]
async fn fetch_vapid_key(server_url: &str) -> Option<String> {
    let client = crate::platform::http::http_client().ok()?;
    let url = format!("{server_url}/api/vapid");
    let resp = client.get(&url).send().await.ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let body: serde_json::Value = resp.json().await.ok()?;
    body.get("vapid_public_key")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

#[cfg(feature = "web")]
async fn subscribe_web_push(vapid_key: &str) -> Option<String> {
    use wasm_bindgen::JsCast;
    use wasm_bindgen_futures::JsFuture;

    let window = web_sys::window()?;

    // Access navigator.serviceWorker via JS interop (avoids needing
    // web_sys's ServiceWorkerContainer feature).
    let navigator = js_sys::Reflect::get(&window, &"navigator".into()).ok()?;
    let sw_container =
        js_sys::Reflect::get(&navigator, &"serviceWorker".into()).ok()?;
    if sw_container.is_undefined() {
        return None;
    }

    // Call serviceWorker.ready (returns a Promise<ServiceWorkerRegistration>).
    let ready_promise =
        js_sys::Reflect::get(&sw_container, &"ready".into()).ok()?;
    let ready_promise = ready_promise.dyn_into::<js_sys::Promise>().ok()?;
    let sw_reg = JsFuture::from(ready_promise).await.ok()?;

    // Access pushManager from the registration.
    let push_manager =
        js_sys::Reflect::get(&sw_reg, &"pushManager".into()).ok()?;

    // Build subscribe options.
    let opts = js_sys::Object::new();
    let _ = js_sys::Reflect::set(&opts, &"userVisibleOnly".into(), &true.into());
    let key_bytes = base64url_to_uint8array(vapid_key)?;
    let _ = js_sys::Reflect::set(&opts, &"applicationServerKey".into(), &key_bytes);

    // Call pushManager.subscribe(opts).
    let subscribe_fn =
        js_sys::Reflect::get(&push_manager, &"subscribe".into()).ok()?;
    let subscribe_fn = subscribe_fn.dyn_into::<js_sys::Function>().ok()?;
    let subscribe_result =
        subscribe_fn.call1(&push_manager, &opts).ok()?;
    let subscribe_promise = subscribe_result.dyn_into::<js_sys::Promise>().ok()?;
    let subscription = JsFuture::from(subscribe_promise).await.ok()?;

    // Extract the endpoint URL from the subscription.
    let endpoint =
        js_sys::Reflect::get(&subscription, &"endpoint".into()).ok()?;
    let endpoint_str = endpoint.as_string()?;

    // Cache the token.
    let _ = storage::set(PUSH_TOKEN_KEY, &Some(endpoint_str.clone()));

    Some(endpoint_str)
}

#[cfg(feature = "web")]
fn base64url_to_uint8array(b64url: &str) -> Option<wasm_bindgen::JsValue> {
    use base64::Engine;
    // Decode base64url to bytes, then create a Uint8Array.
    let padded = match b64url.len() % 4 {
        2 => format!("{b64url}=="),
        3 => format!("{b64url}="),
        _ => b64url.to_string(),
    };
    let b64_std = padded.replace('-', "+").replace('_', "/");
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(&b64_std)
        .ok()?;
    let arr = js_sys::Uint8Array::new_with_length(bytes.len() as u32);
    arr.copy_from(&bytes);
    Some(arr.into())
}

/// Phase 10.2: register a trade pubkey's push token with the push server.
///
/// POSTs `{trade_pubkey, token, platform, mostro_pubkey}` to
/// `/api/register`. Best-effort: silently fails if the server is
/// unreachable.
pub async fn register_token(trade_pubkey_hex: &str, mostro_pubkey_hex: &str) {
    let token = match acquire_push_token().await {
        Some(t) => t,
        None => return,
    };
    let server_url = match push_server_url() {
        Some(u) => u,
        None => return,
    };

    let platform = platform_name();
    let url = format!("{server_url}/api/register");
    let body = serde_json::json!({
        "trade_pubkey": trade_pubkey_hex,
        "token": token,
        "platform": platform,
        "mostro_pubkey": mostro_pubkey_hex,
    });

    let client = match crate::platform::http::http_client() {
        Ok(c) => c,
        Err(_) => return,
    };
    match client.post(&url).json(&body).send().await {
        Ok(resp) => {
            if resp.status().is_success() {
                log::debug!("Push token registered for {trade_pubkey_hex}");
                mark_registered(trade_pubkey_hex);
            } else {
                log::debug!(
                    "Push registration HTTP {} for {trade_pubkey_hex}",
                    resp.status()
                );
            }
        }
        Err(e) => {
            log::debug!("Push registration failed (non-fatal): {e}");
        }
    }
}

/// Phase 10.2: unregister a trade pubkey's push token.
#[allow(dead_code)]
pub async fn unregister_token(trade_pubkey_hex: &str) {
    let token = match storage::get::<Option<String>>(PUSH_TOKEN_KEY).ok().flatten() {
        Some(t) => t,
        None => return,
    };
    let server_url = match push_server_url() {
        Some(u) => u,
        None => return,
    };

    let url = format!("{server_url}/api/unregister");
    let body = serde_json::json!({
        "trade_pubkey": trade_pubkey_hex,
        "token": token,
    });

    let client = match crate::platform::http::http_client() {
        Ok(c) => c,
        Err(_) => return,
    };
    let _ = client.post(&url).json(&body).send().await;
    unmark_registered(trade_pubkey_hex);
}

/// Phase 10.4: notify the peer's device to wake up after sending a P2P
/// chat message. The push server looks up the peer's trade pubkey in its
/// registered-tokens database and sends a push notification to wake
/// their device.
pub async fn notify_peer(peer_pubkey_hex: &str) {
    let server_url = match push_server_url() {
        Some(u) => u,
        None => return,
    };

    let url = format!("{server_url}/api/notify");
    let body = serde_json::json!({
        "peer_pubkey": peer_pubkey_hex,
    });

    let client = match crate::platform::http::http_client() {
        Ok(c) => c,
        Err(_) => return,
    };
    let _ = client.post(&url).json(&body).send().await;
}

/// Register push tokens for all active trades' pubkeys. Called once
/// after login.
pub async fn register_all_active_trades() {
    let trades = crate::stores::mostro::trade_store::active_trades();
    let mostro_pk = match crate::stores::mostro::try_get_node_config() {
        Some(c) => c.pubkey,
        None => return,
    };

    for trade in &trades {
        if let Some(ref tpk) = trade.my_trade_pubkey {
            if !is_registered(tpk) {
                register_token(tpk, &mostro_pk).await;
            }
        }
    }
}

fn platform_name() -> &'static str {
    #[cfg(feature = "web")]
    {
        "web"
    }
    #[cfg(all(feature = "native", not(feature = "mobile_platform")))]
    {
        "desktop"
    }
    #[cfg(feature = "mobile_platform")]
    {
        "android"
    }
}

fn is_registered(trade_pubkey_hex: &str) -> bool {
    storage::get::<Vec<String>>(PUSH_REGISTERED_TRADE_KEYS)
        .unwrap_or_default()
        .iter()
        .any(|k| k == trade_pubkey_hex)
}

fn mark_registered(trade_pubkey_hex: &str) {
    let mut keys = storage::get::<Vec<String>>(PUSH_REGISTERED_TRADE_KEYS).unwrap_or_default();
    if !keys.contains(&trade_pubkey_hex.to_string()) {
        keys.push(trade_pubkey_hex.to_string());
        let _ = storage::set(PUSH_REGISTERED_TRADE_KEYS, &keys);
    }
}

#[allow(dead_code)]
fn unmark_registered(trade_pubkey_hex: &str) {
    let mut keys = storage::get::<Vec<String>>(PUSH_REGISTERED_TRADE_KEYS).unwrap_or_default();
    keys.retain(|k| k != trade_pubkey_hex);
    let _ = storage::set(PUSH_REGISTERED_TRADE_KEYS, &keys);
}

/// Clear all push registration state (e.g., on logout).
#[allow(dead_code)]
pub fn clear_push_state() {
    let _ = storage::delete(PUSH_TOKEN_KEY);
    let _ = storage::delete(PUSH_REGISTERED_TRADE_KEYS);
}
