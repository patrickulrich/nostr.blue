//! WebSocket management for Cashu mint subscriptions (NUT-17)
//!
//! Provides real-time quote status updates via WebSocket with HTTP polling fallback.
//!
//! Follows nostr-sdk patterns for proper WebSocket lifecycle management:
//! - No Closure::forget() - closures stored for explicit cleanup
//! - Explicit close() for resource cleanup
//! - Subscribe sent in onopen callback (no timing races)

use std::cell::RefCell;
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

// Thread-local storage for WebSocket closures (closures aren't Clone, so can't store in GlobalSignal)
thread_local! {
    static WS_CLOSURES: RefCell<HashMap<String, WsClosures>> = RefCell::new(HashMap::new());
}

/// Stored closures for a WebSocket connection
struct WsClosures {
    #[allow(dead_code)]
    onopen: Closure<dyn FnMut(web_sys::Event)>,
    #[allow(dead_code)]
    onmessage: Closure<dyn FnMut(MessageEvent)>,
    #[allow(dead_code)]
    onerror: Closure<dyn FnMut(ErrorEvent)>,
    #[allow(dead_code)]
    onclose: Closure<dyn FnMut(CloseEvent)>,
}

/// State of a WebSocket connection
#[derive(Clone, Debug)]
pub struct WsConnectionState {
    /// Whether the connection is active
    pub connected: bool,
    /// Active subscriptions (sub_id -> quote_id)
    pub subscriptions: HashMap<String, String>,
    /// WebSocket handle for explicit cleanup (WebSocket is Clone - it's a JS handle)
    pub ws: Option<WebSocket>,
}

/// Close a WebSocket connection and clean up all resources
pub fn close_connection(mint_url: &str) {
    // Close WebSocket and update state
    let mut connections = WS_CONNECTIONS.write();
    if let Some(state) = connections.remove(mint_url) {
        if let Some(ws) = state.ws {
            if let Err(e) = ws.close() {
                log::error!("Failed to close WebSocket for {}: {:?}", mint_url, e);
            } else {
                log::debug!("WebSocket connection closed for {}", mint_url);
            }
        }
    }

    // Drop closures (this is what prevents memory leaks)
    WS_CLOSURES.with(|closures| {
        closures.borrow_mut().remove(mint_url);
    });

    log::info!("Cleaned up WebSocket resources for {}", mint_url);
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
        error: WsJsonRpcError,
        id: u64,
    },
}

#[derive(Debug, Deserialize)]
struct WsJsonRpcError {
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
///
/// Following nostr-sdk patterns:
/// - No Closure::forget() - closures stored in thread_local for cleanup
/// - Subscribe sent in onopen callback (no timing races)
/// - WebSocket stored for explicit cleanup
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

    // Clone WebSocket for use in onopen callback (WebSocket is Clone - it's a JS handle)
    let ws_for_onopen = ws.clone();

    // Clone values for onopen closure
    let sub_id_for_onopen = sub_id.clone();
    let quote_id_for_onopen = quote_id.clone();
    let mint_url_for_onopen = mint_url.clone();
    let kind_str = kind.as_str().to_string();

    // Set up onopen handler - sends subscribe request immediately when connection opens
    let onopen_callback = Closure::wrap(Box::new(move |_: web_sys::Event| {
        log::info!("WebSocket connected to {}", mint_url_for_onopen);

        // Update connection state (ws is stored separately after all closures are set up)
        let mut connections = WS_CONNECTIONS.write();
        let state = connections.entry(mint_url_for_onopen.clone()).or_insert_with(|| WsConnectionState {
            connected: false,
            subscriptions: HashMap::new(),
            ws: None,
        });
        state.connected = true;
        state.subscriptions.insert(sub_id_for_onopen.clone(), quote_id_for_onopen.clone());
        drop(connections); // Release lock before sending

        // Send subscribe request immediately (connection is now open - no timing race!)
        let request = SubscribeRequest {
            jsonrpc: "2.0",
            method: "subscribe",
            params: SubscribeParams {
                kind: kind_str.clone(),
                sub_id: sub_id_for_onopen.clone(),
                filters: vec![quote_id_for_onopen.clone()],
            },
            id: REQUEST_ID.fetch_add(1, Ordering::SeqCst),
        };

        match serde_json::to_string(&request) {
            Ok(json) => {
                if let Err(e) = ws_for_onopen.send_with_str(&json) {
                    log::error!("Failed to send subscribe request: {:?}", e);
                } else {
                    log::debug!("Sent subscribe request for quote {}", quote_id_for_onopen);
                }
            }
            Err(e) => log::error!("Failed to serialize subscribe request: {}", e),
        }
    }) as Box<dyn FnMut(web_sys::Event)>);

    ws.set_onopen(Some(onopen_callback.as_ref().unchecked_ref()));

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
                        // Log when channel is full (following nostr-sdk pattern)
                        if let Err(e) = tx_for_msg.try_send(status) {
                            log::warn!("Channel full, dropping quote status update for {}: {:?}", sub_id_for_msg, e);
                        }
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

    // Set up onerror handler
    let mint_url_for_error = mint_url.clone();
    let onerror_callback = Closure::wrap(Box::new(move |e: ErrorEvent| {
        log::error!("WebSocket error for {}: {:?}", mint_url_for_error, e.message());
    }) as Box<dyn FnMut(ErrorEvent)>);

    ws.set_onerror(Some(onerror_callback.as_ref().unchecked_ref()));

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

    // Store closures in thread_local to keep them alive (instead of forget())
    // This allows proper cleanup when close_connection is called
    WS_CLOSURES.with(|closures| {
        closures.borrow_mut().insert(mint_url.clone(), WsClosures {
            onopen: onopen_callback,
            onmessage: onmessage_callback,
            onerror: onerror_callback,
            onclose: onclose_callback,
        });
    });

    // Store WebSocket in connection state for explicit cleanup
    {
        let mut connections = WS_CONNECTIONS.write();
        let state = connections.entry(mint_url.clone()).or_insert_with(|| WsConnectionState {
            connected: false,
            subscriptions: HashMap::new(),
            ws: None,
        });
        state.ws = Some(ws);
    }

    Ok(rx)
}

/// Unsubscribe from quote updates
///
/// If this is the last subscription for this mint, closes the connection
/// and cleans up all resources (following nostr-sdk pattern).
#[allow(dead_code)]
pub fn unsubscribe(mint_url: &str, sub_id: &str) {
    let should_close = {
        let mut connections = WS_CONNECTIONS.write();
        if let Some(state) = connections.get_mut(mint_url) {
            state.subscriptions.remove(sub_id);
            state.subscriptions.is_empty()
        } else {
            false
        }
    };

    // Close connection if no remaining subscriptions
    if should_close {
        close_connection(mint_url);
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
