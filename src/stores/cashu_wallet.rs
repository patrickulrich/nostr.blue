use dioxus::prelude::*;
use dioxus::signals::ReadableExt;
use dioxus_stores::Store;
use nostr_sdk::{Event, EventBuilder, Filter, Kind, PublicKey, SecretKey, EventId, Tag};
use nostr_sdk::nips::nip60::{WalletEvent, TransactionDirection, SpendingHistory};
use nostr_sdk::types::url::Url;
use nostr_sdk::types::time::Timestamp;
use crate::stores::{auth_store, cashu_cdk_bridge, nostr_client};
use std::time::Duration;
use serde::{Deserialize, Serialize};
use js_sys;

// CDK database trait for calling methods on IndexedDbDatabase
use cdk::cdk_database::WalletDatabase;

/// Default unit for Cashu proofs (per NIP-60 spec, defaults to "sat")
fn default_unit() -> String {
    "sat".to_string()
}

/// Custom deserialization structure for token events (more lenient than rust-nostr)
/// Includes unit field per NIP-60 spec
#[derive(Debug, Clone, Serialize, Deserialize)]
struct TokenEventData {
    pub mint: String,
    #[serde(default = "default_unit")]
    pub unit: String,
    pub proofs: Vec<ProofData>,
    #[serde(default)]
    pub del: Vec<String>,
}

/// Extended token event with P2PK support (extends rust-nostr's TokenEvent)
/// Uses ExtendedCashuProof instead of CashuProof to preserve witness/DLEQ fields
/// Includes unit field per NIP-60 spec
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ExtendedTokenEvent {
    pub mint: String,
    #[serde(default = "default_unit")]
    pub unit: String,
    pub proofs: Vec<ExtendedCashuProof>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub del: Vec<String>,
}

/// DLEQ proof data (preserves P2PK verification capability)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct DleqData {
    pub e: String,
    pub s: String,
    pub r: String,
}

/// Custom deserialization structure for proofs (allows missing fields)
/// Uses uppercase "C" per NIP-60 spec, with alias for backward compatibility
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProofData {
    #[serde(default)]
    pub id: String,
    pub amount: u64,
    pub secret: String,
    #[serde(default, rename = "C", alias = "c")]
    pub c: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub witness: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dleq: Option<DleqData>,
}

/// Extended Cashu proof with P2PK support (superset of nostr_sdk::nips::nip60::CashuProof)
/// Preserves witness and DLEQ fields for P2PK verification while maintaining NIP-60 compatibility
/// Uses uppercase "C" per NIP-60 spec, with alias for backward compatibility
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ExtendedCashuProof {
    pub id: String,
    pub amount: u64,
    pub secret: String,
    #[serde(rename = "C", alias = "c")]
    pub c: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub witness: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dleq: Option<DleqData>,
}

impl From<ProofData> for ExtendedCashuProof {
    fn from(p: ProofData) -> Self {
        Self {
            id: p.id,
            amount: p.amount,
            secret: p.secret,
            c: p.c,
            witness: p.witness,
            dleq: p.dleq,
        }
    }
}

/// Wallet state containing configuration
#[derive(Clone, Debug, PartialEq)]
pub struct WalletState {
    pub privkey: String,
    pub mints: Vec<String>,
    pub initialized: bool,
}

/// Token data with event metadata
/// Uses ProofData instead of CashuProof to preserve witness/DLEQ for P2PK support
#[derive(Clone, Debug, PartialEq)]
pub struct TokenData {
    pub event_id: String,
    pub mint: String,
    pub unit: String,
    pub proofs: Vec<ProofData>,
    pub created_at: u64,
}

/// Transaction history item with event metadata
#[derive(Clone, Debug, PartialEq)]
pub struct HistoryItem {
    pub event_id: String,
    pub direction: TransactionDirection,
    pub amount: u64,
    pub unit: String,
    pub created_at: u64,
    pub created_tokens: Vec<String>,
    pub destroyed_tokens: Vec<String>,
    pub redeemed_events: Vec<String>,
}

/// Wallet loading status
#[derive(Clone, Debug, PartialEq)]
pub enum WalletStatus {
    Uninitialized,
    Loading,
    Ready,
    Error(String),
}

/// Store for wallet tokens with fine-grained reactivity
#[derive(Clone, Debug, Default, Store)]
pub struct WalletTokensStore {
    pub data: Vec<TokenData>,
}

/// Store for wallet history with fine-grained reactivity
#[derive(Clone, Debug, Default, Store)]
pub struct WalletHistoryStore {
    pub data: Vec<HistoryItem>,
}

/// Global signal for wallet state
pub static WALLET_STATE: GlobalSignal<Option<WalletState>> = Signal::global(|| None);

/// Global signal for tokens
pub static WALLET_TOKENS: GlobalSignal<Store<WalletTokensStore>> =
    Signal::global(|| Store::new(WalletTokensStore::default()));

/// Global signal for transaction history
pub static WALLET_HISTORY: GlobalSignal<Store<WalletHistoryStore>> =
    Signal::global(|| Store::new(WalletHistoryStore::default()));

/// Global signal for total balance
pub static WALLET_BALANCE: GlobalSignal<u64> = Signal::global(|| 0);

/// Global signal for wallet status
pub static WALLET_STATUS: GlobalSignal<WalletStatus> = Signal::global(|| WalletStatus::Uninitialized);

/// Global signal for terms acceptance status
/// None = not yet checked, Some(true) = accepted, Some(false) = not accepted
pub static TERMS_ACCEPTED: GlobalSignal<Option<bool>> = Signal::global(|| None);

/// NIP-78 d-tag identifier for Cashu wallet terms agreement
const TERMS_D_TAG: &str = "nostr.blue/cashu/terms";

/// Operation lock to prevent concurrent wallet operations on the same mint
/// Uses GlobalSignal with HashSet to track mints currently being operated on
pub static MINT_OPERATION_LOCK: GlobalSignal<std::collections::HashSet<String>> =
    Signal::global(|| std::collections::HashSet::new());

/// Cached wallet instances per mint URL
/// Caching avoids repeated IndexedDB connections, seed derivation, and mint info fetches
pub static WALLET_CACHE: GlobalSignal<std::collections::HashMap<String, std::sync::Arc<cdk::Wallet>>> =
    Signal::global(|| std::collections::HashMap::new());

/// Shared IndexedDB database instance for all wallet operations
/// Using a single connection is more efficient than creating one per operation
pub static SHARED_LOCALSTORE: GlobalSignal<Option<std::sync::Arc<crate::stores::indexeddb_database::IndexedDbDatabase>>> =
    Signal::global(|| None);

/// Event type for pending Nostr event publication
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PendingEventType {
    TokenEvent,
    DeletionEvent,
    HistoryEvent,
    QuoteEvent,
}

/// Pending Nostr event awaiting publication
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingNostrEvent {
    pub id: String,
    pub builder_json: String,  // Serialized event data
    pub event_type: PendingEventType,
    pub created_at: u64,
    pub retry_count: u32,
}

/// Global signal for pending Nostr events (offline queue)
pub static PENDING_NOSTR_EVENTS: GlobalSignal<Vec<PendingNostrEvent>> =
    Signal::global(|| Vec::new());

/// Get or create the shared IndexedDB localstore
async fn get_shared_localstore() -> Result<std::sync::Arc<crate::stores::indexeddb_database::IndexedDbDatabase>, String> {
    // Check if we already have a cached localstore
    if let Some(ref store) = *SHARED_LOCALSTORE.read() {
        return Ok(store.clone());
    }

    // Create new localstore
    let localstore = std::sync::Arc::new(
        crate::stores::indexeddb_database::IndexedDbDatabase::new()
            .await
            .map_err(|e| format!("Failed to create IndexedDB: {}", e))?
    );

    // Cache it
    *SHARED_LOCALSTORE.write() = Some(localstore.clone());
    log::info!("Created shared IndexedDB localstore");

    Ok(localstore)
}

/// Get or create a cached wallet for a mint
///
/// Prefers MultiMintWallet when available, falls back to WALLET_CACHE.
async fn get_or_create_wallet(mint_url: &str) -> Result<std::sync::Arc<cdk::Wallet>, String> {
    use cdk::Wallet;
    use cdk::nuts::CurrencyUnit;
    use crate::stores::cashu_cdk_bridge;

    // Try MultiMintWallet first (preferred path)
    if cashu_cdk_bridge::is_initialized() {
        if let Ok(wallet) = cashu_cdk_bridge::get_wallet(mint_url).await {
            log::debug!("Using wallet from MultiMintWallet for {}", mint_url);
            return Ok(std::sync::Arc::new(wallet));
        }

        // Mint not in MultiMintWallet - try to add it
        log::info!("Adding mint {} to MultiMintWallet", mint_url);
        if let Err(e) = cashu_cdk_bridge::add_mint(mint_url).await {
            log::warn!("Failed to add mint to MultiMintWallet: {}", e);
            // Fall through to legacy path
        } else if let Ok(wallet) = cashu_cdk_bridge::get_wallet(mint_url).await {
            log::info!("Successfully added mint {} to MultiMintWallet", mint_url);
            return Ok(std::sync::Arc::new(wallet));
        }
    }

    // Fallback: Check WALLET_CACHE (legacy path)
    if let Some(wallet) = WALLET_CACHE.read().get(mint_url) {
        log::debug!("Using cached wallet for {} (legacy)", mint_url);
        return Ok(wallet.clone());
    }

    // Create new wallet (legacy path)
    log::info!("Creating new wallet for {} (legacy path)", mint_url);

    let localstore = get_shared_localstore().await?;
    let seed = derive_wallet_seed().await?;

    let wallet = Wallet::new(
        mint_url,
        CurrencyUnit::Sat,
        localstore,
        seed,
        None // target_proof_count
    ).map_err(|e| format!("Failed to create wallet: {}", e))?;

    // Fetch mint info and keysets (only done once per wallet)
    wallet.fetch_mint_info().await
        .map_err(|e| format!("Failed to fetch mint info: {}", e))?;

    wallet.refresh_keysets().await
        .map_err(|e| format!("Failed to refresh keysets: {}", e))?;

    // Wrap in Arc and cache
    let wallet = std::sync::Arc::new(wallet);
    WALLET_CACHE.write().insert(mint_url.to_string(), wallet.clone());

    log::info!("Cached new wallet for {} (legacy)", mint_url);
    Ok(wallet)
}

/// Clear the wallet cache for a specific mint (e.g., when mint is removed)
pub fn clear_wallet_cache(mint_url: &str) {
    WALLET_CACHE.write().remove(mint_url);
    log::info!("Cleared wallet cache for {}", mint_url);
}

/// Clear all wallet caches (e.g., on logout)
#[allow(dead_code)]
pub fn clear_all_wallet_caches() {
    WALLET_CACHE.write().clear();
    *SHARED_LOCALSTORE.write() = None;
    log::info!("Cleared all wallet caches");
}

/// Guard that releases the mint lock when dropped
pub struct MintOperationGuard {
    mint_url: String,
}

impl Drop for MintOperationGuard {
    fn drop(&mut self) {
        MINT_OPERATION_LOCK.write().remove(&self.mint_url);
        log::debug!("Released operation lock for mint: {}", self.mint_url);
    }
}

/// Try to acquire an operation lock for a mint
/// Returns None if the mint is already being operated on
fn try_acquire_mint_lock(mint_url: &str) -> Option<MintOperationGuard> {
    let mut locks = MINT_OPERATION_LOCK.write();
    if locks.contains(mint_url) {
        log::warn!("Operation already in progress for mint: {}", mint_url);
        None
    } else {
        locks.insert(mint_url.to_string());
        log::debug!("Acquired operation lock for mint: {}", mint_url);
        Some(MintOperationGuard {
            mint_url: mint_url.to_string(),
        })
    }
}

// Removed: STORAGE_KEY_WALLET_PRIVKEY - wallet privkey is now derived deterministically
// and no longer stored in plaintext in LocalStorage

/// Queue a Nostr event for publication (with offline support)
///
/// Events are saved to IndexedDB for persistence across app restarts.
/// A background task will publish queued events when possible.
#[allow(dead_code)]
async fn queue_nostr_event(
    event_json: String,
    event_type: PendingEventType,
) -> Result<String, String> {
    use uuid::Uuid;

    let event_id = Uuid::new_v4().to_string();
    let created_at = chrono::Utc::now().timestamp() as u64;

    let pending = PendingNostrEvent {
        id: event_id.clone(),
        builder_json: event_json,
        event_type,
        created_at,
        retry_count: 0,
    };

    // Save to in-memory queue
    PENDING_NOSTR_EVENTS.write().push(pending.clone());

    // Persist to IndexedDB for offline support
    if let Ok(localstore) = get_shared_localstore().await {
        if let Err(e) = localstore.add_pending_event(&pending).await {
            log::warn!("Failed to persist pending event to IndexedDB: {}", e);
            // Non-critical: local state is already updated, DB persistence is optional
        }
    }

    // Log the queued event
    log::debug!("Queued {} event: {}",
        match pending.event_type {
            PendingEventType::TokenEvent => "token",
            PendingEventType::DeletionEvent => "deletion",
            PendingEventType::HistoryEvent => "history",
            PendingEventType::QuoteEvent => "quote",
        },
        event_id);

    Ok(event_id)
}

/// Remove a pending event from the queue and IndexedDB
#[allow(dead_code)]
async fn remove_pending_event(event_id: &str) -> Result<(), String> {
    // Remove from in-memory queue
    PENDING_NOSTR_EVENTS.write().retain(|e| e.id != event_id);

    // Remove from IndexedDB
    if let Ok(localstore) = get_shared_localstore().await {
        if let Err(e) = localstore.remove_pending_event(event_id).await {
            log::warn!("Failed to remove pending event from IndexedDB: {}", e);
            // Non-critical: memory state is already updated
        }
    }

    log::debug!("Removed pending event from queue: {}", event_id);
    Ok(())
}

/// Queue an already-signed Event for retry when publication fails
///
/// Used when we've pre-signed an event to get its ID but publication failed.
async fn queue_signed_event_for_retry(event: nostr_sdk::Event, event_type: PendingEventType) {
    match serde_json::to_string(&event) {
        Ok(event_json) => {
            match queue_nostr_event(event_json, event_type).await {
                Ok(queue_id) => {
                    log::info!("Queued signed event {} for retry: {}", event.id.to_hex(), queue_id);
                }
                Err(queue_err) => {
                    log::error!("Failed to queue event for retry: {}", queue_err);
                }
            }
        }
        Err(json_err) => {
            log::error!("Failed to serialize event for queueing: {}", json_err);
        }
    }
}

/// Queue an EventBuilder for retry when initial publication fails
///
/// Signs the event using the current signer and queues for later retry.
pub(crate) async fn queue_event_for_retry(builder: nostr_sdk::EventBuilder, event_type: PendingEventType) {
    let signer = match crate::stores::signer::get_signer() {
        Some(s) => s,
        None => {
            log::error!("Cannot queue failed event: no signer available");
            return;
        }
    };

    let sign_and_queue = |event: nostr_sdk::Event| async move {
        match serde_json::to_string(&event) {
            Ok(event_json) => {
                match queue_nostr_event(event_json, event_type).await {
                    Ok(queue_id) => {
                        log::info!("Queued failed event for retry: {}", queue_id);
                    }
                    Err(queue_err) => {
                        log::error!("Failed to queue event for retry: {}", queue_err);
                    }
                }
            }
            Err(json_err) => {
                log::error!("Failed to serialize event for queueing: {}", json_err);
            }
        }
    };

    match signer {
        crate::stores::signer::SignerType::Keys(keys) => {
            match builder.sign_with_keys(&keys) {
                Ok(event) => sign_and_queue(event).await,
                Err(sign_err) => {
                    log::error!("Failed to sign event for queueing: {}", sign_err);
                }
            }
        }
        #[cfg(target_family = "wasm")]
        crate::stores::signer::SignerType::BrowserExtension(browser_signer) => {
            match builder.sign(&*browser_signer).await {
                Ok(event) => sign_and_queue(event).await,
                Err(sign_err) => {
                    log::error!("Failed to sign event for queueing: {}", sign_err);
                }
            }
        }
        crate::stores::signer::SignerType::NostrConnect(remote_signer) => {
            match builder.sign(&*remote_signer).await {
                Ok(event) => sign_and_queue(event).await,
                Err(sign_err) => {
                    log::error!("Failed to sign event for queueing: {}", sign_err);
                }
            }
        }
    }
}

/// Get count of pending events waiting to be published
#[allow(dead_code)]
pub fn get_pending_event_count() -> usize {
    PENDING_NOSTR_EVENTS.read().len()
}

/// Publish a quote event to relays (NIP-60 kind 7374)
///
/// Quote events allow clients to track quote state across devices.
/// The quote ID is encrypted with NIP-44 for privacy.
async fn publish_quote_event(
    quote_id: &str,
    mint_url: &str,
    expiration_days: u64,
) -> Result<String, String> {
    let signer = crate::stores::signer::get_signer()
        .ok_or("No signer available")?
        .as_nostr_signer();

    let pubkey_str = auth_store::get_pubkey()
        .ok_or("Not authenticated")?;
    let pubkey = PublicKey::parse(&pubkey_str)
        .map_err(|e| format!("Invalid pubkey: {}", e))?;

    // Encrypt quote ID with NIP-44
    let encrypted = signer.nip44_encrypt(&pubkey, quote_id).await
        .map_err(|e| format!("Failed to encrypt quote ID: {}", e))?;

    // Calculate expiration timestamp (default 14 days)
    let expiration_ts = Timestamp::now() + (expiration_days * 24 * 60 * 60);

    // Build quote event using rust-nostr structure
    let builder = nostr_sdk::EventBuilder::new(Kind::CashuWalletQuote, encrypted)
        .tags(vec![
            nostr_sdk::Tag::custom(nostr_sdk::TagKind::custom("mint"), [mint_url]),
            nostr_sdk::Tag::expiration(expiration_ts),
        ]);

    let client = nostr_client::NOSTR_CLIENT.read().as_ref()
        .ok_or("Nostr client not initialized")?
        .clone();

    match client.send_event_builder(builder).await {
        Ok(output) => {
            let event_id = output.id().to_hex();
            log::info!("Published quote event for quote {}: {}", quote_id, event_id);
            Ok(event_id)
        }
        Err(e) => {
            log::warn!("Failed to publish quote event: {}", e);
            // Non-critical - quote events are optional per NIP-60
            Err(format!("Failed to publish quote event: {}", e))
        }
    }
}

/// Delete a quote event from relays
///
/// Called when a quote expires or is no longer needed.
#[allow(dead_code)]
async fn delete_quote_event(event_id: &str) -> Result<(), String> {
    let client = nostr_client::NOSTR_CLIENT.read().as_ref()
        .ok_or("Nostr client not initialized")?
        .clone();

    let mut tags = vec![nostr_sdk::Tag::event(
        nostr_sdk::EventId::from_hex(event_id)
            .map_err(|e| format!("Invalid event ID: {}", e))?
    )];

    // Add NIP-60 required tag to indicate we're deleting kind 7374 events
    tags.push(nostr_sdk::Tag::custom(
        nostr_sdk::TagKind::custom("k"),
        ["7374"]
    ));

    let deletion_builder = nostr_sdk::EventBuilder::new(
        Kind::from(5),
        "Quote expired"
    ).tags(tags);

    match client.send_event_builder(deletion_builder).await {
        Ok(_) => {
            log::info!("Published deletion for quote event: {}", event_id);
            Ok(())
        }
        Err(e) => {
            log::warn!("Failed to delete quote event: {}", e);
            // Non-critical - deletion is best-effort
            Ok(())
        }
    }
}

/// Check if user has accepted Cashu wallet terms (NIP-78)
/// Returns true if the terms agreement event exists, false otherwise
pub async fn check_terms_accepted() -> Result<bool, String> {
    log::info!("Checking Cashu wallet terms acceptance (NIP-78)...");

    // Get pubkey
    let pubkey_str = auth_store::get_pubkey()
        .ok_or("Not authenticated")?;
    let pubkey = PublicKey::parse(&pubkey_str)
        .map_err(|e| format!("Invalid pubkey: {}", e))?;

    // Get client
    let client = nostr_client::NOSTR_CLIENT.read()
        .as_ref()
        .ok_or("Client not initialized")?
        .clone();

    // Build filter for terms agreement event
    let filter = Filter::new()
        .author(pubkey)
        .kind(Kind::from(30078))
        .identifier(TERMS_D_TAG)
        .limit(1);

    // Ensure relays are ready
    nostr_client::ensure_relays_ready(&client).await;

    // Fetch terms agreement event
    match client.fetch_events(filter, Duration::from_secs(5)).await {
        Ok(events) => {
            let accepted = !events.is_empty();
            log::info!("Terms acceptance check: {}", if accepted { "accepted" } else { "not accepted" });
            *TERMS_ACCEPTED.write() = Some(accepted);
            Ok(accepted)
        }
        Err(e) => {
            log::warn!("Failed to check terms acceptance: {}", e);
            *TERMS_ACCEPTED.write() = Some(false);
            Err(format!("Failed to check terms: {}", e))
        }
    }
}

/// Accept Cashu wallet terms by publishing a NIP-78 event
pub async fn accept_terms() -> Result<(), String> {
    log::info!("Accepting Cashu wallet terms (NIP-78)...");

    // Check if authenticated
    if !auth_store::is_authenticated() {
        return Err("Not authenticated".to_string());
    }

    // Get client
    let client = nostr_client::NOSTR_CLIENT.read()
        .as_ref()
        .ok_or("Client not initialized")?
        .clone();

    // Ensure relays are ready before publishing
    nostr_client::ensure_relays_ready(&client).await;

    // Create content with timestamp and version (use js_sys for WASM compatibility)
    let now = (js_sys::Date::now() / 1000.0) as u64;
    let content = serde_json::json!({
        "accepted_at": now,
        "version": 1
    }).to_string();

    // Build NIP-78 event (kind 30078 with d-tag)
    let builder = EventBuilder::new(Kind::from(30078), content)
        .tag(Tag::identifier(TERMS_D_TAG));

    // Publish to relays
    client.send_event_builder(builder).await
        .map_err(|e| format!("Failed to publish terms acceptance: {}", e))?;

    log::info!("Terms acceptance published successfully");

    // Update signal
    *TERMS_ACCEPTED.write() = Some(true);

    Ok(())
}

/// Initialize wallet by fetching from relays
pub async fn init_wallet() -> Result<(), String> {
    // Guard against concurrent initialization - skip if already loading or ready
    {
        let status = WALLET_STATUS.read();
        if matches!(*status, WalletStatus::Loading | WalletStatus::Ready) {
            log::debug!("Wallet init skipped - already {:?}", *status);
            return Ok(());
        }
    }

    *WALLET_STATUS.write() = WalletStatus::Loading;

    let pubkey_str = auth_store::get_pubkey()
        .ok_or("Not authenticated")?;

    let client = nostr_client::NOSTR_CLIENT.read().as_ref()
        .ok_or("Client not initialized")?.clone();

    let pubkey = PublicKey::parse(&pubkey_str)
        .map_err(|e| format!("Invalid pubkey: {}", e))?;

    log::info!("Loading Cashu wallet for {}", pubkey_str);

    // Fetch wallet event (kind 17375)
    let filter = Filter::new()
        .author(pubkey)
        .kind(Kind::from(17375))
        .limit(1);

    // Ensure relays are ready before fetching
    nostr_client::ensure_relays_ready(&client).await;

    match client.fetch_events(filter, Duration::from_secs(10)).await {
        Ok(events) => {
            if let Some(wallet_event) = events.into_iter().next() {
                // Decrypt and parse wallet event
                match decrypt_wallet_event(&wallet_event).await {
                    Ok(wallet_data) => {
                        log::info!("Wallet loaded with {} mints", wallet_data.mints.len());

                        // Note: wallet privkey is no longer stored in plaintext LocalStorage
                        // It is derived deterministically from the user's Nostr key when needed

                        *WALLET_STATE.write() = Some(WalletState {
                            privkey: wallet_data.privkey.clone(),
                            mints: wallet_data.mints.iter().map(|u| u.to_string()).collect(),
                            initialized: true,
                        });

                        // Initialize MultiMintWallet with all mints
                        if let Err(e) = init_multi_mint_wallet(&wallet_data.mints).await {
                            log::error!("Failed to initialize MultiMintWallet: {}", e);
                            // Continue without MultiMintWallet - fallback to legacy mode
                        }

                        // Fetch tokens and history
                        if let Err(e) = fetch_tokens().await {
                            log::error!("Failed to fetch tokens: {}", e);
                        }

                        if let Err(e) = fetch_history().await {
                            log::error!("Failed to fetch history: {}", e);
                        }

                        // Sync MultiMintWallet state to Dioxus signals
                        if let Err(e) = crate::stores::cashu_cdk_bridge::sync_wallet_state().await {
                            log::warn!("Failed to sync MultiMintWallet state: {}", e);
                        }

                        // Load pending events queue from IndexedDB
                        if let Err(e) = load_pending_events().await {
                            log::warn!("Failed to load pending events: {}", e);
                            // Non-critical: wallet still works even if pending queue can't load
                        }

                        // Start background processor for pending events
                        start_pending_events_processor();

                        *WALLET_STATUS.write() = WalletStatus::Ready;
                        Ok(())
                    }
                    Err(e) => {
                        let error = format!("Failed to decrypt wallet: {}", e);
                        log::error!("{}", error);
                        *WALLET_STATUS.write() = WalletStatus::Error(error.clone());
                        Err(error)
                    }
                }
            } else {
                log::info!("No wallet found");
                *WALLET_STATE.write() = Some(WalletState {
                    privkey: String::new(),
                    mints: Vec::new(),
                    initialized: false,
                });
                *WALLET_STATUS.write() = WalletStatus::Ready;
                Ok(())
            }
        }
        Err(e) => {
            let error = format!("Failed to fetch wallet: {}", e);
            log::error!("{}", error);
            *WALLET_STATUS.write() = WalletStatus::Error(error.clone());
            Err(error)
        }
    }
}

/// Decrypt wallet event (kind 17375)
///
/// Parses the NIP-60 wallet event format: `[["privkey", "hex"], ["mint", "url"], ...]`
/// Returns an error if the wallet is missing required fields (privkey or at least one mint).
async fn decrypt_wallet_event(event: &Event) -> Result<WalletEvent, String> {
    use nostr_sdk::signer::NostrSigner;

    let signer = crate::stores::signer::get_signer()
        .ok_or("No signer available")?
        .as_nostr_signer();

    // Decrypt the content using signer's NIP-44 method
    let decrypted = signer.nip44_decrypt(&event.pubkey, &event.content).await
        .map_err(|e| format!("Failed to decrypt wallet event: {}", e))?;

    // Parse the decrypted JSON array: [["privkey", "hex"], ["mint", "url"], ...]
    let pairs: Vec<Vec<String>> = serde_json::from_str(&decrypted)
        .map_err(|e| format!("Failed to parse wallet event JSON: {}", e))?;

    let mut privkey = String::new();
    let mut mints = Vec::new();
    let mut found_multiple_privkeys = false;

    for pair in pairs {
        // Skip malformed entries (must have exactly 2 elements per NIP-60)
        if pair.len() != 2 {
            log::warn!("Skipping malformed wallet event entry with {} elements", pair.len());
            continue;
        }
        match pair[0].as_str() {
            "privkey" => {
                if !privkey.is_empty() {
                    // Per rust-nostr SDK, multiple privkeys is an error
                    found_multiple_privkeys = true;
                } else {
                    privkey = pair[1].clone();
                }
            }
            "mint" => {
                match nostr_sdk::Url::parse(&pair[1]) {
                    Ok(mint_url) => mints.push(mint_url),
                    Err(e) => {
                        // Log but continue - one bad mint shouldn't break the wallet
                        log::warn!("Skipping invalid mint URL '{}': {}", pair[1], e);
                    }
                }
            }
            _ => {} // Ignore unknown keys for forward compatibility
        }
    }

    // Validate required fields per NIP-60 spec (matching rust-nostr SDK behavior)
    if found_multiple_privkeys {
        return Err("Wallet event contains multiple privkeys (invalid per NIP-60)".to_string());
    }

    if privkey.is_empty() {
        return Err("Missing required field: privkey".to_string());
    }

    if mints.is_empty() {
        return Err("Missing required field: mint (at least one mint URL required)".to_string());
    }

    Ok(WalletEvent::new(privkey, mints))
}

/// Fetch all token events (kind 7375)
pub async fn fetch_tokens() -> Result<(), String> {
    let pubkey_str = auth_store::get_pubkey()
        .ok_or("Not authenticated")?;

    let client = nostr_client::NOSTR_CLIENT.read().as_ref()
        .ok_or("Client not initialized")?.clone();

    let pubkey = PublicKey::parse(&pubkey_str)
        .map_err(|e| format!("Invalid pubkey: {}", e))?;

    log::info!("Fetching token events");

    // Ensure relays are ready before fetching
    nostr_client::ensure_relays_ready(&client).await;

    // Fetch kind-5 deletion events that target kind-7375 token events
    let deletion_filter = Filter::new()
        .author(pubkey.clone())
        .kind(Kind::from(5));

    let mut deleted_event_ids = std::collections::HashSet::new();

    if let Ok(deletion_events) = client.fetch_events(deletion_filter, Duration::from_secs(10)).await {
        for del_event in deletion_events {
            // Check e tags that reference kind-7375 events (using type-safe tag parsing)
            for tag in del_event.tags.iter() {
                if let Some(nostr::TagStandard::Event { event_id, .. }) = tag.as_standardized() {
                    deleted_event_ids.insert(event_id.to_hex());
                }
            }
        }
        if !deleted_event_ids.is_empty() {
            log::info!("Found {} deleted token events via kind-5", deleted_event_ids.len());
        }
    }

    // Fetch all token events
    let filter = Filter::new()
        .author(pubkey)
        .kind(Kind::from(7375));

    match client.fetch_events(filter, Duration::from_secs(10)).await {
        Ok(events) => {
            use nostr_sdk::signer::NostrSigner;
            use std::collections::HashSet;

            let signer = crate::stores::signer::get_signer()
                .ok_or("No signer available")?
                .as_nostr_signer();

            // Convert Events to Vec for multiple iterations
            let events: Vec<_> = events.into_iter().collect();

            // First pass: collect all deleted event IDs from del fields
            // Per NIP-60, the `del` field contains event IDs of token events that were
            // destroyed/replaced by this event, NOT proof secrets
            let mut deleted_via_del_field = HashSet::new();

            for event in &events {
                // Skip events that are deleted by kind-5
                if deleted_event_ids.contains(&event.id.to_hex()) {
                    continue;
                }

                // Decrypt and parse to get del field (contains event IDs)
                if let Ok(decrypted) = signer.nip44_decrypt(&event.pubkey, &event.content).await {
                    if let Ok(token_event) = serde_json::from_str::<TokenEventData>(&decrypted) {
                        // Add all deleted event IDs to the set
                        for del_event_id in &token_event.del {
                            deleted_via_del_field.insert(del_event_id.clone());
                        }
                    }
                }
            }

            if !deleted_via_del_field.is_empty() {
                log::info!("Found {} deleted token events via del field", deleted_via_del_field.len());
            }

            // Combine both deletion sources: kind-5 events and del field references
            let all_deleted_events: HashSet<String> = deleted_event_ids
                .union(&deleted_via_del_field)
                .cloned()
                .collect();

            // Second pass: collect tokens, skipping deleted events
            let mut tokens = Vec::new();
            let mut total_balance = 0u64;

            for event in &events {
                let event_id_hex = event.id.to_hex();

                // Skip events that are deleted (either by kind-5 or del field)
                if all_deleted_events.contains(&event_id_hex) {
                    log::debug!("Skipping deleted token event: {}", event_id_hex);
                    continue;
                }

                // Decrypt token event using signer
                match signer.nip44_decrypt(&event.pubkey, &event.content).await {
                    Ok(decrypted) => {
                        // Parse JSON: { mint: string, proofs: [...], del?: [...] }
                        match serde_json::from_str::<TokenEventData>(&decrypted) {
                            Ok(token_event) => {
                                // Keep proofs as ProofData to preserve witness/DLEQ data
                                let proofs: Vec<ProofData> = token_event.proofs.iter()
                                    .map(|p| ProofData {
                                        id: if p.id.is_empty() {
                                            // Generate a placeholder ID if missing
                                            format!("{}_{}", p.secret, p.amount)
                                        } else {
                                            p.id.clone()
                                        },
                                        amount: p.amount,
                                        secret: p.secret.clone(),
                                        c: p.c.clone(),
                                        witness: p.witness.clone(),  // Preserve witness
                                        dleq: p.dleq.clone(),        // Preserve DLEQ
                                    })
                                    .collect();

                                // Only include tokens with proofs
                                if !proofs.is_empty() {
                                    // Calculate balance using checked arithmetic
                                    let token_balance: u64 = proofs.iter()
                                        .map(|p| p.amount)
                                        .try_fold(0u64, |acc, amount| acc.checked_add(amount))
                                        .ok_or_else(|| format!(
                                            "Proof amount overflow in token event {}",
                                            event_id_hex
                                        ))?;

                                    // Use checked addition to prevent silent overflow
                                    total_balance = total_balance.checked_add(token_balance)
                                        .ok_or_else(|| format!(
                                            "Balance overflow when adding token event {} (balance: {}, adding: {})",
                                            event_id_hex, total_balance, token_balance
                                        ))?;

                                    tokens.push(TokenData {
                                        event_id: event_id_hex,
                                        mint: token_event.mint.clone(),
                                        unit: token_event.unit.clone(),  // Parse unit from event
                                        proofs,
                                        created_at: event.created_at.as_secs(),
                                    });
                                }
                            }
                            Err(e) => {
                                log::error!("Failed to parse token event {}: {}", event.id, e);
                            }
                        }
                    }
                    Err(e) => {
                        log::error!("Failed to decrypt token event {}: {}", event.id, e);
                    }
                }
            }

            log::info!("Loaded {} token events with total balance: {} sats", tokens.len(), total_balance);
            *WALLET_TOKENS.read().data().write() = tokens;
            *WALLET_BALANCE.write() = total_balance;
            Ok(())
        }
        Err(e) => {
            log::error!("Failed to fetch token events: {}", e);
            Err(format!("Failed to fetch token events: {}", e))
        }
    }
}

/// Fetch transaction history (kind 7376)
pub async fn fetch_history() -> Result<(), String> {
    let pubkey_str = auth_store::get_pubkey()
        .ok_or("Not authenticated")?;

    let client = nostr_client::NOSTR_CLIENT.read().as_ref()
        .ok_or("Client not initialized")?.clone();

    let pubkey = PublicKey::parse(&pubkey_str)
        .map_err(|e| format!("Invalid pubkey: {}", e))?;

    log::info!("Fetching transaction history");

    // Ensure relays are ready before fetching
    nostr_client::ensure_relays_ready(&client).await;

    let filter = Filter::new()
        .author(pubkey)
        .kind(Kind::from(7376));

    match client.fetch_events(filter, Duration::from_secs(10)).await {
        Ok(events) => {
            use nostr_sdk::signer::NostrSigner;

            let signer = crate::stores::signer::get_signer()
                .ok_or("No signer available")?
                .as_nostr_signer();
            let mut history = Vec::new();

            for event in events {
                // Decrypt history event using signer
                match signer.nip44_decrypt(&event.pubkey, &event.content).await {
                    Ok(decrypted) => {
                        // Parse JSON array: [["direction", "in"], ["amount", "100"], ["e", "id", "", "created"], ...]
                        match serde_json::from_str::<Vec<Vec<String>>>(&decrypted) {
                            Ok(pairs) => {
                                let mut direction = TransactionDirection::In;
                                let mut amount: Option<u64> = None;
                                let mut created_tokens = Vec::new();
                                let mut destroyed_tokens = Vec::new();

                                for pair in pairs {
                                    if pair.is_empty() {
                                        continue;
                                    }
                                    match pair[0].as_str() {
                                        "direction" => {
                                            if pair.len() > 1 {
                                                direction = if pair[1] == "in" {
                                                    TransactionDirection::In
                                                } else {
                                                    TransactionDirection::Out
                                                };
                                            }
                                        }
                                        "amount" => {
                                            if pair.len() > 1 {
                                                match pair[1].parse::<u64>() {
                                                    Ok(parsed_amount) => {
                                                        amount = Some(parsed_amount);
                                                    }
                                                    Err(e) => {
                                                        log::error!(
                                                            "Failed to parse amount in history event {}: '{}' - {}",
                                                            event.id.to_hex(),
                                                            pair[1],
                                                            e
                                                        );
                                                        // Keep amount as None to skip this event
                                                    }
                                                }
                                            }
                                        }
                                        "e" => {
                                            // Event reference: ["e", "event_id", "", "marker"]
                                            if pair.len() > 3 {
                                                match pair[3].as_str() {
                                                    "created" => created_tokens.push(pair[1].clone()),
                                                    "destroyed" => destroyed_tokens.push(pair[1].clone()),
                                                    _ => {}
                                                }
                                            }
                                        }
                                        _ => {}
                                    }
                                }

                                // Extract redeemed events from unencrypted tags
                                let redeemed_events: Vec<String> = event.tags.iter()
                                    .filter_map(|tag| {
                                        let vec = tag.clone().to_vec();
                                        if vec.len() > 3 && vec[0] == "e" && vec[3] == "redeemed" {
                                            Some(vec[1].clone())
                                        } else {
                                            None
                                        }
                                    })
                                    .collect();

                                // Only add to history if amount was successfully parsed
                                if let Some(parsed_amount) = amount {
                                    history.push(HistoryItem {
                                        event_id: event.id.to_hex(),
                                        direction,
                                        amount: parsed_amount,
                                        unit: "sat".to_string(),
                                        created_at: event.created_at.as_secs(),
                                        created_tokens,
                                        destroyed_tokens,
                                        redeemed_events,
                                    });
                                } else {
                                    log::warn!(
                                        "Skipping history event {} due to missing or invalid amount",
                                        event.id.to_hex()
                                    );
                                }
                            }
                            Err(e) => {
                                log::error!("Failed to parse history event {}: {}", event.id, e);
                            }
                        }
                    }
                    Err(e) => {
                        log::error!("Failed to decrypt history event {}: {}", event.id, e);
                    }
                }
            }

            // Sort by created_at descending (newest first)
            history.sort_by(|a, b| b.created_at.cmp(&a.created_at));

            log::info!("Loaded {} history items", history.len());
            *WALLET_HISTORY.read().data().write() = history;
            Ok(())
        }
        Err(e) => {
            log::error!("Failed to fetch history: {}", e);
            Err(format!("Failed to fetch history: {}", e))
        }
    }
}

/// Create a new wallet with generated P2PK key
pub async fn create_wallet(mints: Vec<String>) -> Result<(), String> {
    use nostr_sdk::signer::NostrSigner;

    if !*nostr_client::HAS_SIGNER.read() {
        return Err("No signer attached".to_string());
    }

    let client = nostr_client::NOSTR_CLIENT.read().as_ref()
        .ok_or("Client not initialized")?.clone();

    let signer = crate::stores::signer::get_signer()
        .ok_or("No signer available")?
        .as_nostr_signer();

    let pubkey_str = auth_store::get_pubkey()
        .ok_or("Not authenticated")?;
    let pubkey = PublicKey::parse(&pubkey_str)
        .map_err(|e| format!("Invalid pubkey: {}", e))?;

    // Generate new private key for P2PK ecash (separate from Nostr key)
    let wallet_secret = SecretKey::generate();
    let wallet_privkey = wallet_secret.to_secret_hex();

    log::info!("Creating new wallet with {} mints", mints.len());

    // Parse mint URLs for rust-nostr compatibility
    let mint_urls: Vec<Url> = mints.iter()
        .filter_map(|m| Url::parse(m).ok())
        .collect();

    // Build wallet event using rust-nostr structure
    let wallet_event = WalletEvent::new(wallet_privkey.clone(), mint_urls);

    // Build wallet data following rust-nostr's internal format
    let mut content_array: Vec<Vec<&str>> = vec![vec!["privkey", &wallet_event.privkey]];
    for mint in wallet_event.mints.iter() {
        content_array.push(vec!["mint", mint.as_str()]);
    }

    let json_content = serde_json::to_string(&content_array)
        .map_err(|e| format!("Failed to serialize wallet data: {}", e))?;

    // Encrypt content using signer (keeps existing pattern)
    let encrypted_content = signer.nip44_encrypt(&pubkey, &json_content).await
        .map_err(|e| format!("Failed to encrypt wallet data: {}", e))?;

    // Build event using rust-nostr kind constant
    let builder = nostr_sdk::EventBuilder::new(
        Kind::CashuWallet,
        encrypted_content
    );

    // Publish wallet event
    match client.send_event_builder(builder).await {
        Ok(_) => {
            log::info!("Wallet created successfully");

            // Note: wallet privkey is no longer stored in plaintext LocalStorage
            // It is derived deterministically from the user's Nostr key when needed

            // Update local state
            *WALLET_STATE.write() = Some(WalletState {
                privkey: wallet_privkey,
                mints: mints.clone(),
                initialized: true,
            });

            *WALLET_STATUS.write() = WalletStatus::Ready;
            Ok(())
        }
        Err(e) => {
            let error = format!("Failed to create wallet: {}", e);
            log::error!("{}", error);
            Err(error)
        }
    }
}


/// Check if wallet is initialized
pub fn is_wallet_initialized() -> bool {
    WALLET_STATE.read()
        .as_ref()
        .map(|w| w.initialized)
        .unwrap_or(false)
}

/// Get total number of mints
#[allow(dead_code)]
pub fn get_mint_count() -> usize {
    WALLET_STATE.read()
        .as_ref()
        .map(|w| w.mints.len())
        .unwrap_or(0)
}

/// Get mints list
pub fn get_mints() -> Vec<String> {
    WALLET_STATE.read()
        .as_ref()
        .map(|w| w.mints.clone())
        .unwrap_or_default()
}

/// Refresh wallet data from relays
pub async fn refresh_wallet() -> Result<(), String> {
    if !is_wallet_initialized() {
        return Err("Wallet not initialized".to_string());
    }

    log::info!("Refreshing wallet data");

    fetch_tokens().await?;
    fetch_history().await?;

    Ok(())
}

// ============================================================================
// Phase 2: Send/Receive Utility Functions
// ============================================================================

/// Derive deterministic wallet seed from Nostr private key or signer
#[cfg(target_arch = "wasm32")]
async fn derive_wallet_seed() -> Result<[u8; 64], String> {
    use sha2::{Sha256, Digest};

    // Try to get Keys first (for nsec login)
    if let Some(keys) = auth_store::get_keys() {
        log::info!("Deriving seed from private key (nsec login)");
        let secret_key = keys.secret_key();

        // Derive seed using SHA-256 with domain separation
        let mut hasher = Sha256::new();
        hasher.update(secret_key.to_secret_bytes());
        hasher.update(b"cashu-wallet-seed-v1");
        let hash = hasher.finalize();

        let mut seed = [0u8; 64];
        seed[..32].copy_from_slice(&hash);

        // Second round for full 64 bytes
        let mut hasher = Sha256::new();
        hasher.update(&hash);
        hasher.update(b"cashu-wallet-seed-v1-ext");
        let hash2 = hasher.finalize();
        seed[32..].copy_from_slice(&hash2);

        return Ok(seed);
    }

    // For browser extension or remote signer, use NIP-07 to sign a deterministic challenge
    // The signature acts as our secret seed material
    // This requires user approval but provides secure, deterministic derivation

    log::info!("Using browser extension - deriving seed from NIP-07 signature");

    let signer = crate::stores::signer::get_signer()
        .ok_or("No signer available for extension user")?
        .as_nostr_signer();

    // Create a deterministic challenge event for wallet seed derivation
    let challenge_content = "nostr.blue Cashu Wallet Seed Derivation - Sign this message to derive your wallet encryption key";

    // Use a fixed timestamp to ensure deterministic challenge
    use nostr_sdk::{EventBuilder, Timestamp};
    let challenge_event = EventBuilder::text_note(challenge_content)
        .custom_created_at(Timestamp::from(1700000000)) // Fixed timestamp for determinism
        .sign(&signer)
        .await
        .map_err(|e| format!("Failed to sign challenge for wallet derivation: {}", e))?;

    // Use the signature as our seed material (64 bytes)
    // Schnorr signatures are exactly 64 bytes
    let sig_bytes = challenge_event.sig.serialize();

    let mut seed = [0u8; 64];
    seed.copy_from_slice(&sig_bytes);

    log::info!("Wallet seed derived from NIP-07 signature (deterministic, no storage needed)");
    Ok(seed)
}

#[cfg(not(target_arch = "wasm32"))]
async fn derive_wallet_seed() -> Result<[u8; 64], String> {
    Err("Seed derivation only available in WASM".to_string())
}

/// Initialize the MultiMintWallet with all mints from the wallet event
///
/// This sets up the CDK MultiMintWallet which manages all mint wallets internally.
/// Should be called after WALLET_STATE is populated from the wallet event.
async fn init_multi_mint_wallet(mints: &[nostr_sdk::Url]) -> Result<(), String> {
    use crate::stores::cashu_cdk_bridge;

    log::info!("Initializing MultiMintWallet with {} mints", mints.len());

    // Get shared localstore
    let localstore = get_shared_localstore().await?;

    // Derive wallet seed
    let seed = derive_wallet_seed().await?;

    // Initialize MultiMintWallet (this also loads existing mints from DB)
    let multi_wallet = cashu_cdk_bridge::init_multi_wallet(localstore, seed).await?;

    // Add all mints from the wallet event
    for mint_url in mints {
        let mint_str = mint_url.to_string();
        if !cashu_cdk_bridge::has_mint(&mint_str).await {
            if let Err(e) = cashu_cdk_bridge::add_mint(&mint_str).await {
                log::warn!("Failed to add mint {}: {}", mint_str, e);
                // Continue with other mints
            }
        }
    }

    log::info!("MultiMintWallet initialized with {} wallets",
        multi_wallet.get_wallets().await.len());

    Ok(())
}

/// Collect all P2PK signing keys that can be used to unlock tokens
///
/// This includes:
/// - The wallet's private key (for tokens locked to wallet pubkey)
/// - The Nostr identity key (for tokens locked to user's npub)
///
/// This enables receiving tokens that are locked to either key.
async fn collect_p2pk_signing_keys() -> Vec<cdk::nuts::SecretKey> {
    let mut keys = Vec::new();

    // 1. Wallet's private key (cashu-specific)
    if let Some(state) = WALLET_STATE.read().as_ref() {
        if !state.privkey.is_empty() {
            match cdk::nuts::SecretKey::from_hex(&state.privkey) {
                Ok(key) => {
                    log::debug!("Added wallet privkey to P2PK signing keys");
                    keys.push(key);
                }
                Err(e) => {
                    log::warn!("Failed to parse wallet privkey for P2PK: {}", e);
                }
            }
        }
    }

    // 2. Nostr identity key (for tokens locked to user's npub)
    // This allows receiving tokens that were locked to the user's Nostr public key
    if let Some(nostr_keys) = auth_store::get_keys() {
        // Convert nostr SecretKey to CDK SecretKey
        let secret_bytes = nostr_keys.secret_key().to_secret_bytes();
        match cdk::nuts::SecretKey::from_slice(&secret_bytes) {
            Ok(key) => {
                log::debug!("Added Nostr identity key to P2PK signing keys");
                keys.push(key);
            }
            Err(e) => {
                log::warn!("Failed to convert Nostr key for P2PK: {}", e);
            }
        }
    }

    // Remove duplicates (in case wallet key and nostr key are the same)
    // CDK SecretKey doesn't implement Hash/Eq, so we compare by serializing
    let mut seen_keys = std::collections::HashSet::new();
    keys.retain(|key| {
        let key_hex = key.to_string();
        seen_keys.insert(key_hex)
    });

    log::info!("Collected {} unique P2PK signing keys for token receive", keys.len());
    keys
}

/// Convert ProofData (our custom type) to CDK Proof
fn proof_data_to_cdk_proof(data: &ProofData) -> Result<cdk::nuts::Proof, String> {
    use cdk::nuts::{Proof, Id, PublicKey, Witness};
    use cdk::nuts::nut12::ProofDleq;
    use cdk::secret::Secret;
    use cdk::Amount;
    use std::str::FromStr;

    // Parse witness if present
    let witness = if let Some(ref witness_str) = data.witness {
        Some(serde_json::from_str::<Witness>(witness_str)
            .map_err(|e| format!("Invalid witness: {}", e))?)
    } else {
        None
    };

    // Parse DLEQ if present
    let dleq = if let Some(ref dleq_data) = data.dleq {
        // Parse the hex strings as raw secret key values
        Some(ProofDleq {
            e: cdk::nuts::SecretKey::from_hex(&dleq_data.e)
                .map_err(|e| format!("Invalid DLEQ e value: {}", e))?,
            s: cdk::nuts::SecretKey::from_hex(&dleq_data.s)
                .map_err(|e| format!("Invalid DLEQ s value: {}", e))?,
            r: cdk::nuts::SecretKey::from_hex(&dleq_data.r)
                .map_err(|e| format!("Invalid DLEQ r value: {}", e))?,
        })
    } else {
        None
    };

    Ok(Proof {
        amount: Amount::from(data.amount),
        keyset_id: Id::from_str(&data.id)
            .map_err(|e| format!("Invalid keyset ID '{}': {}", data.id, e))?,
        secret: Secret::from_str(&data.secret)
            .map_err(|e| format!("Invalid secret: {}", e))?,
        c: PublicKey::from_hex(&data.c)
            .map_err(|e| format!("Invalid C value: {}", e))?,
        witness,
        dleq,
    })
}

/// Convert CDK Proof to ProofData (our custom type)
pub(crate) fn cdk_proof_to_proof_data(proof: &cdk::nuts::Proof) -> ProofData {
    // Serialize witness if present
    let witness = proof.witness.as_ref()
        .and_then(|w| serde_json::to_string(w).ok());

    // Convert DLEQ if present - serialize as strings
    let dleq = proof.dleq.as_ref()
        .map(|d| DleqData {
            e: d.e.to_string(),
            s: d.s.to_string(),
            r: d.r.to_string(),
        });

    ProofData {
        id: proof.keyset_id.to_string(),
        amount: u64::from(proof.amount),
        secret: proof.secret.to_string(),
        c: proof.c.to_hex(),
        witness,
        dleq,
    }
}

/// Remove a melt quote from the database without creating a full wallet
///
/// Uses the shared localstore for consistency with the wallet caching system.
async fn remove_melt_quote_from_db(quote_id: &str) -> Result<(), String> {
    use cdk::cdk_database::WalletDatabase;

    // Use shared localstore for consistency with wallet caching system
    let localstore = get_shared_localstore().await?;

    localstore.remove_melt_quote(quote_id).await
        .map_err(|e| format!("Failed to remove melt quote: {}", e))?;

    log::info!("Successfully removed melt quote {} from database", quote_id);
    Ok(())
}

/// Get a cached wallet and optionally inject proofs into the shared localstore
///
/// This function uses the cached wallet system for improved performance.
/// Proofs are injected into the shared IndexedDB store which all wallets share.
///
/// Note on atomicity and counter safety:
/// - All wallets share a single IndexedDB database connection
/// - The increment_keyset_counter method uses IndexedDB readwrite transactions
///   to perform atomic read-modify-write operations (get → increment → put → commit)
/// - IndexedDB serializes all transactions on the same object store, guaranteeing
///   that concurrent counter increments will never produce duplicate values
/// - The per-mint operation lock prevents concurrent operations on the same mint
async fn create_ephemeral_wallet(
    mint_url: &str,
    proofs: Vec<cdk::nuts::Proof>
) -> Result<std::sync::Arc<cdk::Wallet>, String> {
    use cdk::nuts::{CurrencyUnit, State};
    use cdk::types::ProofInfo;

    // Get or create cached wallet (handles localstore, seed derivation, mint info)
    let wallet = get_or_create_wallet(mint_url).await?;

    // Inject proofs if any provided
    if !proofs.is_empty() {
        use cdk::mint_url::MintUrl as CdkMintUrl;
        let mint_url_parsed: CdkMintUrl = mint_url.parse()
            .map_err(|e| format!("Invalid mint URL: {}", e))?;

        let proof_infos: Vec<_> = proofs.into_iter()
            .map(|p| {
                ProofInfo::new(
                    p,
                    mint_url_parsed.clone(),
                    State::Unspent,
                    CurrencyUnit::Sat
                )
            })
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("Failed to create proof info: {}", e))?;

        // Inject proofs via shared localstore
        let localstore = get_shared_localstore().await?;
        localstore.update_proofs(proof_infos, vec![]).await
            .map_err(|e| format!("Failed to inject proofs: {}", e))?;
    }

    Ok(wallet)
}

// ============================================================================
// Phase 2B: Receive Implementation
// ============================================================================

/// Options for receiving tokens
#[derive(Clone, Debug, Default)]
pub struct ReceiveTokensOptions {
    /// Whether to verify DLEQ proofs before accepting tokens (NUT-12)
    /// If true and verification fails, the receive will be rejected
    pub verify_dleq: bool,
}

/// Receive ecash from a token string (default options - no DLEQ verification)
#[allow(dead_code)]
pub async fn receive_tokens(token_string: String) -> Result<u64, String> {
    receive_tokens_with_options(token_string, ReceiveTokensOptions::default()).await
}

/// Receive ecash from a token string with options
///
/// If `options.verify_dleq` is true, will verify DLEQ proofs (NUT-12) before accepting.
/// This provides offline verification that the mint's signatures are valid.
pub async fn receive_tokens_with_options(token_string: String, options: ReceiveTokensOptions) -> Result<u64, String> {
    use cdk::nuts::Token;
    use cdk::wallet::ReceiveOptions;
    use std::str::FromStr;
    use nostr_sdk::signer::NostrSigner;

    log::info!("Receiving token (verify_dleq: {})...", options.verify_dleq);

    // Sanitize token string - remove ALL whitespace (spaces, tabs, newlines)
    // This is crucial because copy/paste often adds line breaks in the middle
    let token_string = token_string
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect::<String>();

    if token_string.is_empty() {
        return Err("Token string is empty".to_string());
    }

    log::info!("Token string length: {}, starts with: {}",
        token_string.len(),
        token_string.chars().take(10).collect::<String>());

    // Validate token format
    if !token_string.starts_with("cashuA") && !token_string.starts_with("cashuB") {
        return Err(format!(
            "Invalid token format. Cashu tokens must start with 'cashuA' or 'cashuB'. Your token starts with: '{}'",
            token_string.chars().take(10).collect::<String>()
        ));
    }

    // Additional validation: check for non-ASCII or control characters that might indicate encoding issues
    if token_string.chars().any(|c| c.is_control()) {
        log::warn!("Token contains control characters");
        return Err("Token contains invalid control characters. Please copy the token again.".to_string());
    }

    // Extract and validate the base64 portion
    let base64_part = if token_string.starts_with("cashuA") {
        &token_string[6..]
    } else if token_string.starts_with("cashuB") {
        &token_string[6..]
    } else {
        ""
    };

    log::info!("Base64 portion length: {}, last 20 chars: {}",
        base64_part.len(),
        base64_part.chars().rev().take(20).collect::<String>().chars().rev().collect::<String>());

    // Check if base64 length is valid and try auto-correction
    let remainder = base64_part.len() % 4;
    let token_to_parse = if remainder != 0 {
        log::warn!("Base64 portion length {} is not a multiple of 4. Remainder: {}",
            base64_part.len(), remainder);

        // Try adding padding if it's close to being valid
        if remainder == 2 || remainder == 3 {
            let padding_needed = 4 - remainder;
            log::warn!(
                "Auto-correcting malformed token: adding {} padding character(s) to base64 portion (original length: {})",
                padding_needed,
                base64_part.len()
            );
            let padded = format!("{}{}", token_string, "=".repeat(padding_needed));
            log::info!("Attempting to parse with {} padding characters added", padding_needed);
            padded
        } else {
            token_string.clone()
        }
    } else {
        token_string.clone()
    };

    // Parse token (try padded version if applicable, otherwise use original)
    let token = Token::from_str(&token_to_parse)
        .map_err(|e| {
            log::error!("Token parse error: {:?}", e);
            log::error!("Token length: {}, base64 part length: {}", token_to_parse.len(), base64_part.len());
            let error_str = e.to_string();

            // Provide helpful error messages
            if error_str.contains("6-bit remainder") || error_str.contains("InvalidLength") {
                return format!(
                    "Token appears to be incomplete or corrupted (base64 length: {}, remainder: {}). Please ensure you copied the entire token.",
                    base64_part.len(),
                    remainder
                );
            } else if error_str.contains("InvalidByte") {
                return "Token contains invalid characters. Please copy the token again carefully.".to_string();
            }

            format!("Invalid token format: {}", e)
        })?;

    if token_to_parse != token_string {
        log::info!("Successfully parsed token after adding padding!");
    }

    let mint_url = token.mint_url()
        .map_err(|e| {
            log::error!("Mint URL extraction error: {:?}", e);
            format!("Failed to get mint URL: {}", e)
        })?
        .to_string();

    log::info!("Token from mint: {}", mint_url);

    // Acquire mint operation lock to prevent concurrent operations
    let _lock_guard = try_acquire_mint_lock(&mint_url)
        .ok_or_else(|| format!("Another operation is in progress for mint: {}", mint_url))?;

    // Create ephemeral wallet
    let wallet = create_ephemeral_wallet(&mint_url, vec![]).await?;

    // NUT-12: Verify DLEQ proofs if requested
    // This provides offline verification that the mint's blind signatures are valid
    if options.verify_dleq {
        log::info!("Verifying DLEQ proofs (NUT-12)...");
        match wallet.verify_token_dleq(&token).await {
            Ok(()) => {
                log::info!("DLEQ verification successful - token signatures are valid");
            }
            Err(e) => {
                let error_msg = e.to_string();
                if error_msg.contains("MissingDleqProof") || error_msg.contains("missing") {
                    log::warn!("Token does not contain DLEQ proofs - cannot verify offline");
                    return Err("Token verification failed: This token does not contain DLEQ proofs for offline verification. The mint may not support NUT-12.".to_string());
                } else {
                    log::error!("DLEQ verification failed: {}", e);
                    return Err(format!("Token verification failed: Invalid DLEQ proof. The mint's signature could not be verified. Error: {}", e));
                }
            }
        }
    }

    // Receive token (contacts mint to swap proofs) with auto-cleanup on spent errors
    // Use the corrected/padded token to ensure padding fix is preserved

    // Collect ALL potential P2PK signing keys for unlock (NUT-11)
    // This allows receiving tokens locked to various keys we control:
    // 1. Wallet's private key (cashu-specific)
    // 2. Nostr identity key (for tokens locked to user's npub)
    let p2pk_signing_keys = collect_p2pk_signing_keys().await;

    log::debug!("Using {} P2PK signing keys for receive", p2pk_signing_keys.len());

    let receive_opts = ReceiveOptions {
        p2pk_signing_keys,
        ..Default::default()
    };

    let amount_received = match wallet.receive(
        &token_to_parse,
        receive_opts
    ).await {
        Ok(amount) => amount,
        Err(e) => {
            let error_msg = e.to_string();
            if error_msg.contains("already spent") || error_msg.contains("already redeemed") {
                log::warn!("Token already spent or redeemed, checking for spent proofs in wallet");

                // Cleanup any spent proofs in our wallet to keep state clean (use internal since we hold lock)
                match cleanup_spent_proofs_internal(mint_url.clone()).await {
                    Ok((cleaned_count, cleaned_amount)) if cleaned_count > 0 => {
                        log::info!("Cleaned up {} spent proofs worth {} sats from wallet", cleaned_count, cleaned_amount);
                        return Err(format!(
                            "This token has already been spent. However, we cleaned up {} spent proofs ({} sats) from your wallet.",
                            cleaned_count, cleaned_amount
                        ));
                    }
                    Ok(_) => {
                        log::info!("No spent proofs found in wallet");
                        return Err("This token has already been spent and cannot be redeemed.".to_string());
                    }
                    Err(cleanup_err) => {
                        log::error!("Cleanup failed: {}", cleanup_err);
                        return Err("This token has already been spent and cannot be redeemed.".to_string());
                    }
                }
            }
            return Err(format!("Failed to receive token: {}", e));
        }
    };

    log::info!("Received {} sats", u64::from(amount_received));

    // Get received proofs
    let new_proofs = wallet.get_unspent_proofs().await
        .map_err(|e| format!("Failed to get proofs: {}", e))?;

    // Convert to ProofData and then ExtendedCashuProof
    let proof_data: Vec<ProofData> = new_proofs.iter()
        .map(|p| cdk_proof_to_proof_data(p))
        .collect();

    // Create extended token event with P2PK support
    let extended_proofs: Vec<ExtendedCashuProof> = proof_data.iter()
        .map(|p| ExtendedCashuProof::from(p.clone()))
        .collect();

    let token_event_data = ExtendedTokenEvent {
        mint: mint_url.clone(),
        unit: "sat".to_string(),
        proofs: extended_proofs,
        del: vec![],
    };

    let signer = crate::stores::signer::get_signer()
        .ok_or("No signer available")?
        .as_nostr_signer();

    let pubkey_str = auth_store::get_pubkey()
        .ok_or("Not authenticated")?;
    let pubkey = PublicKey::parse(&pubkey_str)
        .map_err(|e| format!("Invalid pubkey: {}", e))?;

    let json_content = serde_json::to_string(&token_event_data)
        .map_err(|e| format!("Failed to serialize token event: {}", e))?;

    let encrypted = signer.nip44_encrypt(&pubkey, &json_content).await
        .map_err(|e| format!("Failed to encrypt token event: {}", e))?;

    let builder = nostr_sdk::EventBuilder::new(
        Kind::CashuWalletUnspentProof,
        encrypted
    );

    let client = nostr_client::NOSTR_CLIENT.read().as_ref()
        .ok_or("Client not initialized")?.clone();

    let event_output = client.send_event_builder(builder).await
        .map_err(|e| format!("Failed to publish event: {}", e))?;

    let event_id = event_output.id().to_hex();

    log::info!("Published token event: {}", event_id);

    // Update local state
    let store = WALLET_TOKENS.read();
    let mut data = store.data();
    let mut tokens = data.write();
    tokens.push(TokenData {
        event_id: event_id.clone(),
        mint: mint_url.clone(),
        unit: "sat".to_string(),
        proofs: proof_data.clone(),
        created_at: chrono::Utc::now().timestamp() as u64,
    });

    // Recalculate balance from all tokens using checked arithmetic
    let new_balance: u64 = tokens.iter()
        .flat_map(|t| &t.proofs)
        .map(|p| p.amount)
        .try_fold(0u64, |acc, amount| acc.checked_add(amount))
        .ok_or_else(|| "Balance calculation overflow in receive_tokens".to_string())?;

    // Update balance atomically while holding token lock
    *WALLET_BALANCE.write() = new_balance;
    drop(tokens);

    // Update balance
    let amount = u64::from(amount_received);

    log::info!("Balance after receive: {} sats", new_balance);

    // Create history event (kind 7376) with direction: "in"
    if let Err(e) = create_history_event("in", amount, vec![event_id.clone()], vec![]).await {
        log::error!("Failed to create history event: {}", e);
        // Don't fail the whole operation if history event creation fails
    }

    // Sync MultiMintWallet state (non-critical)
    if let Err(e) = crate::stores::cashu_cdk_bridge::sync_wallet_state().await {
        log::warn!("Failed to sync MultiMintWallet state after receive: {}", e);
    }

    Ok(amount)
}

// ============================================================================
// Phase 2C: Send Implementation
// ============================================================================

/// Send ecash tokens
pub async fn send_tokens(
    mint_url: String,
    amount: u64,
) -> Result<String, String> {
    use cdk::wallet::{SendOptions, SendKind};
    use cdk::Amount;
    use nostr_sdk::signer::NostrSigner;

    log::info!("Sending {} sats from {}", amount, mint_url);

    // Acquire mint operation lock to prevent concurrent sends
    let _lock_guard = try_acquire_mint_lock(&mint_url)
        .ok_or_else(|| format!("Another operation is in progress for mint: {}", mint_url))?;

    // Get available proofs and capture state snapshot while holding lock
    let (all_proofs, event_ids_to_delete) = {
        let store = WALLET_TOKENS.read();
        let data = store.data();
        let tokens = data.read();
        let mint_tokens: Vec<_> = tokens.iter()
            .filter(|t| t.mint == mint_url)
            .collect();

        if mint_tokens.is_empty() {
            return Err("No tokens found for this mint".to_string());
        }

        // Convert to CDK proofs
        let mut all_proofs = Vec::new();
        let mut event_ids_to_delete = Vec::new();

        for token in &mint_tokens {
            event_ids_to_delete.push(token.event_id.clone());
            for proof in &token.proofs {
                // Convert ProofData to CDK proof (now preserves witness/dleq)
                all_proofs.push(proof_data_to_cdk_proof(proof)?);
            }
        }

        (all_proofs, event_ids_to_delete)
    }; // Read lock is released - async operations happen without lock
       // This is safe because: 1) We've captured all_proofs, 2) Local state will be updated first
       // 3) If Nostr publish fails, local state remains valid and operation is queued for retry

    // Check balance
    let total_available: u64 = all_proofs.iter()
        .map(|p| u64::from(p.amount))
        .sum();

    if total_available < amount {
        return Err(format!(
            "Insufficient funds. Available: {} sats, Required: {} sats",
            total_available, amount
        ));
    }

    // Prepare and confirm send with auto-retry on spent proofs
    let (token_string, keep_proofs) = {
        // Try sending with current proofs
        let result = async {
            let wallet = create_ephemeral_wallet(&mint_url, all_proofs.clone()).await?;

            let prepared = wallet.prepare_send(
                Amount::from(amount),
                SendOptions {
                    conditions: None, // TODO: Support P2PK in Phase 2E
                    include_fee: true,
                    send_kind: SendKind::OnlineTolerance(Amount::from(1)),
                    ..Default::default()
                }
            ).await
            .map_err(|e| e.to_string())?;

            log::info!("Send fee: {} sats", u64::from(prepared.fee()));

            let token = prepared.confirm(None).await
                .map_err(|e| e.to_string())?;
            let keep_proofs = wallet.get_unspent_proofs().await
                .map_err(|e| e.to_string())?;

            Ok::<(cdk::nuts::Token, Vec<cdk::nuts::Proof>), String>((token, keep_proofs))
        }.await;

        match result {
            Ok((token, proofs)) => (token.to_string(), proofs),
            Err(e) => {
                let error_msg = e.to_string();

                // Auto-retry if proofs are already spent
                if error_msg.contains("already spent") || error_msg.contains("already redeemed") {
                    log::warn!("Some proofs already spent, cleaning up and retrying...");

                    // Cleanup spent proofs (use internal version since we already hold the lock)
                    let (cleaned_count, cleaned_amount) = cleanup_spent_proofs_internal(mint_url.clone()).await
                        .map_err(|e| format!("Cleanup failed: {}", e))?;

                    log::info!("Cleaned up {} spent proofs worth {} sats, retrying send", cleaned_count, cleaned_amount);

                    // Get fresh proofs after cleanup
                    let fresh_proofs = {
                        let store = WALLET_TOKENS.read();
                        let data = store.data();
                        let tokens = data.read();
                        let mut proofs = Vec::new();

                        for token in tokens.iter().filter(|t| t.mint == mint_url) {
                            for proof in &token.proofs {
                                let temp = ProofData {
                                    id: proof.id.clone(),
                                    amount: proof.amount,
                                    secret: proof.secret.clone(),
                                    c: proof.c.clone(),
                                    witness: None,
                                    dleq: None,
                                };
                                proofs.push(proof_data_to_cdk_proof(&temp)?);
                            }
                        }
                        proofs
                    };

                    // Check we still have enough after cleanup
                    let fresh_total: u64 = fresh_proofs.iter().map(|p| u64::from(p.amount)).sum();
                    if fresh_total < amount {
                        return Err(format!(
                            "Insufficient funds after cleanup. Available: {} sats, Required: {} sats",
                            fresh_total, amount
                        ));
                    }

                    // Retry send with fresh proofs
                    let wallet = create_ephemeral_wallet(&mint_url, fresh_proofs).await?;

                    let prepared = wallet.prepare_send(
                        Amount::from(amount),
                        SendOptions {
                            conditions: None,
                            include_fee: true,
                            send_kind: SendKind::OnlineTolerance(Amount::from(1)),
                            ..Default::default()
                        }
                    ).await
                    .map_err(|e| format!("Retry failed: {}", e))?;

                    let token = prepared.confirm(None).await
                        .map_err(|e| format!("Retry confirm failed: {}", e))?;

                    let keep_proofs = wallet.get_unspent_proofs().await
                        .map_err(|e| format!("Failed to get remaining proofs: {}", e))?;

                    log::info!("Send succeeded after cleanup and retry");
                    (token.to_string(), keep_proofs)
                } else {
                    return Err(format!("Failed to send: {}", e));
                }
            }
        }
    };

    // Prepare event data before updating local state
    let signer = crate::stores::signer::get_signer()
        .ok_or("No signer available")?
        .as_nostr_signer();

    let pubkey_str = auth_store::get_pubkey()
        .ok_or("Not authenticated")?;
    let pubkey = PublicKey::parse(&pubkey_str)
        .map_err(|e| format!("Invalid pubkey: {}", e))?;

    let client = nostr_client::NOSTR_CLIENT.read().as_ref()
        .ok_or("Client not initialized")?.clone();

    // Track the real event ID from Nostr publication (not synthetic)
    let mut new_event_id: Option<String> = None;

    // STEP 1: Publish token event first to get real EventId
    // This ensures we have a valid hex EventId for local state and history
    if !keep_proofs.is_empty() {
        let proof_data: Vec<ProofData> = keep_proofs.iter()
            .map(|p| cdk_proof_to_proof_data(p))
            .collect();

        // Convert to extended proofs with P2PK support
        let extended_proofs: Vec<ExtendedCashuProof> = proof_data.iter()
            .map(|p| ExtendedCashuProof::from(p.clone()))
            .collect();

        let token_event_data = ExtendedTokenEvent {
            mint: mint_url.clone(),
            unit: "sat".to_string(),
            proofs: extended_proofs,
            del: event_ids_to_delete.clone(),
        };

        let json_content = serde_json::to_string(&token_event_data)
            .map_err(|e| format!("Failed to serialize token event: {}", e))?;

        let encrypted = signer.nip44_encrypt(&pubkey, &json_content).await
            .map_err(|e| format!("Failed to encrypt token event: {}", e))?;

        let builder = nostr_sdk::EventBuilder::new(Kind::CashuWalletUnspentProof, encrypted);

        // Pre-compute event ID using UnsignedEvent before signing
        // Event ID is deterministic (SHA256 of pubkey + created_at + kind + tags + content)
        // This allows us to track the event ID even if publish fails
        let mut unsigned = builder.clone().build(pubkey);
        let event_id_hex = unsigned.id().to_hex();
        log::debug!("Pre-computed token event ID: {}", event_id_hex);

        // Sign the event
        let signed_event = unsigned.sign(&signer).await
            .map_err(|e| format!("Failed to sign token event: {}", e))?;

        // Try to publish - if it fails, queue the already-signed event
        match client.send_event(&signed_event).await {
            Ok(_) => {
                log::info!("Published new token event: {}", event_id_hex);
            }
            Err(e) => {
                log::warn!("Failed to publish token event, queuing for retry: {}", e);
                // Queue the signed event for retry
                queue_signed_event_for_retry(signed_event, PendingEventType::TokenEvent).await;
            }
        }

        // Always set event ID - it's valid regardless of publish success
        // The proofs are tracked locally with this ID and will sync on retry
        new_event_id = Some(event_id_hex);
    }

    // STEP 2: Update local state with the real event ID
    {
        let store = WALLET_TOKENS.read();
        let mut data = store.data();
        let mut tokens_write = data.write();

        // Remove old token events
        tokens_write.retain(|t| !event_ids_to_delete.contains(&t.event_id));

        // Add new token with remaining proofs (only if we have a real event ID)
        if let Some(ref event_id) = new_event_id {
            let keep_proof_data: Vec<ProofData> = keep_proofs.iter()
                .map(|p| cdk_proof_to_proof_data(p))
                .collect();

            tokens_write.push(TokenData {
                event_id: event_id.clone(),
                mint: mint_url.clone(),
                unit: "sat".to_string(),
                proofs: keep_proof_data,
                created_at: chrono::Utc::now().timestamp() as u64,
            });
        }

        // Update balance atomically
        let new_balance: u64 = tokens_write.iter()
            .flat_map(|t| &t.proofs)
            .map(|p| p.amount)
            .try_fold(0u64, |acc, amount| acc.checked_add(amount))
            .ok_or_else(|| "Balance calculation overflow in send_tokens".to_string())?;

        *WALLET_BALANCE.write() = new_balance;
        drop(tokens_write);

        log::info!("Local state updated. Balance after send: {} sats", new_balance);
    }

    // STEP 3: Publish deletion event for old token events
    if !event_ids_to_delete.is_empty() {
        // Filter to only valid hex event IDs before creating deletion tags
        let valid_event_ids: Vec<_> = event_ids_to_delete.iter()
            .filter(|id| EventId::from_hex(id).is_ok())
            .collect();

        if !valid_event_ids.is_empty() {
            let mut tags = Vec::new();
            for event_id in &valid_event_ids {
                // Safe to unwrap since we filtered above
                tags.push(nostr_sdk::Tag::event(
                    nostr_sdk::EventId::from_hex(event_id).unwrap()
                ));
            }

            // Add NIP-60 required tag
            tags.push(nostr_sdk::Tag::custom(
                nostr_sdk::TagKind::custom("k"),
                ["7375"]
            ));

            let deletion_builder = nostr_sdk::EventBuilder::new(
                Kind::from(5),
                "Spent token"
            ).tags(tags);

            match client.send_event_builder(deletion_builder.clone()).await {
                Ok(_) => {
                    log::info!("Published deletion events for {} token events", valid_event_ids.len());
                }
                Err(e) => {
                    log::warn!("Failed to publish deletion event, will queue for retry: {}", e);
                    queue_event_for_retry(deletion_builder, PendingEventType::DeletionEvent).await;
                }
            }
        }

        // Log warning for any invalid IDs that were skipped
        let invalid_count = event_ids_to_delete.len() - valid_event_ids.len();
        if invalid_count > 0 {
            log::warn!("Skipped {} invalid event IDs in deletion (non-hex format)", invalid_count);
        }
    }

    // STEP 4: Create history event with valid IDs only
    // Filter created tokens to only include valid hex EventIds
    let valid_created: Vec<String> = new_event_id.iter().cloned().collect();
    let valid_destroyed: Vec<String> = event_ids_to_delete.iter()
        .filter(|id| EventId::from_hex(id).is_ok())
        .cloned()
        .collect();

    if let Err(e) = create_history_event("out", amount, valid_created, valid_destroyed).await {
        log::error!("Failed to create history event: {}", e);
    }

    // Sync MultiMintWallet state (non-critical)
    if let Err(e) = crate::stores::cashu_cdk_bridge::sync_wallet_state().await {
        log::warn!("Failed to sync MultiMintWallet state after send: {}", e);
    }

    Ok(token_string)
}

/// Send ecash tokens locked to a recipient's public key (P2PK / NUT-11)
///
/// This creates tokens that can only be spent by the holder of the corresponding
/// private key. The recipient can be specified as:
/// - A hex pubkey (64 chars)
/// - An npub (bech32 encoded)
///
/// The resulting token can only be redeemed by signing with the recipient's key.
pub async fn send_tokens_p2pk(
    mint_url: String,
    amount: u64,
    recipient_pubkey: String,
) -> Result<String, String> {
    use cdk::wallet::{SendOptions, SendKind};
    use cdk::nuts::SpendingConditions;
    use cdk::Amount;
    use nostr_sdk::signer::NostrSigner;

    log::info!("Sending {} sats P2PK to {} from {}", amount, recipient_pubkey, mint_url);

    // Parse recipient pubkey (support both hex and npub)
    let recipient_hex = if recipient_pubkey.starts_with("npub") {
        // Decode npub to hex
        nostr_sdk::PublicKey::parse(&recipient_pubkey)
            .map_err(|e| format!("Invalid npub: {}", e))?
            .to_hex()
    } else {
        recipient_pubkey.clone()
    };

    // Convert to CDK PublicKey (uses secp256k1 directly, not nostr format)
    let cdk_pubkey = cdk::nuts::PublicKey::from_hex(&recipient_hex)
        .map_err(|e| format!("Invalid recipient pubkey: {}", e))?;

    // Create P2PK spending conditions
    let spending_conditions = SpendingConditions::new_p2pk(cdk_pubkey, None);

    // Acquire mint operation lock
    let _lock_guard = try_acquire_mint_lock(&mint_url)
        .ok_or_else(|| format!("Another operation is in progress for mint: {}", mint_url))?;

    // Get available proofs
    let (all_proofs, event_ids_to_delete) = {
        let store = WALLET_TOKENS.read();
        let data = store.data();
        let tokens = data.read();
        let mint_tokens: Vec<_> = tokens.iter()
            .filter(|t| t.mint == mint_url)
            .collect();

        if mint_tokens.is_empty() {
            return Err("No tokens found for this mint".to_string());
        }

        let mut all_proofs = Vec::new();
        let mut event_ids_to_delete = Vec::new();

        for token in &mint_tokens {
            event_ids_to_delete.push(token.event_id.clone());
            for proof in &token.proofs {
                all_proofs.push(proof_data_to_cdk_proof(proof)?);
            }
        }

        (all_proofs, event_ids_to_delete)
    };

    // Check balance
    let total_available: u64 = all_proofs.iter()
        .map(|p| u64::from(p.amount))
        .sum();

    if total_available < amount {
        return Err(format!(
            "Insufficient funds. Available: {} sats, Required: {} sats",
            total_available, amount
        ));
    }

    // Create wallet and send with P2PK conditions
    let wallet = create_ephemeral_wallet(&mint_url, all_proofs.clone()).await?;

    let prepared = wallet.prepare_send(
        Amount::from(amount),
        SendOptions {
            conditions: Some(spending_conditions),
            include_fee: true,
            send_kind: SendKind::OnlineTolerance(Amount::from(1)),
            ..Default::default()
        }
    ).await
    .map_err(|e| format!("Failed to prepare P2PK send: {}", e))?;

    log::info!("P2PK send fee: {} sats", u64::from(prepared.fee()));

    let token = prepared.confirm(None).await
        .map_err(|e| format!("Failed to confirm P2PK send: {}", e))?;

    let keep_proofs = wallet.get_unspent_proofs().await
        .map_err(|e| format!("Failed to get remaining proofs: {}", e))?;

    let token_string = token.to_string();

    // Update local state (same as regular send)
    let signer = crate::stores::signer::get_signer()
        .ok_or("No signer available")?
        .as_nostr_signer();

    let pubkey_str = auth_store::get_pubkey()
        .ok_or("Not authenticated")?;
    let pubkey = PublicKey::parse(&pubkey_str)
        .map_err(|e| format!("Invalid pubkey: {}", e))?;

    let client = nostr_client::NOSTR_CLIENT.read().as_ref()
        .ok_or("Client not initialized")?.clone();

    let mut new_event_id: Option<String> = None;

    // Publish remaining proofs as token event
    if !keep_proofs.is_empty() {
        let proof_data: Vec<ProofData> = keep_proofs.iter()
            .map(|p| cdk_proof_to_proof_data(p))
            .collect();

        let extended_proofs: Vec<ExtendedCashuProof> = proof_data.iter()
            .map(|p| ExtendedCashuProof::from(p.clone()))
            .collect();

        let token_event_data = ExtendedTokenEvent {
            mint: mint_url.clone(),
            unit: "sat".to_string(),
            proofs: extended_proofs,
            del: event_ids_to_delete.clone(),
        };

        let json_content = serde_json::to_string(&token_event_data)
            .map_err(|e| format!("Failed to serialize token event: {}", e))?;

        let encrypted = signer.nip44_encrypt(&pubkey, &json_content).await
            .map_err(|e| format!("Failed to encrypt: {}", e))?;

        let builder = nostr_sdk::EventBuilder::new(Kind::CashuWalletUnspentProof, encrypted);

        match client.send_event_builder(builder).await {
            Ok(output) => {
                new_event_id = Some(output.id().to_hex());
                log::info!("Published P2PK send token event: {}", output.id().to_hex());
            }
            Err(e) => {
                log::warn!("Failed to publish token event: {}", e);
            }
        }
    }

    // Update local state
    {
        let store = WALLET_TOKENS.read();
        let mut data_signal = store.data();
        let mut tokens = data_signal.write();

        // Remove old tokens
        tokens.retain(|t| !event_ids_to_delete.contains(&t.event_id));

        // Add new token if we have change
        if !keep_proofs.is_empty() {
            let proof_data: Vec<ProofData> = keep_proofs.iter()
                .map(|p| cdk_proof_to_proof_data(p))
                .collect();

            tokens.push(TokenData {
                event_id: new_event_id.clone().unwrap_or_default(),
                mint: mint_url.clone(),
                unit: "sat".to_string(),
                proofs: proof_data,
                created_at: chrono::Utc::now().timestamp() as u64,
            });
        }

        // Update balance
        let new_balance: u64 = tokens.iter()
            .flat_map(|t| &t.proofs)
            .map(|p| p.amount)
            .fold(0u64, |acc, amount| acc.saturating_add(amount));
        *WALLET_BALANCE.write() = new_balance;
    }

    // Create history event
    let valid_created: Vec<String> = new_event_id.iter().cloned().collect();
    let valid_destroyed: Vec<String> = event_ids_to_delete.iter()
        .filter(|id| id.len() == 64)
        .cloned()
        .collect();

    if let Err(e) = create_history_event("out", amount, valid_created, valid_destroyed).await {
        log::error!("Failed to create history event: {}", e);
    }

    // Sync MultiMintWallet state (non-critical)
    if let Err(e) = crate::stores::cashu_cdk_bridge::sync_wallet_state().await {
        log::warn!("Failed to sync MultiMintWallet state after P2PK send: {}", e);
    }

    log::info!("P2PK send complete: {} sats locked to {}", amount, recipient_hex);

    Ok(token_string)
}

/// Get the wallet's P2PK public key for receiving locked tokens
///
/// Returns the hex-encoded public key derived from the wallet's private key.
/// Senders can lock tokens to this key using send_tokens_p2pk.
pub fn get_wallet_pubkey() -> Result<String, String> {
    let wallet_state = WALLET_STATE.read();
    let state = wallet_state.as_ref().ok_or("Wallet not initialized")?;

    // Parse the wallet privkey to get the pubkey
    let secret_key = cdk::nuts::SecretKey::from_hex(&state.privkey)
        .map_err(|e| format!("Invalid wallet privkey: {}", e))?;

    let pubkey = secret_key.public_key();
    Ok(pubkey.to_hex())
}

// ============================================================================
// Phase 2D: Cleanup & Error Handling
// ============================================================================

/// Create a history event (kind 7376)
///
/// Per NIP-60, spending history events have:
/// - Encrypted content: direction, amount, created/destroyed event references
/// - Unencrypted tags: redeemed event references (for P2PK redemptions)
///
/// The `redeemed_tokens` parameter should contain event IDs of token events
/// that were redeemed via P2PK spending conditions.
async fn create_history_event(
    direction: &str,
    amount: u64,
    created_tokens: Vec<String>,
    destroyed_tokens: Vec<String>,
) -> Result<(), String> {
    // Delegate to the full function with empty redeemed list
    create_history_event_full(direction, amount, created_tokens, destroyed_tokens, vec![]).await
}

/// Create a history event with full control over all fields including redeemed tokens
///
/// Per NIP-60 spec and rust-nostr SDK:
/// - `created` and `destroyed` go in encrypted content
/// - `redeemed` goes in unencrypted tags (for P2PK redemption tracking)
async fn create_history_event_full(
    direction: &str,
    amount: u64,
    created_tokens: Vec<String>,
    destroyed_tokens: Vec<String>,
    redeemed_tokens: Vec<String>,
) -> Result<(), String> {
    use nostr_sdk::signer::NostrSigner;

    let signer = crate::stores::signer::get_signer()
        .ok_or("No signer available")?
        .as_nostr_signer();

    let pubkey_str = auth_store::get_pubkey()
        .ok_or("Not authenticated")?;
    let pubkey = PublicKey::parse(&pubkey_str)
        .map_err(|e| format!("Invalid pubkey: {}", e))?;

    // Build spending history using rust-nostr's SpendingHistory
    let direction_enum = match direction {
        "in" => TransactionDirection::In,
        "out" => TransactionDirection::Out,
        _ => return Err("Invalid direction".to_string()),
    };

    let mut spending_history = SpendingHistory::new(direction_enum, amount);

    // Add created event IDs (skip invalid IDs with warning)
    for token_id in created_tokens {
        match EventId::from_hex(&token_id) {
            Ok(event_id) => {
                spending_history = spending_history.add_created(event_id);
            }
            Err(_) => {
                log::warn!(
                    "Skipping invalid created token ID in history event: {} (direction={}, amount={})",
                    token_id, direction, amount
                );
            }
        }
    }

    // Add destroyed event IDs (skip invalid IDs with warning)
    for token_id in destroyed_tokens {
        match EventId::from_hex(&token_id) {
            Ok(event_id) => {
                spending_history = spending_history.add_destroyed(event_id);
            }
            Err(_) => {
                log::warn!(
                    "Skipping invalid destroyed token ID in history event: {} (direction={}, amount={})",
                    token_id, direction, amount
                );
            }
        }
    }

    // Add redeemed event IDs (skip invalid IDs with warning)
    for token_id in redeemed_tokens {
        match EventId::from_hex(&token_id) {
            Ok(event_id) => {
                spending_history = spending_history.add_redeemed(event_id);
            }
            Err(_) => {
                log::warn!(
                    "Skipping invalid redeemed token ID in history event: {} (direction={}, amount={})",
                    token_id, direction, amount
                );
            }
        }
    }

    // Build encrypted content manually (keeping signer pattern)
    // following rust-nostr's internal format
    // Convert to Strings first to avoid lifetime issues with Vec<Vec<&str>>
    let mut content_data: Vec<Vec<String>> = vec![
        vec!["direction".to_string(), spending_history.direction.to_string()],
        vec!["amount".to_string(), spending_history.amount.to_string()],
    ];

    // Add created event references (encrypted)
    for event_id in &spending_history.created {
        content_data.push(vec![
            "e".to_string(),
            event_id.to_hex(),
            String::new(),
            "created".to_string()
        ]);
    }

    // Add destroyed event references (encrypted)
    for event_id in &spending_history.destroyed {
        content_data.push(vec![
            "e".to_string(),
            event_id.to_hex(),
            String::new(),
            "destroyed".to_string()
        ]);
    }

    let json_content = serde_json::to_string(&content_data)
        .map_err(|e| format!("Failed to serialize history event: {}", e))?;

    let encrypted = signer.nip44_encrypt(&pubkey, &json_content).await
        .map_err(|e| format!("Failed to encrypt history event: {}", e))?;

    // Build unencrypted tags for redeemed events (per NIP-60 spec)
    // These are public so other clients can verify P2PK redemptions
    let mut tags = Vec::new();
    for event_id in &spending_history.redeemed {
        tags.push(nostr_sdk::Tag::parse([
            "e".to_string(),
            event_id.to_hex(),
            String::new(),
            "redeemed".to_string()
        ]).map_err(|e| format!("Failed to create redeemed tag: {}", e))?);
    }

    let builder = nostr_sdk::EventBuilder::new(
        Kind::CashuWalletSpendingHistory,
        encrypted
    ).tags(tags);

    let client = nostr_client::NOSTR_CLIENT.read().as_ref()
        .ok_or("Client not initialized")?.clone();

    client.send_event_builder(builder).await
        .map_err(|e| format!("Failed to publish history event: {}", e))?;

    log::info!("Created history event: {} {} sats (created: {}, destroyed: {}, redeemed: {})",
        direction, amount,
        spending_history.created.len(),
        spending_history.destroyed.len(),
        spending_history.redeemed.len()
    );

    Ok(())
}

/// Check and cleanup spent proofs for a mint
/// Returns the number of proofs cleaned up and the amount
pub async fn cleanup_spent_proofs(mint_url: String) -> Result<(usize, u64), String> {
    // Acquire mint operation lock to prevent concurrent operations
    let _lock_guard = try_acquire_mint_lock(&mint_url)
        .ok_or_else(|| format!("Another operation is in progress for mint: {}", mint_url))?;

    cleanup_spent_proofs_internal(mint_url).await
}

/// Internal cleanup function that assumes caller already holds the lock
async fn cleanup_spent_proofs_internal(mint_url: String) -> Result<(usize, u64), String> {
    use cdk::nuts::State;
    use nostr_sdk::signer::NostrSigner;

    log::info!("Checking for spent proofs on {}", mint_url);

    // Get all token events and proofs for this mint (scope the read to drop lock early)
    let (cdk_proofs, event_ids_to_delete, all_mint_proofs) = {
        let store = WALLET_TOKENS.read();
        let data = store.data();
        let tokens = data.read();
        let mint_tokens: Vec<_> = tokens.iter()
            .filter(|t| t.mint == mint_url)
            .collect();

        if mint_tokens.is_empty() {
            log::info!("No proofs to check");
            return Ok((0, 0));
        }

        let event_ids: Vec<String> = mint_tokens.iter()
            .map(|t| t.event_id.clone())
            .collect();

        let all_proofs: Vec<ProofData> = mint_tokens.iter()
            .flat_map(|t| &t.proofs)
            .cloned()
            .collect();

        // Convert to CDK proofs (now preserves witness/DLEQ)
        let cdk_proofs: Result<Vec<_>, _> = all_proofs.iter()
            .map(|p| proof_data_to_cdk_proof(p))
            .collect();

        (cdk_proofs?, event_ids, all_proofs)
    }; // Read lock dropped here

    // Create ephemeral wallet
    let wallet = create_ephemeral_wallet(&mint_url, vec![]).await?;

    // Check states at mint
    let states = wallet.check_proofs_spent(cdk_proofs.clone()).await
        .map_err(|e| format!("Failed to check proof states: {}", e))?;

    // Find unavailable proofs (spent, reserved, or pending)
    // - Spent: Already redeemed at mint
    // - Reserved: Held for a pending melt operation
    // - Pending: In process of being spent
    let mut unavailable_secrets = std::collections::HashSet::new();
    let mut unavailable_amount = 0u64;

    for (state, proof) in states.iter().zip(cdk_proofs.iter()) {
        if matches!(state.state, State::Spent | State::Reserved | State::Pending) {
            unavailable_secrets.insert(proof.secret.to_string());
            unavailable_amount += u64::from(proof.amount);
        }
    }

    if unavailable_secrets.is_empty() {
        log::info!("No spent/reserved/pending proofs found");
        return Ok((0, 0));
    }

    let unavailable_count = unavailable_secrets.len();
    log::info!("Found {} unavailable proofs worth {} sats, cleaning up", unavailable_count, unavailable_amount);

    // Filter to keep only available proofs (not spent/reserved/pending)
    let available_proofs: Vec<ProofData> = all_mint_proofs.into_iter()
        .filter(|p| !unavailable_secrets.contains(&p.secret))
        .collect();

    // Get signer and pubkey for creating events
    let signer = crate::stores::signer::get_signer()
        .ok_or("No signer available")?
        .as_nostr_signer();

    let pubkey_str = auth_store::get_pubkey()
        .ok_or("Not authenticated")?;
    let pubkey = PublicKey::parse(&pubkey_str)
        .map_err(|e| format!("Invalid pubkey: {}", e))?;

    let client = nostr_client::NOSTR_CLIENT.read().as_ref()
        .ok_or("Client not initialized")?.clone();

    let mut new_event_id: Option<String> = None;

    // Create new token event with available proofs if any remain
    if !available_proofs.is_empty() {
        let proof_data: Vec<ProofData> = available_proofs.iter()
            .map(|p| ProofData {
                id: p.id.clone(),
                amount: p.amount,
                secret: p.secret.clone(),
                c: p.c.clone(),
                witness: None,
                dleq: None,
            })
            .collect();

        // Convert to extended proofs with P2PK support
        let extended_proofs: Vec<ExtendedCashuProof> = proof_data.iter()
            .map(|p| ExtendedCashuProof::from(p.clone()))
            .collect();

        let token_event_data = ExtendedTokenEvent {
            mint: mint_url.clone(),
            unit: "sat".to_string(),
            proofs: extended_proofs,
            del: event_ids_to_delete.clone(),
        };

        let json_content = serde_json::to_string(&token_event_data)
            .map_err(|e| format!("Failed to serialize token event: {}", e))?;

        let encrypted = signer.nip44_encrypt(&pubkey, &json_content).await
            .map_err(|e| format!("Failed to encrypt token event: {}", e))?;

        let builder = nostr_sdk::EventBuilder::new(
            Kind::CashuWalletUnspentProof,
            encrypted
        );

        let event_output = client.send_event_builder(builder).await
            .map_err(|e| format!("Failed to publish token event: {}", e))?;

        new_event_id = Some(event_output.id().to_hex());
        log::info!("Published new token event with {} available proofs: {}", available_proofs.len(), new_event_id.as_ref().unwrap());
    }

    // Delete old token events with kind-5
    if !event_ids_to_delete.is_empty() {
        let mut tags = Vec::new();
        for event_id in &event_ids_to_delete {
            tags.push(nostr_sdk::Tag::event(
                nostr_sdk::EventId::from_hex(event_id)
                    .map_err(|e| format!("Invalid event ID: {}", e))?
            ));
        }

        // Add NIP-60 required tag to indicate we're deleting kind 7375 events
        tags.push(nostr_sdk::Tag::custom(
            nostr_sdk::TagKind::custom("k"),
            ["7375"]
        ));

        let deletion_builder = nostr_sdk::EventBuilder::new(
            Kind::from(5),
            "Cleaned up spent proofs"
        ).tags(tags);

        client.send_event_builder(deletion_builder).await
            .map_err(|e| format!("Failed to publish deletion event: {}", e))?;

        log::info!("Published deletion events for {} token events", event_ids_to_delete.len());
    }

    // Update local state
    let store = WALLET_TOKENS.read();
    let mut data = store.data();
    let mut tokens_write = data.write();

    // Remove old tokens for this mint
    tokens_write.retain(|t| t.mint != mint_url);

    // Add new token with available proofs if any
    if let Some(ref event_id) = new_event_id {
        tokens_write.push(TokenData {
            event_id: event_id.clone(),
            mint: mint_url.clone(),
            unit: "sat".to_string(),
            proofs: available_proofs,
            created_at: chrono::Utc::now().timestamp() as u64,
        });
    }

    // Recalculate balance from all tokens using checked arithmetic
    let new_balance: u64 = tokens_write.iter()
        .flat_map(|t| &t.proofs)
        .map(|p| p.amount)
        .try_fold(0u64, |acc, amount| acc.checked_add(amount))
        .ok_or_else(|| "Balance calculation overflow in cleanup_spent_proofs".to_string())?;

    drop(tokens_write);

    // Update balance
    *WALLET_BALANCE.write() = new_balance;

    log::info!("Cleanup complete. Removed {} proofs worth {} sats. New balance: {} sats",
        unavailable_count, unavailable_amount, new_balance);

    Ok((unavailable_count, unavailable_amount))
}

/// Remove a mint and all its associated tokens from the wallet
/// Creates deletion events for all token events from this mint
/// Returns (event_count, total_amount) of removed tokens
pub async fn remove_mint(mint_url: String) -> Result<(usize, u64), String> {
    log::info!("Removing mint: {}", mint_url);

    // Get all token events for this mint (scoped read)
    let (event_ids_to_delete, total_amount, token_count) = {
        let store = WALLET_TOKENS.read();
        let data = store.data();
        let tokens = data.read();
        let mint_tokens: Vec<_> = tokens.iter()
            .filter(|t| t.mint == mint_url)
            .collect();

        let event_ids: Vec<String> = mint_tokens.iter()
            .map(|t| t.event_id.clone())
            .collect();

        let amount: u64 = mint_tokens.iter()
            .flat_map(|t| &t.proofs)
            .map(|p| p.amount)
            .sum();

        (event_ids, amount, mint_tokens.len())
    }; // Read lock dropped

    log::info!("Found {} token events worth {} sats to remove", token_count, total_amount);

    // Only create deletion event if there are tokens to delete
    if !event_ids_to_delete.is_empty() {
        // Create kind-5 deletion event for all token events
        let mut tags = Vec::new();
        for event_id in &event_ids_to_delete {
            tags.push(nostr_sdk::Tag::event(
                nostr_sdk::EventId::parse(event_id)
                    .map_err(|e| format!("Invalid event ID: {}", e))?
            ));
        }

        // Add NIP-60 required tag to indicate we're deleting kind 7375 events
        tags.push(nostr_sdk::Tag::custom(
            nostr_sdk::TagKind::custom("k"),
            ["7375"]
        ));

        let deletion_builder = nostr_sdk::EventBuilder::new(
            Kind::from(5),
            format!("Removed mint: {}", mint_url)
        ).tags(tags);

        let client = nostr_client::NOSTR_CLIENT.read().as_ref()
            .ok_or("Client not initialized")?.clone();

        client.send_event_builder(deletion_builder).await
            .map_err(|e| format!("Failed to publish deletion event: {}", e))?;

        log::info!("Published deletion event for {} token events", event_ids_to_delete.len());
    }

    // Update local state - remove all tokens for this mint
    let store = WALLET_TOKENS.read();
    let mut data = store.data();
    let mut tokens_write = data.write();
    tokens_write.retain(|t| t.mint != mint_url);
    drop(tokens_write);

    // Remove mint from wallet state
    {
        let mut state_write = WALLET_STATE.write();
        if let Some(ref mut state) = *state_write {
            state.mints.retain(|m| m != &mint_url);
        }
    }

    // Update wallet event on relays to persist the mint removal
    {
        use nostr_sdk::signer::NostrSigner;

        let wallet_state = WALLET_STATE.read().clone();
        if let Some(ref state) = wallet_state {
            let signer = crate::stores::signer::get_signer()
                .ok_or("No signer available")?
                .as_nostr_signer();

            let pubkey_str = auth_store::get_pubkey()
                .ok_or("Not authenticated")?;
            let pubkey = PublicKey::parse(&pubkey_str)
                .map_err(|e| format!("Invalid pubkey: {}", e))?;

            let client = nostr_client::NOSTR_CLIENT.read().as_ref()
                .ok_or("Client not initialized")?.clone();

            // Build wallet event content array following NIP-60 format
            let mut content_array: Vec<Vec<&str>> = vec![vec!["privkey", &state.privkey]];
            for mint in state.mints.iter() {
                content_array.push(vec!["mint", mint.as_str()]);
            }

            let json_content = serde_json::to_string(&content_array)
                .map_err(|e| format!("Failed to serialize wallet data: {}", e))?;

            let encrypted = signer.nip44_encrypt(&pubkey, &json_content).await
                .map_err(|e| format!("Failed to encrypt: {}", e))?;

            let builder = nostr_sdk::EventBuilder::new(Kind::CashuWallet, encrypted);

            match client.send_event_builder(builder).await {
                Ok(_) => log::info!("Published updated wallet event without removed mint"),
                Err(e) => log::warn!("Failed to publish updated wallet event: {}", e),
            }
        }
    }

    // Recalculate balance using checked arithmetic
    let store = WALLET_TOKENS.read();
    let data = store.data();
    let tokens = data.read();
    let new_balance: u64 = tokens.iter()
        .flat_map(|t| &t.proofs)
        .map(|p| p.amount)
        .try_fold(0u64, |acc, amount| acc.checked_add(amount))
        .ok_or_else(|| "Balance calculation overflow in remove_mint".to_string())?;
    drop(tokens);

    *WALLET_BALANCE.write() = new_balance;

    // Clear the wallet cache for this mint
    clear_wallet_cache(&mint_url);

    // Also remove from MultiMintWallet if initialized
    if let Err(e) = cashu_cdk_bridge::remove_mint(&mint_url).await {
        log::debug!("Note: {}", e); // Not critical if MultiMintWallet wasn't initialized
    }

    log::info!("Mint removed. Deleted {} events worth {} sats. New balance: {} sats",
        token_count, total_amount, new_balance);

    Ok((token_count, total_amount))
}

// ============================================================================
// Phase 3: Lightning Payment Support (Mint & Melt Operations)
// ============================================================================

/// Mint quote information for receiving lightning payments
#[derive(Clone, Debug, PartialEq)]
pub struct MintQuoteInfo {
    pub quote_id: String,
    pub invoice: String,
    pub amount: u64,
    pub expiry: u64,
    pub mint_url: String,
}

/// Melt quote information for sending lightning payments
#[derive(Clone, Debug, PartialEq)]
pub struct MeltQuoteInfo {
    pub quote_id: String,
    pub invoice: String,
    pub amount: u64,
    pub fee_reserve: u64,
    pub mint_url: String,
}

/// Mint information for display
#[derive(Clone, Debug, Default)]
pub struct MintInfoDisplay {
    pub name: Option<String>,
    pub description: Option<String>,
    pub description_long: Option<String>,
    pub supported_nuts: Vec<u8>,
    pub contact: Vec<(String, String)>,
    pub motd: Option<String>,
    pub version: Option<String>,
}

/// NIP-87: Discovered Cashu mint from kind:38172 events
#[derive(Clone, Debug, PartialEq)]
pub struct DiscoveredMint {
    /// Mint URL
    pub url: String,
    /// Mint name (from content metadata or kind:0)
    pub name: Option<String>,
    /// Mint description
    pub description: Option<String>,
    /// Supported NUTs as comma-separated string
    pub nuts: Option<String>,
    /// Network (mainnet, testnet, etc.)
    pub network: Option<String>,
    /// Mint pubkey (d tag)
    pub mint_pubkey: Option<String>,
    /// Event author pubkey
    pub author_pubkey: String,
    /// Number of recommendations
    pub recommendation_count: usize,
    /// Recommenders (pubkeys of users who recommended this mint)
    pub recommenders: Vec<String>,
    /// Detailed recommendations with comments
    pub recommendations: Vec<MintRecommendation>,
}

/// NIP-87: Mint recommendation from kind:38000 events.
#[derive(Clone, Debug, PartialEq)]
pub struct MintRecommendation {
    /// Recommender pubkey
    pub recommender: String,
    /// Review/comment content
    pub content: String,
}

/// Quote status for polling
#[derive(Clone, Debug, PartialEq)]
#[allow(dead_code)] // Some variants reserved for future use
pub enum QuoteStatus {
    Unpaid,
    Paid,
    Pending,
    Failed,
    Expired,
}

/// Melt (Lightning payment) progress status for UI feedback
#[derive(Clone, Debug, PartialEq)]
pub enum MeltProgress {
    /// Creating melt quote with mint
    CreatingQuote,
    /// Quote created, ready to pay
    QuoteCreated { quote_id: String, amount: u64, fee: u64 },
    /// Preparing proofs for payment
    PreparingPayment,
    /// Sending payment to Lightning Network
    PayingInvoice,
    /// Waiting for payment confirmation
    WaitingForConfirmation,
    /// Payment completed successfully
    Completed { preimage: Option<String>, fee_paid: u64 },
    /// Payment failed
    Failed { error: String },
}

/// Store for pending mint quotes with fine-grained reactivity
#[derive(Clone, Debug, Default, Store)]
pub struct PendingMintQuotesStore {
    pub data: Vec<MintQuoteInfo>,
}

/// Store for pending melt quotes with fine-grained reactivity
#[derive(Clone, Debug, Default, Store)]
pub struct PendingMeltQuotesStore {
    pub data: Vec<MeltQuoteInfo>,
}

/// Global signal for pending mint quotes
pub static PENDING_MINT_QUOTES: GlobalSignal<Store<PendingMintQuotesStore>> =
    Signal::global(|| Store::new(PendingMintQuotesStore::default()));

/// Global signal for pending melt quotes
pub static PENDING_MELT_QUOTES: GlobalSignal<Store<PendingMeltQuotesStore>> =
    Signal::global(|| Store::new(PendingMeltQuotesStore::default()));

/// Global signal for melt (Lightning payment) progress
/// Used by UI to show real-time payment status
pub static MELT_PROGRESS: GlobalSignal<Option<MeltProgress>> = Signal::global(|| None);

/// Create a mint quote (request lightning invoice to receive sats)
pub async fn create_mint_quote(
    mint_url: String,
    amount_sats: u64,
    description: Option<String>,
) -> Result<MintQuoteInfo, String> {
    use cdk::Amount;

    log::info!("Creating mint quote for {} sats at {}", amount_sats, mint_url);

    // Create ephemeral wallet
    let wallet = create_ephemeral_wallet(&mint_url, vec![]).await?;

    // Create mint quote
    let quote = wallet.mint_quote(
        Amount::from(amount_sats),
        description
    ).await
    .map_err(|e| format!("Failed to create mint quote: {}", e))?;

    log::info!("Mint quote created: {}", quote.id);

    let quote_info = MintQuoteInfo {
        quote_id: quote.id.clone(),
        invoice: quote.request,
        amount: u64::from(quote.amount.unwrap_or(Amount::ZERO)),
        expiry: quote.expiry,
        mint_url: mint_url.clone(),
    };

    // Store in global state for tracking
    PENDING_MINT_QUOTES.read().data().write().push(quote_info.clone());

    // Publish quote event to Nostr (NIP-60 kind 7374) for cross-device sync
    // This is optional per spec, so we don't fail if publishing fails
    match publish_quote_event(&quote.id, &mint_url, 14).await {
        Ok(event_id) => {
            log::info!("Quote event published: {}", event_id);
        }
        Err(e) => {
            log::warn!("Failed to publish quote event: {}", e);
            // Continue anyway - quote is usable locally even if publishing fails
        }
    }

    Ok(quote_info)
}

/// Check mint quote payment status
pub async fn check_mint_quote_status(
    mint_url: String,
    quote_id: String,
) -> Result<QuoteStatus, String> {
    log::info!("Checking mint quote status: {}", quote_id);

    // Create ephemeral wallet
    let wallet = create_ephemeral_wallet(&mint_url, vec![]).await?;

    // Check quote status
    let response = wallet.mint_quote_state(&quote_id).await
        .map_err(|e| format!("Failed to check mint quote status: {}", e))?;

    use cdk::nuts::MintQuoteState;

    let status = match response.state {
        MintQuoteState::Unpaid => QuoteStatus::Unpaid,
        MintQuoteState::Paid => QuoteStatus::Paid,
        MintQuoteState::Issued => QuoteStatus::Paid, // Already minted
    };

    log::info!("Quote {} status: {:?}", quote_id, status);

    Ok(status)
}

/// Mint tokens from a paid quote
pub async fn mint_tokens_from_quote(
    mint_url: String,
    quote_id: String,
) -> Result<u64, String> {
    use cdk::nuts::MintQuoteState;

    log::info!("Minting tokens from quote: {}", quote_id);

    // Create ephemeral wallet (now shares database with the wallet that created the quote)
    let wallet = create_ephemeral_wallet(&mint_url, vec![]).await?;

    // Verify the quote is paid and ready to mint
    let quote_response = wallet.mint_quote_state(&quote_id).await
        .map_err(|e| format!("Failed to fetch quote state: {}", e))?;

    log::info!("Quote state: {:?}, amount: {:?}", quote_response.state, quote_response.amount);

    // Verify the quote is paid AND not already issued
    match quote_response.state {
        MintQuoteState::Paid => {
            // Good to proceed
        }
        MintQuoteState::Issued => {
            return Err("Quote has already been minted. Tokens were already issued for this payment.".to_string());
        }
        MintQuoteState::Unpaid => {
            return Err("Quote has not been paid yet. Please pay the lightning invoice first.".to_string());
        }
    }

    log::info!("Quote is paid, proceeding to mint tokens");

    // Mint tokens - the quote is already in the shared database from create_mint_quote
    let proofs = match wallet.mint(
        &quote_id,
        cdk::amount::SplitTarget::default(),
        None  // No spending conditions (TODO: support P2PK in future)
    ).await {
        Ok(proofs) => {
            log::info!("Mint succeeded, received {} proofs", proofs.len());
            proofs
        }
        Err(e) => {
            let error_msg = e.to_string();
            log::error!("Mint failed with error: {:?}", e);
            log::error!("Error message: {}", error_msg);

            // CRITICAL: Clean up the quote from shared database even on failure
            // Why: The mint might have partially processed the blinded messages before failing
            // If the mint marked them as "used" but didn't return signatures, retrying with
            // the same quote will fail with "Blinded Message is already signed"
            if let Err(cleanup_err) = wallet.localstore.remove_mint_quote(&quote_id).await {
                log::warn!("Failed to remove mint quote after error: {}", cleanup_err);
            } else {
                log::info!("Cleaned up failed quote {} from database", quote_id);
            }

            // Also remove from pending quotes signal
            PENDING_MINT_QUOTES.read().data().write().retain(|q| q.quote_id != quote_id);

            // If the quote lookup fails, it means the shared database lost the quote somehow
            // 1. The quote was already used/issued
            // 2. The quote is not in the correct state
            // 3. The request was malformed

            if error_msg.contains("missing field `signatures`") {
                // Try to extract more context about what went wrong
                if error_msg.contains("line 1 column") {
                    return Err(format!(
                        "Mint returned an error instead of tokens. The quote has been cleaned up. \
                        Please generate a NEW invoice and try again (do not retry with the same invoice). \
                        Technical error: {}",
                        error_msg
                    ));
                }
            }

            return Err(format!("Failed to mint tokens: {}", error_msg));
        }
    };

    let amount_minted: u64 = proofs.iter()
        .map(|p| u64::from(p.amount))
        .sum();

    log::info!("Minted {} sats", amount_minted);

    // Convert to ProofData and then ExtendedCashuProof
    let proof_data: Vec<ProofData> = proofs.iter()
        .map(|p| cdk_proof_to_proof_data(p))
        .collect();

    // Create extended token event with P2PK support (same as receive_tokens)
    let extended_proofs: Vec<ExtendedCashuProof> = proof_data.iter()
        .map(|p| ExtendedCashuProof::from(p.clone()))
        .collect();

    let token_event_data = ExtendedTokenEvent {
        mint: mint_url.clone(),
        unit: "sat".to_string(),
        proofs: extended_proofs,
        del: vec![],
    };

    let signer = crate::stores::signer::get_signer()
        .ok_or("No signer available")?
        .as_nostr_signer();

    let pubkey_str = auth_store::get_pubkey()
        .ok_or("Not authenticated")?;
    let pubkey = PublicKey::parse(&pubkey_str)
        .map_err(|e| format!("Invalid pubkey: {}", e))?;

    let json_content = serde_json::to_string(&token_event_data)
        .map_err(|e| format!("Failed to serialize token event: {}", e))?;

    let encrypted = signer.nip44_encrypt(&pubkey, &json_content).await
        .map_err(|e| format!("Failed to encrypt token event: {}", e))?;

    let builder = nostr_sdk::EventBuilder::new(
        Kind::CashuWalletUnspentProof,
        encrypted
    );

    let client = nostr_client::NOSTR_CLIENT.read().as_ref()
        .ok_or("Client not initialized")?.clone();

    let event_output = client.send_event_builder(builder).await
        .map_err(|e| format!("Failed to publish event: {}", e))?;

    let event_id = event_output.id().to_hex();

    log::info!("Published token event: {}", event_id);

    // Update local state
    let store = WALLET_TOKENS.read();
    let mut data = store.data();
    let mut tokens = data.write();
    tokens.push(TokenData {
        event_id: event_id.clone(),
        mint: mint_url.clone(),
        unit: "sat".to_string(),
        proofs: proof_data.clone(),
        created_at: chrono::Utc::now().timestamp() as u64,
    });
    drop(tokens);

    // Update balance using checked arithmetic
    let current_balance = *WALLET_BALANCE.read();
    let new_balance = current_balance.checked_add(amount_minted)
        .ok_or_else(|| format!("Balance overflow when adding {} to {}", amount_minted, current_balance))?;
    *WALLET_BALANCE.write() = new_balance;

    // Create history event
    create_history_event_with_type(
        "in",
        amount_minted,
        vec![event_id.clone()],
        vec![],
        Some("lightning_mint"),
        None,
    ).await?;

    // Clean up quote: DB first, then signal (ensures DB cleanup even if signal fails)
    if let Err(e) = wallet.localstore.remove_mint_quote(&quote_id).await {
        log::warn!("Failed to remove mint quote from database: {}", e);
    }
    PENDING_MINT_QUOTES.read().data().write().retain(|q| q.quote_id != quote_id);

    // Sync MultiMintWallet state (non-critical)
    if let Err(e) = crate::stores::cashu_cdk_bridge::sync_wallet_state().await {
        log::warn!("Failed to sync MultiMintWallet state after mint: {}", e);
    }

    log::info!("Mint complete: {} sats", amount_minted);

    Ok(amount_minted)
}

/// Create a melt quote (request to pay a lightning invoice)
pub async fn create_melt_quote(
    mint_url: String,
    invoice: String,
) -> Result<MeltQuoteInfo, String> {
    log::info!("Creating melt quote for invoice at {}", mint_url);

    // Set progress: Creating quote
    *MELT_PROGRESS.write() = Some(MeltProgress::CreatingQuote);

    // Create ephemeral wallet
    let wallet = create_ephemeral_wallet(&mint_url, vec![]).await?;

    // Create melt quote
    let quote = wallet.melt_quote(invoice.clone(), None).await
        .map_err(|e| {
            *MELT_PROGRESS.write() = Some(MeltProgress::Failed { error: e.to_string() });
            format!("Failed to create melt quote: {}", e)
        })?;

    log::info!("Melt quote created: {}", quote.id);

    // Set progress: Quote created
    *MELT_PROGRESS.write() = Some(MeltProgress::QuoteCreated {
        quote_id: quote.id.clone(),
        amount: u64::from(quote.amount),
        fee: u64::from(quote.fee_reserve),
    });

    let quote_info = MeltQuoteInfo {
        quote_id: quote.id.clone(),
        invoice: quote.request,
        amount: u64::from(quote.amount),
        fee_reserve: u64::from(quote.fee_reserve),
        mint_url: mint_url.clone(),
    };

    // Store in global state
    PENDING_MELT_QUOTES.read().data().write().push(quote_info.clone());

    // Publish quote event to Nostr (NIP-60 kind 7374) for cross-device sync
    // This is optional per spec, so we don't fail if publishing fails
    match publish_quote_event(&quote.id, &mint_url, 14).await {
        Ok(event_id) => {
            log::info!("Melt quote event published: {}", event_id);
        }
        Err(e) => {
            log::warn!("Failed to publish melt quote event: {}", e);
            // Continue anyway - quote is usable locally even if publishing fails
        }
    }

    Ok(quote_info)
}

/// Check melt quote status
#[allow(dead_code)] // Reserved for future melt quote status polling
pub async fn check_melt_quote_status(
    mint_url: String,
    quote_id: String,
) -> Result<QuoteStatus, String> {
    log::info!("Checking melt quote status: {}", quote_id);

    // Create ephemeral wallet
    let wallet = create_ephemeral_wallet(&mint_url, vec![]).await?;

    // Check quote status
    let response = wallet.melt_quote_status(&quote_id).await
        .map_err(|e| format!("Failed to check melt quote status: {}", e))?;

    use cdk::nuts::MeltQuoteState;

    let status = match response.state {
        MeltQuoteState::Unpaid => QuoteStatus::Unpaid,
        MeltQuoteState::Paid => QuoteStatus::Paid,
        MeltQuoteState::Pending => QuoteStatus::Pending,
        MeltQuoteState::Failed => QuoteStatus::Failed,
        _ => QuoteStatus::Unpaid,
    };

    log::info!("Melt quote {} status: {:?}", quote_id, status);

    Ok(status)
}

/// Melt tokens to pay a lightning invoice
pub async fn melt_tokens(
    mint_url: String,
    quote_id: String,
) -> Result<(bool, Option<String>, u64), String> {
    use nostr_sdk::signer::NostrSigner;

    log::info!("Melting tokens to pay invoice via quote: {}", quote_id);

    // Reset progress at start
    *MELT_PROGRESS.write() = Some(MeltProgress::PreparingPayment);

    // Acquire mint operation lock to prevent concurrent operations
    let _lock_guard = try_acquire_mint_lock(&mint_url)
        .ok_or_else(|| {
            *MELT_PROGRESS.write() = Some(MeltProgress::Failed {
                error: format!("Another operation is in progress for mint: {}", mint_url)
            });
            format!("Another operation is in progress for mint: {}", mint_url)
        })?;

    // Get melt quote details
    let quote_info = PENDING_MELT_QUOTES.read().data().read()
        .iter()
        .find(|q| q.quote_id == quote_id)
        .cloned()
        .ok_or("Melt quote not found")?;

    let amount_needed = quote_info.amount + quote_info.fee_reserve;

    // Get available proofs for this mint
    let (all_proofs, event_ids_to_delete) = {
        let store = WALLET_TOKENS.read();
        let data = store.data();
        let tokens = data.read();
        let mint_tokens: Vec<_> = tokens.iter()
            .filter(|t| t.mint == mint_url)
            .collect();

        if mint_tokens.is_empty() {
            return Err("No tokens found for this mint".to_string());
        }

        // Convert to CDK proofs
        let mut all_proofs = Vec::new();
        let mut event_ids_to_delete = Vec::new();

        for token in &mint_tokens {
            event_ids_to_delete.push(token.event_id.clone());
            for proof in &token.proofs {
                // Convert ProofData to CDK proof (now preserves witness/dleq)
                all_proofs.push(proof_data_to_cdk_proof(proof)?);
            }
        }

        (all_proofs, event_ids_to_delete)
    };

    // Check balance
    let total_available: u64 = all_proofs.iter()
        .map(|p| u64::from(p.amount))
        .sum();

    if total_available < amount_needed {
        return Err(format!(
            "Insufficient funds. Need {} sats (amount: {}, fee: {}), have: {} sats",
            amount_needed, quote_info.amount, quote_info.fee_reserve, total_available
        ));
    }

    // Update progress before melt
    *MELT_PROGRESS.write() = Some(MeltProgress::PayingInvoice);

    // Melt with auto-retry on spent proofs
    let (melted, keep_proofs) = {
        let result = async {
            let wallet = create_ephemeral_wallet(&mint_url, all_proofs.clone()).await?;

            // Execute melt
            let melted = wallet.melt(&quote_id).await
                .map_err(|e| e.to_string())?;

            let keep_proofs = wallet.get_unspent_proofs().await
                .map_err(|e| e.to_string())?;

            Ok::<(cdk::types::Melted, Vec<cdk::nuts::Proof>), String>((melted, keep_proofs))
        }.await;

        match result {
            Ok((melted, proofs)) => (melted, proofs),
            Err(e) => {
                let error_msg = e.to_string();

                // Auto-retry if proofs are already spent
                if error_msg.contains("already spent") || error_msg.contains("already redeemed") {
                    log::warn!("Some proofs already spent, cleaning up and retrying...");

                    // Cleanup spent proofs (use internal version since we already hold the lock)
                    let (cleaned_count, cleaned_amount) = cleanup_spent_proofs_internal(mint_url.clone()).await
                        .map_err(|e| format!("Cleanup failed: {}", e))?;

                    log::info!("Cleaned up {} spent proofs worth {} sats, retrying melt", cleaned_count, cleaned_amount);

                    // Get fresh proofs after cleanup
                    let fresh_proofs = {
                        let store = WALLET_TOKENS.read();
                        let data = store.data();
                        let tokens = data.read();
                        let mut proofs = Vec::new();

                        for token in tokens.iter().filter(|t| t.mint == mint_url) {
                            for proof in &token.proofs {
                                let temp = ProofData {
                                    id: proof.id.clone(),
                                    amount: proof.amount,
                                    secret: proof.secret.clone(),
                                    c: proof.c.clone(),
                                    witness: None,
                                    dleq: None,
                                };
                                proofs.push(proof_data_to_cdk_proof(&temp)?);
                            }
                        }
                        proofs
                    };

                    // Check we still have enough after cleanup
                    let fresh_total: u64 = fresh_proofs.iter().map(|p| u64::from(p.amount)).sum();
                    if fresh_total < amount_needed {
                        return Err(format!(
                            "Insufficient funds after cleanup. Need: {} sats, have: {} sats",
                            amount_needed, fresh_total
                        ));
                    }

                    // Retry melt with fresh proofs
                    let wallet = create_ephemeral_wallet(&mint_url, fresh_proofs).await?;
                    let melted = wallet.melt(&quote_id).await
                        .map_err(|e| format!("Retry failed: {}", e))?;
                    let keep_proofs = wallet.get_unspent_proofs().await
                        .map_err(|e| format!("Failed to get remaining proofs: {}", e))?;

                    log::info!("Melt succeeded after cleanup and retry");
                    (melted, keep_proofs)
                } else {
                    // Clean up quote: DB first, then signal (ensures DB cleanup even if signal fails)
                    log::warn!("Melt failed, cleaning up quote from database");
                    if let Err(cleanup_err) = remove_melt_quote_from_db(&quote_id).await {
                        log::error!("Failed to remove melt quote from database: {}", cleanup_err);
                    }
                    PENDING_MELT_QUOTES.read().data().write().retain(|q| q.quote_id != quote_id);

                    // Update progress to failed
                    *MELT_PROGRESS.write() = Some(MeltProgress::Failed {
                        error: e.to_string()
                    });

                    return Err(format!("Failed to melt: {}. Quote has been cleaned up, please try again with a new quote.", e));
                }
            }
        }
    };

    let paid = melted.state == cdk::nuts::MeltQuoteState::Paid;
    let preimage = melted.preimage;
    let fee_paid = u64::from(melted.fee_paid);

    log::info!("Melt result: paid={}, fee_paid={}", paid, fee_paid);

    // Update progress based on payment state
    if paid {
        *MELT_PROGRESS.write() = Some(MeltProgress::Completed {
            preimage: preimage.clone(),
            fee_paid,
        });
    } else {
        *MELT_PROGRESS.write() = Some(MeltProgress::WaitingForConfirmation);
    }

    // Prepare event data before updating local state
    let signer = crate::stores::signer::get_signer()
        .ok_or("No signer available")?
        .as_nostr_signer();

    let pubkey_str = auth_store::get_pubkey()
        .ok_or("Not authenticated")?;
    let pubkey = PublicKey::parse(&pubkey_str)
        .map_err(|e| format!("Invalid pubkey: {}", e))?;

    let client = nostr_client::NOSTR_CLIENT.read().as_ref()
        .ok_or("Client not initialized")?.clone();

    // Track the real event ID from Nostr publication (not synthetic)
    let mut new_event_id: Option<String> = None;

    // STEP 1: Publish token event first to get real EventId
    if !keep_proofs.is_empty() {
        let proof_data: Vec<ProofData> = keep_proofs.iter()
            .map(|p| cdk_proof_to_proof_data(p))
            .collect();

        // Convert to extended proofs with P2PK support
        let extended_proofs: Vec<ExtendedCashuProof> = proof_data.iter()
            .map(|p| ExtendedCashuProof::from(p.clone()))
            .collect();

        let token_event_data = ExtendedTokenEvent {
            mint: mint_url.clone(),
            unit: "sat".to_string(),
            proofs: extended_proofs,
            del: event_ids_to_delete.clone(),
        };

        let json_content = serde_json::to_string(&token_event_data)
            .map_err(|e| format!("Failed to serialize token event: {}", e))?;

        let encrypted = signer.nip44_encrypt(&pubkey, &json_content).await
            .map_err(|e| format!("Failed to encrypt token event: {}", e))?;

        let builder = nostr_sdk::EventBuilder::new(Kind::CashuWalletUnspentProof, encrypted);

        // Publish immediately to get real event ID
        match client.send_event_builder(builder.clone()).await {
            Ok(event_output) => {
                let real_id = event_output.id().to_hex();
                log::info!("Published new token event: {}", real_id);
                new_event_id = Some(real_id);
            }
            Err(e) => {
                log::warn!("Failed to publish token event, will queue for retry: {}", e);
                queue_event_for_retry(builder, PendingEventType::TokenEvent).await;
            }
        }
    }

    // STEP 2: Update local state with the real event ID
    {
        let store = WALLET_TOKENS.read();
        let mut data = store.data();
        let mut tokens_write = data.write();

        // Remove old token events
        tokens_write.retain(|t| !event_ids_to_delete.contains(&t.event_id));

        // Add new token with remaining proofs (only if we have a real event ID)
        if let Some(ref event_id) = new_event_id {
            let proof_data: Vec<ProofData> = keep_proofs.iter()
                .map(|p| cdk_proof_to_proof_data(p))
                .collect();

            tokens_write.push(TokenData {
                event_id: event_id.clone(),
                mint: mint_url.clone(),
                unit: "sat".to_string(),
                proofs: proof_data,
                created_at: chrono::Utc::now().timestamp() as u64,
            });
        }

        // Update balance atomically
        let new_balance: u64 = tokens_write.iter()
            .flat_map(|t| &t.proofs)
            .map(|p| p.amount)
            .try_fold(0u64, |acc, amount| acc.checked_add(amount))
            .ok_or_else(|| "Balance calculation overflow in melt_tokens".to_string())?;

        *WALLET_BALANCE.write() = new_balance;
        drop(tokens_write);

        log::info!("Local state updated. New balance: {} sats", new_balance);
    }

    // STEP 3: Publish deletion event for old token events
    if !event_ids_to_delete.is_empty() {
        // Filter to only valid hex event IDs
        let valid_event_ids: Vec<_> = event_ids_to_delete.iter()
            .filter(|id| EventId::from_hex(id).is_ok())
            .collect();

        if !valid_event_ids.is_empty() {
            let mut tags = Vec::new();
            for event_id in &valid_event_ids {
                tags.push(nostr_sdk::Tag::event(
                    nostr_sdk::EventId::from_hex(event_id).unwrap()
                ));
            }

            // Add NIP-60 required tag
            tags.push(nostr_sdk::Tag::custom(
                nostr_sdk::TagKind::custom("k"),
                ["7375"]
            ));

            let deletion_builder = nostr_sdk::EventBuilder::new(
                Kind::from(5),
                "Melted token"
            ).tags(tags);

            match client.send_event_builder(deletion_builder.clone()).await {
                Ok(_) => {
                    log::info!("Published deletion events for {} token events", valid_event_ids.len());
                }
                Err(e) => {
                    log::warn!("Failed to publish deletion event, will queue for retry: {}", e);
                    queue_event_for_retry(deletion_builder, PendingEventType::DeletionEvent).await;
                }
            }
        }

        let invalid_count = event_ids_to_delete.len() - valid_event_ids.len();
        if invalid_count > 0 {
            log::warn!("Skipped {} invalid event IDs in deletion (non-hex format)", invalid_count);
        }
    }

    // STEP 4: Create history event with valid IDs only
    let valid_created: Vec<String> = new_event_id.iter().cloned().collect();
    let valid_destroyed: Vec<String> = event_ids_to_delete.iter()
        .filter(|id| EventId::from_hex(id).is_ok())
        .cloned()
        .collect();

    create_history_event_with_type(
        "out",
        quote_info.amount + fee_paid,
        valid_created,
        valid_destroyed,
        Some("lightning_melt"),
        Some(&quote_info.invoice),
    ).await?;

    // Clean up quote: DB first, then signal (ensures DB cleanup even if signal fails)
    if let Err(e) = remove_melt_quote_from_db(&quote_id).await {
        log::warn!("Failed to remove melt quote from database: {}", e);
    }
    PENDING_MELT_QUOTES.read().data().write().retain(|q| q.quote_id != quote_id);

    // Sync MultiMintWallet state (non-critical)
    if let Err(e) = crate::stores::cashu_cdk_bridge::sync_wallet_state().await {
        log::warn!("Failed to sync MultiMintWallet state after melt: {}", e);
    }

    log::info!("Melt complete: paid={}, amount={}, fee={}", paid, quote_info.amount, fee_paid);

    Ok((paid, preimage, fee_paid))
}

/// Result of a proof consolidation operation
#[derive(Clone, Debug)]
pub struct ConsolidationResult {
    /// Number of proofs before consolidation
    pub proofs_before: usize,
    /// Number of proofs after consolidation
    pub proofs_after: usize,
    /// Fee paid for the swap (usually 0)
    pub fee_paid: u64,
}

/// Consolidate proofs for a single mint to optimize denominations
///
/// This swaps multiple small proofs into fewer larger proofs using
/// power-of-2 denomination targets, which is more efficient for future spending.
pub async fn consolidate_proofs(mint_url: String) -> Result<ConsolidationResult, String> {
    use cdk::amount::SplitTarget;
    use cdk::Amount;
    use nostr_sdk::signer::NostrSigner;

    log::info!("Consolidating proofs for mint: {}", mint_url);

    // Acquire mint operation lock
    let _lock_guard = try_acquire_mint_lock(&mint_url)
        .ok_or_else(|| format!("Another operation is in progress for mint: {}", mint_url))?;

    // Get all proofs for this mint
    let (all_proofs, event_ids_to_delete) = {
        let store = WALLET_TOKENS.read();
        let data = store.data();
        let tokens = data.read();
        let mint_tokens: Vec<_> = tokens.iter()
            .filter(|t| t.mint == mint_url)
            .collect();

        if mint_tokens.is_empty() {
            return Err("No tokens found for this mint".to_string());
        }

        let mut all_proofs = Vec::new();
        let mut event_ids = Vec::new();

        for token in &mint_tokens {
            event_ids.push(token.event_id.clone());
            for proof in &token.proofs {
                all_proofs.push(proof_data_to_cdk_proof(proof)?);
            }
        }

        (all_proofs, event_ids)
    };

    let proofs_before = all_proofs.len();

    // Skip if already optimized (8 or fewer proofs is reasonable)
    if proofs_before <= 8 {
        log::info!("Wallet already optimized with {} proofs", proofs_before);
        return Ok(ConsolidationResult {
            proofs_before,
            proofs_after: proofs_before,
            fee_paid: 0,
        });
    }

    // Calculate total amount
    let total_amount: u64 = all_proofs.iter()
        .map(|p| u64::from(p.amount))
        .sum();

    log::info!("Consolidating {} proofs worth {} sats", proofs_before, total_amount);

    // Create wallet and swap proofs
    let wallet = create_ephemeral_wallet(&mint_url, all_proofs.clone()).await?;

    let new_proofs = wallet.swap(
        Some(Amount::from(total_amount)),
        SplitTarget::default(),  // PowerOfTwo split
        all_proofs,
        None,   // No spending conditions
        false,  // Don't add fees to amount
    ).await
        .map_err(|e| format!("Swap failed: {}", e))?
        .ok_or_else(|| "Swap returned no proofs".to_string())?;

    let proofs_after = new_proofs.len();
    log::info!("Consolidated to {} proofs", proofs_after);

    // Prepare event data
    let signer = crate::stores::signer::get_signer()
        .ok_or("No signer available")?
        .as_nostr_signer();

    let pubkey_str = auth_store::get_pubkey()
        .ok_or("Not authenticated")?;
    let pubkey = PublicKey::parse(&pubkey_str)
        .map_err(|e| format!("Invalid pubkey: {}", e))?;

    let client = nostr_client::NOSTR_CLIENT.read().as_ref()
        .ok_or("Client not initialized")?.clone();

    // Convert new proofs to ProofData
    let proof_data: Vec<ProofData> = new_proofs.iter()
        .map(|p| cdk_proof_to_proof_data(p))
        .collect();

    // Create extended proofs for NIP-60 event
    let extended_proofs: Vec<ExtendedCashuProof> = proof_data.iter()
        .map(|p| ExtendedCashuProof::from(p.clone()))
        .collect();

    let token_event_data = ExtendedTokenEvent {
        mint: mint_url.clone(),
        unit: "sat".to_string(),
        proofs: extended_proofs,
        del: event_ids_to_delete.clone(),
    };

    let json_content = serde_json::to_string(&token_event_data)
        .map_err(|e| format!("Failed to serialize token event: {}", e))?;

    let encrypted = signer.nip44_encrypt(&pubkey, &json_content).await
        .map_err(|e| format!("Failed to encrypt token event: {}", e))?;

    let builder = nostr_sdk::EventBuilder::new(Kind::CashuWalletUnspentProof, encrypted);

    // Publish new token event
    let new_event_id = match client.send_event_builder(builder.clone()).await {
        Ok(event_output) => {
            let id = event_output.id().to_hex();
            log::info!("Published consolidated token event: {}", id);
            id
        }
        Err(e) => {
            log::error!("Failed to publish token event: {}", e);
            return Err(format!("Failed to publish: {}", e));
        }
    };

    // Update local state with new proofs
    {
        let store = WALLET_TOKENS.read();
        let mut data_signal = store.data();
        let mut data = data_signal.write();

        // Remove old token events
        data.retain(|t| !event_ids_to_delete.contains(&t.event_id));

        // Add new consolidated token
        data.push(TokenData {
            event_id: new_event_id.clone(),
            mint: mint_url.clone(),
            unit: "sat".to_string(),
            proofs: proof_data,
            created_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
        });
    }

    // Publish deletion event for old tokens
    use nostr::nips::nip09::EventDeletionRequest;
    let mut deletion_request = EventDeletionRequest::new();
    for event_id_str in &event_ids_to_delete {
        if let Ok(event_id) = nostr_sdk::EventId::from_hex(event_id_str) {
            deletion_request = deletion_request.id(event_id);
        }
    }
    let delete_builder = nostr_sdk::EventBuilder::delete(deletion_request);
    if let Err(e) = client.send_event_builder(delete_builder).await {
        log::warn!("Failed to publish deletion event: {}", e);
    }

    // Recalculate balance from all tokens
    {
        let store = WALLET_TOKENS.read();
        let data = store.data();
        let tokens = data.read();
        let new_balance: u64 = tokens.iter()
            .flat_map(|t| &t.proofs)
            .map(|p| p.amount)
            .fold(0u64, |acc, amount| acc.saturating_add(amount));
        *WALLET_BALANCE.write() = new_balance;
    }

    // Sync MultiMintWallet state (non-critical)
    if let Err(e) = crate::stores::cashu_cdk_bridge::sync_wallet_state().await {
        log::warn!("Failed to sync MultiMintWallet state after consolidation: {}", e);
    }

    log::info!("Consolidation complete: {} -> {} proofs", proofs_before, proofs_after);

    Ok(ConsolidationResult {
        proofs_before,
        proofs_after,
        fee_paid: 0,
    })
}

/// Consolidate proofs across all mints
pub async fn consolidate_all_mints() -> Result<Vec<(String, ConsolidationResult)>, String> {
    let mints = get_mints();
    let mut results = Vec::new();

    for mint in mints {
        match consolidate_proofs(mint.clone()).await {
            Ok(result) => {
                results.push((mint, result));
            }
            Err(e) => {
                log::warn!("Failed to consolidate {}: {}", mint, e);
                // Continue with other mints even if one fails
            }
        }
    }

    Ok(results)
}

/// Get proof count for a specific mint
pub fn get_mint_proof_count(mint_url: &str) -> usize {
    let store = WALLET_TOKENS.read();
    let data = store.data();
    let tokens = data.read();

    tokens.iter()
        .filter(|t| t.mint == mint_url)
        .map(|t| t.proofs.len())
        .sum()
}

/// Get total proof count across all mints
pub fn get_total_proof_count() -> usize {
    let store = WALLET_TOKENS.read();
    let data = store.data();
    let tokens = data.read();

    tokens.iter()
        .map(|t| t.proofs.len())
        .sum()
}

/// Fetch mint info for display
///
/// Connects to the mint and fetches its info endpoint to get name, description,
/// supported NUTs, contact info, etc.
pub async fn get_mint_info(mint_url: &str) -> Result<MintInfoDisplay, String> {
    log::info!("Fetching mint info for: {}", mint_url);

    // Create ephemeral wallet to fetch mint info
    let wallet = create_ephemeral_wallet(mint_url, vec![]).await?;

    let mint_info = wallet.fetch_mint_info().await
        .map_err(|e| format!("Failed to fetch mint info: {}", e))?
        .ok_or("Mint info not available")?;

    // Extract supported NUTs from the nuts field
    // NUT-04 and NUT-05 are always present (mint/melt), check if they have methods
    let mut supported_nuts: Vec<u8> = Vec::new();

    // NUT-04 (minting) - check if methods list is not empty
    if !mint_info.nuts.nut04.methods.is_empty() {
        supported_nuts.push(4);
    }
    // NUT-05 (melting) - check if methods list is not empty
    if !mint_info.nuts.nut05.methods.is_empty() {
        supported_nuts.push(5);
    }
    // Other NUTs with SupportedSettings - check supported field
    if mint_info.nuts.nut07.supported { supported_nuts.push(7); }
    if mint_info.nuts.nut08.supported { supported_nuts.push(8); }
    if mint_info.nuts.nut09.supported { supported_nuts.push(9); }
    if mint_info.nuts.nut10.supported { supported_nuts.push(10); }
    if mint_info.nuts.nut11.supported { supported_nuts.push(11); }
    if mint_info.nuts.nut12.supported { supported_nuts.push(12); }
    if mint_info.nuts.nut14.supported { supported_nuts.push(14); }
    if mint_info.nuts.nut20.supported { supported_nuts.push(20); }

    supported_nuts.sort();

    // Extract contact info
    let contact: Vec<(String, String)> = mint_info.contact
        .unwrap_or_default()
        .into_iter()
        .map(|c| (c.method.to_string(), c.info))
        .collect();

    Ok(MintInfoDisplay {
        name: mint_info.name,
        description: mint_info.description,
        description_long: mint_info.description_long,
        supported_nuts,
        contact,
        motd: mint_info.motd,
        version: mint_info.version.map(|v| v.to_string()),
    })
}

/// Add a new mint to the wallet
///
/// This function:
/// 1. Validates the URL format
/// 2. Fetches mint info to verify connectivity
/// 3. Verifies minimum NUT support (NUT-4, NUT-5)
/// 4. Adds the mint to WALLET_STATE
/// 5. Updates the wallet event on relays
pub async fn add_mint(mint_url: String) -> Result<MintInfoDisplay, String> {
    use nostr_sdk::signer::NostrSigner;

    log::info!("Adding mint: {}", mint_url);

    // Validate URL format
    let url = Url::parse(&mint_url)
        .map_err(|e| format!("Invalid URL format: {}", e))?;

    // Ensure it's https (or http for localhost testing)
    if url.scheme() != "https" && !url.host_str().unwrap_or("").contains("localhost") {
        return Err("Mint URL must use HTTPS".to_string());
    }

    // Check if mint already exists
    let existing_mints = get_mints();
    if existing_mints.contains(&mint_url) {
        return Err("Mint already exists in wallet".to_string());
    }

    // Fetch mint info to verify connectivity and get display info
    let mint_info = get_mint_info(&mint_url).await?;

    // Verify minimum NUT support (NUT-4 for minting, NUT-5 for melting)
    let has_nut4 = mint_info.supported_nuts.contains(&4);
    let has_nut5 = mint_info.supported_nuts.contains(&5);

    if !has_nut4 || !has_nut5 {
        return Err(format!(
            "Mint doesn't support required features. NUT-4: {}, NUT-5: {}",
            has_nut4, has_nut5
        ));
    }

    // Update WALLET_STATE with new mint
    {
        let mut state = WALLET_STATE.write();
        if let Some(ref mut wallet_state) = *state {
            wallet_state.mints.push(mint_url.clone());
        } else {
            return Err("Wallet not initialized".to_string());
        }
    }

    // Update wallet event on relays
    let wallet_state = WALLET_STATE.read().clone()
        .ok_or("Wallet state not available")?;

    let signer = crate::stores::signer::get_signer()
        .ok_or("No signer available")?
        .as_nostr_signer();

    let pubkey_str = auth_store::get_pubkey()
        .ok_or("Not authenticated")?;
    let pubkey = PublicKey::parse(&pubkey_str)
        .map_err(|e| format!("Invalid pubkey: {}", e))?;

    let client = nostr_client::NOSTR_CLIENT.read().as_ref()
        .ok_or("Client not initialized")?.clone();

    // Build wallet event content array following NIP-60 format
    // Format: [["privkey", "hex"], ["mint", "url"], ["mint", "url2"], ...]
    let mut content_array: Vec<Vec<&str>> = vec![vec!["privkey", &wallet_state.privkey]];
    for mint in wallet_state.mints.iter() {
        content_array.push(vec!["mint", mint.as_str()]);
    }

    let json_content = serde_json::to_string(&content_array)
        .map_err(|e| format!("Failed to serialize wallet data: {}", e))?;

    let encrypted = signer.nip44_encrypt(&pubkey, &json_content).await
        .map_err(|e| format!("Failed to encrypt: {}", e))?;

    let builder = nostr_sdk::EventBuilder::new(Kind::CashuWallet, encrypted);

    // Publish
    match client.send_event_builder(builder).await {
        Ok(_) => log::info!("Published updated wallet event with new mint"),
        Err(e) => log::warn!("Failed to publish wallet event: {}", e),
    }

    log::info!("Successfully added mint: {}", mint_url);

    Ok(mint_info)
}

/// NIP-87: Discover Cashu mints from Nostr events
///
/// This function queries for:
/// 1. kind:38172 - Cashu mint announcements
/// 2. kind:38000 - User recommendations (filtered by k=38172)
///
/// Returns a list of discovered mints with recommendation counts.
pub async fn discover_mints() -> Result<Vec<DiscoveredMint>, String> {
    use std::collections::HashMap;

    log::info!("Discovering mints via NIP-87");

    let client = nostr_client::NOSTR_CLIENT.read().as_ref()
        .ok_or("Client not initialized")?.clone();

    // Ensure relays are ready
    nostr_client::ensure_relays_ready(&client).await;

    // Query for kind:38172 (Cashu mint announcements)
    let mint_filter = Filter::new()
        .kind(Kind::from(38172))
        .limit(50);

    // Query for kind:38000 (recommendations) with k=38172
    let recommendation_filter = Filter::new()
        .kind(Kind::from(38000))
        .custom_tag(
            nostr_sdk::SingleLetterTag::lowercase(nostr_sdk::Alphabet::K),
            "38172"
        )
        .limit(100);

    // Fetch both in parallel
    let (mint_events, recommendation_events) = futures::join!(
        client.fetch_events(mint_filter, Duration::from_secs(10)),
        client.fetch_events(recommendation_filter, Duration::from_secs(10))
    );

    let mint_events = mint_events.map_err(|e| format!("Failed to fetch mint events: {}", e))?;
    let recommendation_events = recommendation_events.map_err(|e| format!("Failed to fetch recommendations: {}", e))?;

    log::info!("Found {} mint announcements, {} recommendations",
        mint_events.len(), recommendation_events.len());

    // Parse mint announcements into a map by URL
    let mut mints_by_url: HashMap<String, DiscoveredMint> = HashMap::new();

    for event in mint_events.iter() {
        // Extract u tag (mint URL)
        let url = event.tags.iter()
            .find_map(|tag| {
                let values: Vec<&str> = tag.as_slice().iter().map(|s| s.as_str()).collect();
                if values.first() == Some(&"u") {
                    values.get(1).map(|s| s.to_string())
                } else {
                    None
                }
            });

        let Some(url) = url else { continue };

        // Extract d tag (mint pubkey)
        let mint_pubkey = event.tags.iter()
            .find_map(|tag| {
                let values: Vec<&str> = tag.as_slice().iter().map(|s| s.as_str()).collect();
                if values.first() == Some(&"d") {
                    values.get(1).map(|s| s.to_string())
                } else {
                    None
                }
            });

        // Extract nuts tag
        let nuts = event.tags.iter()
            .find_map(|tag| {
                let values: Vec<&str> = tag.as_slice().iter().map(|s| s.as_str()).collect();
                if values.first() == Some(&"nuts") {
                    values.get(1).map(|s| s.to_string())
                } else {
                    None
                }
            });

        // Extract n tag (network)
        let network = event.tags.iter()
            .find_map(|tag| {
                let values: Vec<&str> = tag.as_slice().iter().map(|s| s.as_str()).collect();
                if values.first() == Some(&"n") {
                    values.get(1).map(|s| s.to_string())
                } else {
                    None
                }
            });

        // Parse content for metadata (kind:0 style)
        let (name, description) = if !event.content.is_empty() {
            if let Ok(metadata) = serde_json::from_str::<serde_json::Value>(&event.content) {
                (
                    metadata.get("name").and_then(|v| v.as_str()).map(|s| s.to_string()),
                    metadata.get("about").and_then(|v| v.as_str()).map(|s| s.to_string()),
                )
            } else {
                (None, None)
            }
        } else {
            (None, None)
        };

        mints_by_url.insert(url.clone(), DiscoveredMint {
            url,
            name,
            description,
            nuts,
            network,
            mint_pubkey,
            author_pubkey: event.pubkey.to_hex(),
            recommendation_count: 0,
            recommenders: Vec::new(),
            recommendations: Vec::new(),
        });
    }

    // Process recommendations to count endorsements and store detailed recommendation data
    for event in recommendation_events.iter() {
        // Extract u tags with cashu type
        for tag in event.tags.iter() {
            let values: Vec<&str> = tag.as_slice().iter().map(|s| s.as_str()).collect();
            if values.first() == Some(&"u") {
                if let Some(url) = values.get(1) {
                    // Check if it's a cashu URL (not fedimint)
                    let is_cashu = values.get(2).map(|t| *t == "cashu").unwrap_or(true);
                    if is_cashu && url.starts_with("http") {
                        let recommender = event.pubkey.to_hex();

                        // Create recommendation struct with content/review
                        let recommendation = MintRecommendation {
                            recommender: recommender.clone(),
                            content: event.content.clone(),
                        };

                        if let Some(mint) = mints_by_url.get_mut(*url) {
                            if !mint.recommenders.contains(&recommender) {
                                mint.recommenders.push(recommender);
                                mint.recommendation_count += 1;
                                mint.recommendations.push(recommendation);
                            }
                        } else {
                            // Mint not in announcements but recommended
                            mints_by_url.insert(url.to_string(), DiscoveredMint {
                                url: url.to_string(),
                                name: None,
                                description: None,
                                nuts: None,
                                network: Some("mainnet".to_string()),
                                mint_pubkey: None,
                                author_pubkey: String::new(),
                                recommendation_count: 1,
                                recommenders: vec![recommender],
                                recommendations: vec![recommendation],
                            });
                        }
                    }
                }
            }
        }
    }

    // Convert to sorted list (by recommendation count, descending)
    let mut mints: Vec<DiscoveredMint> = mints_by_url.into_values().collect();
    mints.sort_by(|a, b| b.recommendation_count.cmp(&a.recommendation_count));

    log::info!("Discovered {} unique mints", mints.len());

    Ok(mints)
}

/// Extended history event creation with operation type and invoice
///
/// This function creates NIP-60 spending history events (kind 7376) with additional
/// nostr.blue-specific extension fields for enhanced transaction tracking.
///
/// # Standard NIP-60 Fields (interoperable)
/// - `direction`: "in" or "out"
/// - `amount`: Amount in sats
/// - `e` tags with "created"/"destroyed" markers
///
/// # nostr.blue Extension Fields (non-standard)
/// These fields are NOT part of the NIP-60 spec and may not be understood by other clients:
/// - `unit`: Currency unit (always "sat" currently) - useful for multi-currency support
/// - `type`: Operation type ("lightning_mint", "lightning_melt", etc.) - for categorization
/// - `invoice`: Lightning invoice (for lightning operations) - for reference/debugging
///
/// Other NIP-60 clients will ignore these extension fields per standard JSON parsing behavior.
pub(crate) async fn create_history_event_with_type(
    direction: &str,
    amount: u64,
    created_tokens: Vec<String>,
    destroyed_tokens: Vec<String>,
    operation_type: Option<&str>,
    invoice: Option<&str>,
) -> Result<(), String> {
    use nostr_sdk::signer::NostrSigner;

    let signer = crate::stores::signer::get_signer()
        .ok_or("No signer available")?
        .as_nostr_signer();

    let pubkey_str = auth_store::get_pubkey()
        .ok_or("Not authenticated")?;
    let pubkey = PublicKey::parse(&pubkey_str)
        .map_err(|e| format!("Invalid pubkey: {}", e))?;

    // Build content array with standard NIP-60 fields first
    let mut content_array = vec![
        vec!["direction".to_string(), direction.to_string()],
        vec!["amount".to_string(), amount.to_string()],
    ];

    // Extension field: unit (non-standard, for future multi-currency support)
    content_array.push(vec!["unit".to_string(), "sat".to_string()]);

    // Extension field: operation type (non-standard, for categorization)
    if let Some(op_type) = operation_type {
        content_array.push(vec!["type".to_string(), op_type.to_string()]);
    }

    // Extension field: invoice (non-standard, for lightning operation reference)
    if let Some(inv) = invoice {
        content_array.push(vec!["invoice".to_string(), inv.to_string()]);
    }

    // Standard NIP-60: Add created token events
    for event_id in created_tokens {
        content_array.push(vec!["e".to_string(), event_id, "".to_string(), "created".to_string()]);
    }

    // Standard NIP-60: Add destroyed token events
    for event_id in destroyed_tokens {
        content_array.push(vec!["e".to_string(), event_id, "".to_string(), "destroyed".to_string()]);
    }

    let json_content = serde_json::to_string(&content_array)
        .map_err(|e| format!("Failed to serialize: {}", e))?;

    let encrypted = signer.nip44_encrypt(&pubkey, &json_content).await
        .map_err(|e| format!("Failed to encrypt: {}", e))?;

    let builder = nostr_sdk::EventBuilder::new(
        Kind::CashuWalletSpendingHistory,
        encrypted
    );

    let client = nostr_client::NOSTR_CLIENT.read().as_ref()
        .ok_or("Client not initialized")?.clone();

    let event_output = client.send_event_builder(builder).await
        .map_err(|e| format!("Failed to publish history event: {}", e))?;

    log::info!("Published history event: {}", event_output.id().to_hex());

    Ok(())
}

/// Load pending events from IndexedDB into memory on wallet startup
async fn load_pending_events() -> Result<(), String> {
    log::info!("Loading pending events from IndexedDB");

    let localstore = get_shared_localstore().await?;
    let stored_events = localstore.get_all_pending_events().await
        .map_err(|e| format!("Failed to load pending events: {}", e))?;

    let count = stored_events.len();
    *PENDING_NOSTR_EVENTS.write() = stored_events;

    if count > 0 {
        log::info!("Loaded {} pending events from IndexedDB", count);
    }

    Ok(())
}

/// Process pending events with exponential backoff retry logic
pub async fn process_pending_events() -> Result<usize, String> {
    const MAX_RETRIES: u32 = 5;
    const BASE_RETRY_DELAY_SECS: u64 = 60;

    let pending_events = PENDING_NOSTR_EVENTS.read().clone();
    let mut processed_count = 0;

    log::info!("Processing {} pending events", pending_events.len());

    for event in pending_events {
        // Skip if too many retries
        if event.retry_count >= MAX_RETRIES {
            log::warn!("Event {} exceeded max retries ({}), removing from queue", event.id, MAX_RETRIES);
            let _ = remove_pending_event(&event.id).await;
            continue;
        }

        // Check if enough time has passed since creation (exponential backoff)
        let now = chrono::Utc::now().timestamp() as u64;
        let elapsed = now.saturating_sub(event.created_at);
        let retry_delay = BASE_RETRY_DELAY_SECS * (2_u64.pow(event.retry_count));

        if elapsed < retry_delay {
            log::debug!(
                "Event {} not ready for retry yet (elapsed: {}, required: {})",
                event.id, elapsed, retry_delay
            );
            continue;
        }

        // Attempt to publish
        match publish_pending_event(&event).await {
            Ok(_) => {
                log::info!("Successfully published pending event: {}", event.id);
                let _ = remove_pending_event(&event.id).await;
                processed_count += 1;
            }
            Err(e) => {
                log::warn!("Failed to publish pending event {} (attempt {}): {}",
                    event.id, event.retry_count + 1, e);

                // Increment retry count and update in both memory and DB
                let mut updated_event = event.clone();
                updated_event.retry_count += 1;

                // Update in memory
                let mut events = PENDING_NOSTR_EVENTS.write();
                if let Some(pos) = events.iter().position(|e| e.id == event.id) {
                    events[pos] = updated_event.clone();
                }
                drop(events);

                // Update in IndexedDB
                if let Ok(localstore) = get_shared_localstore().await {
                    let _ = localstore.update_pending_event(&updated_event).await;
                }
            }
        }
    }

    if processed_count > 0 {
        log::info!("Successfully processed {} pending events", processed_count);
    }

    Ok(processed_count)
}

// ============================================================================
// Cross-Mint Transfer Implementation
// ============================================================================

/// Transfer progress status for UI feedback
#[derive(Clone, Debug, PartialEq)]
pub enum TransferProgress {
    /// Creating mint quote at target mint
    CreatingMintQuote,
    /// Creating melt quote at source mint
    CreatingMeltQuote,
    /// Quote created, ready to transfer
    QuotesReady { amount: u64, fee_estimate: u64 },
    /// Executing melt (paying lightning invoice)
    Melting,
    /// Waiting for payment confirmation
    WaitingForPayment,
    /// Minting tokens at target
    Minting,
    /// Transfer completed successfully
    Completed { amount_received: u64, fees_paid: u64 },
    /// Transfer failed
    Failed { error: String },
}

/// Global signal for transfer progress
pub static TRANSFER_PROGRESS: GlobalSignal<Option<TransferProgress>> = Signal::global(|| None);

/// Result of a cross-mint transfer
#[derive(Clone, Debug)]
pub struct TransferResult {
    /// Amount deducted from source mint
    pub amount_sent: u64,
    /// Amount received at target mint
    pub amount_received: u64,
    /// Total fees paid for the transfer
    pub fees_paid: u64,
}

/// Estimate fees for a cross-mint transfer
///
/// This creates temporary quotes to determine the fee, then cancels them.
/// Returns (estimated_fee, amount_to_receive) for the given send_amount.
pub async fn estimate_transfer_fees(
    source_mint: String,
    target_mint: String,
    amount: u64,
) -> Result<(u64, u64), String> {
    use cdk::Amount;

    log::info!("Estimating transfer fees: {} sats from {} to {}",
        amount, source_mint, target_mint);

    if source_mint == target_mint {
        return Err("Source and target mints must be different".to_string());
    }

    if amount == 0 {
        return Err("Amount must be greater than 0".to_string());
    }

    // Create wallet for target mint to get a mint quote (Lightning invoice)
    let target_wallet = get_or_create_wallet(&target_mint).await?;

    // Create mint quote at target to get invoice amount
    let mint_quote = target_wallet.mint_quote(Amount::from(amount), None).await
        .map_err(|e| format!("Failed to create mint quote: {}", e))?;

    // Create wallet for source mint to get melt quote (fee estimate)
    let source_wallet = get_or_create_wallet(&source_mint).await?;

    // Create melt quote to see what fees would be
    let melt_quote = source_wallet.melt_quote(mint_quote.request.clone(), None).await
        .map_err(|e| format!("Failed to create melt quote: {}", e))?;

    let fee_estimate = u64::from(melt_quote.fee_reserve);
    let total_needed = amount + fee_estimate;

    log::info!("Transfer fee estimate: {} sats (total needed: {} sats)",
        fee_estimate, total_needed);

    Ok((fee_estimate, amount))
}

/// Get balance for a specific mint
pub fn get_mint_balance(mint_url: &str) -> u64 {
    let store = WALLET_TOKENS.read();
    let data = store.data();
    let tokens = data.read();

    tokens.iter()
        .filter(|t| t.mint == mint_url)
        .flat_map(|t| t.proofs.iter())
        .map(|p| p.amount)
        .sum()
}

/// Transfer tokens from one mint to another via Lightning
///
/// This performs a melt at the source mint and mint at the target mint,
/// effectively moving tokens between mints via Lightning payment.
pub async fn transfer_between_mints(
    source_mint: String,
    target_mint: String,
    amount: u64,
) -> Result<TransferResult, String> {
    use cdk::Amount;
    use nostr_sdk::signer::NostrSigner;

    log::info!("Starting cross-mint transfer: {} sats from {} to {}",
        amount, source_mint, target_mint);

    // Reset progress
    *TRANSFER_PROGRESS.write() = Some(TransferProgress::CreatingMintQuote);

    // Validate inputs
    if source_mint == target_mint {
        *TRANSFER_PROGRESS.write() = Some(TransferProgress::Failed {
            error: "Source and target mints must be different".to_string()
        });
        return Err("Source and target mints must be different".to_string());
    }

    if amount == 0 {
        *TRANSFER_PROGRESS.write() = Some(TransferProgress::Failed {
            error: "Amount must be greater than 0".to_string()
        });
        return Err("Amount must be greater than 0".to_string());
    }

    // Check source balance
    let source_balance = get_mint_balance(&source_mint);
    if source_balance < amount {
        let error = format!(
            "Insufficient balance at source mint. Have: {} sats, need: {} sats",
            source_balance, amount
        );
        *TRANSFER_PROGRESS.write() = Some(TransferProgress::Failed { error: error.clone() });
        return Err(error);
    }

    // Acquire lock for source mint
    let _source_lock = try_acquire_mint_lock(&source_mint)
        .ok_or_else(|| {
            let error = format!("Another operation is in progress for source mint: {}", source_mint);
            *TRANSFER_PROGRESS.write() = Some(TransferProgress::Failed { error: error.clone() });
            error
        })?;

    // STEP 1: Create mint quote at target mint (get Lightning invoice)
    log::info!("Creating mint quote at target mint for {} sats", amount);
    *TRANSFER_PROGRESS.write() = Some(TransferProgress::CreatingMintQuote);

    let target_wallet = get_or_create_wallet(&target_mint).await?;
    let mint_quote = target_wallet.mint_quote(Amount::from(amount), None).await
        .map_err(|e| {
            let error = format!("Failed to create mint quote at target: {}", e);
            *TRANSFER_PROGRESS.write() = Some(TransferProgress::Failed { error: error.clone() });
            error
        })?;

    log::info!("Mint quote created: {}", mint_quote.id);

    // STEP 2: Create melt quote at source mint for that invoice
    log::info!("Creating melt quote at source mint");
    *TRANSFER_PROGRESS.write() = Some(TransferProgress::CreatingMeltQuote);

    let source_wallet = get_or_create_wallet(&source_mint).await?;
    let melt_quote = source_wallet.melt_quote(mint_quote.request.clone(), None).await
        .map_err(|e| {
            let error = format!("Failed to create melt quote at source: {}", e);
            *TRANSFER_PROGRESS.write() = Some(TransferProgress::Failed { error: error.clone() });
            error
        })?;

    let fee_estimate = u64::from(melt_quote.fee_reserve);
    let total_needed = amount + fee_estimate;

    log::info!("Melt quote created: {}, fee reserve: {} sats", melt_quote.id, fee_estimate);

    // Verify we have enough balance including fees
    if source_balance < total_needed {
        let error = format!(
            "Insufficient balance including fees. Have: {} sats, need: {} sats (amount: {} + fee: {})",
            source_balance, total_needed, amount, fee_estimate
        );
        *TRANSFER_PROGRESS.write() = Some(TransferProgress::Failed { error: error.clone() });
        return Err(error);
    }

    *TRANSFER_PROGRESS.write() = Some(TransferProgress::QuotesReady {
        amount,
        fee_estimate
    });

    // STEP 3: Get proofs from source mint and execute melt
    log::info!("Preparing proofs for melt");
    *TRANSFER_PROGRESS.write() = Some(TransferProgress::Melting);

    let (all_proofs, event_ids_to_delete) = {
        let store = WALLET_TOKENS.read();
        let data = store.data();
        let tokens = data.read();
        let mint_tokens: Vec<_> = tokens.iter()
            .filter(|t| t.mint == source_mint)
            .collect();

        let mut all_proofs = Vec::new();
        let mut event_ids = Vec::new();

        for token in &mint_tokens {
            event_ids.push(token.event_id.clone());
            for proof in &token.proofs {
                all_proofs.push(proof_data_to_cdk_proof(proof)?);
            }
        }

        (all_proofs, event_ids)
    };

    // Create wallet with proofs and execute melt
    let wallet_with_proofs = create_ephemeral_wallet(&source_mint, all_proofs).await?;

    let melted = wallet_with_proofs.melt(&melt_quote.id).await
        .map_err(|e| {
            let error = format!("Failed to melt tokens: {}", e);
            *TRANSFER_PROGRESS.write() = Some(TransferProgress::Failed { error: error.clone() });
            error
        })?;

    let paid = melted.state == cdk::nuts::MeltQuoteState::Paid;
    let fee_paid = u64::from(melted.fee_paid);

    log::info!("Melt result: paid={}, fee_paid={} sats", paid, fee_paid);

    if !paid {
        let error = "Lightning payment failed".to_string();
        *TRANSFER_PROGRESS.write() = Some(TransferProgress::Failed { error: error.clone() });
        return Err(error);
    }

    // Get remaining proofs at source after melt
    let source_keep_proofs = wallet_with_proofs.get_unspent_proofs().await
        .map_err(|e| format!("Failed to get remaining proofs: {}", e))?;

    // STEP 4: Wait for payment confirmation and mint at target
    log::info!("Payment sent, waiting for mint quote to be paid");
    *TRANSFER_PROGRESS.write() = Some(TransferProgress::WaitingForPayment);

    // Poll for mint quote status (will be replaced with WebSocket in Phase 3)
    let max_attempts = 60; // 2 minutes with 2-second intervals
    let mut mint_quote_paid = false;

    for attempt in 0..max_attempts {
        let state = target_wallet.mint_quote_state(&mint_quote.id).await
            .map_err(|e| format!("Failed to check mint quote status: {}", e))?;

        if state.state == cdk::nuts::MintQuoteState::Paid {
            mint_quote_paid = true;
            log::info!("Mint quote is paid after {} attempts", attempt + 1);
            break;
        }

        if state.state == cdk::nuts::MintQuoteState::Issued {
            // Already minted elsewhere? This shouldn't happen in normal flow
            let error = "Mint quote was already issued".to_string();
            *TRANSFER_PROGRESS.write() = Some(TransferProgress::Failed { error: error.clone() });
            return Err(error);
        }

        // Wait 2 seconds before next check
        #[cfg(target_arch = "wasm32")]
        {
            use gloo_timers::future::TimeoutFuture;
            TimeoutFuture::new(2000).await;
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        }
    }

    if !mint_quote_paid {
        let error = "Timeout waiting for Lightning payment confirmation".to_string();
        *TRANSFER_PROGRESS.write() = Some(TransferProgress::Failed { error: error.clone() });
        return Err(error);
    }

    // STEP 5: Mint tokens at target
    log::info!("Minting tokens at target mint");
    *TRANSFER_PROGRESS.write() = Some(TransferProgress::Minting);

    let target_proofs = target_wallet.mint(
        &mint_quote.id,
        cdk::amount::SplitTarget::default(),
        None
    ).await
    .map_err(|e| {
        let error = format!("Failed to mint tokens at target: {}", e);
        *TRANSFER_PROGRESS.write() = Some(TransferProgress::Failed { error: error.clone() });
        error
    })?;

    let amount_received: u64 = target_proofs.iter()
        .map(|p| u64::from(p.amount))
        .sum();

    log::info!("Minted {} sats at target mint ({} proofs)", amount_received, target_proofs.len());

    // STEP 6: Update local state and publish to Nostr
    let signer = crate::stores::signer::get_signer()
        .ok_or("No signer available")?
        .as_nostr_signer();

    let pubkey_str = auth_store::get_pubkey()
        .ok_or("Not authenticated")?;
    let pubkey = PublicKey::parse(&pubkey_str)
        .map_err(|e| format!("Invalid pubkey: {}", e))?;

    let client = nostr_client::NOSTR_CLIENT.read().as_ref()
        .ok_or("Client not initialized")?.clone();

    // Publish source mint token event (remaining proofs)
    let mut source_new_event_id: Option<String> = None;
    if !source_keep_proofs.is_empty() {
        let proof_data: Vec<ProofData> = source_keep_proofs.iter()
            .map(|p| cdk_proof_to_proof_data(p))
            .collect();

        let extended_proofs: Vec<ExtendedCashuProof> = proof_data.iter()
            .map(|p| ExtendedCashuProof::from(p.clone()))
            .collect();

        let token_event_data = ExtendedTokenEvent {
            mint: source_mint.clone(),
            unit: "sat".to_string(),
            proofs: extended_proofs,
            del: event_ids_to_delete.clone(),
        };

        let json_content = serde_json::to_string(&token_event_data)
            .map_err(|e| format!("Failed to serialize token event: {}", e))?;

        let encrypted = signer.nip44_encrypt(&pubkey, &json_content).await
            .map_err(|e| format!("Failed to encrypt token event: {}", e))?;

        let builder = nostr_sdk::EventBuilder::new(Kind::CashuWalletUnspentProof, encrypted);

        match client.send_event_builder(builder.clone()).await {
            Ok(event_output) => {
                source_new_event_id = Some(event_output.id().to_hex());
                log::info!("Published source token event: {:?}", source_new_event_id);
            }
            Err(e) => {
                log::warn!("Failed to publish source token event: {}", e);
                queue_event_for_retry(builder, PendingEventType::TokenEvent).await;
            }
        }
    } else {
        // No remaining proofs - publish deletion for all old events
        if !event_ids_to_delete.is_empty() {
            use nostr::nips::nip09::EventDeletionRequest;
            let mut deletion_request = EventDeletionRequest::new();
            for event_id_str in &event_ids_to_delete {
                if let Ok(event_id) = EventId::parse(event_id_str) {
                    deletion_request = deletion_request.id(event_id);
                }
            }

            let builder = nostr_sdk::EventBuilder::delete(deletion_request);
            if let Err(e) = client.send_event_builder(builder.clone()).await {
                log::warn!("Failed to publish deletion event: {}", e);
                queue_event_for_retry(builder, PendingEventType::DeletionEvent).await;
            }
        }
    }

    // Publish target mint token event (new proofs)
    let mut target_new_event_id: Option<String> = None;
    {
        let proof_data: Vec<ProofData> = target_proofs.iter()
            .map(|p| cdk_proof_to_proof_data(p))
            .collect();

        let extended_proofs: Vec<ExtendedCashuProof> = proof_data.iter()
            .map(|p| ExtendedCashuProof::from(p.clone()))
            .collect();

        let token_event_data = ExtendedTokenEvent {
            mint: target_mint.clone(),
            unit: "sat".to_string(),
            proofs: extended_proofs,
            del: vec![],
        };

        let json_content = serde_json::to_string(&token_event_data)
            .map_err(|e| format!("Failed to serialize target token event: {}", e))?;

        let encrypted = signer.nip44_encrypt(&pubkey, &json_content).await
            .map_err(|e| format!("Failed to encrypt target token event: {}", e))?;

        let builder = nostr_sdk::EventBuilder::new(Kind::CashuWalletUnspentProof, encrypted);

        match client.send_event_builder(builder.clone()).await {
            Ok(event_output) => {
                target_new_event_id = Some(event_output.id().to_hex());
                log::info!("Published target token event: {:?}", target_new_event_id);
            }
            Err(e) => {
                log::warn!("Failed to publish target token event: {}", e);
                queue_event_for_retry(builder, PendingEventType::TokenEvent).await;
            }
        }
    }

    // Update local state
    {
        let store = WALLET_TOKENS.read();
        let mut data = store.data();
        let mut tokens = data.write();

        // Remove old source tokens
        tokens.retain(|t| !event_ids_to_delete.contains(&t.event_id));

        // Add new source tokens (if any remain)
        if !source_keep_proofs.is_empty() {
            let proof_data: Vec<ProofData> = source_keep_proofs.iter()
                .map(|p| cdk_proof_to_proof_data(p))
                .collect();

            tokens.push(TokenData {
                event_id: source_new_event_id.unwrap_or_else(|| format!("local-{}", chrono::Utc::now().timestamp())),
                mint: source_mint.clone(),
                unit: "sat".to_string(),
                proofs: proof_data,
                created_at: chrono::Utc::now().timestamp() as u64,
            });
        }

        // Add new target tokens
        let target_proof_data: Vec<ProofData> = target_proofs.iter()
            .map(|p| cdk_proof_to_proof_data(p))
            .collect();

        tokens.push(TokenData {
            event_id: target_new_event_id.unwrap_or_else(|| format!("local-{}", chrono::Utc::now().timestamp())),
            mint: target_mint.clone(),
            unit: "sat".to_string(),
            proofs: target_proof_data,
            created_at: chrono::Utc::now().timestamp() as u64,
        });
    }

    // Update balance
    {
        let store = WALLET_TOKENS.read();
        let data = store.data();
        let tokens = data.read();
        let new_balance: u64 = tokens.iter()
            .flat_map(|t| &t.proofs)
            .map(|p| p.amount)
            .fold(0u64, |acc, amount| acc.saturating_add(amount));
        *WALLET_BALANCE.write() = new_balance;
    }

    // Calculate result
    let amount_sent = amount + fee_paid;
    let result = TransferResult {
        amount_sent,
        amount_received,
        fees_paid: fee_paid,
    };

    log::info!("Transfer complete: sent {} sats, received {} sats, fees {} sats",
        amount_sent, amount_received, fee_paid);

    *TRANSFER_PROGRESS.write() = Some(TransferProgress::Completed {
        amount_received,
        fees_paid: fee_paid
    });

    Ok(result)
}

// ============================================================================
// Payment Request (NUT-18) Implementation
// ============================================================================

/// Payment request data for creating requests
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PaymentRequestData {
    /// Unique request ID
    #[serde(rename = "i", skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// Amount in sats (optional - allows any amount if not set)
    #[serde(rename = "a", skip_serializing_if = "Option::is_none")]
    pub amount: Option<u64>,
    /// Currency unit (e.g., "sat")
    #[serde(rename = "u", skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
    /// Single use flag
    #[serde(rename = "s", skip_serializing_if = "Option::is_none")]
    pub single_use: Option<bool>,
    /// Accepted mints
    #[serde(rename = "m", skip_serializing_if = "Option::is_none")]
    pub mints: Option<Vec<String>>,
    /// Description
    #[serde(rename = "d", skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Transport methods
    #[serde(rename = "t", skip_serializing_if = "Vec::is_empty", default)]
    pub transports: Vec<PaymentTransport>,
}

/// Payment transport (Nostr or HTTP)
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PaymentTransport {
    /// Transport type ("nostr" or "http")
    #[serde(rename = "t")]
    pub transport_type: String,
    /// Target (nprofile for Nostr, URL for HTTP)
    #[serde(rename = "a")]
    pub target: String,
    /// Tags (e.g., [["n", "17"]] for NIP-17)
    #[serde(rename = "g", skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<Vec<String>>>,
}

/// Info needed to wait for a Nostr payment
#[derive(Clone, Debug)]
pub struct NostrPaymentWaitInfo {
    /// Request ID (UUID) for looking up this request
    pub request_id: String,
    /// Ephemeral secret key for receiving
    pub secret_key: nostr_sdk::SecretKey,
    /// Relays to listen on
    pub relays: Vec<String>,
    /// Public key to receive on
    pub pubkey: nostr_sdk::PublicKey,
}

/// Payment request payload (what's sent via transport)
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PaymentRequestPayload {
    /// Request ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// Optional memo
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memo: Option<String>,
    /// Mint URL
    pub mint: String,
    /// Currency unit
    pub unit: String,
    /// Proofs
    pub proofs: Vec<ProofData>,
}

/// Progress for creating a payment request
#[derive(Clone, Debug, PartialEq)]
pub enum PaymentRequestProgress {
    /// Waiting for payment
    WaitingForPayment,
    /// Payment received
    Received { amount: u64 },
    /// Timeout or cancelled
    Cancelled,
    /// Error
    Error { message: String },
}

/// Global signal for payment request progress
pub static PAYMENT_REQUEST_PROGRESS: GlobalSignal<Option<PaymentRequestProgress>> = Signal::global(|| None);

/// Pending payment requests awaiting Nostr payments
pub static PENDING_PAYMENT_REQUESTS: GlobalSignal<std::collections::HashMap<String, NostrPaymentWaitInfo>> =
    Signal::global(|| std::collections::HashMap::new());

const PAYMENT_REQUEST_PREFIX: &str = "creqA";

/// Create a payment request (NUT-18)
///
/// Returns the request string (creqA...) and optionally NostrPaymentWaitInfo
/// if Nostr transport is enabled.
pub async fn create_payment_request(
    amount: Option<u64>,
    description: Option<String>,
    use_nostr_transport: bool,
) -> Result<(String, Option<NostrPaymentWaitInfo>), String> {
    use base64::Engine;
    use nostr_sdk::ToBech32;

    log::info!("Creating payment request: amount={:?}, nostr={}", amount, use_nostr_transport);

    let mints = get_mints();
    if mints.is_empty() {
        return Err("No mints available. Add a mint first.".to_string());
    }

    let request_id = uuid::Uuid::new_v4().to_string()[..8].to_string();

    // Build transport
    let (transports, nostr_info): (Vec<PaymentTransport>, Option<NostrPaymentWaitInfo>) = if use_nostr_transport {
        // Generate ephemeral keys for receiving
        let keys = nostr_sdk::Keys::generate();

        // Get user's relays
        let relays = crate::services::profile_search::get_user_relays().await;
        if relays.is_empty() {
            return Err("No relays configured for Nostr transport".to_string());
        }

        // Create nprofile with relays
        let relay_urls: Vec<nostr_sdk::RelayUrl> = relays.iter()
            .filter_map(|r| nostr_sdk::RelayUrl::parse(r).ok())
            .collect();

        let nprofile = nostr_sdk::nips::nip19::Nip19Profile::new(
            keys.public_key(),
            relay_urls
        );

        let nprofile_str = nprofile.to_bech32()
            .map_err(|e| format!("Failed to encode nprofile: {}", e))?;

        let transport = PaymentTransport {
            transport_type: "nostr".to_string(),
            target: nprofile_str,
            tags: Some(vec![vec!["n".to_string(), "17".to_string()]]),
        };

        let wait_info = NostrPaymentWaitInfo {
            request_id: request_id.clone(),
            secret_key: keys.secret_key().clone(),
            relays,
            pubkey: keys.public_key(),
        };

        (vec![transport], Some(wait_info))
    } else {
        (vec![], None)
    };

    // Build request
    let request = PaymentRequestData {
        id: Some(request_id.clone()),
        amount,
        unit: Some("sat".to_string()),
        single_use: Some(true),
        mints: Some(mints),
        description,
        transports,
    };

    // Encode as CBOR then base64
    let mut cbor_data = Vec::new();
    ciborium::into_writer(&request, &mut cbor_data)
        .map_err(|e| format!("Failed to encode request: {}", e))?;

    let encoded = base64::engine::general_purpose::URL_SAFE.encode(&cbor_data);
    let request_string = format!("{}{}", PAYMENT_REQUEST_PREFIX, encoded);

    // Store wait info for later if Nostr transport is enabled
    if let Some(ref info) = nostr_info {
        PENDING_PAYMENT_REQUESTS.write().insert(request_id, info.clone());
    }

    log::info!("Created payment request: {}", &request_string[..50.min(request_string.len())]);

    Ok((request_string, nostr_info))
}

/// Parse a payment request string (creqA...)
pub fn parse_payment_request(request_string: &str) -> Result<PaymentRequestData, String> {
    use base64::Engine;

    let request_string = request_string.trim();

    if !request_string.starts_with(PAYMENT_REQUEST_PREFIX) {
        return Err(format!(
            "Invalid payment request. Must start with '{}'. Got: '{}'",
            PAYMENT_REQUEST_PREFIX,
            &request_string[..10.min(request_string.len())]
        ));
    }

    let encoded = &request_string[PAYMENT_REQUEST_PREFIX.len()..];

    // Decode base64
    let cbor_data = base64::engine::general_purpose::URL_SAFE
        .decode(encoded)
        .or_else(|_| {
            // Try with standard base64 as fallback
            base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(encoded)
        })
        .map_err(|e| format!("Failed to decode base64: {}", e))?;

    // Decode CBOR
    let request: PaymentRequestData = ciborium::from_reader(&cbor_data[..])
        .map_err(|e| format!("Failed to decode CBOR: {}", e))?;

    Ok(request)
}

/// Pay a payment request
///
/// Parses the request, prepares tokens, and sends via the appropriate transport.
pub async fn pay_payment_request(
    request_string: String,
    custom_amount: Option<u64>,
) -> Result<u64, String> {
    use nostr_sdk::nips::nip19::Nip19Profile;
    use nostr_sdk::FromBech32;

    log::info!("Paying payment request");

    // Parse the request
    let request = parse_payment_request(&request_string)?;

    // Determine amount
    let amount = request.amount.or(custom_amount)
        .ok_or("Amount required but not specified in request or provided")?;

    if amount == 0 {
        return Err("Amount must be greater than 0".to_string());
    }

    // Find a compatible mint
    let our_mints = get_mints();
    let compatible_mint = if let Some(accepted_mints) = &request.mints {
        our_mints.iter()
            .find(|m| accepted_mints.contains(m))
            .cloned()
    } else {
        // If no mints specified, use our first mint
        our_mints.first().cloned()
    };

    let mint_url = compatible_mint
        .ok_or("No compatible mint found. You don't have tokens from any of the accepted mints.")?;

    // Check balance
    let balance = get_mint_balance(&mint_url);
    if balance < amount {
        return Err(format!(
            "Insufficient balance at {}. Have: {} sats, need: {} sats",
            shorten_url_internal(&mint_url), balance, amount
        ));
    }

    // Acquire lock
    let _lock = try_acquire_mint_lock(&mint_url)
        .ok_or_else(|| format!("Another operation is in progress for mint: {}", mint_url))?;

    // Get proofs and send
    let (all_proofs, event_ids_to_delete) = {
        let store = WALLET_TOKENS.read();
        let data = store.data();
        let tokens = data.read();
        let mint_tokens: Vec<_> = tokens.iter()
            .filter(|t| t.mint == mint_url)
            .collect();

        let mut all_proofs = Vec::new();
        let mut event_ids = Vec::new();

        for token in &mint_tokens {
            event_ids.push(token.event_id.clone());
            for proof in &token.proofs {
                all_proofs.push(proof_data_to_cdk_proof(proof)?);
            }
        }

        (all_proofs, event_ids)
    };

    // Prepare send
    let wallet = create_ephemeral_wallet(&mint_url, all_proofs).await?;

    let prepared = wallet.prepare_send(
        cdk::Amount::from(amount),
        cdk::wallet::SendOptions {
            include_fee: true,
            ..Default::default()
        }
    ).await
    .map_err(|e| format!("Failed to prepare send: {}", e))?;

    let token = prepared.confirm(None).await
        .map_err(|e| format!("Failed to confirm send: {}", e))?;

    // Get remaining proofs
    let keep_proofs = wallet.get_unspent_proofs().await
        .map_err(|e| format!("Failed to get remaining proofs: {}", e))?;

    // Get keysets to extract proofs from token
    let keysets_info = wallet.get_mint_keysets().await
        .map_err(|e| format!("Failed to get keysets: {}", e))?;

    // Convert token proofs to our format
    let proofs = token.proofs(&keysets_info)
        .map_err(|e| format!("Failed to extract proofs from token: {}", e))?;
    let token_proofs: Vec<ProofData> = proofs.iter()
        .map(|p| cdk_proof_to_proof_data(p))
        .collect();

    // Build payload
    let payload = PaymentRequestPayload {
        id: request.id.clone(),
        memo: None,
        mint: mint_url.clone(),
        unit: "sat".to_string(),
        proofs: token_proofs,
    };

    // Find transport
    let transport = request.transports.iter()
        .find(|t| t.transport_type == "nostr")
        .or_else(|| request.transports.iter().find(|t| t.transport_type == "http"));

    if let Some(transport) = transport {
        match transport.transport_type.as_str() {
            "nostr" => {
                // Send via Nostr gift wrap
                log::info!("Sending payment via Nostr transport");

                // Parse nprofile
                let nprofile = Nip19Profile::from_bech32(&transport.target)
                    .map_err(|e| format!("Invalid nprofile: {}", e))?;

                // Create ephemeral client
                let ephemeral_keys = nostr_sdk::Keys::generate();
                let client = nostr_sdk::Client::new(ephemeral_keys);

                // Add relays
                for relay in &nprofile.relays {
                    if let Err(e) = client.add_write_relay(relay.clone()).await {
                        log::warn!("Failed to add relay {}: {}", relay, e);
                    }
                }

                client.connect().await;

                // Create rumor (kind 14 - gift wrap payload)
                let payload_json = serde_json::to_string(&payload)
                    .map_err(|e| format!("Failed to serialize payload: {}", e))?;

                let rumor = nostr_sdk::EventBuilder::new(
                    nostr_sdk::Kind::from_u16(14),
                    payload_json
                ).build(nprofile.public_key);

                // Send gift wrap
                let result = client.gift_wrap_to(
                    nprofile.relays.clone(),
                    &nprofile.public_key,
                    rumor,
                    None
                ).await
                .map_err(|e| format!("Failed to send gift wrap: {}", e))?;

                log::info!("Payment sent via Nostr: {} successes, {} failures",
                    result.success.len(), result.failed.len());

                if result.success.is_empty() {
                    return Err("Failed to deliver payment to any relay".to_string());
                }
            }
            "http" => {
                // Send via HTTP POST
                log::info!("Sending payment via HTTP transport to {}", transport.target);

                let http_client = reqwest::Client::new();
                let response = http_client
                    .post(&transport.target)
                    .json(&payload)
                    .send()
                    .await
                    .map_err(|e| format!("HTTP request failed: {}", e))?;

                if !response.status().is_success() {
                    let status = response.status();
                    let body = response.text().await.unwrap_or_default();
                    return Err(format!("HTTP request failed with status {}: {}", status, body));
                }

                log::info!("Payment sent via HTTP");
            }
            _ => {
                return Err(format!("Unknown transport type: {}", transport.transport_type));
            }
        }
    } else {
        return Err("No transport available in payment request. Cannot deliver payment.".to_string());
    }

    // Update local state - remove old tokens, add remaining
    use nostr_sdk::signer::NostrSigner;

    let signer = crate::stores::signer::get_signer()
        .ok_or("No signer available")?
        .as_nostr_signer();

    let pubkey_str = auth_store::get_pubkey()
        .ok_or("Not authenticated")?;
    let pubkey = PublicKey::parse(&pubkey_str)
        .map_err(|e| format!("Invalid pubkey: {}", e))?;

    let client = nostr_client::NOSTR_CLIENT.read().as_ref()
        .ok_or("Client not initialized")?.clone();

    // Publish new token event (remaining proofs)
    let mut new_event_id: Option<String> = None;
    if !keep_proofs.is_empty() {
        let proof_data: Vec<ProofData> = keep_proofs.iter()
            .map(|p| cdk_proof_to_proof_data(p))
            .collect();

        let extended_proofs: Vec<ExtendedCashuProof> = proof_data.iter()
            .map(|p| ExtendedCashuProof::from(p.clone()))
            .collect();

        let token_event_data = ExtendedTokenEvent {
            mint: mint_url.clone(),
            unit: "sat".to_string(),
            proofs: extended_proofs,
            del: event_ids_to_delete.clone(),
        };

        let json_content = serde_json::to_string(&token_event_data)
            .map_err(|e| format!("Failed to serialize token event: {}", e))?;

        let encrypted = signer.nip44_encrypt(&pubkey, &json_content).await
            .map_err(|e| format!("Failed to encrypt token event: {}", e))?;

        let builder = nostr_sdk::EventBuilder::new(Kind::CashuWalletUnspentProof, encrypted);

        match client.send_event_builder(builder.clone()).await {
            Ok(event_output) => {
                new_event_id = Some(event_output.id().to_hex());
            }
            Err(e) => {
                log::warn!("Failed to publish token event: {}", e);
                queue_event_for_retry(builder, PendingEventType::TokenEvent).await;
            }
        }
    } else {
        // No remaining proofs - publish deletion
        if !event_ids_to_delete.is_empty() {
            use nostr::nips::nip09::EventDeletionRequest;
            let mut deletion_request = EventDeletionRequest::new();
            for event_id_str in &event_ids_to_delete {
                if let Ok(event_id) = EventId::parse(event_id_str) {
                    deletion_request = deletion_request.id(event_id);
                }
            }

            let builder = nostr_sdk::EventBuilder::delete(deletion_request);
            if let Err(e) = client.send_event_builder(builder.clone()).await {
                log::warn!("Failed to publish deletion event: {}", e);
                queue_event_for_retry(builder, PendingEventType::DeletionEvent).await;
            }
        }
    }

    // Update local state
    {
        let store = WALLET_TOKENS.read();
        let mut data = store.data();
        let mut tokens = data.write();

        // Remove old tokens
        tokens.retain(|t| !event_ids_to_delete.contains(&t.event_id));

        // Add remaining tokens
        if !keep_proofs.is_empty() {
            let proof_data: Vec<ProofData> = keep_proofs.iter()
                .map(|p| cdk_proof_to_proof_data(p))
                .collect();

            tokens.push(TokenData {
                event_id: new_event_id.unwrap_or_else(|| format!("local-{}", chrono::Utc::now().timestamp())),
                mint: mint_url,
                unit: "sat".to_string(),
                proofs: proof_data,
                created_at: chrono::Utc::now().timestamp() as u64,
            });
        }
    }

    // Update balance
    {
        let store = WALLET_TOKENS.read();
        let data = store.data();
        let tokens = data.read();
        let new_balance: u64 = tokens.iter()
            .flat_map(|t| &t.proofs)
            .map(|p| p.amount)
            .fold(0u64, |acc, amount| acc.saturating_add(amount));
        *WALLET_BALANCE.write() = new_balance;
    }

    log::info!("Payment request paid: {} sats", amount);

    Ok(amount)
}

/// Wait for a Nostr payment for a created request
///
/// This listens for gift-wrapped events on the relays and processes
/// incoming payments.
pub async fn wait_for_nostr_payment(
    request_id: String,
    timeout_secs: u64,
) -> Result<u64, String> {
    use nostr_sdk::prelude::*;

    log::info!("Waiting for Nostr payment for request: {}", request_id);

    *PAYMENT_REQUEST_PROGRESS.write() = Some(PaymentRequestProgress::WaitingForPayment);

    // Get wait info
    let wait_info = PENDING_PAYMENT_REQUESTS.read()
        .get(&request_id)
        .cloned()
        .ok_or("No pending request found for this ID")?;

    // Create client with ephemeral keys
    let keys = nostr_sdk::Keys::new(wait_info.secret_key);
    let client = nostr_sdk::Client::new(keys);

    // Add relays
    for relay in &wait_info.relays {
        if let Err(e) = client.add_read_relay(relay.clone()).await {
            log::warn!("Failed to add relay {}: {}", relay, e);
        }
    }

    client.connect().await;

    // Subscribe to events for our pubkey
    let filter = Filter::new().pubkey(wait_info.pubkey);
    client.subscribe(filter, None).await
        .map_err(|e| format!("Failed to subscribe: {}", e))?;

    // Wait for notifications with timeout
    let start = chrono::Utc::now().timestamp() as u64;
    let mut notifications = client.notifications();

    loop {
        // Check if cancelled (request removed from pending map by cancel_payment_request)
        if !PENDING_PAYMENT_REQUESTS.read().contains_key(&request_id) {
            return Err("Payment request cancelled".to_string());
        }

        // Check timeout
        let elapsed = chrono::Utc::now().timestamp() as u64 - start;
        if elapsed > timeout_secs {
            *PAYMENT_REQUEST_PROGRESS.write() = Some(PaymentRequestProgress::Cancelled);
            PENDING_PAYMENT_REQUESTS.write().remove(&request_id);
            return Err("Timeout waiting for payment".to_string());
        }

        // Wait for next notification with timeout
        let notification = {
            #[cfg(target_arch = "wasm32")]
            {
                use gloo_timers::future::TimeoutFuture;
                use futures::future::{select, Either};
                use futures::pin_mut;

                let timeout_fut = TimeoutFuture::new(5000); // 5 second intervals
                let recv_fut = notifications.recv();
                pin_mut!(timeout_fut);
                pin_mut!(recv_fut);

                match select(recv_fut, timeout_fut).await {
                    Either::Left((Ok(n), _)) => Some(n),
                    Either::Left((Err(_), _)) => break, // Channel closed
                    Either::Right((_, _)) => continue, // Timeout, check again
                }
            }
            #[cfg(not(target_arch = "wasm32"))]
            {
                match tokio::time::timeout(
                    std::time::Duration::from_secs(5),
                    notifications.recv()
                ).await {
                    Ok(Ok(n)) => Some(n),
                    Ok(Err(_)) => break, // Channel closed
                    Err(_) => continue, // Timeout, check again
                }
            }
        };

        if let Some(RelayPoolNotification::Event { event, .. }) = notification {
            // Try to unwrap gift wrap
            match client.unwrap_gift_wrap(&event).await {
                Ok(unwrapped) => {
                    let rumor = unwrapped.rumor;

                    // Try to parse payload
                    match serde_json::from_str::<PaymentRequestPayload>(&rumor.content) {
                        Ok(payload) => {
                            log::info!("Received payment payload: {} proofs", payload.proofs.len());

                            // Calculate amount
                            let amount: u64 = payload.proofs.iter()
                                .map(|p| p.amount)
                                .sum();

                            // Receive the tokens
                            match receive_payment_proofs(&payload.mint, payload.proofs).await {
                                Ok(_) => {
                                    *PAYMENT_REQUEST_PROGRESS.write() = Some(PaymentRequestProgress::Received { amount });
                                    PENDING_PAYMENT_REQUESTS.write().remove(&request_id);
                                    return Ok(amount);
                                }
                                Err(e) => {
                                    log::error!("Failed to receive payment proofs: {}", e);
                                    // Continue listening - might be a different payment
                                }
                            }
                        }
                        Err(e) => {
                            log::debug!("Failed to parse payment payload: {}", e);
                            // Continue listening
                        }
                    }
                }
                Err(e) => {
                    log::debug!("Failed to unwrap gift wrap: {}", e);
                    // Continue listening
                }
            }
        }
    }

    // Check if we exited due to cancellation (request already removed)
    if !PENDING_PAYMENT_REQUESTS.read().contains_key(&request_id) {
        return Err("Payment request cancelled".to_string());
    }

    *PAYMENT_REQUEST_PROGRESS.write() = Some(PaymentRequestProgress::Error {
        message: "Connection closed".to_string()
    });
    PENDING_PAYMENT_REQUESTS.write().remove(&request_id);
    Err("Connection closed while waiting for payment".to_string())
}

/// Receive proofs from a payment request payload
async fn receive_payment_proofs(
    mint_url: &str,
    proofs: Vec<ProofData>,
) -> Result<u64, String> {
    use nostr_sdk::signer::NostrSigner;

    log::info!("Receiving {} proofs from {}", proofs.len(), mint_url);

    // Convert to CDK proofs
    let cdk_proofs: Vec<cdk::nuts::Proof> = proofs.iter()
        .map(|p| proof_data_to_cdk_proof(p))
        .collect::<Result<Vec<_>, _>>()?;

    // Calculate amount
    let amount: u64 = cdk_proofs.iter()
        .map(|p| u64::from(p.amount))
        .sum();

    // Acquire lock
    let _lock = try_acquire_mint_lock(mint_url)
        .ok_or_else(|| format!("Another operation is in progress for mint: {}", mint_url))?;

    // Create wallet and receive
    let wallet = create_ephemeral_wallet(mint_url, cdk_proofs.clone()).await?;

    // Swap proofs to ensure they're ours (contacts mint)
    // Use swap with the proofs to get fresh ones
    let swapped = wallet.swap(
        None, // amount - None means all
        cdk::amount::SplitTarget::default(),
        cdk_proofs.clone(),
        None, // spending_conditions
        true, // include_fees
    ).await
    .map_err(|e| format!("Failed to swap proofs: {}", e))?;

    // Get final proofs
    let final_proofs = if let Some(swap_result) = swapped {
        swap_result
    } else {
        // Swap returned None, meaning proofs were already valid
        cdk_proofs
    };

    // Publish to Nostr
    let signer = crate::stores::signer::get_signer()
        .ok_or("No signer available")?
        .as_nostr_signer();

    let pubkey_str = auth_store::get_pubkey()
        .ok_or("Not authenticated")?;
    let pubkey = PublicKey::parse(&pubkey_str)
        .map_err(|e| format!("Invalid pubkey: {}", e))?;

    let client = nostr_client::NOSTR_CLIENT.read().as_ref()
        .ok_or("Client not initialized")?.clone();

    let proof_data: Vec<ProofData> = final_proofs.iter()
        .map(|p| cdk_proof_to_proof_data(p))
        .collect();

    let extended_proofs: Vec<ExtendedCashuProof> = proof_data.iter()
        .map(|p| ExtendedCashuProof::from(p.clone()))
        .collect();

    let token_event_data = ExtendedTokenEvent {
        mint: mint_url.to_string(),
        unit: "sat".to_string(),
        proofs: extended_proofs,
        del: vec![],
    };

    let json_content = serde_json::to_string(&token_event_data)
        .map_err(|e| format!("Failed to serialize token event: {}", e))?;

    let encrypted = signer.nip44_encrypt(&pubkey, &json_content).await
        .map_err(|e| format!("Failed to encrypt token event: {}", e))?;

    let builder = nostr_sdk::EventBuilder::new(Kind::CashuWalletUnspentProof, encrypted);

    let new_event_id = match client.send_event_builder(builder.clone()).await {
        Ok(event_output) => Some(event_output.id().to_hex()),
        Err(e) => {
            log::warn!("Failed to publish token event: {}", e);
            queue_event_for_retry(builder, PendingEventType::TokenEvent).await;
            None
        }
    };

    // Update local state
    {
        let store = WALLET_TOKENS.read();
        let mut data = store.data();
        let mut tokens = data.write();

        tokens.push(TokenData {
            event_id: new_event_id.unwrap_or_else(|| format!("local-{}", chrono::Utc::now().timestamp())),
            mint: mint_url.to_string(),
            unit: "sat".to_string(),
            proofs: proof_data,
            created_at: chrono::Utc::now().timestamp() as u64,
        });
    }

    // Update balance
    {
        let store = WALLET_TOKENS.read();
        let data = store.data();
        let tokens = data.read();
        let new_balance: u64 = tokens.iter()
            .flat_map(|t| &t.proofs)
            .map(|p| p.amount)
            .fold(0u64, |acc, amount| acc.saturating_add(amount));
        *WALLET_BALANCE.write() = new_balance;
    }

    log::info!("Received {} sats from payment request", amount);

    Ok(amount)
}

/// Cancel waiting for a payment request
pub fn cancel_payment_request(request_id: &str) {
    PENDING_PAYMENT_REQUESTS.write().remove(request_id);
    *PAYMENT_REQUEST_PROGRESS.write() = Some(PaymentRequestProgress::Cancelled);
}

/// Helper to shorten URL for display (internal use)
fn shorten_url_internal(url: &str) -> String {
    let url = url.trim_start_matches("https://").trim_start_matches("http://");
    if url.len() > 30 {
        format!("{}...", &url[..27])
    } else {
        url.to_string()
    }
}

/// Publish a single pending event
async fn publish_pending_event(event: &PendingNostrEvent) -> Result<(), String> {
    let client = nostr_client::NOSTR_CLIENT.read().as_ref()
        .ok_or("Nostr client not initialized")?
        .clone();

    // Deserialize the event JSON to Event
    let evt: nostr_sdk::Event = serde_json::from_str(&event.builder_json)
        .map_err(|e| format!("Failed to deserialize event: {}", e))?;

    // Publish to relays
    client.send_event(&evt).await
        .map_err(|e| format!("Failed to publish event: {}", e))?;

    Ok(())
}

/// Start background task to process pending events periodically
pub fn start_pending_events_processor() {
    spawn(async {
        loop {
            // Wait 5 minutes between checks
            #[cfg(target_arch = "wasm32")]
            {
                use gloo_timers::future::TimeoutFuture;
                TimeoutFuture::new(5 * 60 * 1000).await;
            }

            if let Err(e) = process_pending_events().await {
                log::error!("Error processing pending events: {}", e);
            }
        }
    });

    log::info!("Started pending events background processor (5 minute interval)");
}
