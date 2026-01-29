use dioxus::prelude::*;
use dioxus::signals::ReadableExt;
use nostr::{Keys, PublicKey};
use nostr_sdk::ToBech32;
use gloo_storage::{LocalStorage, Storage};
use serde::{Deserialize, Serialize};
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
#[derive(Default)]
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


/// Global authentication state
pub static AUTH_STATE: GlobalSignal<AuthState> = Signal::global(AuthState::default);

/// Global keys (if using private key login)
static KEYS: GlobalSignal<Option<Keys>> = Signal::global(|| None);

const STORAGE_KEY_NSEC: &str = "nostr_nsec";  // Legacy plaintext (for migration)
const STORAGE_KEY_NCRYPTSEC: &str = "nostr_ncryptsec";  // NIP-49 encrypted key
const STORAGE_KEY_NPUB: &str = "nostr_npub";
const STORAGE_KEY_METHOD: &str = "nostr_login_method";
const STORAGE_KEY_BUNKER_URI: &str = "nostr_bunker_uri";
const STORAGE_KEY_APP_KEYS: &str = "nostr_app_keys";

/// State for password prompt modal (NIP-49 decryption)
#[derive(Clone, Debug, Default)]
pub struct PasswordPromptState {
    /// Whether the password modal should be shown
    pub required: bool,
    /// The encrypted key to decrypt (None = migration from plaintext nsec)
    pub ncryptsec: Option<String>,
    /// Error message from last attempt
    pub error: Option<String>,
    /// Whether decryption is in progress
    pub loading: bool,
}

/// Global password prompt state
pub static PASSWORD_PROMPT: GlobalSignal<PasswordPromptState> = Signal::global(PasswordPromptState::default);

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
                // Check for NIP-49 encrypted key first (new format)
                if let Ok(ncryptsec) = LocalStorage::get::<String>(STORAGE_KEY_NCRYPTSEC) {
                    if crate::utils::nip49::is_ncryptsec(&ncryptsec) {
                        // Signal that password is required - UI will show modal
                        *PASSWORD_PROMPT.write() = PasswordPromptState {
                            required: true,
                            ncryptsec: Some(ncryptsec),
                            error: None,
                            loading: false,
                        };
                        log::info!("Password required to restore encrypted session");
                    } else {
                        // Corrupted storage - clear and require re-login
                        log::error!("Invalid ncryptsec format in storage");
                        clear_auth();
                        LocalStorage::delete(STORAGE_KEY_NCRYPTSEC);
                        LocalStorage::delete(STORAGE_KEY_METHOD);
                    }
                }
                // Check for legacy plaintext nsec (migration path)
                else if let Ok(_nsec) = LocalStorage::get::<String>(STORAGE_KEY_NSEC) {
                    // Signal migration needed - UI will prompt for password to encrypt
                    *PASSWORD_PROMPT.write() = PasswordPromptState {
                        required: true,
                        ncryptsec: None,  // None indicates migration mode
                        error: Some("Your key needs to be encrypted for security. Please set a password.".to_string()),
                        loading: false,
                    };
                    log::info!("Legacy nsec found, migration to encrypted format needed");
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
                } else {
                    // Missing or invalid bunker URI or app keys
                    log::error!("Cannot restore remote signer session: incomplete saved credentials (missing bunker URI or app keys)");
                    clear_auth();
                    // Clean up inconsistent localStorage entries
                    LocalStorage::delete(STORAGE_KEY_BUNKER_URI);
                    LocalStorage::delete(STORAGE_KEY_APP_KEYS);
                    LocalStorage::delete(STORAGE_KEY_METHOD);
                    LocalStorage::delete(STORAGE_KEY_NPUB);
                }
            }
            _ => {}
        }
    }
}

/// Login with private key (nsec) and encrypt with password (NIP-49)
///
/// The nsec will be encrypted with the provided password before storage.
/// On subsequent logins, the password will be required to decrypt.
pub async fn login_with_nsec(nsec: &str, password: &str) -> Result<(), String> {
    log::info!("Logging in with private key...");

    // Validate password
    if let Some(err) = crate::utils::nip49::validate_password(password) {
        return Err(err);
    }

    // Parse the private key
    let keys = Keys::parse(nsec).map_err(|e| format!("Invalid private key: {}", e))?;

    let pubkey = keys.public_key().to_string();

    // Encrypt the secret key with password (NIP-49)
    let ncryptsec = crate::utils::nip49::encrypt_secret_key(keys.secret_key(), password)
        .map_err(|e| format!("Encryption failed: {}", e))?;

    // Store keys in memory
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

    // Save ENCRYPTED key to localStorage (not plaintext!)
    LocalStorage::set(STORAGE_KEY_NCRYPTSEC, &ncryptsec).ok();
    LocalStorage::set(STORAGE_KEY_NPUB, &pubkey).ok();
    LocalStorage::set(STORAGE_KEY_METHOD, "private_key").ok();

    // Remove any legacy plaintext nsec
    LocalStorage::delete(STORAGE_KEY_NSEC);

    log::info!("Successfully logged in with encrypted key, pubkey: {}", pubkey);

    // Run post-login initialization
    run_post_login_init().await;

    Ok(())
}

/// Login with private key without password (internal use only for session restore)
async fn login_with_keys_internal(keys: Keys) -> Result<(), String> {
    let pubkey = keys.public_key().to_string();

    // Store keys in memory
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

    log::info!("Session restored with pubkey: {}", pubkey);

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

    // Fetch user's Blossom servers from kind 10063 (NIP-B7)
    if let Err(e) = crate::stores::blossom_store::fetch_user_servers().await {
        log::warn!("Failed to fetch Blossom servers: {}", e);
    }

    // Start real-time notification subscription
    crate::stores::notifications::start_realtime_subscription().await;

    // Start NIP-65 relay list subscription for cache invalidation
    crate::stores::relay::start_relay_list_subscription().await;

    // Fetch custom emojis
    crate::stores::emoji_store::init_emoji_fetch();

    // Batch prefetch metadata for all contacts (runs in background)
    // This populates IndexedDB so avatars are ready when feed loads
    spawn(async move {
        if let Some(client) = crate::stores::nostr_client::get_client() {
            log::info!("Prefetching metadata for all contacts...");
            // Ensure relays are ready before making network calls
            crate::stores::nostr_client::ensure_relays_ready(&client).await;
            match client.get_contact_list_metadata(std::time::Duration::from_secs(15)).await {
                Ok(contacts) => {
                    log::info!("Prefetched metadata for {} contacts", contacts.len());
                }
                Err(e) => {
                    log::warn!("Failed to prefetch contact metadata: {}", e);
                }
            }
        } else {
            log::debug!("Skipping contact metadata prefetch: no client available");
        }
    });
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

    // Stop NIP-65 relay list subscription
    crate::stores::relay::stop_relay_list_subscription().await;

    // Clear Cashu wallet state
    crate::stores::cashu_cdk_bridge::clear_multi_wallet();

    // Clear shop store caches and reset orders loaded flag
    crate::stores::shop_store::clear_caches();
    crate::stores::shop_store::reset_orders_loaded_flag();

    // Invalidate search relay cache (keyed by pubkey)
    spawn(async move {
        crate::services::search_relays::invalidate_search_relay_cache().await;
    });

    // Unset signer from client
    let _ = nostr_client::set_read_only().await;

    clear_auth();

    // Clear from localStorage (both old and new formats)
    LocalStorage::delete(STORAGE_KEY_NSEC);
    LocalStorage::delete(STORAGE_KEY_NCRYPTSEC);
    LocalStorage::delete(STORAGE_KEY_NPUB);
    LocalStorage::delete(STORAGE_KEY_METHOD);
    LocalStorage::delete(STORAGE_KEY_BUNKER_URI);
    LocalStorage::delete(STORAGE_KEY_APP_KEYS);

    // Clear password prompt state
    *PASSWORD_PROMPT.write() = PasswordPromptState::default();
}

/// Clear authentication state
fn clear_auth() {
    *AUTH_STATE.write() = AuthState::default();
    *KEYS.write() = None;
}

/// Restore session by decrypting stored ncryptsec with password
///
/// This function handles both:
/// - Decryption of existing ncryptsec
/// - Migration of legacy plaintext nsec to encrypted format
pub async fn restore_with_password(password: &str) -> Result<(), String> {
    let prompt_state = PASSWORD_PROMPT.read().clone();

    // Set loading state
    PASSWORD_PROMPT.write().loading = true;
    PASSWORD_PROMPT.write().error = None;

    // Check if this is a decryption (has ncryptsec) or migration (has legacy nsec)
    if let Some(ncryptsec) = prompt_state.ncryptsec {
        // Decrypt existing ncryptsec
        match crate::utils::nip49::decrypt_ncryptsec(&ncryptsec, password) {
            Ok(keys) => {
                // Login with decrypted keys
                if let Err(e) = login_with_keys_internal(keys).await {
                    PASSWORD_PROMPT.write().loading = false;
                    PASSWORD_PROMPT.write().error = Some(e.clone());
                    return Err(e);
                }

                // Clear password prompt
                *PASSWORD_PROMPT.write() = PasswordPromptState::default();

                Ok(())
            }
            Err(e) => {
                PASSWORD_PROMPT.write().loading = false;
                PASSWORD_PROMPT.write().error = Some(e.to_string());
                Err(e.to_string())
            }
        }
    } else {
        // Migration: encrypt existing plaintext nsec
        if let Ok(nsec) = LocalStorage::get::<String>(STORAGE_KEY_NSEC) {
            // Validate password
            if let Some(err) = crate::utils::nip49::validate_password(password) {
                PASSWORD_PROMPT.write().loading = false;
                PASSWORD_PROMPT.write().error = Some(err.clone());
                return Err(err);
            }

            // Parse the key
            let keys = match Keys::parse(&nsec) {
                Ok(k) => k,
                Err(e) => {
                    let err = format!("Invalid stored key: {}", e);
                    PASSWORD_PROMPT.write().loading = false;
                    PASSWORD_PROMPT.write().error = Some(err.clone());
                    return Err(err);
                }
            };

            // Encrypt with password (using Weak security since it was stored unencrypted)
            let ncryptsec = match crate::utils::nip49::encrypt_secret_key_with_security(
                keys.secret_key(),
                password,
                nostr::nips::nip49::KeySecurity::Weak,
            ) {
                Ok(enc) => enc,
                Err(e) => {
                    let err = format!("Encryption failed: {}", e);
                    PASSWORD_PROMPT.write().loading = false;
                    PASSWORD_PROMPT.write().error = Some(err.clone());
                    return Err(err);
                }
            };

            // Store encrypted version
            LocalStorage::set(STORAGE_KEY_NCRYPTSEC, &ncryptsec).ok();
            // Remove plaintext only after successful encryption
            LocalStorage::delete(STORAGE_KEY_NSEC);

            // Login with keys
            if let Err(e) = login_with_keys_internal(keys).await {
                PASSWORD_PROMPT.write().loading = false;
                PASSWORD_PROMPT.write().error = Some(e.clone());
                return Err(e);
            }

            // Clear password prompt
            *PASSWORD_PROMPT.write() = PasswordPromptState::default();

            log::info!("Successfully migrated key to encrypted format");
            Ok(())
        } else {
            PASSWORD_PROMPT.write().loading = false;
            let err = "No stored key found".to_string();
            PASSWORD_PROMPT.write().error = Some(err.clone());
            Err(err)
        }
    }
}

/// Cancel password prompt and clear auth state
pub fn cancel_password_prompt() {
    *PASSWORD_PROMPT.write() = PasswordPromptState::default();
    clear_auth();
    LocalStorage::delete(STORAGE_KEY_NCRYPTSEC);
    LocalStorage::delete(STORAGE_KEY_NSEC);
    LocalStorage::delete(STORAGE_KEY_NPUB);
    LocalStorage::delete(STORAGE_KEY_METHOD);
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
    keys.secret_key().to_bech32().map_err(|e| e.to_string())
}

/// Export public key as npub
pub fn export_npub() -> Result<String, String> {
    let pubkey = get_pubkey().ok_or("Not logged in")?;
    Ok(pubkey)
}
