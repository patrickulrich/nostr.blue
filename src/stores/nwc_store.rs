use dioxus::prelude::*;
use nwc::prelude::*;
use std::str::FromStr;
use std::sync::Arc;
use indexed_db_futures::prelude::*;
use wasm_bindgen::JsValue;
use std::future::IntoFuture;

const DB_NAME: &str = "nostr_blue_nwc";
const DB_VERSION: u32 = 1;
const STORE_NAME: &str = "nwc_settings";
const KEY_NWC_URI: &str = "nwc_uri";

/// Connection status for NWC
#[derive(Clone, Debug, PartialEq)]
pub enum ConnectionStatus {
    Disconnected,
    Connecting,
    Connected,
    Error(String),
}

/// Global NWC client
pub static NWC_CLIENT: GlobalSignal<Option<Arc<NWC>>> = Signal::global(|| None);

/// Connection status
pub static NWC_STATUS: GlobalSignal<ConnectionStatus> =
    Signal::global(|| ConnectionStatus::Disconnected);

/// Cached wallet balance in millisatoshis
pub static NWC_BALANCE: GlobalSignal<Option<u64>> = Signal::global(|| None);

/// Open or create IndexedDB for NWC settings
async fn open_db() -> Result<IdbDatabase, String> {
    let mut db_req = IdbDatabase::open_u32(DB_NAME, DB_VERSION)
        .map_err(|e| format!("Failed to open IndexedDB: {:?}", e))?;

    db_req.set_on_upgrade_needed(Some(|evt: &IdbVersionChangeEvent| {
        let db = evt.db();
        if !db.object_store_names().any(|n| n == STORE_NAME) {
            db.create_object_store(STORE_NAME)?;
        }
        Ok(())
    }));

    db_req.into_future().await
        .map_err(|e| format!("Failed to open IndexedDB: {:?}", e))
}

/// Save NWC URI to IndexedDB
async fn save_nwc_uri(uri: &str) -> Result<(), String> {
    let db = open_db().await?;
    let tx = db
        .transaction_on_one_with_mode(STORE_NAME, IdbTransactionMode::Readwrite)
        .map_err(|e| format!("Failed to create transaction: {:?}", e))?;
    let store = tx
        .object_store(STORE_NAME)
        .map_err(|e| format!("Failed to get object store: {:?}", e))?;

    let js_key = JsValue::from_str(KEY_NWC_URI);
    let js_value = JsValue::from_str(uri);

    store
        .put_key_val(&js_key, &js_value)
        .map_err(|e| format!("Failed to save NWC URI: {:?}", e))?;

    tx.await.into_result()
        .map_err(|e| format!("Transaction failed: {:?}", e))?;

    Ok(())
}

/// Load NWC URI from IndexedDB
async fn load_nwc_uri() -> Result<Option<String>, String> {
    let db = open_db().await?;
    let tx = db
        .transaction_on_one(STORE_NAME)
        .map_err(|e| format!("Failed to create transaction: {:?}", e))?;
    let store = tx
        .object_store(STORE_NAME)
        .map_err(|e| format!("Failed to get object store: {:?}", e))?;

    let js_key = JsValue::from_str(KEY_NWC_URI);
    let js_value_opt = store
        .get(&js_key)
        .map_err(|e| format!("Failed to get NWC URI: {:?}", e))?
        .await
        .map_err(|e| format!("Failed to get NWC URI: {:?}", e))?;

    // Check if the value exists
    let js_value = match js_value_opt {
        Some(val) => val,
        None => return Ok(None),
    };

    // Check if it's undefined or null
    if js_value.is_undefined() || js_value.is_null() {
        return Ok(None);
    }

    let uri = js_value.as_string()
        .ok_or_else(|| "Invalid URI value in IndexedDB".to_string())?;

    Ok(Some(uri))
}

/// Delete NWC URI from IndexedDB
async fn delete_nwc_uri() -> Result<(), String> {
    let db = open_db().await?;
    let tx = db
        .transaction_on_one_with_mode(STORE_NAME, IdbTransactionMode::Readwrite)
        .map_err(|e| format!("Failed to create transaction: {:?}", e))?;
    let store = tx
        .object_store(STORE_NAME)
        .map_err(|e| format!("Failed to get object store: {:?}", e))?;

    let js_key = JsValue::from_str(KEY_NWC_URI);
    store
        .delete(&js_key)
        .map_err(|e| format!("Failed to delete NWC URI: {:?}", e))?;

    tx.await.into_result()
        .map_err(|e| format!("Transaction failed: {:?}", e))?;

    Ok(())
}

/// Connect to NWC using a connection URI
pub async fn connect_nwc(uri_string: &str) -> Result<(), String> {
    NWC_STATUS.write().clone_from(&ConnectionStatus::Connecting);

    // Parse the NWC URI
    let uri = NostrWalletConnectURI::from_str(uri_string.trim())
        .map_err(|e| {
            let error_msg = format!("Invalid NWC URI: {}", e);
            *NWC_STATUS.write() = ConnectionStatus::Error(error_msg.clone());
            error_msg
        })?;

    // Create NWC client
    let nwc = NWC::new(uri);

    // Test connection by getting wallet info
    match nwc.get_info().await {
        Ok(info) => {
            log::info!("Connected to NWC wallet: {}", info.alias.as_deref().unwrap_or("Unknown"));

            // Save URI to IndexedDB
            if let Err(e) = save_nwc_uri(uri_string.trim()).await {
                log::warn!("Failed to save NWC URI to IndexedDB: {}", e);
            }

            // Update global state
            *NWC_CLIENT.write() = Some(Arc::new(nwc));
            *NWC_STATUS.write() = ConnectionStatus::Connected;

            // Fetch initial balance
            spawn(async {
                let _ = refresh_balance().await;
            });

            Ok(())
        }
        Err(e) => {
            let error_msg = format!("Failed to connect to wallet: {}", e);
            *NWC_STATUS.write() = ConnectionStatus::Error(error_msg.clone());
            Err(error_msg)
        }
    }
}

/// Disconnect from NWC
pub fn disconnect_nwc() {
    // Clear global state
    *NWC_CLIENT.write() = None;
    *NWC_STATUS.write() = ConnectionStatus::Disconnected;
    *NWC_BALANCE.write() = None;

    // Clear IndexedDB (async, fire and forget)
    spawn(async {
        if let Err(e) = delete_nwc_uri().await {
            log::warn!("Failed to delete NWC URI from IndexedDB: {}", e);
        }
    });

    log::info!("Disconnected from NWC wallet");
}

/// Restore NWC connection from IndexedDB
pub async fn restore_connection() {
    // Try to load URI from IndexedDB
    match load_nwc_uri().await {
        Ok(Some(uri)) => {
            log::info!("Restoring NWC connection from IndexedDB");
            if let Err(e) = connect_nwc(&uri).await {
                log::warn!("Failed to restore NWC connection: {}", e);
                // Clear invalid connection
                disconnect_nwc();
            }
        }
        Ok(None) => {
            log::debug!("No NWC connection to restore");
        }
        Err(e) => {
            log::error!("Failed to load NWC URI from IndexedDB: {}", e);
        }
    }
}

/// Get wallet balance in millisatoshis
pub async fn get_balance() -> Result<u64, String> {
    let client = NWC_CLIENT
        .read()
        .clone()
        .ok_or("NWC not connected")?;

    client
        .get_balance()
        .await
        .map_err(|e| format!("Failed to get balance: {}", e))
}

/// Refresh the cached balance
pub async fn refresh_balance() -> Result<(), String> {
    match get_balance().await {
        Ok(balance) => {
            *NWC_BALANCE.write() = Some(balance);
            Ok(())
        }
        Err(e) => {
            log::error!("Failed to refresh balance: {}", e);
            Err(e)
        }
    }
}

/// Pay a lightning invoice
pub async fn pay_invoice(invoice: String) -> Result<PayInvoiceResponse, String> {
    let client = NWC_CLIENT
        .read()
        .clone()
        .ok_or("NWC not connected")?;

    let request = PayInvoiceRequest::new(&invoice);

    match client.pay_invoice(request).await {
        Ok(response) => {
            // Refresh balance after payment
            spawn(async {
                let _ = refresh_balance().await;
            });
            Ok(response)
        }
        Err(e) => {
            let error_msg = format_nwc_error(e);
            Err(error_msg)
        }
    }
}

/// Format NWC errors into user-friendly messages
fn format_nwc_error(error: nwc::Error) -> String {
    // Try to extract NIP47 error if available
    if let nwc::Error::NIP47(nip47_err) = error {
        return format!("{}", nip47_err);
    }

    // Default error formatting
    format!("{}", error)
}

/// Check if NWC is connected
pub fn is_connected() -> bool {
    NWC_CLIENT.read().is_some()
}
