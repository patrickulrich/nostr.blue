use crate::stores::nostr_client;
use crate::stores::signer::{set_signer_with_pubkey, SignerType};
use dioxus::prelude::*;
use dioxus::signals::ReadableExt;
use nostr::{Keys, PublicKey};
#[cfg(any(target_family = "wasm", feature = "mobile_platform"))]
use nostr::nips::nip06::FromMnemonic;
use nostr_sdk::prelude::NostrDatabaseExt;
#[cfg(target_family = "wasm")]
use nostr_browser_signer::BrowserSigner;
use nostr_connect::client::NostrConnect;
use nostr_sdk::nips::nip46::NostrConnectURI;
use nostr_sdk::ToBech32;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
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
    #[cfg(feature = "mobile_platform")]
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
const STORAGE_KEY_SIGNER_PACKAGE: &str = "signer_package";
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
pub static PASSWORD_PROMPT: GlobalSignal<PasswordPromptState> =
    Signal::global(PasswordPromptState::default);

#[cfg(any(target_family = "wasm", feature = "mobile_platform"))]
pub static GOOGLE_BACKUP_STATE: GlobalSignal<
    crate::services::cloud_backup::GoogleBackupState,
> = Signal::global(crate::services::cloud_backup::GoogleBackupState::default);
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
                    match crate::utils::nip19::normalize_pubkey(&npub) {
                        Ok(pubkey_hex) => {
                            *AUTH_STATE.write() = AuthState {
                                pubkey: Some(pubkey_hex),
                                is_authenticated: true,
                                login_method: Some(LoginMethod::BrowserExtension),
                            };
                        }
                        Err(_) => {
                            log::warn!("Corrupted extension pubkey in storage, clearing");
                            let _ = crate::platform::storage::delete(STORAGE_KEY_NPUB);
                            let _ = crate::platform::storage::delete(STORAGE_KEY_METHOD);
                        }
                    }
                }
            }
            "private_key" => {
                if let Ok(npub) = crate::platform::storage::get::<String>(STORAGE_KEY_NPUB) {
                    log::info!("Found stored private key session");
                    match crate::utils::nip19::normalize_pubkey(&npub) {
                        Ok(pubkey_hex) => {
                            *AUTH_STATE.write() = AuthState {
                                pubkey: Some(pubkey_hex),
                                is_authenticated: true,
                                login_method: Some(LoginMethod::PrivateKey),
                            };
                        }
                        Err(_) => {
                            log::warn!("Corrupted private key pubkey in storage, clearing");
                            let _ = crate::platform::storage::delete(STORAGE_KEY_NPUB);
                            let _ = crate::platform::storage::delete(STORAGE_KEY_METHOD);
                        }
                    }
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
                            let _ = crate::platform::storage::delete(STORAGE_KEY_NPUB);
                            let _ = crate::platform::storage::delete(STORAGE_KEY_METHOD);
                        }
                    }
                }
            }
            "remote_signer" => {
                if let Ok(stored_pubkey) = crate::platform::storage::get::<String>(STORAGE_KEY_NPUB)
                {
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
                            let _ = crate::platform::storage::delete(STORAGE_KEY_NPUB);
                            let _ = crate::platform::storage::delete(STORAGE_KEY_BUNKER_URI);
                            let _ = crate::platform::storage::delete(STORAGE_KEY_APP_KEYS);
                            let _ = crate::platform::storage::delete(STORAGE_KEY_METHOD);
                        }
                    }
                }
            }
            #[cfg(feature = "mobile_platform")]
            "android_signer" => {
                if let Ok(npub) = crate::platform::storage::get::<String>(STORAGE_KEY_NPUB) {
                    log::info!("Found stored Android signer session");
                    match crate::utils::nip19::normalize_pubkey(&npub) {
                        Ok(pubkey_hex) => {
                            *AUTH_STATE.write() = AuthState {
                                pubkey: Some(pubkey_hex),
                                is_authenticated: true,
                                login_method: Some(LoginMethod::AndroidSigner),
                            };
                        }
                        Err(_) => {
                            log::warn!("Corrupted Android signer pubkey in storage, clearing");
                            let _ = crate::platform::storage::delete(STORAGE_KEY_NPUB);
                            let _ = crate::platform::storage::delete(STORAGE_KEY_METHOD);
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
                if let Ok(ncryptsec) =
                    crate::platform::storage::get::<String>(STORAGE_KEY_NCRYPTSEC)
                {
                    if crate::utils::nip49::is_ncryptsec(&ncryptsec) {
                        if let Ok(auto_pw) =
                            crate::platform::storage::get::<String>(STORAGE_KEY_AUTO_PASSWORD)
                        {
                            match crate::utils::nip49::decrypt_ncryptsec(&ncryptsec, &auto_pw) {
                                Ok(keys) => {
                                    if let Err(e) =
                                        login_with_keys_internal(keys).await
                                    {
                                        log::error!(
                                            "Failed to restore auto-password session: {}",
                                            e
                                        );
                                        clear_auth();
                                    }
                                }
                                Err(_) => {
                                    log::warn!(
                                        "Auto-password decryption failed, prompting user"
                                    );
                                    *PASSWORD_PROMPT.write() = PasswordPromptState {
                                        required: true,
                                        ncryptsec: Some(ncryptsec),
                                        error: None,
                                        loading: false,
                                    };
                                }
                            }
                        } else {
                            *PASSWORD_PROMPT.write() = PasswordPromptState {
                                required: true,
                                ncryptsec: Some(ncryptsec),
                                error: None,
                                loading: false,
                            };
                            log::info!("Password required to restore encrypted session");
                        }
                    } else {
                        log::error!("Invalid ncryptsec format in storage");
                        clear_auth();
                        if let Err(e) = crate::platform::storage::delete(STORAGE_KEY_NCRYPTSEC) {
                            log::error!("Failed to delete ncryptsec: {}", e);
                        }
                        if let Err(e) = crate::platform::storage::delete(STORAGE_KEY_METHOD) {
                            log::error!("Failed to delete method: {}", e);
                        }
                    }
                } else if let Ok(_nsec) = crate::platform::storage::get::<String>(STORAGE_KEY_NSEC)
                {
                    *PASSWORD_PROMPT.write() = PasswordPromptState {
                        required: true,
                        ncryptsec: None,
                        error: Some(
                            "Your key needs to be encrypted for security. Please set a password."
                                .to_string(),
                        ),
                        loading: false,
                    };
                    log::info!("Legacy nsec found, migration to encrypted format needed");
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
            #[cfg(feature = "mobile_platform")]
            "android_signer" => {
                if let Ok(npub) = crate::platform::storage::get::<String>(STORAGE_KEY_NPUB) {
                    let package =
                        crate::platform::storage::get::<String>(STORAGE_KEY_SIGNER_PACKAGE)
                            .unwrap_or_else(|_| {
                                crate::platform::Nip55Signer::default_package().to_string()
                            });
                    if let Err(e) = login_with_android_signer(&npub, Some(&package)).await {
                        log::error!("Failed to restore Android signer session: {}", e);
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
                        Ok((nostr_connect, public_key)) => {
                            let signer_type = SignerType::NostrConnect(Arc::new(nostr_connect));
                            match set_signer_with_pubkey(signer_type.clone(), public_key).await {
                                Ok(_) => match nostr_client::set_signer(signer_type).await {
                                    Ok(_) => {
                                        run_post_login_init();
                                        log::info!("Successfully restored remote signer session");
                                    }
                                    Err(e) => {
                                        log::error!("Failed to set remote signer on client: {}", e);
                                        clear_auth();
                                    }
                                },
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
                    let _ = crate::platform::storage::delete(STORAGE_KEY_BUNKER_URI);
                    let _ = crate::platform::storage::delete(STORAGE_KEY_APP_KEYS);
                    let _ = crate::platform::storage::delete(STORAGE_KEY_METHOD);
                    let _ = crate::platform::storage::delete(STORAGE_KEY_NPUB);
                }
            }
            _ => {}
        }
    }
}

const STORAGE_KEY_AUTO_PASSWORD: &str = "nostr_auto_password";
const STORAGE_KEY_GOOGLE_BACKUP_USER: &str = "google_backup_user";

#[cfg(any(target_family = "wasm", feature = "mobile_platform"))]
pub async fn start_google_sign_in() {
    use crate::services::cloud_backup;

    *GOOGLE_BACKUP_STATE.write() = cloud_backup::GoogleBackupState::SigningIn;

    let auth = match cloud_backup::google_sign_in().await {
        Ok(a) => a,
        Err(e) => {
            *GOOGLE_BACKUP_STATE.write() = cloud_backup::GoogleBackupState::Error(e);
            return;
        }
    };

    *GOOGLE_BACKUP_STATE.write() = cloud_backup::GoogleBackupState::CheckingDrive;

    let mut entries = match cloud_backup::list_cloud_backups(&auth).await {
        Ok(e) => e,
        Err(e) => {
            *GOOGLE_BACKUP_STATE.write() = cloud_backup::GoogleBackupState::Error(e);
            return;
        }
    };

    if !entries.is_empty() {
        enrich_backup_entries(&mut entries).await;
    }

    if entries.is_empty() {
        *GOOGLE_BACKUP_STATE.write() = cloud_backup::GoogleBackupState::NoBackup(auth);
    } else {
        *GOOGLE_BACKUP_STATE.write() = cloud_backup::GoogleBackupState::Choose { entries, auth };
    }
}

#[cfg(any(target_family = "wasm", feature = "mobile_platform"))]
async fn enrich_backup_entries(entries: &mut [crate::services::cloud_backup::BackupEntry]) {
    let npubs: Vec<String> = entries.iter().map(|e| e.npub.clone()).collect();

    let profiles = if crate::stores::nostr_client::get_client().is_some() {
        crate::stores::profiles::fetch_profiles_batch(npubs.clone())
            .await
            .ok()
    } else {
        enrich_with_temp_client(&npubs).await
    };

    if let Some(profiles) = profiles {
        for entry in entries.iter_mut() {
            if let Some(profile) = profiles.get(&entry.npub) {
                let name = profile.get_display_name();
                let pic = profile.get_avatar_url();
                if !name.starts_with("npub1") && !name.contains("...") {
                    entry.display_name = Some(name);
                }
                if !pic.contains("dicebear") {
                    entry.picture = Some(pic);
                }
            }
        }
    }
}

#[cfg(any(target_family = "wasm", feature = "mobile_platform"))]
async fn enrich_with_temp_client(
    npubs: &[String],
) -> Option<std::collections::HashMap<String, crate::stores::profiles::Profile>> {
    use std::collections::HashSet;

    let authors: Vec<nostr::PublicKey> = npubs
        .iter()
        .filter_map(|npub| nostr::PublicKey::parse(npub).ok())
        .collect();
    if authors.is_empty() {
        return None;
    }

    let indexer_urls = crate::stores::relay::nip65::get_indexer_relay_urls();
    let discovery_urls: Vec<String> = crate::stores::nostr_client::DEFAULT_DISCOVERY_RELAYS
        .iter()
        .map(|s| s.to_string())
        .collect();
    let mut relay_set: HashSet<String> = HashSet::new();
    for url in indexer_urls.iter().chain(discovery_urls.iter()) {
        relay_set.insert(url.clone());
    }
    let relay_urls: Vec<String> = relay_set.into_iter().collect();

    let ephemeral_keys = nostr_sdk::Keys::generate();
    let client = nostr_sdk::Client::new(ephemeral_keys);
    for url in &relay_urls {
        if let Ok(relay_url) = nostr_sdk::RelayUrl::parse(url) {
            client.add_read_relay(relay_url).await.ok();
        }
    }
    client.connect().await;

    let filter = nostr::Filter::new()
        .kind(nostr::Kind::Metadata)
        .authors(authors);

    let events = match client
        .fetch_events(filter, std::time::Duration::from_secs(6))
        .await
    {
        Ok(events) => events,
        Err(e) => {
            log::warn!("Profile enrichment fetch failed: {}", e);
            return None;
        }
    };

    let mut profiles = std::collections::HashMap::new();
    for event in events.into_iter() {
        let pubkey_bech32 = event.pubkey.to_bech32().unwrap_or_else(|_| event.pubkey.to_hex());
        if let Ok(profile) = crate::stores::profiles::parse_profile_event(&event) {
            profiles.insert(pubkey_bech32, profile);
        }
    }

    Some(profiles)
}

#[cfg(any(target_family = "wasm", feature = "mobile_platform"))]
pub async fn restore_google_backup(file_id: &str) {
    use crate::services::cloud_backup;


    let state = GOOGLE_BACKUP_STATE.read().clone();
    let auth = match state {
        cloud_backup::GoogleBackupState::Choose { auth, .. } => auth,
        _ => {
            *GOOGLE_BACKUP_STATE.write() =
                cloud_backup::GoogleBackupState::Error("Invalid state for restore".to_string());
            return;
        }
    };

    *GOOGLE_BACKUP_STATE.write() = cloud_backup::GoogleBackupState::Working;

    match cloud_backup::restore_from_cloud(file_id, &auth).await {
        Ok(bundle) => match store_key_from_google_restore(&bundle.nsec_hex, bundle.nwc_uri.as_deref())
            .await
        {
            Ok(()) => {
                *GOOGLE_BACKUP_STATE.write() =
                    cloud_backup::GoogleBackupState::Done { is_new_account: false };
            }
            Err(e) => {
                *GOOGLE_BACKUP_STATE.write() =
                    cloud_backup::GoogleBackupState::Error(format!("Failed to store key: {}", e));
            }
        },
        Err(e) => {
            *GOOGLE_BACKUP_STATE.write() =
                cloud_backup::GoogleBackupState::Error(format!("Failed to restore: {}", e));
        }
    }
}

#[cfg(any(target_family = "wasm", feature = "mobile_platform"))]
pub async fn import_key_to_google(nsec: &str) {
    use crate::services::cloud_backup;


    let state = GOOGLE_BACKUP_STATE.read().clone();
    let auth = match state {
        cloud_backup::GoogleBackupState::ImportKey { auth, .. }
        | cloud_backup::GoogleBackupState::NoBackup(auth) => auth,
        _ => {
            *GOOGLE_BACKUP_STATE.write() =
                cloud_backup::GoogleBackupState::Error("Invalid state for import".to_string());
            return;
        }
    };

    *GOOGLE_BACKUP_STATE.write() = cloud_backup::GoogleBackupState::Working;

    let keys = match nostr::Keys::parse(nsec) {
        Ok(k) => k,
        Err(e) => {
            *GOOGLE_BACKUP_STATE.write() =
                cloud_backup::GoogleBackupState::Error(format!("Invalid nsec: {}", e));
            return;
        }
    };
    let nsec_hex = keys.secret_key().to_secret_hex();

    if let Err(e) = cloud_backup::backup_to_cloud(&nsec_hex, None, None, &auth).await {
        *GOOGLE_BACKUP_STATE.write() =
            cloud_backup::GoogleBackupState::Error(format!("Upload failed: {}", e));
        return;
    }

    match store_key_from_google_restore(&nsec_hex, None).await {
        Ok(()) => {
            *GOOGLE_BACKUP_STATE.write() =
                cloud_backup::GoogleBackupState::Done { is_new_account: false };
        }
        Err(e) => {
            *GOOGLE_BACKUP_STATE.write() =
                cloud_backup::GoogleBackupState::Error(format!("Failed to store key: {}", e));
        }
    }
}

#[cfg(any(target_family = "wasm", feature = "mobile_platform"))]
pub async fn create_key_with_google() {
    use crate::services::cloud_backup;


    let state = GOOGLE_BACKUP_STATE.read().clone();
    let auth = match state {
        cloud_backup::GoogleBackupState::NoBackup(auth)
        | cloud_backup::GoogleBackupState::Choose { auth, .. } => auth,
        _ => {
            *GOOGLE_BACKUP_STATE.write() =
                cloud_backup::GoogleBackupState::Error("Invalid state".to_string());
            return;
        }
    };

    let (words, _keys) = match cloud_backup::crypto::generate_mnemonic_and_keys() {
        Ok(r) => r,
        Err(e) => {
            *GOOGLE_BACKUP_STATE.write() =
                cloud_backup::GoogleBackupState::Error(format!("Mnemonic generation failed: {}", e));
            return;
        }
    };

    *GOOGLE_BACKUP_STATE.write() = cloud_backup::GoogleBackupState::ShowMnemonic {
        auth,
        words,
        acknowledged: false,
    };
}

#[cfg(any(target_family = "wasm", feature = "mobile_platform"))]
pub async fn confirm_mnemonic_and_create() {
    use crate::services::cloud_backup;


    let state = GOOGLE_BACKUP_STATE.read().clone();
    let (auth, words) = match state {
        cloud_backup::GoogleBackupState::ShowMnemonic { auth, words, .. } => (auth, words),
        _ => {
            *GOOGLE_BACKUP_STATE.write() =
                cloud_backup::GoogleBackupState::Error("Invalid state".to_string());
            return;
        }
    };

    *GOOGLE_BACKUP_STATE.write() = cloud_backup::GoogleBackupState::Working;

    let keys = match nostr::Keys::from_mnemonic(&words, None) {
        Ok(k) => k,
        Err(e) => {
            *GOOGLE_BACKUP_STATE.write() =
                cloud_backup::GoogleBackupState::Error(format!("Key derivation failed: {}", e));
            return;
        }
    };
    let nsec_hex = keys.secret_key().to_secret_hex();

    if let Err(e) = cloud_backup::backup_to_cloud(&nsec_hex, None, None, &auth).await {
        *GOOGLE_BACKUP_STATE.write() =
            cloud_backup::GoogleBackupState::Error(format!("Upload failed: {}", e));
        return;
    }

    match store_key_from_google_restore(&nsec_hex, None).await {
        Ok(()) => {
            *GOOGLE_BACKUP_STATE.write() =
                cloud_backup::GoogleBackupState::Done { is_new_account: true };
        }
        Err(e) => {
            *GOOGLE_BACKUP_STATE.write() =
                cloud_backup::GoogleBackupState::Error(format!("Failed to store key: {}", e));
        }
    }
}

#[cfg(any(target_family = "wasm", feature = "mobile_platform"))]
pub fn reset_google_backup_state() {

    *GOOGLE_BACKUP_STATE.write() = crate::services::cloud_backup::GoogleBackupState::Idle;
}

#[cfg(any(target_family = "wasm", feature = "mobile_platform"))]
pub async fn backup_current_account_to_cloud(
    auth: &crate::services::cloud_backup::GoogleAuthResult,
) -> Result<(), String> {
    use crate::services::cloud_backup;

    let keys = get_keys().ok_or("Not logged in with private key")?;
    let nsec_hex = keys.secret_key().to_secret_hex();
    let nwc_uri = crate::stores::nwc_store::current_nwc_uri();
    let label = AUTH_STATE
        .read()
        .pubkey
        .as_ref()
        .and_then(|p| {
            let cache = crate::stores::profiles::PROFILE_CACHE.read();
            cache.peek(p).and_then(|prof| {
                prof.display_name
                    .clone()
                    .or(prof.name.clone())
            })
        });

    cloud_backup::backup_to_cloud(
        &nsec_hex,
        nwc_uri.as_deref(),
        label.as_deref(),
        auth,
    )
    .await
}

#[cfg(any(target_family = "wasm", feature = "mobile_platform"))]
async fn store_key_from_google_restore(
    nsec_hex: &str,
    nwc_uri: Option<&str>,
) -> Result<(), String> {
    let secret_key = nostr::SecretKey::from_hex(nsec_hex).map_err(|e| e.to_string())?;
    let keys = nostr::Keys::new(secret_key);
    let auto_password = crate::services::cloud_backup::crypto::generate_auto_password();

    let ncryptsec = crate::utils::nip49::encrypt_secret_key(keys.secret_key(), &auto_password)
        .map_err(|e| format!("Encryption failed: {}", e))?;

    let pubkey_str = keys.public_key().to_string();
    crate::platform::storage::set(STORAGE_KEY_NCRYPTSEC, &ncryptsec)?;
    crate::platform::storage::set(STORAGE_KEY_AUTO_PASSWORD, &auto_password)?;
    crate::platform::storage::set(STORAGE_KEY_GOOGLE_BACKUP_USER, "true")?;
    crate::platform::storage::set(STORAGE_KEY_NPUB, &pubkey_str)?;
    crate::platform::storage::set(STORAGE_KEY_METHOD, "private_key")?;
    crate::platform::storage::delete(STORAGE_KEY_NSEC)?;

    if let Some(uri) = nwc_uri {
        let _ = crate::stores::nwc_store::save_nwc_uri_secure(uri);
    }

    *KEYS.write() = Some(keys.clone());
    let signer = SignerType::Keys(keys);
    let pubkey = PublicKey::parse(&pubkey_str).map_err(|e| e.to_string())?;
    set_signer_with_pubkey(signer.clone(), pubkey).await?;
    nostr_client::set_signer(signer).await?;
    *AUTH_STATE.write() = AuthState {
        pubkey: Some(pubkey_str),
        is_authenticated: true,
        login_method: Some(LoginMethod::PrivateKey),
    };
    run_post_login_init();
    Ok(())
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
    let pubkey = keys.public_key();
    let pubkey_str = pubkey.to_string();
    let ncryptsec = crate::utils::nip49::encrypt_secret_key(keys.secret_key(), password)
        .map_err(|e| format!("Encryption failed: {}", e))?;
    *KEYS.write() = Some(keys.clone());
    let signer = SignerType::Keys(keys);
    set_signer_with_pubkey(signer.clone(), pubkey).await?;
    nostr_client::set_signer(signer).await?;
    *AUTH_STATE.write() = AuthState {
        pubkey: Some(pubkey_str.clone()),
        is_authenticated: true,
        login_method: Some(LoginMethod::PrivateKey),
    };
    crate::platform::storage::set(STORAGE_KEY_NCRYPTSEC, &ncryptsec)?;
    crate::platform::storage::set(STORAGE_KEY_NPUB, &pubkey_str)?;
    crate::platform::storage::set(STORAGE_KEY_METHOD, "private_key")?;
    crate::platform::storage::delete(STORAGE_KEY_NSEC)?;
    log::info!(
        "Successfully logged in with encrypted key, pubkey: {}",
        pubkey
    );
    run_post_login_init();
    Ok(())
}
/// Login with private key without password (internal use only for session restore)
async fn login_with_keys_internal(keys: Keys) -> Result<(), String> {
    let pubkey = keys.public_key();
    let pubkey_str = pubkey.to_string();
    *KEYS.write() = Some(keys.clone());
    let signer = SignerType::Keys(keys);
    set_signer_with_pubkey(signer.clone(), pubkey).await?;
    nostr_client::set_signer(signer).await?;
    *AUTH_STATE.write() = AuthState {
        pubkey: Some(pubkey_str.clone()),
        is_authenticated: true,
        login_method: Some(LoginMethod::PrivateKey),
    };
    log::info!("Session restored with pubkey: {}", pubkey_str);
    run_post_login_init();
    Ok(())
}
/// Login with public key only (read-only mode)
pub async fn login_with_npub(npub: &str) -> Result<(), String> {
    log::info!("Logging in with public key (read-only)...");
    let pubkey = PublicKey::parse(npub).map_err(|e| format!("Invalid public key: {}", e))?;
    let pubkey_str = pubkey.to_string();
    nostr_client::set_read_only().await?;
    *AUTH_STATE.write() = AuthState {
        pubkey: Some(pubkey_str.clone()),
        is_authenticated: false,
        login_method: Some(LoginMethod::ReadOnly),
    };
    crate::platform::storage::set(STORAGE_KEY_NPUB, &pubkey_str)?;
    crate::platform::storage::set(STORAGE_KEY_METHOD, "read_only")?;
    log::info!("Loaded read-only mode with pubkey: {}", pubkey_str);
    if let Some(client) = nostr_client::get_client() {
        match client.database().contacts(pubkey).await {
            Ok(db_contacts) => {
                let mut inserted = 0u32;
                crate::stores::profiles::PROFILE_CACHE.with_mut(|cache| {
                    for contact in &db_contacts {
                        let pk_hex = contact.public_key().to_hex();
                        if cache.peek(&pk_hex).is_some() {
                            continue;
                        }
                        let metadata = contact.metadata();
                        let profile = crate::stores::profiles::metadata_to_profile(
                            pk_hex.clone(),
                            &metadata,
                        );
                        cache.put(pk_hex, profile);
                        inserted += 1;
                    }
                });
                log::info!(
                    "Read-only: loaded {}/{} profiles from SDK database",
                    inserted,
                    db_contacts.len()
                );
            }
            Err(e) => {
                log::debug!("Read-only: no contacts in SDK database ({})", e);
            }
        }
    }
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
        set_signer_with_pubkey(signer.clone(), pubkey).await?;
        nostr_client::set_signer(signer).await?;
        *AUTH_STATE.write() = AuthState {
            pubkey: Some(pubkey_str.clone()),
            is_authenticated: true,
            login_method: Some(LoginMethod::BrowserExtension),
        };
        crate::platform::storage::set(STORAGE_KEY_METHOD, "extension")?;
        crate::platform::storage::set(STORAGE_KEY_NPUB, &pubkey_str)?;
        log::info!(
            "Successfully logged in via browser extension with pubkey: {}",
            pubkey_str
        );
        run_post_login_init();
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
) -> Result<(NostrConnect, PublicKey), String> {
    let uri = NostrConnectURI::parse(bunker_uri)
        .map_err(|e| format!("Invalid stored bunker URI: {}", e))?;
    let app_keys =
        Keys::parse(app_keys_str).map_err(|e| format!("Invalid stored app keys: {}", e))?;
    let timeout = Duration::from_secs(120);
    let nostr_connect = NostrConnect::new(uri, app_keys, timeout, None)
        .map_err(|e| format!("Failed to reconnect: {}", e))?;
    use nostr::signer::NostrSigner;
    let public_key = nostr_connect
        .get_public_key()
        .await
        .map_err(|e| format!("Remote signer not responding: {}", e))?;
    Ok((nostr_connect, public_key))
}
/// Run post-login initialization steps (notifications, subscriptions, emoji fetch)
/// This should be called after any successful login or session restoration
fn run_post_login_init() {
    dioxus_core::spawn_forever(async move {
        log::info!("Running post-login initialization...");
        crate::stores::notifications::load_checked_at();

        let Some(pubkey_str) = get_pubkey() else { return; };
        let Ok(pk) = PublicKey::from_hex(&pubkey_str) else { return; };

        // Track A (profile warming — Phase 1: DB-only, instant paint):
        // `warmup_profiles_from_db` reads contacts + metadata from the SDK
        // local database without touching the network. It provides instant
        // profile data for any follows already cached from prior sessions.
        // `spawn_forever` pins the task to `ScopeId::ROOT` so it isn't
        // cancelled when the outer scope ends.
        dioxus_core::spawn_forever(async move {
            warmup_profiles_from_db(pk).await;
        });

        // RELAY SETUP (sequential — eliminates the race that existed when this
        // was a fire-and-forget spawn_forever in set_signer racing with a
        // wait_for_user_relays poll). Everything here must complete before the
        // NIP-78 loaders run.
        let client = match crate::stores::nostr_client::get_client() {
            Some(c) => c,
            None => {
                log::error!("run_post_login_init: no client, aborting");
                return;
            }
        };
        crate::stores::relay::apply_local_relays_to_client(client.clone()).await;

        // SEED relay lists from disk (localStorage mirror + SDK DB) BEFORE the
        // first connect(). This eliminates the USER_RELAYS_APPLIED race (the
        // architectural root cause of issue #351): the user's relays are in the
        // pool and the gate can flip synchronously, rather than after a network
        // round-trip. The mirror is the durable tier (survives the SDK DB's 50k
        // eviction cap); the DB is supplemental.
        //
        // Gift-wrapped lists (indexer/proxy/trusted) are NOT seeded here —
        // unwrapping risks a disruptive NIP-07 prompt at boot. Indexers get the
        // defaults via `add_indexer_relays_to_client` below (which falls back to
        // DEFAULT_INDEXER_RELAYS); custom lists load later via
        // `init_private_relay_lists` + the reconciliation re-call below.
        let seeded =
            crate::stores::relay::persistence::collect_relay_lists_from_disk(&client, pk).await;
        crate::stores::relay::persistence::apply_seeded_relays_to_pool(client.clone(), &seeded)
            .await;

        // Add indexer relays as early as possible — before the first connect()
        // — so they connect in the FIRST wave (in parallel with the default
        // relays) rather than after the NIP-65 round-trip. `pool.connect()`
        // connects every pool member regardless of flags, and indexers are
        // DISCOVERY-only so they stay invisible to broadcast subscriptions.
        // This makes metadata fetches (`fetch_events_from_indexers`) usable
        // within ~1-3s on a cold start instead of after the NIP-65 fetch, and
        // widens the relay set the connect-poll below can succeed on.
        crate::stores::relay::nip65::add_indexer_relays_to_client(client.clone()).await;

        // Write seeded relay lists to signals synchronously (no awaits between
        // writes — Dioxus WritableRef must not be held across a yield point).
        let user_metadata_seeded =
            crate::stores::relay::persistence::write_seeded_relay_lists_to_signals(&seeded);

        // Connect and poll for at least one connected relay before fetching
        // NIP-65. On WASM, WebSocket TLS handshakes can take 3-5s on a cold
        // start. This poll loop keeps checking every 200ms for up to 15s.
        // (Also connects the indexer relays added above.)
        client.connect().await;
        let connect_start = instant::Instant::now();
        loop {
            let relays = client.relays().await;
            if relays.values().any(|r| r.is_connected()) {
                log::info!(
                    "Relay connected after {}ms, proceeding with NIP-65 fetch",
                    connect_start.elapsed().as_millis()
                );
                if !*crate::stores::relay::RELAY_CONNECTED.peek() {
                    *crate::stores::relay::RELAY_CONNECTED.write() = true;
                }
                break;
            }
            if connect_start.elapsed() > std::time::Duration::from_secs(15) {
                log::warn!("No relays connected after 15s, proceeding anyway");
                break;
            }
            crate::stores::nostr_client::platform_sleep_ms(200).await;
        }

        // Flip the readiness gate synchronously when the disk seed provided the
        // user's relay list. This unblocks every `wait_for_user_relays` caller
        // (44 sites) and `USER_RELAYS_APPLIED` reader (11 sites) immediately,
        // rather than after the NIP-65 network fetch below. The network fetch
        // still runs as a refresh (last-write-wins via the existing
        // reconciliation in fetch_own_lists_from_indexers).
        if user_metadata_seeded && !*crate::stores::relay::USER_RELAYS_APPLIED.peek() {
            *crate::stores::relay::USER_RELAYS_APPLIED.write() = true;
            log::info!(
                "User relays seeded from disk — USER_RELAYS_APPLIED flipped early (no race)"
            );
        }

        // Fetch the user's NIP-65 relay list from connected relays.
        // This is now a NETWORK REFRESH — the disk seed already populated the
        // pool and signals. It overwrites via last-write-wins reconciliation.
        if let Err(e) = crate::stores::relay::init_user_relay_lists(client.clone()).await {
            log::warn!("Failed to load user relay lists: {}", e);
        }
        crate::stores::relay::persistence::persist_public_relay_lists();

        // Connect the user's NIP-65 relays that were just added.
        client.connect().await;

        if !*crate::stores::relay::USER_RELAYS_APPLIED.peek() {
            *crate::stores::relay::USER_RELAYS_APPLIED.write() = true;
        }
        log::info!("User relays applied and connected, feed fetching unblocked");

        // Track A (profile warming — Phase 2+3: network backfill). Now that
        // the user's NIP-65 relays are in the pool, `fetch_contacts` can
        // actually reach the user's kind 3 and `fetch_profiles_batch_native`
        // can batch-fetch all follows' metadata from the indexers. This MUST
        // run after `USER_RELAYS_APPLIED` — previously it raced relay setup
        // and silently returned empty when the kind 3 wasn't on the default
        // relays, causing follow metadata to never load.
        {
            let pubkey_str_clone = pubkey_str.clone();
            dioxus_core::spawn_forever(async move {
                warmup_profiles_from_network(&pubkey_str_clone).await;
            });
        }

        // Post-relay-setup init (was previously in set_signer's spawn_forever).
        if let Err(e) = crate::stores::relay::init_nip51_relay_lists(client.clone()).await {
            log::warn!("Failed to load NIP-51 relay lists: {}", e);
        }
        crate::stores::relay::persistence::persist_public_relay_lists();
        if let Err(e) = crate::stores::relay::init_private_relay_lists(client.clone()).await {
            log::warn!("Failed to load private relay lists: {}", e);
        }
        // Reconcile the pool: init_private_relay_lists may have populated
        // INDEXER_RELAYS with the user's custom kind 10086 list. Re-call
        // add_indexer_relays_to_client to add those custom indexers (the
        // defaults are already in the pool from the pre-connect call above;
        // add_discovery_relay is idempotent for relays already present).
        crate::stores::relay::nip65::add_indexer_relays_to_client(client.clone()).await;
        crate::stores::relay::pool::remove_blocked_relays_from_pool(&client).await;
        crate::stores::relay::nip65::fetch_own_lists_from_indexers(client.clone()).await;

        // NIP-78 LOADERS — now guaranteed to run AFTER user relays are
        // connected and USER_RELAYS_APPLIED is true.
        // Run all NIP-78 loads in parallel now that the relay pool is correct.
        // Mostro + Cashu terms checks are included here (moved from main.rs's
        // outer `futures::join!`) so they too benefit from `wait_for_user_relays`
        // gating. Previously they raced relay-pool population and could return
        // false negatives on NIP-46/55 logins, forcing a terms re-prompt.
        //
        // The unified blob loaders (`user_prefs::load_user_prefs` and
        // `load_mostro_prefs`) are Phase 1 dual-read: they try the unified
        // d-tag first, falling back to the legacy per-store loaders below.
        // Until Phase 2 starts writing unified blobs, they return Not Found
        // and the legacy loaders handle everything.
        futures::join!(
            crate::stores::notifications::fetch_and_merge_from_nip78(),
            async {
                if let Err(e) = crate::stores::blossom_store::fetch_user_servers().await {
                    log::warn!("Failed to fetch Blossom servers: {}", e);
                }
            },
            crate::stores::sidebar_store::load_sidebar_preferences(),
            crate::stores::reactions_store::load_preferred_reactions(),
            crate::stores::ai_provider_store::sync_provider_state_from_relays(),
            async {
                if let Err(e) = crate::stores::settings_store::load_settings().await {
                    log::warn!("Failed to load settings: {}", e);
                }
            },
            async {
                if let Err(e) = crate::stores::ui::p2p_settings::load_settings().await {
                    log::warn!("Failed to load P2P settings: {e}");
                }
            },
            async {
                if let Err(e) = crate::stores::mostro::nip78::check_p2p_terms_accepted().await {
                    log::warn!("Failed to check Mostro terms: {}", e);
                }
            },
            async {
                #[cfg(feature = "cashu")]
                {
                    let settings = crate::stores::settings_store::SETTINGS.read().clone();
                    if settings.cashu_wallet_auto_load {
                        match crate::stores::cashu::check_terms_accepted().await {
                            Ok(true) => {
                                log::info!("Auto-loading Cashu wallet...");
                                if let Err(e) = crate::stores::cashu::init_wallet().await {
                                    log::warn!("Failed to auto-load Cashu wallet: {}", e);
                                }
                            }
                            Ok(false) => {
                                log::debug!(
                                    "Cashu terms not yet accepted, skipping auto-load"
                                );
                            }
                            Err(e) => {
                                log::warn!("Failed to check Cashu terms: {}", e);
                            }
                        }
                    }
                }
                #[cfg(not(feature = "cashu"))]
                {
                    // Cashu feature disabled at compile time; nothing to do.
                }
            },
            async {
                let _ = crate::stores::user_prefs::load::load_user_prefs().await;
            },
            async {
                let _ = crate::stores::user_prefs::load::load_mostro_prefs().await;
            },
        );
        crate::stores::notifications::start_realtime_subscription().await;
        crate::stores::relay::start_relay_list_subscription().await;
        crate::stores::emoji_store::init_emoji_fetch();
        crate::stores::mostro::client::start_background_trade_monitor().await;
        // E6 invariant: `mostro::init_node_config_from_cache()` and
        // `mostro::init()` both run synchronously in `main.rs:66-67`
        // (well before this function), so the monitor can rely on
        // `try_get_node_config()` returning Some on the first poll.
        // The monitor's `dispute_status_filter` (client.rs:1720) and
        // `build_trade_key_map` (client.rs:434) both depend on this.
        dioxus_core::spawn_forever(async move {
            crate::stores::mostro::cleanup::run_all_cleanup_loops().await;
        });

        // B2: hydrate persisted Mostro notifications from local cache (for
        // instant availability on app boot), then refresh from relays in
        // the background (best-effort cross-device sync).
        crate::stores::mostro::notification_store::init_from_cache();
        dioxus_core::spawn_forever(async move {
            let _ = crate::stores::mostro::notification_store::refresh_from_relays().await;
        });

        // Phase 10.5: register push tokens for all active trades so the
        // Mostro push server can wake the user's device when trade events
        // arrive while the app is closed. Best-effort — silently fails
        // if the push server is unreachable.
        crate::services::mostro_push::register_all_active_trades().await;
    });
}

/// Phase 1: stream DB-warm profiles into PROFILE_CACHE. `contacts()` issues
/// a kind 3 DB query plus a kind 0 DB query for the contact pubkeys and
/// returns a `BTreeSet<Profile>` (pubkey + metadata) without touching the
/// network. We insert each as the iterator yields so early NoteCards can
/// react before the full set has been iterated.
///
/// This runs early (spawned from the top of `run_post_login_init`, before
/// relay setup) because `client.database().contacts()` is a LOCAL-only
/// operation (SDK `NostrDatabaseExt::contacts` — never touches relays).
/// It provides instant paint for any profiles already in the SDK database
/// from prior sessions or prior subscriptions.
async fn warmup_profiles_from_db(pk: PublicKey) {
    let Some(client) = nostr_client::get_client() else {
        return;
    };
    match client.database().contacts(pk).await {
        Ok(db_contacts) => {
            let count = db_contacts.len();
            if count > 0 {
                let mut inserted = 0u32;
                crate::stores::profiles::PROFILE_CACHE.with_mut(|cache| {
                    for profile in &db_contacts {
                        let pk_hex = profile.public_key().to_hex();
                        if cache.peek(&pk_hex).is_some() {
                            continue;
                        }
                        let p = crate::stores::profiles::metadata_to_profile(
                            pk_hex.clone(),
                            &profile.metadata(),
                        );
                        cache.put(pk_hex, p);
                        inserted += 1;
                    }
                });
                if inserted > 0 {
                    log::info!(
                        "Loaded {inserted}/{count} followed profiles into PROFILE_CACHE from SDK database"
                    );
                    crate::stores::profiles::bump_cache_version();
                }
                // Pre-populate CONTACTS_CACHE so is_following is instant on
                // the first profile view. Without this, the first
                // fetch_contacts for the current user blocks up to 5s.
                let contact_pubkeys: Vec<String> = db_contacts
                    .iter()
                    .map(|p| p.public_key().to_hex())
                    .collect();
                nostr_client::warm_contacts_cache(&pk.to_hex(), contact_pubkeys);
            }
        }
        Err(e) => {
            log::warn!("Failed to load contacts from SDK database: {}", e);
        }
    }
}

/// Phase 2+3: network backfill for follow metadata. MUST be called after
/// `USER_RELAYS_APPLIED` is true — otherwise `fetch_contacts` targets only
/// the default relays (which rarely have the user's kind 3), returns empty,
/// and the entire metadata warmup is skipped.
///
/// A single `fetch_contacts` (OnceCell-deduped with any concurrent caller)
/// feeds the batched `fetch_profiles_batch_native` for the missing authors.
/// `fetch_profiles_batch_native` internally re-checks the cache and the local
/// DB before issuing the relay REQ, so profiles already loaded by Phase 1
/// are skipped.
async fn warmup_profiles_from_network(pubkey_str: &str) {
    // Defense-in-depth: ensure user relays are applied before fetching
    // contacts. `run_post_login_init` calls this AFTER setting
    // `USER_RELAYS_APPLIED`, but the gate is cheap and protects against
    // future refactor regressions.
    crate::stores::relay::wait_for_user_relays(
        std::time::Duration::from_secs(10),
        "warmup_profiles_from_network",
    )
    .await;

    let pubkeys = match nostr_client::fetch_contacts(pubkey_str.to_string()).await {
        Ok(p) => p,
        Err(e) => {
            log::warn!("Cannot warm profiles, no contacts: {e}");
            return;
        }
    };
    let contact_pubkeys: std::collections::HashSet<PublicKey> = pubkeys
        .into_iter()
        .filter_map(|p| PublicKey::from_hex(&p).ok())
        .collect();
    if contact_pubkeys.is_empty() {
        log::info!("warmup_profiles_from_network: no contacts to warm");
        return;
    }
    // Wait for an indexer to actually connect before fetching. The indexers
    // are added at boot and connect via `pool.connect()` (boot + login), but on
    // a cold WASM start the TLS handshakes take 3-5s each. Fetching before any
    // indexer is connected yields nothing (and `fetch_events_from_indexers`
    // returns Err so those pubkeys stay retryable instead of being exhausted).
    if let Some(client) = nostr_client::get_client() {
        crate::stores::relay::nip65::wait_for_indexer_connected(
            &client,
            std::time::Duration::from_secs(15),
        )
        .await;
    }
    match crate::stores::profiles::fetch_profiles_batch_native(contact_pubkeys).await {
        Ok(loaded) => {
            log::info!("warmup_profiles_from_network: loaded {} profiles", loaded.len());
        }
        Err(e) => log::warn!("Profile warmup failed: {e}"),
    }
    crate::stores::profiles::bump_cache_version();

    // Prefetch relay lists for all followed users to warm the coverage map.
    crate::stores::relay::coverage::prefetch_relay_lists_for_follows().await;

    // Start the periodic profile sweep safety net (catches profiles missed
    // by the event-driven queue due to races, timeouts, or component
    // unmounts). Modelled after Wisp's sweepMissingProfiles which runs at
    // 5s/15s/30s/120s after startup.
    crate::stores::profiles::start_profile_sweep();
}
/// Login with NIP-46 remote signer (nostr-connect)
pub async fn login_with_nostr_connect(bunker_uri: &str) -> Result<(), String> {
    log::info!("Logging in with remote signer (NIP-46)...");
    let uri =
        NostrConnectURI::parse(bunker_uri).map_err(|e| format!("Invalid bunker URI: {}", e))?;
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
    set_signer_with_pubkey(signer_type.clone(), public_key).await?;
    nostr_client::set_signer(signer_type).await?;
    *AUTH_STATE.write() = AuthState {
        pubkey: Some(pubkey_str.clone()),
        is_authenticated: true,
        login_method: Some(LoginMethod::RemoteSigner),
    };
    log::info!(
        "Successfully logged in via remote signer with pubkey: {}",
        pubkey_str
    );
    run_post_login_init();
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
        debug_assert!(
            !s.starts_with("npub"),
            "get_pubkey returned bech32 instead of hex"
        );
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
#[cfg(any(target_family = "wasm", feature = "mobile_platform"))]
pub fn is_google_backup_user() -> bool {
    crate::platform::storage::get::<String>(STORAGE_KEY_GOOGLE_BACKUP_USER).is_ok()
}
/// Logout and clear credentials.
///
/// Returns an error if local AI chat history could not be cleared so the caller can
/// keep the user signed in and surface the failure.
pub async fn logout() -> Result<(), String> {
    log::info!("Logging out...");
    let ai_chat_account_key = crate::stores::ai_chat_store::current_account_key();
    crate::stores::ai_chat_store::clear_chat_state(&ai_chat_account_key)
        .await
        .map_err(|e| format!("Failed to clear AI chat history during logout: {}", e))?;
    crate::stores::notifications::stop_realtime_subscription().await;
    crate::stores::relay::stop_relay_list_subscription().await;
    // Flush any pending unified blob saves before clearing auth state.
    crate::stores::user_prefs::sidecar::flush_all().await;
    #[cfg(feature = "cashu")]
    crate::stores::cashu_cdk_bridge::clear_multi_wallet();
    crate::stores::shop_store::clear_caches();
    crate::stores::dms::clear_caches();
    crate::stores::shop_store::reset_orders_loaded_flag();
    #[cfg(feature = "cashu")]
    {
        crate::stores::cashu::internal::clear_seed_cache();
        crate::stores::cashu::internal::clear_nip44_decrypt_cache();
    }
    spawn(async move {
        crate::services::search_relays::invalidate_search_relay_cache().await;
    });
    // Clear per-pubkey Cashu terms cache before we lose the pubkey reference.
    if let Some(ref pk) = AUTH_STATE.read().pubkey {
        let cashu_cache_key = format!("cashu_terms_accepted/{pk}");
        let _ = crate::platform::storage::delete(&cashu_cache_key);
    }
    nostr_client::set_read_only()
        .await
        .map_err(|e| format!("Failed to set client to read-only during logout: {}", e))?;
    for (storage_key, label) in [
        (STORAGE_KEY_NSEC, "nsec"),
        (STORAGE_KEY_NCRYPTSEC, "ncryptsec"),
        (STORAGE_KEY_NPUB, "npub"),
        (STORAGE_KEY_METHOD, "method"),
        (STORAGE_KEY_BUNKER_URI, "bunker URI"),
        (STORAGE_KEY_APP_KEYS, "app keys"),
        (STORAGE_KEY_SIGNER_PACKAGE, "signer package"),
        (STORAGE_KEY_AUTO_PASSWORD, "auto password"),
        (STORAGE_KEY_GOOGLE_BACKUP_USER, "google backup user"),
    ] {
        crate::platform::storage::delete(storage_key)
            .map_err(|e| format!("Failed to delete {} during logout: {}", label, e))?;
    }
    crate::stores::nwc_store::disconnect_nwc(false);
    // Clear global NIP-78 preference caches so the next login doesn't briefly
    // show the previous user's sidebar/reactions/settings. These keys are
    // global (not per-account); proper per-account namespacing arrives with
    // the unified prefs blob refactor (Phase 0).
    for nip78_cache_key in [
        "nostr_blue_settings",
        "nostr_blue_sidebar_prefs",
        "nostr_blue_reaction_prefs",
        "nostr_blue_p2p_settings",
        "nostr_blue_ai_provider_state",
        "nostr_blue_blossom_servers",
    ] {
        let _ = crate::platform::storage::delete(nip78_cache_key);
    }
    // Reset NIP-78 GlobalSignals to defaults so the UI doesn't flash stale
    // data between logout and the next login's cache load.
    *crate::stores::settings_store::SETTINGS.write() =
        crate::stores::settings_store::AppSettings::default();
    *crate::stores::settings_store::SETTINGS_STATE.write() =
        crate::stores::ui::sidebar_store::Nip78LoadState::default();
    *crate::stores::sidebar_store::SIDEBAR_STATE.write() =
        crate::stores::ui::sidebar_store::Nip78LoadState::default();
    *crate::stores::reactions_store::REACTIONS_STATE.write() =
        crate::stores::ui::sidebar_store::Nip78LoadState::default();
    *crate::stores::ui::p2p_settings::MOSTRO_SETTINGS_STATE.write() =
        crate::stores::ui::sidebar_store::Nip78LoadState::default();
    clear_auth();
    crate::stores::mostro::reset_all();
    #[cfg(any(target_family = "wasm", feature = "mobile_platform"))]
    {
        *GOOGLE_BACKUP_STATE.write() = crate::services::cloud_backup::GoogleBackupState::Idle;
    }
    *PASSWORD_PROMPT.write() = PasswordPromptState::default();
    Ok(())
}
/// Clear authentication state
fn clear_auth() {
    *AUTH_STATE.write() = AuthState::default();
    *KEYS.write() = None;
    crate::stores::ai_provider_store::clear_relay_state();
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
        crate::platform::storage::set(STORAGE_KEY_NCRYPTSEC, &ncryptsec)?;
        crate::platform::storage::delete(STORAGE_KEY_NSEC)?;
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
    if let Err(e) = crate::platform::storage::delete(STORAGE_KEY_NCRYPTSEC) {
        log::error!("Failed to delete ncryptsec: {}", e);
    }
    if let Err(e) = crate::platform::storage::delete(STORAGE_KEY_NSEC) {
        log::error!("Failed to delete nsec: {}", e);
    }
    if let Err(e) = crate::platform::storage::delete(STORAGE_KEY_NPUB) {
        log::error!("Failed to delete npub: {}", e);
    }
    if let Err(e) = crate::platform::storage::delete(STORAGE_KEY_METHOD) {
        log::error!("Failed to delete method: {}", e);
    }
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
/// Result of the auto-detection flow for Android signer login.
#[derive(Debug, Clone)]
#[allow(dead_code)] // Variants used on mobile but not on desktop/WASM targets
pub enum AndroidSignerAutoResult {
    /// Successfully logged in; contains the signer package name.
    LoggedIn(String),
    /// An error occurred during auto-detection.
    Error(String),
}

/// Check if an Android signer (NIP-55) is available
pub fn is_android_signer_available() -> bool {
    #[cfg(feature = "mobile_platform")]
    {
        crate::platform::Nip55Signer::is_signer_installed()
    }
    #[cfg(not(feature = "mobile_platform"))]
    {
        false
    }
}
/// Login with Android signer (NIP-55)
///
/// `npub` can be bech32 npub or hex public key.
/// `signer_package` overrides the default Amber package if provided.
#[allow(dead_code)] // Called on mobile but not on desktop/WASM targets
pub async fn login_with_android_signer(
    npub: &str,
    signer_package: Option<&str>,
) -> Result<(), String> {
    #[cfg(feature = "mobile_platform")]
    {
        use crate::platform::Nip55Signer;

        log::info!("Attempting Android signer login...");

        let public_key =
            PublicKey::parse(npub).map_err(|e| format!("Invalid public key: {}", e))?;
        let pubkey_hex = public_key.to_hex();

        let package = signer_package
            .unwrap_or_else(|| Nip55Signer::default_package())
            .to_string();

        let signer = Nip55Signer::new(public_key, package.clone());
        let signer_type = SignerType::AndroidSigner(Arc::new(signer));

        set_signer_with_pubkey(signer_type.clone(), public_key).await?;
        nostr_client::set_signer(signer_type).await?;

        *AUTH_STATE.write() = AuthState {
            pubkey: Some(pubkey_hex.clone()),
            is_authenticated: true,
            login_method: Some(LoginMethod::AndroidSigner),
        };

        crate::platform::storage::set(STORAGE_KEY_NPUB, &pubkey_hex)?;
        crate::platform::storage::set(STORAGE_KEY_METHOD, "android_signer")?;
        crate::platform::storage::set(STORAGE_KEY_SIGNER_PACKAGE, &package)?;

        log::info!(
            "Successfully logged in via Android signer with pubkey: {}",
            pubkey_hex
        );
        run_post_login_init();
        Ok(())
    }
    #[cfg(not(feature = "mobile_platform"))]
    {
        let _ = (npub, signer_package);
        Err("Android signer is only available on mobile".to_string())
    }
}

/// Auto-detect and login with Android signer (NIP-55)
///
/// Implements a 3-phase detection flow with automatic polling:
/// 1. **Poll**: Check for pending Intent result from a previous launch
/// 2. **ContentResolver**: Try to get pubkey from already-approved signers
/// 3. **Intent**: Launch get_public_key Intent, then auto-poll until the user
///    approves in the signer app and returns. No manual confirmation needed.
#[cfg(feature = "mobile_platform")]
pub async fn login_with_android_signer_auto() -> Result<AndroidSignerAutoResult, String> {
    use crate::platform::{IntentPollResult, Nip55Signer};

    async fn poll_until_login() -> Result<AndroidSignerAutoResult, String> {
        use crate::platform::{IntentPollResult, Nip55Signer};

        const LOGIN_POLL_INTERVAL: Duration = Duration::from_millis(500);
        const LOGIN_POLL_TIMEOUT: Duration = Duration::from_secs(120);

        let start = std::time::Instant::now();
        loop {
            crate::platform::timer::sleep(LOGIN_POLL_INTERVAL).await;
            match Nip55Signer::poll_intent_result() {
                IntentPollResult::Ready { pubkey, package } => {
                    let pubkey_hex = pubkey.to_hex();
                    log::info!("NIP-55 auto-poll: got result from {}: {}", package, pubkey_hex);
                    Nip55Signer::clear_pending_result();
                    login_with_android_signer(&pubkey_hex, Some(&package)).await?;
                    return Ok(AndroidSignerAutoResult::LoggedIn(package));
                }
                IntentPollResult::Error(e) => {
                    log::warn!("NIP-55 auto-poll: error: {}", e);
                    Nip55Signer::clear_pending_result();
                    return Ok(AndroidSignerAutoResult::Error(e));
                }
                IntentPollResult::InFlight | IntentPollResult::None => {
                    if start.elapsed() >= LOGIN_POLL_TIMEOUT {
                        log::warn!("NIP-55 auto-poll: timed out after {:?}", LOGIN_POLL_TIMEOUT);
                        Nip55Signer::clear_pending_result();
                        return Ok(AndroidSignerAutoResult::Error(
                            "Timed out waiting for signer approval".to_string(),
                        ));
                    }
                }
            }
        }
    }

    log::info!("NIP-55 auto-detect: starting 3-phase flow");

    // Phase 1: Poll for pending Intent result from previous launch
    log::info!("NIP-55 auto-detect: phase 1 — polling for pending Intent result");
    match Nip55Signer::poll_intent_result() {
        IntentPollResult::Ready { pubkey, package } => {
            let pubkey_hex = pubkey.to_hex();
            log::info!(
                "NIP-55 auto-detect: found pending result from {}: {}",
                package,
                pubkey_hex
            );
            Nip55Signer::clear_pending_result();
            login_with_android_signer(&pubkey_hex, Some(&package)).await?;
            return Ok(AndroidSignerAutoResult::LoggedIn(package));
        }
        IntentPollResult::InFlight => {
            log::info!("NIP-55 auto-detect: Intent still in flight, auto-polling");
            return poll_until_login().await;
        }
        IntentPollResult::Error(e) => {
            log::warn!("NIP-55 auto-detect: previous Intent error: {}", e);
            Nip55Signer::clear_pending_result();
        }
        IntentPollResult::None => {
            log::debug!("NIP-55 auto-detect: no pending Intent result");
        }
    }

    // Phase 2: Try ContentResolver for already-approved signers
    log::info!("NIP-55 auto-detect: phase 2 — trying ContentResolver");
    let packages = Nip55Signer::get_signer_packages();
    if packages.is_empty() {
        return Err("No NIP-55 signer apps installed".to_string());
    }

    for package in &packages {
        log::info!("NIP-55 auto-detect: trying ContentResolver for {}", package);
        if let Some(pubkey) = Nip55Signer::request_public_key(package) {
            let pubkey_hex = pubkey.to_hex();
            log::info!(
                "NIP-55 auto-detect: ContentResolver success from {}: {}",
                package,
                pubkey_hex
            );
            login_with_android_signer(&pubkey_hex, Some(package)).await?;
            return Ok(AndroidSignerAutoResult::LoggedIn(package.clone()));
        }
    }

    // Phase 3: Launch Intent for first-time approval, then auto-poll
    log::info!("NIP-55 auto-detect: phase 3 — launching get_public_key Intent");
    match Nip55Signer::launch_get_public_key() {
        Ok(_) => {
            log::info!("NIP-55 auto-detect: Intent launched, auto-polling for result");
            poll_until_login().await
        }
        Err(e) => {
            log::error!("NIP-55 auto-detect: failed to launch Intent: {}", e);
            Ok(AndroidSignerAutoResult::Error(e))
        }
    }
}

/// Auto-detect and login with Android signer (NIP-55) — non-mobile stub.
#[cfg(not(feature = "mobile_platform"))]
pub async fn login_with_android_signer_auto() -> Result<AndroidSignerAutoResult, String> {
    Err("Android signer is only available on mobile".to_string())
}
