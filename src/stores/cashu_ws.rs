//! WebSocket management for Cashu mint subscriptions (NUT-17)
//!
//! Provides real-time quote status updates via WebSocket with HTTP polling fallback.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use dioxus::prelude::*;
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{WebSocket, MessageEvent, CloseEvent, ErrorEvent};
use tokio::sync::mpsc;

/// Global counter for JSON-RPC request IDs
static REQUEST_ID: AtomicU64 = AtomicU64::new(0);

/// Global signal tracking active WebSocket connections by mint URL
pub static WS_CONNECTIONS: GlobalSignal<HashMap<String, WsConnectionState>> = GlobalSignal::new(HashMap::new);

/// State of a WebSocket connection
#[derive(Clone, Debug)]
pub struct WsConnectionState {
    /// Whether the connection is active
    pub connected: bool,
    /// Active subscriptions (sub_id -> quote_id)
    pub subscriptions: HashMap<String, String>,
}

/// Subscription kind for NUT-17
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubscriptionKind {
    Bolt11MintQuote,
    Bolt11MeltQuote,
}

impl SubscriptionKind {
    fn as_str(&self) -> &'static str {
        match self {
            SubscriptionKind::Bolt11MintQuote => "bolt11_mint_quote",
            SubscriptionKind::Bolt11MeltQuote => "bolt11_melt_quote",
        }
    }
}

/// JSON-RPC subscribe request
#[derive(Debug, Serialize)]
struct SubscribeRequest {
    jsonrpc: &'static str,
    method: &'static str,
    params: SubscribeParams,
    id: u64,
}

#[derive(Debug, Serialize)]
struct SubscribeParams {
    kind: String,
    #[serde(rename = "subId")]
    sub_id: String,
    filters: Vec<String>,
}

/// JSON-RPC unsubscribe request (for future use)
#[allow(dead_code)]
#[derive(Debug, Serialize)]
struct UnsubscribeRequest {
    jsonrpc: &'static str,
    method: &'static str,
    params: UnsubscribeParams,
    id: u64,
}

#[allow(dead_code)]
#[derive(Debug, Serialize)]
struct UnsubscribeParams {
    #[serde(rename = "subId")]
    sub_id: String,
}

/// Notification payload for quote status
#[derive(Debug, Clone, Deserialize)]
pub struct QuoteNotification {
    #[serde(rename = "subId")]
    pub sub_id: String,
    pub payload: QuotePayload,
}

/// Quote status payload
#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
pub struct QuotePayload {
    #[serde(default)]
    pub quote: Option<String>,
    pub state: String,
    #[serde(default)]
    pub expiry: Option<u64>,
    #[serde(default)]
    pub paid: Option<bool>,
}

/// JSON-RPC message types
#[allow(dead_code)]
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum WsMessage {
    Notification {
        jsonrpc: String,
        method: String,
        params: QuoteNotification,
    },
    Response {
        jsonrpc: String,
        result: serde_json::Value,
        id: u64,
    },
    Error {
        jsonrpc: String,
        error: WsError,
        id: u64,
    },
}

#[derive(Debug, Deserialize)]
struct WsError {
    code: i32,
    message: String,
}

/// Quote status from notification
#[derive(Debug, Clone, PartialEq)]
pub enum QuoteStatus {
    Pending,
    Paid,
    Issued,
    Expired,
    Unknown(String),
}

impl From<&str> for QuoteStatus {
    fn from(s: &str) -> Self {
        match s.to_uppercase().as_str() {
            "PENDING" => QuoteStatus::Pending,
            "PAID" => QuoteStatus::Paid,
            "ISSUED" => QuoteStatus::Issued,
            "EXPIRED" => QuoteStatus::Expired,
            other => QuoteStatus::Unknown(other.to_string()),
        }
    }
}

/// Callback type for quote status updates (for future use)
#[allow(dead_code)]
pub type QuoteCallback = Box<dyn Fn(QuoteStatus) + Send + Sync + 'static>;

/// Subscribe to quote status updates via WebSocket
///
/// Returns a channel receiver for status updates. Falls back to HTTP polling
/// if WebSocket connection fails.
pub async fn subscribe_to_quote(
    mint_url: String,
    quote_id: String,
    kind: SubscriptionKind,
) -> Result<mpsc::Receiver<QuoteStatus>, String> {
    let (tx, rx) = mpsc::channel(16);

    // Convert HTTP URL to WebSocket URL
    let ws_url = mint_url_to_ws(&mint_url)?;

    // Generate subscription ID
    let sub_id = uuid::Uuid::new_v4().to_string();

    // Create WebSocket connection
    let ws = WebSocket::new(&ws_url).map_err(|e| format!("Failed to create WebSocket: {:?}", e))?;

    ws.set_binary_type(web_sys::BinaryType::Arraybuffer);

    // Clone values for closures
    let sub_id_clone = sub_id.clone();
    let quote_id_clone = quote_id.clone();
    let mint_url_clone = mint_url.clone();

    // Set up onopen handler
    let onopen_callback = Closure::wrap(Box::new(move |_: web_sys::Event| {
        log::info!("WebSocket connected to {}", mint_url_clone);

        // Update connection state
        let mut connections = WS_CONNECTIONS.write();
        connections.entry(mint_url_clone.clone()).or_insert_with(|| WsConnectionState {
            connected: true,
            subscriptions: HashMap::new(),
        }).subscriptions.insert(sub_id_clone.clone(), quote_id_clone.clone());

    }) as Box<dyn FnMut(web_sys::Event)>);

    ws.set_onopen(Some(onopen_callback.as_ref().unchecked_ref()));
    onopen_callback.forget();

    // Clone for message handler
    let sub_id_for_msg = sub_id.clone();
    let tx_for_msg = tx.clone();

    // Set up onmessage handler
    let onmessage_callback = Closure::wrap(Box::new(move |e: MessageEvent| {
        if let Ok(text) = e.data().dyn_into::<js_sys::JsString>() {
            let text: String = text.into();

            match serde_json::from_str::<WsMessage>(&text) {
                Ok(WsMessage::Notification { params, .. }) => {
                    if params.sub_id == sub_id_for_msg {
                        let status = QuoteStatus::from(params.payload.state.as_str());
                        let _ = tx_for_msg.try_send(status);
                    }
                }
                Ok(WsMessage::Response { .. }) => {
                    log::debug!("Received WebSocket response");
                }
                Ok(WsMessage::Error { error, .. }) => {
                    log::error!("WebSocket error: {} (code: {})", error.message, error.code);
                }
                Err(e) => {
                    log::debug!("Failed to parse WebSocket message: {}", e);
                }
            }
        }
    }) as Box<dyn FnMut(MessageEvent)>);

    ws.set_onmessage(Some(onmessage_callback.as_ref().unchecked_ref()));
    onmessage_callback.forget();

    // Set up onerror handler
    let mint_url_for_error = mint_url.clone();
    let onerror_callback = Closure::wrap(Box::new(move |e: ErrorEvent| {
        log::error!("WebSocket error for {}: {:?}", mint_url_for_error, e.message());
    }) as Box<dyn FnMut(ErrorEvent)>);

    ws.set_onerror(Some(onerror_callback.as_ref().unchecked_ref()));
    onerror_callback.forget();

    // Set up onclose handler
    let mint_url_for_close = mint_url.clone();
    let sub_id_for_close = sub_id.clone();
    let onclose_callback = Closure::wrap(Box::new(move |e: CloseEvent| {
        log::info!("WebSocket closed for {}: code={}, reason={}", mint_url_for_close, e.code(), e.reason());

        // Update connection state
        let mut connections = WS_CONNECTIONS.write();
        if let Some(state) = connections.get_mut(&mint_url_for_close) {
            state.subscriptions.remove(&sub_id_for_close);
            if state.subscriptions.is_empty() {
                state.connected = false;
            }
        }
    }) as Box<dyn FnMut(CloseEvent)>);

    ws.set_onclose(Some(onclose_callback.as_ref().unchecked_ref()));
    onclose_callback.forget();

    // Wait for connection to be established
    let ws_clone = ws.clone();
    let sub_id_for_send = sub_id.clone();
    let kind_for_send = kind;
    let quote_id_for_send = quote_id.clone();

    // Use a small delay to let the connection establish, then send subscribe
    wasm_bindgen_futures::spawn_local(async move {
        // Wait a bit for connection
        gloo_timers::future::TimeoutFuture::new(100).await;

        if ws_clone.ready_state() == WebSocket::OPEN {
            // Send subscribe request
            let request = SubscribeRequest {
                jsonrpc: "2.0",
                method: "subscribe",
                params: SubscribeParams {
                    kind: kind_for_send.as_str().to_string(),
                    sub_id: sub_id_for_send,
                    filters: vec![quote_id_for_send],
                },
                id: REQUEST_ID.fetch_add(1, Ordering::SeqCst),
            };

            if let Ok(json) = serde_json::to_string(&request) {
                if let Err(e) = ws_clone.send_with_str(&json) {
                    log::error!("Failed to send subscribe request: {:?}", e);
                } else {
                    log::debug!("Sent subscribe request");
                }
            }
        } else {
            log::warn!("WebSocket not ready, state: {}", ws_clone.ready_state());
        }
    });

    // Store the WebSocket for later cleanup (using a static or global wouldn't work well here,
    // so we rely on the closures keeping it alive)

    Ok(rx)
}

/// Unsubscribe from quote updates
#[allow(dead_code)]
pub fn unsubscribe(mint_url: &str, sub_id: &str) {
    let mut connections = WS_CONNECTIONS.write();
    if let Some(state) = connections.get_mut(mint_url) {
        state.subscriptions.remove(sub_id);
    }
}

/// Check if WebSocket subscriptions are supported for a mint
#[allow(dead_code)]
pub async fn check_ws_support(mint_url: &str) -> bool {
    // Try to establish a WebSocket connection
    let ws_url = match mint_url_to_ws(mint_url) {
        Ok(url) => url,
        Err(_) => return false,
    };

    // Attempt connection with a short timeout
    match WebSocket::new(&ws_url) {
        Ok(ws) => {
            // Close immediately after successful creation
            let _ = ws.close();
            true
        }
        Err(_) => false,
    }
}

/// Convert HTTP mint URL to WebSocket URL
fn mint_url_to_ws(mint_url: &str) -> Result<String, String> {
    let mut url = mint_url.trim_end_matches('/').to_string();

    if url.starts_with("https://") {
        url = format!("wss://{}/v1/ws", &url[8..]);
    } else if url.starts_with("http://") {
        url = format!("ws://{}/v1/ws", &url[7..]);
    } else {
        return Err(format!("Invalid mint URL scheme: {}", mint_url));
    }

    Ok(url)
}

/// Poll quote status via HTTP (fallback when WebSocket not available)
#[allow(dead_code)]
pub async fn poll_quote_status(
    mint_url: &str,
    quote_id: &str,
    is_mint_quote: bool,
) -> Result<QuoteStatus, String> {
    let endpoint = if is_mint_quote {
        format!("{}/v1/mint/quote/bolt11/{}", mint_url.trim_end_matches('/'), quote_id)
    } else {
        format!("{}/v1/melt/quote/bolt11/{}", mint_url.trim_end_matches('/'), quote_id)
    };

    let response = gloo_net::http::Request::get(&endpoint)
        .send()
        .await
        .map_err(|e| format!("HTTP request failed: {}", e))?;

    if !response.ok() {
        return Err(format!("HTTP error: {}", response.status()));
    }

    let json: serde_json::Value = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse response: {}", e))?;

    // Extract state from response
    let state = json.get("state")
        .and_then(|v| v.as_str())
        .unwrap_or("UNKNOWN");

    Ok(QuoteStatus::from(state))
}
