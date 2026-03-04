use dioxus::prelude::*;
use dioxus::signals::ReadableExt;
use nwc::prelude::*;
use std::str::FromStr;
use std::sync::Arc;
const STORAGE_KEY_NWC_URI: &str = "nwc_uri";
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
pub static NWC_STATUS: GlobalSignal<ConnectionStatus> = Signal::global(|| {
    ConnectionStatus::Disconnected
});
/// Cached wallet balance in millisatoshis
pub static NWC_BALANCE: GlobalSignal<Option<u64>> = Signal::global(|| None);
/// Save NWC URI to persistent storage
fn save_nwc_uri(uri: &str) -> std::result::Result<(), String> {
    crate::platform::storage::set_string(STORAGE_KEY_NWC_URI, uri)
}
/// Load NWC URI from persistent storage
fn load_nwc_uri() -> Option<String> {
    crate::platform::storage::get_string(STORAGE_KEY_NWC_URI)
}
/// Delete NWC URI from persistent storage
fn delete_nwc_uri() {
    crate::platform::storage::delete(STORAGE_KEY_NWC_URI);
}
/// Connect to NWC using a connection URI
pub async fn connect_nwc(uri_string: &str) -> std::result::Result<(), String> {
    NWC_STATUS.write().clone_from(&ConnectionStatus::Connecting);
    let uri = NostrWalletConnectURI::from_str(uri_string.trim())
        .map_err(|e| {
            let error_msg = format!("Invalid NWC URI: {}", e);
            *NWC_STATUS.write() = ConnectionStatus::Error(error_msg.clone());
            error_msg
        })?;
    let nwc = NWC::new(uri);
    match nwc.get_info().await {
        Ok(info) => {
            log::info!(
                "Connected to NWC wallet: {}", info.alias.as_deref().unwrap_or("Unknown")
            );
            if let Err(e) = save_nwc_uri(uri_string.trim()) {
                log::warn!("Failed to save NWC URI: {}", e);
            }
            *NWC_CLIENT.write() = Some(Arc::new(nwc));
            *NWC_STATUS.write() = ConnectionStatus::Connected;
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
    *NWC_CLIENT.write() = None;
    *NWC_STATUS.write() = ConnectionStatus::Disconnected;
    *NWC_BALANCE.write() = None;
    delete_nwc_uri();
    log::info!("Disconnected from NWC wallet");
}
/// Restore NWC connection from persistent storage
pub async fn restore_connection() {
    match load_nwc_uri() {
        Some(uri) => {
            log::info!("Restoring NWC connection from storage");
            if let Err(e) = connect_nwc(&uri).await {
                log::warn!("Failed to restore NWC connection: {}", e);
                disconnect_nwc();
            }
        }
        None => {
            log::debug!("No NWC connection to restore");
        }
    }
}
/// Get wallet balance in millisatoshis
pub async fn get_balance() -> std::result::Result<u64, String> {
    let client = NWC_CLIENT.read().clone().ok_or("NWC not connected")?;
    client.get_balance().await.map_err(|e| format!("Failed to get balance: {}", e))
}
/// Refresh the cached balance
pub async fn refresh_balance() -> std::result::Result<(), String> {
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
pub async fn pay_invoice(
    invoice: String,
) -> std::result::Result<PayInvoiceResponse, String> {
    let client = NWC_CLIENT.read().clone().ok_or("NWC not connected")?;
    let request = PayInvoiceRequest::new(&invoice);
    match client.pay_invoice(request).await {
        Ok(response) => {
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
    if let nwc::Error::NIP47(nip47_err) = error {
        return format!("{}", nip47_err);
    }
    format!("{}", error)
}
/// Check if NWC is connected
pub fn is_connected() -> bool {
    NWC_CLIENT.read().is_some()
}
