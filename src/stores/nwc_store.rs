use dioxus::prelude::*;
use dioxus::signals::ReadableExt;
use nwc::prelude::*;
use std::str::FromStr;
use std::sync::Arc;

#[cfg(all(feature = "web", feature = "native"))]
compile_error!("Cannot enable both 'web' and 'native' features simultaneously");

#[cfg(not(any(feature = "web", feature = "native")))]
compile_error!("Must enable either 'web' or 'native' feature");

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
/// Save NWC URI to secure storage (file with restricted permissions on native, web storage on web)
#[cfg(feature = "native")]
fn save_nwc_uri_secure(uri: &str) -> std::result::Result<(), String> {
    use std::fs;
    let dir = crate::platform::storage::data_dir();
    fs::create_dir_all(&dir).map_err(|e| format!("Failed to create storage directory: {}", e))?;
    let path = dir.join("nwc_uri.secure");
    fs::write(&path, uri).map_err(|e| format!("Failed to write secure file: {}", e))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Err(e) = fs::set_permissions(&path, fs::Permissions::from_mode(0o600)) {
            log::warn!(target: "nwc_store", "Failed to set permissions on {:?}: {}", path, e);
        }
    }
    Ok(())
}
#[cfg(feature = "native")]
fn load_nwc_uri_secure() -> Option<String> {
    use std::fs;
    let dir = crate::platform::storage::data_dir();
    let path = dir.join("nwc_uri.secure");
    fs::read_to_string(&path).ok()
}
#[cfg(feature = "native")]
fn delete_nwc_uri_secure() {
    use std::fs;
    let dir = crate::platform::storage::data_dir();
    let path = dir.join("nwc_uri.secure");
    let _ = fs::remove_file(path);
}
/// Save NWC URI to storage (web - "secure" is naming convention, maps to same localStorage key)
#[cfg(feature = "web")]
fn save_nwc_uri_secure(uri: &str) -> std::result::Result<(), String> {
    crate::platform::storage::set_string(STORAGE_KEY_NWC_URI, uri)
}
/// Load NWC URI from storage (web - "secure" is naming convention, maps to same localStorage key)
#[cfg(feature = "web")]
fn load_nwc_uri_secure() -> Option<String> {
    crate::platform::storage::get_string(STORAGE_KEY_NWC_URI)
}
/// Delete NWC URI from storage (web - "secure" is naming convention, maps to same localStorage key)
#[cfg(feature = "web")]
fn delete_nwc_uri_secure() {
    let _ = crate::platform::storage::delete(STORAGE_KEY_NWC_URI);
}
/// Load NWC URI from persistent storage (legacy - for backward compatibility)
fn load_nwc_uri() -> Option<String> {
    crate::platform::storage::get_string(STORAGE_KEY_NWC_URI)
}
/// Delete NWC URI from persistent storage
fn delete_nwc_uri() {
    let _ = crate::platform::storage::delete(STORAGE_KEY_NWC_URI);
}
/// Connect to NWC using a connection URI
/// If remember_wallet is true, the URI will be stored securely
pub async fn connect_nwc(uri_string: &str, remember_wallet: bool) -> std::result::Result<(), String> {
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
            if remember_wallet {
                if let Err(e) = save_nwc_uri_secure(uri_string.trim()) {
                    log::warn!("Failed to save NWC URI securely: {}", e);
                }
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
/// If preserve_storage is true, the stored URI is kept for reconnection
pub fn disconnect_nwc(preserve_storage: bool) {
    *NWC_CLIENT.write() = None;
    *NWC_STATUS.write() = ConnectionStatus::Disconnected;
    *NWC_BALANCE.write() = None;
    if !preserve_storage {
        delete_nwc_uri_secure();
        delete_nwc_uri();
    }
    log::info!("Disconnected from NWC wallet");
}
/// Restore NWC connection from persistent storage
pub async fn restore_connection() {
    match load_nwc_uri_secure().or_else(load_nwc_uri) {
        Some(uri) => {
            log::info!("Restoring NWC connection from secure storage");
            if let Err(e) = connect_nwc(&uri, true).await {
                log::warn!("Failed to restore NWC connection: {}", e);
                disconnect_nwc(true);
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
