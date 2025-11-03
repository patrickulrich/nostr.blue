use dioxus::prelude::*;
use nostr::{Keys, PublicKey};
use nostr_sdk::ToBech32;
use gloo_storage::{LocalStorage, Storage};
use serde::{Deserialize, Serialize};
#[cfg(target_family = "wasm")]
use std::sync::Arc;
use std::time::Duration;

use crate::stores::signer::{SignerType, set_signer as store_signer};
use crate::stores::nostr_client;

#[cfg(target_family = "wasm")]
use nostr_browser_signer::BrowserSigner;

use nostr_connect::client::NostrConnect;
use nostr_sdk::nips::nip46::NostrConnectURI;

/// Authentication state
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AuthState {
    pub pubkey: Option<String>,
    pub is_authenticated: bool,
    pub login_method: Option<LoginMethod>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum LoginMethod {
    BrowserExtension,  // NIP-07
    PrivateKey,        // nsec stored locally
    ReadOnly,          // npub only
    RemoteSigner,      // NIP-46 (nostr-connect)
}

impl Default for AuthState {
    fn default() -> Self {
        Self {
            pubkey: None,
            is_authenticated: false,
            login_method: None,
        }
    }
}

/// Global authentication state
pub static AUTH_STATE: GlobalSignal<AuthState> = Signal::global(AuthState::default);

/// Global keys (if using private key login)
static KEYS: GlobalSignal<Option<Keys>> = Signal::global(|| None);

const STORAGE_KEY_NSEC: &str = "nostr_nsec";
const STORAGE_KEY_NPUB: &str = "nostr_npub";
const STORAGE_KEY_METHOD: &str = "nostr_login_method";
const STORAGE_KEY_BUNKER_URI: &str = "nostr_bunker_uri";
const STORAGE_KEY_APP_KEYS: &str = "nostr_app_keys";

/// Initialize authentication from stored credentials
/// Note: This only loads the auth state from localStorage.
/// Actual signer restoration should be done via restore_session_async()
pub fn init_auth() {
    log::info!("Initializing authentication...");

    // Try to load stored login method
    if let Ok(method_str) = LocalStorage::get::<String>(STORAGE_KEY_METHOD) {
        match method_str.as_str() {
            "extension" => {
                log::info!("Found extension login method");
                if let Ok(npub) = LocalStorage::get::<String>(STORAGE_KEY_NPUB) {
                    *AUTH_STATE.write() = AuthState {
                        pubkey: Some(npub),
                        is_authenticated: true,
                        login_method: Some(LoginMethod::BrowserExtension),
                    };
                }
            }
            "private_key" => {
                if let Ok(npub) = LocalStorage::get::<String>(STORAGE_KEY_NPUB) {
                    log::info!("Found stored private key session");
                    *AUTH_STATE.write() = AuthState {
                        pubkey: Some(npub),
                        is_authenticated: true,
                        login_method: Some(LoginMethod::PrivateKey),
                    };
                }
            }
            "read_only" => {
                if let Ok(npub) = LocalStorage::get::<String>(STORAGE_KEY_NPUB) {
                    log::info!("Found stored read-only session");
                    *AUTH_STATE.write() = AuthState {
                        pubkey: Some(npub),
                        is_authenticated: false,
                        login_method: Some(LoginMethod::ReadOnly),
                    };
                }
            }
            "remote_signer" => {
                if let Ok(npub) = LocalStorage::get::<String>(STORAGE_KEY_NPUB) {
                    log::info!("Found stored remote signer session");
                    *AUTH_STATE.write() = AuthState {
                        pubkey: Some(npub),
                        is_authenticated: true,
                        login_method: Some(LoginMethod::RemoteSigner),
                    };
                }
            }
            _ => {}
        }
    }
}

/// Restore session asynchronously (call this after app initialization)
pub async fn restore_session_async() {
    log::info!("Restoring session...");

    if let Ok(method_str) = LocalStorage::get::<String>(STORAGE_KEY_METHOD) {
        match method_str.as_str() {
            "extension" => {
                // Try to restore browser extension session
                if let Err(e) = login_with_browser_extension().await {
                    log::error!("Failed to restore browser extension session: {}", e);
                    clear_auth();
                }
            }
            "private_key" => {
                // Re-login with stored nsec
                if let Ok(nsec) = LocalStorage::get::<String>(STORAGE_KEY_NSEC) {
                    if let Err(e) = login_with_nsec(&nsec).await {
                        log::error!("Failed to restore private key session: {}", e);
                        clear_auth();
                    }
                }
            }
            "read_only" => {
                // Re-login with stored npub
                if let Ok(npub) = LocalStorage::get::<String>(STORAGE_KEY_NPUB) {
                    if let Err(e) = login_with_npub(&npub).await {
                        log::error!("Failed to restore read-only session: {}", e);
                        clear_auth();
                    }
                }
            }
            "remote_signer" => {
                // Try to restore remote signer session
                if let (Ok(bunker_uri), Ok(app_keys_str)) = (
                    LocalStorage::get::<String>(STORAGE_KEY_BUNKER_URI),
                    LocalStorage::get::<String>(STORAGE_KEY_APP_KEYS)
                ) {
                    match restore_nostr_connect(&bunker_uri, &app_keys_str).await {
                        Ok(nostr_connect) => {
                            let signer_type = SignerType::NostrConnect(Arc::new(nostr_connect));
                            match store_signer(signer_type.clone()).await {
                                Ok(_) => {
                                    match nostr_client::set_signer(signer_type).await {
                                        Ok(_) => {
                                            // Run post-login initialization
                                            run_post_login_init().await;
                                            log::info!("Successfully restored remote signer session");
                                        }
                                        Err(e) => {
                                            log::error!("Failed to set remote signer on client: {}", e);
                                            clear_auth();
                                        }
                                    }
                                }
                                Err(e) => {
                                    log::error!("Failed to restore remote signer: {}", e);
                                    clear_auth();
                                }
                            }
                        }
                        Err(e) => {
                            log::error!("Failed to reconnect to remote signer: {}", e);
                            clear_auth();
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

/// Login with private key (nsec)
pub async fn login_with_nsec(nsec: &str) -> Result<(), String> {
    log::info!("Logging in with private key...");

    // Parse the private key
    let keys = Keys::parse(nsec).map_err(|e| format!("Invalid private key: {}", e))?;

    let pubkey = keys.public_key().to_string();

    // Store keys
    *KEYS.write() = Some(keys.clone());

    // Create signer and update client
    let signer = SignerType::Keys(keys);
    store_signer(signer.clone()).await?;
    nostr_client::set_signer(signer).await?;

    // Update auth state
    *AUTH_STATE.write() = AuthState {
        pubkey: Some(pubkey.clone()),
        is_authenticated: true,
        login_method: Some(LoginMethod::PrivateKey),
    };

    // Save to localStorage
    LocalStorage::set(STORAGE_KEY_NSEC, nsec).ok();
    LocalStorage::set(STORAGE_KEY_NPUB, &pubkey).ok();
    LocalStorage::set(STORAGE_KEY_METHOD, "private_key").ok();

    log::info!("Successfully logged in with pubkey: {}", pubkey);

    // Run post-login initialization
    run_post_login_init().await;

    Ok(())
}

/// Login with public key only (read-only mode)
pub async fn login_with_npub(npub: &str) -> Result<(), String> {
    log::info!("Logging in with public key (read-only)...");

    // Parse the public key
    let pubkey = PublicKey::parse(npub).map_err(|e| format!("Invalid public key: {}", e))?;

    let pubkey_str = pubkey.to_string();

    // Set client to read-only mode
    nostr_client::set_read_only().await?;

    // Update auth state (no keys, read-only)
    *AUTH_STATE.write() = AuthState {
        pubkey: Some(pubkey_str.clone()),
        is_authenticated: false, // Not authenticated for write operations
        login_method: Some(LoginMethod::ReadOnly),
    };

    // Save to localStorage
    LocalStorage::set(STORAGE_KEY_NPUB, npub).ok();
    LocalStorage::set(STORAGE_KEY_METHOD, "read_only").ok();

    log::info!("Loaded read-only mode with pubkey: {}", pubkey_str);
    Ok(())
}

/// Login with NIP-07 browser extension (official implementation)
pub async fn login_with_browser_extension() -> Result<(), String> {
    #[cfg(target_family = "wasm")]
    {
        log::info!("Attempting browser extension login...");

        // Create browser signer
        let browser_signer = BrowserSigner::new()
            .map_err(|e| format!("Failed to initialize browser signer: {}", e))?;

        // Get public key from extension
        use nostr::signer::NostrSigner;
        let pubkey = browser_signer.get_public_key()
            .await
            .map_err(|e| format!("Failed to get public key from extension: {}", e))?;

        let pubkey_str = pubkey.to_string();

        // Create signer and update client
        let signer = SignerType::BrowserExtension(Arc::new(browser_signer));
        store_signer(signer.clone()).await?;
        nostr_client::set_signer(signer).await?;

        // Update auth state
        *AUTH_STATE.write() = AuthState {
            pubkey: Some(pubkey_str.clone()),
            is_authenticated: true,
            login_method: Some(LoginMethod::BrowserExtension),
        };

        // Save login method to localStorage
        LocalStorage::set(STORAGE_KEY_METHOD, "extension").ok();
        LocalStorage::set(STORAGE_KEY_NPUB, &pubkey_str).ok();

        log::info!("Successfully logged in via browser extension with pubkey: {}", pubkey_str);

        // Run post-login initialization
        run_post_login_init().await;

        Ok(())
    }
    #[cfg(not(target_family = "wasm"))]
    {
        Err("Browser extension login is only available in browser".to_string())
    }
}

/// Deprecated: Use login_with_browser_extension instead
#[deprecated(note = "Use login_with_browser_extension instead")]
#[allow(dead_code)]
pub async fn login_with_nip07() -> Result<(), String> {
    login_with_browser_extension().await
}

/// Check if browser extension (NIP-07) is available
pub fn is_browser_extension_available() -> bool {
    #[cfg(target_family = "wasm")]
    {
        // Try to create a BrowserSigner to check if extension is available
        BrowserSigner::new().is_ok()
    }
    #[cfg(not(target_family = "wasm"))]
    {
        false
    }
}

/// Deprecated: Use is_browser_extension_available instead
#[deprecated(note = "Use is_browser_extension_available instead")]
#[allow(dead_code)]
pub fn is_nip07_available() -> bool {
    is_browser_extension_available()
}

/// Get or create app keys for NIP-46 connection
/// These keys are used by the app to authenticate to the remote signer
fn get_or_create_app_keys() -> Result<Keys, String> {
    // Try to load existing app keys from localStorage
    if let Ok(stored_keys) = LocalStorage::get::<String>(STORAGE_KEY_APP_KEYS) {
        if let Ok(keys) = Keys::parse(&stored_keys) {
            return Ok(keys);
        }
    }

    // Generate new app keys if none exist
    Ok(Keys::generate())
}

/// Restore NostrConnect instance from stored credentials
async fn restore_nostr_connect(bunker_uri: &str, app_keys_str: &str) -> Result<NostrConnect, String> {
    let uri = NostrConnectURI::parse(bunker_uri)
        .map_err(|e| format!("Invalid stored bunker URI: {}", e))?;

    let app_keys = Keys::parse(app_keys_str)
        .map_err(|e| format!("Invalid stored app keys: {}", e))?;

    let timeout = Duration::from_secs(120);
    let nostr_connect = NostrConnect::new(uri, app_keys, timeout, None)
        .map_err(|e| format!("Failed to reconnect: {}", e))?;

    // Verify connection by getting public key
    use nostr::signer::NostrSigner;
    nostr_connect.get_public_key().await
        .map_err(|e| format!("Remote signer not responding: {}", e))?;

    Ok(nostr_connect)
}

/// Run post-login initialization steps (notifications, subscriptions, emoji fetch)
/// This should be called after any successful login or session restoration
async fn run_post_login_init() {
    log::info!("Running post-login initialization...");

    // Load notification checked_at timestamp from localStorage
    crate::stores::notifications::load_checked_at();

    // Fetch and merge notification checked_at from NIP-78 (if sync enabled)
    crate::stores::notifications::fetch_and_merge_from_nip78().await;

    // Start real-time notification subscription
    crate::stores::notifications::start_realtime_subscription().await;

    // Fetch custom emojis
    crate::stores::emoji_store::init_emoji_fetch();
}

/// Login with NIP-46 remote signer (nostr-connect)
pub async fn login_with_nostr_connect(bunker_uri: &str) -> Result<(), String> {
    log::info!("Logging in with remote signer (NIP-46)...");

    // 1. Validate and parse bunker URI
    let uri = NostrConnectURI::parse(bunker_uri)
        .map_err(|e| format!("Invalid bunker URI: {}", e))?;

    // 2. Load or generate app keys
    let app_keys = get_or_create_app_keys()?;

    // 3. Create NostrConnect client with 120s timeout
    let timeout = Duration::from_secs(120);
    let nostr_connect = NostrConnect::new(uri, app_keys.clone(), timeout, None)
        .map_err(|e| format!("Failed to create connection: {}", e))?;

    // 4. Get public key from signer
    use nostr::signer::NostrSigner;
    let public_key = nostr_connect.get_public_key().await
        .map_err(|e| format!("Failed to get public key: {}", e))?;

    let pubkey_str = public_key.to_bech32()
        .map_err(|e| format!("Failed to convert public key: {}", e))?;

    // 5. Store credentials in localStorage
    LocalStorage::set(STORAGE_KEY_BUNKER_URI, bunker_uri)
        .map_err(|e| format!("Failed to store bunker URI: {}", e))?;

    let app_keys_bech32 = app_keys.secret_key().to_bech32()
        .map_err(|e| format!("Failed to convert app keys: {}", e))?;
    LocalStorage::set(STORAGE_KEY_APP_KEYS, &app_keys_bech32)
        .map_err(|e| format!("Failed to store app keys: {}", e))?;

    LocalStorage::set(STORAGE_KEY_METHOD, "remote_signer")
        .map_err(|e| format!("Failed to store login method: {}", e))?;
    LocalStorage::set(STORAGE_KEY_NPUB, &pubkey_str)
        .map_err(|e| format!("Failed to store public key: {}", e))?;

    // 6. Set signer in signer store
    let signer_type = SignerType::NostrConnect(Arc::new(nostr_connect));
    store_signer(signer_type.clone()).await?;
    nostr_client::set_signer(signer_type).await?;

    // 7. Update auth state
    *AUTH_STATE.write() = AuthState {
        pubkey: Some(pubkey_str.clone()),
        is_authenticated: true,
        login_method: Some(LoginMethod::RemoteSigner),
    };

    log::info!("Successfully logged in via remote signer with pubkey: {}", pubkey_str);

    // 8. Run post-login initialization
    run_post_login_init().await;

    Ok(())
}

/// Generate new keypair
pub fn generate_keys() -> Keys {
    let keys = Keys::generate();
    log::info!("Generated new keypair: {}", keys.public_key());
    keys
}

/// Get current keys (if logged in with private key)
pub fn get_keys() -> Option<Keys> {
    KEYS.read().clone()
}

/// Get current public key
pub fn get_pubkey() -> Option<String> {
    AUTH_STATE.read().pubkey.clone()
}

/// Check if user is authenticated (can sign events)
pub fn is_authenticated() -> bool {
    AUTH_STATE.read().is_authenticated
}

/// Get login method
pub fn get_login_method() -> Option<LoginMethod> {
    AUTH_STATE.read().login_method.clone()
}

/// Logout and clear credentials
pub async fn logout() {
    log::info!("Logging out...");

    // Stop real-time notification subscription
    crate::stores::notifications::stop_realtime_subscription().await;

    // Unset signer from client
    let _ = nostr_client::set_read_only().await;

    clear_auth();

    // Clear from localStorage
    LocalStorage::delete(STORAGE_KEY_NSEC);
    LocalStorage::delete(STORAGE_KEY_NPUB);
    LocalStorage::delete(STORAGE_KEY_METHOD);
    LocalStorage::delete(STORAGE_KEY_BUNKER_URI);
    LocalStorage::delete(STORAGE_KEY_APP_KEYS);
}

/// Clear authentication state
fn clear_auth() {
    *AUTH_STATE.write() = AuthState::default();
    *KEYS.write() = None;
}

/// Sign a message with current keys
#[allow(dead_code)]
pub fn sign_message(message: &str) -> Result<String, String> {
    let _keys = get_keys().ok_or("Not logged in with private key")?;

    // In a real implementation, you'd use the keys to sign the message
    // For now, just return a placeholder
    Ok(format!("signed_{}", message))
}

/// Export private key as nsec
pub fn export_nsec() -> Result<String, String> {
    let keys = get_keys().ok_or("Not logged in with private key")?;
    Ok(keys.secret_key().to_bech32().map_err(|e| e.to_string())?)
}

/// Export public key as npub
pub fn export_npub() -> Result<String, String> {
    let pubkey = get_pubkey().ok_or("Not logged in")?;
    Ok(pubkey)
}
