use dioxus::prelude::*;
use dioxus::signals::ReadableExt;
use nostr::{Keys, PublicKey};
use nostr_sdk::ToBech32;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use crate::stores::nostr_client;
use crate::stores::signer::{set_signer as store_signer, SignerType};
#[cfg(target_family = "wasm")]
use nostr_browser_signer::BrowserSigner;
use nostr_connect::client::NostrConnect;
use nostr_sdk::nips::nip46::NostrConnectURI;
/// Authentication state
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
pub struct AuthState {
    pub pubkey: Option<String>,
    pub is_authenticated: bool,
    pub login_method: Option<LoginMethod>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum LoginMethod {
    BrowserExtension,
    PrivateKey,
    ReadOnly,
    RemoteSigner,
    #[cfg(feature = "mobile")]
    AndroidSigner,
}
/// Global authentication state
pub static AUTH_STATE: GlobalSignal<AuthState> = Signal::global(AuthState::default);
/// Global keys (if using private key login)
static KEYS: GlobalSignal<Option<Keys>> = Signal::global(|| None);
const STORAGE_KEY_NSEC: &str = "nostr_nsec";
const STORAGE_KEY_NCRYPTSEC: &str = "nostr_ncryptsec";
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
pub static PASSWORD_PROMPT: GlobalSignal<PasswordPromptState> = Signal::global(
    PasswordPromptState::default,
);
/// Initialize authentication from stored credentials
/// Note: This only loads the auth state from localStorage.
/// Actual signer restoration should be done via restore_session_async()
pub fn init_auth() {
    log::info!("Initializing authentication...");
    if let Ok(method_str) = crate::platform::storage::get::<String>(STORAGE_KEY_METHOD) {
        match method_str.as_str() {
            "extension" => {
                log::info!("Found extension login method");
                if let Ok(npub) = crate::platform::storage::get::<String>(STORAGE_KEY_NPUB) {
                    *AUTH_STATE.write() = AuthState {
                        pubkey: Some(npub),
                        is_authenticated: true,
                        login_method: Some(LoginMethod::BrowserExtension),
                    };
                }
            }
            "private_key" => {
                if let Ok(npub) = crate::platform::storage::get::<String>(STORAGE_KEY_NPUB) {
                    log::info!("Found stored private key session");
                    *AUTH_STATE.write() = AuthState {
                        pubkey: Some(npub),
                        is_authenticated: true,
                        login_method: Some(LoginMethod::PrivateKey),
                    };
                }
            }
            "read_only" => {
                if let Ok(npub) = crate::platform::storage::get::<String>(STORAGE_KEY_NPUB) {
                    log::info!("Found stored read-only session");
                    match crate::utils::nip19::normalize_pubkey(&npub) {
                        Ok(pubkey_hex) => {
                            *AUTH_STATE.write() = AuthState {
                                pubkey: Some(pubkey_hex),
                                is_authenticated: false,
                                login_method: Some(LoginMethod::ReadOnly),
                            };
                        }
                        Err(_) => {
                            log::warn!("Corrupted read-only pubkey in storage, clearing");
                            crate::platform::storage::delete(STORAGE_KEY_NPUB);
                            crate::platform::storage::delete(STORAGE_KEY_METHOD);
                        }
                    }
                }
            }
            "remote_signer" => {
                if let Ok(stored_pubkey) = crate::platform::storage::get::<String>(STORAGE_KEY_NPUB) {
                    log::info!("Found stored remote signer session");
                    match crate::utils::nip19::normalize_pubkey(&stored_pubkey) {
                        Ok(pubkey_hex) => {
                            *AUTH_STATE.write() = AuthState {
                                pubkey: Some(pubkey_hex),
                                is_authenticated: true,
                                login_method: Some(LoginMethod::RemoteSigner),
                            };
                        }
                        Err(_) => {
                            log::warn!("Corrupted remote signer pubkey in storage, clearing");
                            crate::platform::storage::delete(STORAGE_KEY_NPUB);
                            crate::platform::storage::delete(STORAGE_KEY_BUNKER_URI);
                            crate::platform::storage::delete(STORAGE_KEY_APP_KEYS);
                            crate::platform::storage::delete(STORAGE_KEY_METHOD);
                        }
                    }
                }
            }
            _ => {}
        }
    }
}
/// Restore session asynchronously (call this after app initialization)
pub async fn restore_session_async() {
    log::info!("Restoring session...");
    if let Ok(method_str) = crate::platform::storage::get::<String>(STORAGE_KEY_METHOD) {
        match method_str.as_str() {
            "extension" => {
                if let Err(e) = login_with_browser_extension().await {
                    log::error!("Failed to restore browser extension session: {}", e);
                    clear_auth();
                }
            }
            "private_key" => {
                if let Ok(ncryptsec) = crate::platform::storage::get::<String>(STORAGE_KEY_NCRYPTSEC) {
                    if crate::utils::nip49::is_ncryptsec(&ncryptsec) {
                        *PASSWORD_PROMPT.write() = PasswordPromptState {
                            required: true,
                            ncryptsec: Some(ncryptsec),
                            error: None,
                            loading: false,
                        };
                        log::info!("Password required to restore encrypted session");
                    } else {
                        log::error!("Invalid ncryptsec format in storage");
                        clear_auth();
                        crate::platform::storage::delete(STORAGE_KEY_NCRYPTSEC);
                        crate::platform::storage::delete(STORAGE_KEY_METHOD);
                    }
                } else if let Ok(_nsec) = crate::platform::storage::get::<String>(STORAGE_KEY_NSEC) {
                    *PASSWORD_PROMPT.write() = PasswordPromptState {
                        required: true,
                        ncryptsec: None,
                        error: Some(
                            "Your key needs to be encrypted for security. Please set a password."
                                .to_string(),
                        ),
                        loading: false,
                    };
                    log::info!(
                        "Legacy nsec found, migration to encrypted format needed"
                    );
                }
            }
            "read_only" => {
                if let Ok(npub) = crate::platform::storage::get::<String>(STORAGE_KEY_NPUB) {
                    if let Err(e) = login_with_npub(&npub).await {
                        log::error!("Failed to restore read-only session: {}", e);
                        clear_auth();
                    }
                }
            }
            "remote_signer" => {
                if let (Ok(bunker_uri), Ok(app_keys_str)) = (
                    crate::platform::storage::get::<String>(STORAGE_KEY_BUNKER_URI),
                    crate::platform::storage::get::<String>(STORAGE_KEY_APP_KEYS),
                ) {
                    match restore_nostr_connect(&bunker_uri, &app_keys_str).await {
                        Ok(nostr_connect) => {
                            let signer_type = SignerType::NostrConnect(
                                Arc::new(nostr_connect),
                            );
                            match store_signer(signer_type.clone()).await {
                                Ok(_) => {
                                    match nostr_client::set_signer(signer_type).await {
                                        Ok(_) => {
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
                    log::error!(
                        "Cannot restore remote signer session: incomplete saved credentials (missing bunker URI or app keys)"
                    );
                    clear_auth();
                    crate::platform::storage::delete(STORAGE_KEY_BUNKER_URI);
                    crate::platform::storage::delete(STORAGE_KEY_APP_KEYS);
                    crate::platform::storage::delete(STORAGE_KEY_METHOD);
                    crate::platform::storage::delete(STORAGE_KEY_NPUB);
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
    if let Some(err) = crate::utils::nip49::validate_password(password) {
        return Err(err);
    }
    let keys = Keys::parse(nsec).map_err(|e| format!("Invalid private key: {}", e))?;
    let pubkey = keys.public_key().to_string();
    let ncryptsec = crate::utils::nip49::encrypt_secret_key(keys.secret_key(), password)
        .map_err(|e| format!("Encryption failed: {}", e))?;
    *KEYS.write() = Some(keys.clone());
    let signer = SignerType::Keys(keys);
    store_signer(signer.clone()).await?;
    nostr_client::set_signer(signer).await?;
    *AUTH_STATE.write() = AuthState {
        pubkey: Some(pubkey.clone()),
        is_authenticated: true,
        login_method: Some(LoginMethod::PrivateKey),
    };
    crate::platform::storage::set(STORAGE_KEY_NCRYPTSEC, &ncryptsec).ok();
    crate::platform::storage::set(STORAGE_KEY_NPUB, &pubkey).ok();
    crate::platform::storage::set(STORAGE_KEY_METHOD, "private_key").ok();
    crate::platform::storage::delete(STORAGE_KEY_NSEC);
    log::info!("Successfully logged in with encrypted key, pubkey: {}", pubkey);
    run_post_login_init().await;
    Ok(())
}
/// Login with private key without password (internal use only for session restore)
async fn login_with_keys_internal(keys: Keys) -> Result<(), String> {
    let pubkey = keys.public_key().to_string();
    *KEYS.write() = Some(keys.clone());
    let signer = SignerType::Keys(keys);
    store_signer(signer.clone()).await?;
    nostr_client::set_signer(signer).await?;
    *AUTH_STATE.write() = AuthState {
        pubkey: Some(pubkey.clone()),
        is_authenticated: true,
        login_method: Some(LoginMethod::PrivateKey),
    };
    log::info!("Session restored with pubkey: {}", pubkey);
    run_post_login_init().await;
    Ok(())
}
/// Login with public key only (read-only mode)
pub async fn login_with_npub(npub: &str) -> Result<(), String> {
    log::info!("Logging in with public key (read-only)...");
    let pubkey = PublicKey::parse(npub)
        .map_err(|e| format!("Invalid public key: {}", e))?;
    let pubkey_str = pubkey.to_string();
    nostr_client::set_read_only().await?;
    *AUTH_STATE.write() = AuthState {
        pubkey: Some(pubkey_str.clone()),
        is_authenticated: false,
        login_method: Some(LoginMethod::ReadOnly),
    };
    crate::platform::storage::set(STORAGE_KEY_NPUB, &pubkey_str).ok();
    crate::platform::storage::set(STORAGE_KEY_METHOD, "read_only").ok();
    log::info!("Loaded read-only mode with pubkey: {}", pubkey_str);
    Ok(())
}
/// Login with NIP-07 browser extension (official implementation)
pub async fn login_with_browser_extension() -> Result<(), String> {
    #[cfg(target_family = "wasm")]
    {
        log::info!("Attempting browser extension login...");
        let browser_signer = BrowserSigner::new()
            .map_err(|e| format!("Failed to initialize browser signer: {}", e))?;
        use nostr::signer::NostrSigner;
        let pubkey = browser_signer
            .get_public_key()
            .await
            .map_err(|e| format!("Failed to get public key from extension: {}", e))?;
        let pubkey_str = pubkey.to_string();
        let signer = SignerType::BrowserExtension(Arc::new(browser_signer));
        store_signer(signer.clone()).await?;
        nostr_client::set_signer(signer).await?;
        *AUTH_STATE.write() = AuthState {
            pubkey: Some(pubkey_str.clone()),
            is_authenticated: true,
            login_method: Some(LoginMethod::BrowserExtension),
        };
        crate::platform::storage::set(STORAGE_KEY_METHOD, "extension").ok();
        crate::platform::storage::set(STORAGE_KEY_NPUB, &pubkey_str).ok();
        log::info!(
            "Successfully logged in via browser extension with pubkey: {}", pubkey_str
        );
        run_post_login_init().await;
        Ok(())
    }
    #[cfg(not(target_family = "wasm"))]
    { Err("Browser extension login is only available in browser".to_string()) }
}
/// Deprecated: Use login_with_browser_extension instead
#[deprecated(note = "Use login_with_browser_extension instead")]
#[allow(dead_code)]
pub async fn login_with_nip07() -> Result<(), String> {
    login_with_browser_extension().await
}
/// Check if browser extension (NIP-07) is available
pub fn is_browser_extension_available() -> bool {
    #[cfg(target_family = "wasm")] { BrowserSigner::new().is_ok() }
    #[cfg(not(target_family = "wasm"))] { false }
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
    if let Ok(stored_keys) = crate::platform::storage::get::<String>(STORAGE_KEY_APP_KEYS) {
        if let Ok(keys) = Keys::parse(&stored_keys) {
            return Ok(keys);
        }
    }
    Ok(Keys::generate())
}
/// Restore NostrConnect instance from stored credentials
async fn restore_nostr_connect(
    bunker_uri: &str,
    app_keys_str: &str,
) -> Result<NostrConnect, String> {
    let uri = NostrConnectURI::parse(bunker_uri)
        .map_err(|e| format!("Invalid stored bunker URI: {}", e))?;
    let app_keys = Keys::parse(app_keys_str)
        .map_err(|e| format!("Invalid stored app keys: {}", e))?;
    let timeout = Duration::from_secs(120);
    let nostr_connect = NostrConnect::new(uri, app_keys, timeout, None)
        .map_err(|e| format!("Failed to reconnect: {}", e))?;
    use nostr::signer::NostrSigner;
    nostr_connect
        .get_public_key()
        .await
        .map_err(|e| format!("Remote signer not responding: {}", e))?;
    Ok(nostr_connect)
}
/// Run post-login initialization steps (notifications, subscriptions, emoji fetch)
/// This should be called after any successful login or session restoration
async fn run_post_login_init() {
    log::info!("Running post-login initialization...");
    crate::stores::notifications::load_checked_at();
    crate::stores::notifications::fetch_and_merge_from_nip78().await;
    if let Err(e) = crate::stores::blossom_store::fetch_user_servers().await {
        log::warn!("Failed to fetch Blossom servers: {}", e);
    }
    // Wait for user relay lists before starting subscriptions
    // Critical for NIP-46 where signer restoration is slow and
    // relay application happens concurrently in set_signer()'s spawn_forever
    crate::stores::relay::wait_for_user_relays(
        std::time::Duration::from_secs(5), "run_post_login_init"
    ).await;
    crate::stores::notifications::start_realtime_subscription().await;
    crate::stores::relay::start_relay_list_subscription().await;
    crate::stores::emoji_store::init_emoji_fetch();
    // Prefetch contacts into memory cache so Home feed loads faster
    if let Some(pubkey) = get_pubkey() {
        spawn(async move {
            match nostr_client::fetch_contacts(pubkey).await {
                Ok(contacts) => {
                    log::info!("Prefetched {} contacts into memory cache", contacts.len());
                }
                Err(e) => {
                    log::warn!("Failed to prefetch contacts: {}", e);
                }
            }
        });
    }
    spawn(async move {
        if let Some(client) = crate::stores::nostr_client::get_client() {
            log::info!("Prefetching metadata for all contacts...");
            crate::stores::nostr_client::ensure_relays_ready(&client).await;
            match client
                .get_contact_list_metadata(std::time::Duration::from_secs(15))
                .await
            {
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
    let uri = NostrConnectURI::parse(bunker_uri)
        .map_err(|e| format!("Invalid bunker URI: {}", e))?;
    let app_keys = get_or_create_app_keys()?;
    let timeout = Duration::from_secs(120);
    let nostr_connect = NostrConnect::new(uri, app_keys.clone(), timeout, None)
        .map_err(|e| format!("Failed to create connection: {}", e))?;
    use nostr::signer::NostrSigner;
    let public_key = nostr_connect
        .get_public_key()
        .await
        .map_err(|e| format!("Failed to get public key: {}", e))?;
    let pubkey_str = public_key.to_hex();
    crate::platform::storage::set(STORAGE_KEY_BUNKER_URI, bunker_uri)
        .map_err(|e| format!("Failed to store bunker URI: {}", e))?;
    let app_keys_bech32 = app_keys
        .secret_key()
        .to_bech32()
        .map_err(|e| format!("Failed to convert app keys: {}", e))?;
    crate::platform::storage::set(STORAGE_KEY_APP_KEYS, &app_keys_bech32)
        .map_err(|e| format!("Failed to store app keys: {}", e))?;
    crate::platform::storage::set(STORAGE_KEY_METHOD, "remote_signer")
        .map_err(|e| format!("Failed to store login method: {}", e))?;
    crate::platform::storage::set(STORAGE_KEY_NPUB, &pubkey_str)
        .map_err(|e| format!("Failed to store public key: {}", e))?;
    let signer_type = SignerType::NostrConnect(Arc::new(nostr_connect));
    store_signer(signer_type.clone()).await?;
    nostr_client::set_signer(signer_type).await?;
    *AUTH_STATE.write() = AuthState {
        pubkey: Some(pubkey_str.clone()),
        is_authenticated: true,
        login_method: Some(LoginMethod::RemoteSigner),
    };
    log::info!("Successfully logged in via remote signer with pubkey: {}", pubkey_str);
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
/// Get current public key (hex format)
pub fn get_pubkey() -> Option<String> {
    let p = AUTH_STATE.read().pubkey.clone();
    if let Some(ref s) = p {
        debug_assert!(!s.starts_with("npub"), "get_pubkey returned bech32 instead of hex");
    }
    p
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
    crate::stores::notifications::stop_realtime_subscription().await;
    crate::stores::relay::stop_relay_list_subscription().await;
    crate::stores::cashu_cdk_bridge::clear_multi_wallet();
    crate::stores::shop_store::clear_caches();
    crate::stores::dms::clear_caches();
    crate::stores::shop_store::reset_orders_loaded_flag();
    spawn(async move {
        crate::services::search_relays::invalidate_search_relay_cache().await;
    });
    let _ = nostr_client::set_read_only().await;
    clear_auth();
    crate::platform::storage::delete(STORAGE_KEY_NSEC);
    crate::platform::storage::delete(STORAGE_KEY_NCRYPTSEC);
    crate::platform::storage::delete(STORAGE_KEY_NPUB);
    crate::platform::storage::delete(STORAGE_KEY_METHOD);
    crate::platform::storage::delete(STORAGE_KEY_BUNKER_URI);
    crate::platform::storage::delete(STORAGE_KEY_APP_KEYS);
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
    PASSWORD_PROMPT.write().loading = true;
    PASSWORD_PROMPT.write().error = None;
    if let Some(ncryptsec) = prompt_state.ncryptsec {
        match crate::utils::nip49::decrypt_ncryptsec(&ncryptsec, password) {
            Ok(keys) => {
                if let Err(e) = login_with_keys_internal(keys).await {
                    PASSWORD_PROMPT.write().loading = false;
                    PASSWORD_PROMPT.write().error = Some(e.clone());
                    return Err(e);
                }
                *PASSWORD_PROMPT.write() = PasswordPromptState::default();
                Ok(())
            }
            Err(e) => {
                PASSWORD_PROMPT.write().loading = false;
                PASSWORD_PROMPT.write().error = Some(e.to_string());
                Err(e.to_string())
            }
        }
    } else if let Ok(nsec) = crate::platform::storage::get::<String>(STORAGE_KEY_NSEC) {
        if let Some(err) = crate::utils::nip49::validate_password(password) {
            PASSWORD_PROMPT.write().loading = false;
            PASSWORD_PROMPT.write().error = Some(err.clone());
            return Err(err);
        }
        let keys = match Keys::parse(&nsec) {
            Ok(k) => k,
            Err(e) => {
                let err = format!("Invalid stored key: {}", e);
                PASSWORD_PROMPT.write().loading = false;
                PASSWORD_PROMPT.write().error = Some(err.clone());
                return Err(err);
            }
        };
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
        crate::platform::storage::set(STORAGE_KEY_NCRYPTSEC, &ncryptsec).ok();
        crate::platform::storage::delete(STORAGE_KEY_NSEC);
        if let Err(e) = login_with_keys_internal(keys).await {
            PASSWORD_PROMPT.write().loading = false;
            PASSWORD_PROMPT.write().error = Some(e.clone());
            return Err(e);
        }
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
/// Cancel password prompt and clear auth state
pub fn cancel_password_prompt() {
    *PASSWORD_PROMPT.write() = PasswordPromptState::default();
    clear_auth();
    crate::platform::storage::delete(STORAGE_KEY_NCRYPTSEC);
    crate::platform::storage::delete(STORAGE_KEY_NSEC);
    crate::platform::storage::delete(STORAGE_KEY_NPUB);
    crate::platform::storage::delete(STORAGE_KEY_METHOD);
}
/// Sign a message with current keys
#[allow(dead_code)]
pub fn sign_message(message: &str) -> Result<String, String> {
    let _keys = get_keys().ok_or("Not logged in with private key")?;
    Ok(format!("signed_{}", message))
}
/// Export private key as nsec
pub fn export_nsec() -> Result<String, String> {
    let keys = get_keys().ok_or("Not logged in with private key")?;
    keys.secret_key().to_bech32().map_err(|e| e.to_string())
}
/// Export public key as npub
pub fn export_npub() -> Result<String, String> {
    let pubkey_hex = get_pubkey().ok_or("Not logged in")?;
    let pubkey = PublicKey::parse(&pubkey_hex).map_err(|e| e.to_string())?;
    pubkey.to_bech32().map_err(|e| e.to_string())
}
/// Check if an Android signer (NIP-55) is available
pub fn is_android_signer_available() -> bool {
    #[cfg(feature = "mobile")]
    { crate::platform::Nip55Signer::is_signer_installed() }
    #[cfg(not(feature = "mobile"))]
    { false }
}
/// Login with Android signer (NIP-55)
pub async fn login_with_android_signer(npub: &str) -> Result<(), String> {
    #[cfg(feature = "mobile")]
    {
        let _ = npub;
        // TODO: Implement AndroidSigner login once SignerType::AndroidSigner is added
        Err("Android signer login not yet implemented".to_string())
    }
    #[cfg(not(feature = "mobile"))]
    {
        let _ = npub;
        Err("Android signer is only available on mobile".to_string())
    }
}
