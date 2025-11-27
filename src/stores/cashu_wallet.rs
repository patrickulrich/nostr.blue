use dioxus::prelude::*;
use dioxus::signals::ReadableExt;
use dioxus_stores::Store;
use nostr_sdk::{Event, Filter, Kind, PublicKey, SecretKey, EventId};
use nostr_sdk::nips::nip60::{WalletEvent, CashuProof, TransactionDirection, SpendingHistory};
use nostr_sdk::types::url::Url;
use nostr_sdk::types::time::Timestamp;
use crate::stores::{auth_store, nostr_client};
use std::time::Duration;
use serde::{Deserialize, Serialize};

// CDK database trait for calling methods on IndexedDbDatabase
use cdk::cdk_database::WalletDatabase;

/// Custom deserialization structure for token events (more lenient than rust-nostr)
#[derive(Debug, Clone, Serialize, Deserialize)]
struct TokenEventData {
    pub mint: String,
    pub proofs: Vec<ProofData>,
    #[serde(default)]
    pub del: Vec<String>,
}

/// Extended token event with P2PK support (extends rust-nostr's TokenEvent)
/// Uses ExtendedCashuProof instead of CashuProof to preserve witness/DLEQ fields
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ExtendedTokenEvent {
    pub mint: String,
    pub proofs: Vec<ExtendedCashuProof>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub del: Vec<String>,
}

/// DLEQ proof data (preserves P2PK verification capability)
#[derive(Debug, Clone, Serialize, Deserialize)]
struct DleqData {
    pub e: String,
    pub s: String,
    pub r: String,
}

/// Custom deserialization structure for proofs (allows missing fields)
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ProofData {
    #[serde(default)]
    pub id: String,
    pub amount: u64,
    pub secret: String,
    #[serde(default)]
    pub c: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub witness: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dleq: Option<DleqData>,
}

/// Extended Cashu proof with P2PK support (superset of nostr_sdk::nips::nip60::CashuProof)
/// Preserves witness and DLEQ fields for P2PK verification while maintaining NIP-60 compatibility
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ExtendedCashuProof {
    pub id: String,
    pub amount: u64,
    pub secret: String,
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
#[derive(Clone, Debug, PartialEq)]
pub struct TokenData {
    pub event_id: String,
    pub mint: String,
    pub unit: String,
    pub proofs: Vec<CashuProof>,
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
async fn get_or_create_wallet(mint_url: &str) -> Result<std::sync::Arc<cdk::Wallet>, String> {
    use cdk::Wallet;
    use cdk::nuts::CurrencyUnit;

    // Check cache first
    if let Some(wallet) = WALLET_CACHE.read().get(mint_url) {
        log::debug!("Using cached wallet for {}", mint_url);
        return Ok(wallet.clone());
    }

    // Create new wallet
    log::info!("Creating new wallet for {}", mint_url);

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

    log::info!("Cached new wallet for {}", mint_url);
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
async fn queue_event_for_retry(builder: nostr_sdk::EventBuilder, event_type: PendingEventType) {
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

/// Initialize wallet by fetching from relays
pub async fn init_wallet() -> Result<(), String> {
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

                        // Fetch tokens and history
                        if let Err(e) = fetch_tokens().await {
                            log::error!("Failed to fetch tokens: {}", e);
                        }

                        if let Err(e) = fetch_history().await {
                            log::error!("Failed to fetch history: {}", e);
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
                                // Convert ProofData to CashuProof
                                let proofs: Vec<CashuProof> = token_event.proofs.iter()
                                    .map(|p| CashuProof {
                                        id: if p.id.is_empty() {
                                            // Generate a placeholder ID if missing
                                            format!("{}_{}", p.secret, p.amount)
                                        } else {
                                            p.id.clone()
                                        },
                                        amount: p.amount,
                                        secret: p.secret.clone(),
                                        c: p.c.clone(),
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
                                        unit: "sat".to_string(), // TODO: Parse unit from event
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
fn cdk_proof_to_proof_data(proof: &cdk::nuts::Proof) -> ProofData {
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

/// Receive ecash from a token string
pub async fn receive_tokens(token_string: String) -> Result<u64, String> {
    use cdk::nuts::Token;
    use cdk::wallet::ReceiveOptions;
    use std::str::FromStr;
    use nostr_sdk::signer::NostrSigner;

    log::info!("Receiving token...");

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

    // Receive token (contacts mint to swap proofs) with auto-cleanup on spent errors
    // Use the corrected/padded token to ensure padding fix is preserved
    let amount_received = match wallet.receive(
        &token_to_parse,
        ReceiveOptions::default()
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
        proofs: proof_data.iter().map(|p| CashuProof {
            id: p.id.clone(),
            amount: p.amount,
            secret: p.secret.clone(),
            c: p.c.clone(),
        }).collect(),
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
                // Convert CashuProof to CDK proof (CashuProof doesn't store witness/dleq)
                let temp_proof_data = ProofData {
                    id: proof.id.clone(),
                    amount: proof.amount,
                    secret: proof.secret.clone(),
                    c: proof.c.clone(),
                    witness: None,
                    dleq: None,
                };
                all_proofs.push(proof_data_to_cdk_proof(&temp_proof_data)?);
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
                proofs: keep_proof_data.iter().map(|p| CashuProof {
                    id: p.id.clone(),
                    amount: p.amount,
                    secret: p.secret.clone(),
                    c: p.c.clone(),
                }).collect(),
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

    Ok(token_string)
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

        let all_proofs: Vec<CashuProof> = mint_tokens.iter()
            .flat_map(|t| &t.proofs)
            .cloned()
            .collect();

        // Convert to CDK proofs
        let cdk_proofs: Result<Vec<_>, _> = all_proofs.iter()
            .map(|p| {
                let temp = ProofData {
                    id: p.id.clone(),
                    amount: p.amount,
                    secret: p.secret.clone(),
                    c: p.c.clone(),
                    witness: None,
                    dleq: None,
                };
                proof_data_to_cdk_proof(&temp)
            })
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
    let available_proofs: Vec<CashuProof> = all_mint_proofs.into_iter()
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

        if mint_tokens.is_empty() {
            log::info!("No tokens found for this mint");
            return Ok((0, 0));
        }

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

    // Update local state - remove all tokens for this mint
    let store = WALLET_TOKENS.read();
    let mut data = store.data();
    let mut tokens_write = data.write();
    tokens_write.retain(|t| t.mint != mint_url);
    drop(tokens_write);

    // Remove mint from wallet state
    let mut state_write = WALLET_STATE.write();
    if let Some(ref mut state) = *state_write {
        state.mints.retain(|m| m != &mint_url);
    }
    drop(state_write);

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
        proofs: proof_data.iter().map(|p| CashuProof {
            id: p.id.clone(),
            amount: p.amount,
            secret: p.secret.clone(),
            c: p.c.clone(),
        }).collect(),
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

    log::info!("Mint complete: {} sats", amount_minted);

    Ok(amount_minted)
}

/// Create a melt quote (request to pay a lightning invoice)
pub async fn create_melt_quote(
    mint_url: String,
    invoice: String,
) -> Result<MeltQuoteInfo, String> {
    log::info!("Creating melt quote for invoice at {}", mint_url);

    // Create ephemeral wallet
    let wallet = create_ephemeral_wallet(&mint_url, vec![]).await?;

    // Create melt quote
    let quote = wallet.melt_quote(invoice.clone(), None).await
        .map_err(|e| format!("Failed to create melt quote: {}", e))?;

    log::info!("Melt quote created: {}", quote.id);

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

    // Acquire mint operation lock to prevent concurrent operations
    let _lock_guard = try_acquire_mint_lock(&mint_url)
        .ok_or_else(|| format!("Another operation is in progress for mint: {}", mint_url))?;

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
                let temp_proof_data = ProofData {
                    id: proof.id.clone(),
                    amount: proof.amount,
                    secret: proof.secret.clone(),
                    c: proof.c.clone(),
                    witness: None,
                    dleq: None,
                };
                all_proofs.push(proof_data_to_cdk_proof(&temp_proof_data)?);
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

                    return Err(format!("Failed to melt: {}. Quote has been cleaned up, please try again with a new quote.", e));
                }
            }
        }
    };

    let paid = melted.state == cdk::nuts::MeltQuoteState::Paid;
    let preimage = melted.preimage;
    let fee_paid = u64::from(melted.fee_paid);

    log::info!("Melt result: paid={}, fee_paid={}", paid, fee_paid);

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
                proofs: proof_data.iter().map(|p| CashuProof {
                    id: p.id.clone(),
                    amount: p.amount,
                    secret: p.secret.clone(),
                    c: p.c.clone(),
                }).collect(),
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

    log::info!("Melt complete: paid={}, amount={}, fee={}", paid, quote_info.amount, fee_paid);

    Ok((paid, preimage, fee_paid))
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
async fn create_history_event_with_type(
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
