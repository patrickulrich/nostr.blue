use dioxus::prelude::*;
use nostr::{Keys, PublicKey, ToBech32};
use gloo_storage::{LocalStorage, Storage};
use serde::{Deserialize, Serialize};
#[cfg(target_family = "wasm")]
use std::sync::Arc;

use crate::stores::signer::{SignerType, set_signer as store_signer};
use crate::stores::nostr_client;

#[cfg(target_family = "wasm")]
use nostr_browser_signer::BrowserSigner;

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

    // Start real-time notification subscription
    crate::stores::notifications::start_realtime_subscription().await;

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

        // Start real-time notification subscription
        crate::stores::notifications::start_realtime_subscription().await;

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

    clear_auth();

    // Clear from localStorage
    LocalStorage::delete(STORAGE_KEY_NSEC);
    LocalStorage::delete(STORAGE_KEY_NPUB);
    LocalStorage::delete(STORAGE_KEY_METHOD);
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
