//! Cashu wallet global signals
//!
//! All Dioxus GlobalSignal definitions for wallet state management.
#![allow(dead_code)]
use super::types::*;
use dioxus::prelude::*;
use dioxus_stores::Store;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
/// Global signal for wallet state (privkey, mints list)
pub static WALLET_STATE: GlobalSignal<Option<WalletState>> = Signal::global(|| None);
/// Global signal for tokens (NIP-60 token events)
pub static WALLET_TOKENS: GlobalSignal<Store<WalletTokensStore>> =
    Signal::global(|| Store::new(WalletTokensStore::default()));
/// Global signal for transaction history
pub static WALLET_HISTORY: GlobalSignal<Store<WalletHistoryStore>> =
    Signal::global(|| Store::new(WalletHistoryStore::default()));
/// Global signal for wallet status (loading state)
pub static WALLET_STATUS: GlobalSignal<WalletStatus> =
    Signal::global(|| WalletStatus::Uninitialized);
/// Global signal for terms acceptance status
/// None = not yet checked, Some(true) = accepted, Some(false) = not accepted
pub static TERMS_ACCEPTED: GlobalSignal<Option<bool>> = Signal::global(|| None);
/// Operation lock to prevent concurrent wallet operations on the same mint
/// Uses GlobalSignal with HashSet to track mints currently being operated on
pub static MINT_OPERATION_LOCK: GlobalSignal<HashSet<String>> = Signal::global(HashSet::new);
/// Proof-to-EventId mapping for fast lookup of which Nostr event contains each proof
/// Key: proof secret, Value: event_id of the token event containing this proof
/// This enables correct deletion events when spending proofs
pub static PROOF_EVENT_MAP: GlobalSignal<HashMap<String, String>> = Signal::global(HashMap::new);
/// Global signal for active transactions (local-only tracking)
pub static ACTIVE_TRANSACTIONS: GlobalSignal<Vec<ActiveTransaction>> = Signal::global(Vec::new);
/// Proof secrets that the mint has reported as PENDING state
/// Different from local pending (is_pending flag) - this tracks proofs
/// where the mint itself says they're pending (e.g., lightning payment in-flight)
///
/// CDK best practice: Track timestamps for TTL cleanup to prevent stale entries
/// Key: proof secret, Value: timestamp when registered (seconds since epoch)
pub static PENDING_BY_MINT_SECRETS: GlobalSignal<HashMap<String, u64>> =
    Signal::global(HashMap::new);
/// In-flight melt requests for crash recovery
///
/// Persisted to IndexedDB BEFORE the melt network call to ensure we can
/// recover change proofs if the app crashes during the operation.
///
/// SAFETY: This is critical for fund safety - without this, a crash during
/// melt could lose change proofs that the mint has already issued.
pub static IN_FLIGHT_MELT_REQUESTS: GlobalSignal<Vec<InFlightMeltRequest>> =
    Signal::global(Vec::new);
/// Currently executing operations (by transaction_id)
///
/// Used to prevent timeout recovery from interfering with active operations.
/// Operations must register here before starting and unregister when done.
pub static ACTIVE_OPERATIONS: GlobalSignal<HashSet<String>> = Signal::global(HashSet::new);
/// In-flight send/swap requests for proof recovery protection
///
/// Tracks active send/swap operations to prevent proof_recovery from
/// incorrectly reclaiming proofs that are actively being used.
///
/// SAFETY: This is critical for fund safety - without this, recovery could
/// reclaim proofs from active send/swap operations, causing fund loss.
pub static IN_FLIGHT_SEND_REQUESTS: GlobalSignal<Vec<super::types::InFlightSendRequest>> =
    Signal::global(Vec::new);
/// Add an in-flight melt request to tracking
pub fn add_in_flight_melt_request(request: InFlightMeltRequest) {
    let tx_id = request.transaction_id.clone();
    {
        let mut requests = IN_FLIGHT_MELT_REQUESTS.write();
        if requests.iter().any(|r| r.transaction_id == tx_id) {
            log::debug!("In-flight request {} already exists, skipping", tx_id);
            return;
        }
        requests.push(request);
    }
    ACTIVE_OPERATIONS.write().insert(tx_id.clone());
    log::debug!("Added in-flight melt request: {}", tx_id);
}
/// Remove an in-flight melt request from tracking
pub fn remove_in_flight_melt_request(transaction_id: &str) {
    IN_FLIGHT_MELT_REQUESTS
        .write()
        .retain(|r| r.transaction_id != transaction_id);
    ACTIVE_OPERATIONS.write().remove(transaction_id);
    log::debug!("Removed in-flight melt request: {}", transaction_id);
}
/// Check if a transaction is currently active
pub fn is_transaction_active(transaction_id: &Option<String>) -> bool {
    transaction_id
        .as_ref()
        .map(|id| ACTIVE_OPERATIONS.read().contains(id))
        .unwrap_or(false)
}
/// Check if a string transaction_id is currently active
pub fn is_transaction_id_active(transaction_id: &str) -> bool {
    ACTIVE_OPERATIONS.read().contains(transaction_id)
}
/// Add an in-flight send/swap request to tracking (also adds to ACTIVE_OPERATIONS)
///
/// SAFETY: Must be called BEFORE the send/swap operation to ensure proofs are
/// protected from timeout recovery during the operation.
pub fn add_in_flight_send_request(request: super::types::InFlightSendRequest) {
    let tx_id = request.transaction_id.clone();
    {
        let mut requests = IN_FLIGHT_SEND_REQUESTS.write();
        if requests.iter().any(|r| r.transaction_id == tx_id) {
            log::debug!("In-flight send request {} already exists, skipping", tx_id);
            return;
        }
        requests.push(request);
    }
    ACTIVE_OPERATIONS.write().insert(tx_id.clone());
    log::debug!("Added in-flight send request: {}", tx_id);
}
/// Remove an in-flight send/swap request from tracking
///
/// SAFETY: Must be called AFTER the send/swap operation completes (success or error)
/// to allow proofs to be recovered if needed.
pub fn remove_in_flight_send_request(transaction_id: &str) {
    IN_FLIGHT_SEND_REQUESTS
        .write()
        .retain(|r| r.transaction_id != transaction_id);
    ACTIVE_OPERATIONS.write().remove(transaction_id);
    log::debug!("Removed in-flight send request: {}", transaction_id);
}
/// Persist in-flight melt requests to IndexedDB
///
/// CRITICAL: This MUST be awaited and verified before proceeding with melt.
/// If persistence fails, the melt operation should be aborted to prevent
/// potential fund loss from crash recovery inability.
pub async fn persist_in_flight_melt_requests() -> Result<(), String> {
    let localstore = match SHARED_LOCALSTORE.read().as_ref() {
        Some(store) => store.clone(),
        None => {
            return Err("Localstore not initialized".to_string());
        }
    };
    let requests = IN_FLIGHT_MELT_REQUESTS.read().clone();
    localstore
        .save_in_flight_melt_requests(&requests)
        .await
        .map_err(|e| format!("Failed to persist in-flight melt requests: {}", e))?;
    log::debug!(
        "Persisted {} in-flight melt requests to IndexedDB",
        requests.len()
    );
    Ok(())
}
/// Persist a single in-flight melt request atomically
///
/// SAFETY: Uses Dioxus pattern - update in-memory signal first (instant, non-async),
/// then persist to IndexedDB. This prevents race conditions in the load-append-save
/// pattern by making the in-memory signal the source of truth during execution.
/// On crash/reload, IndexedDB provides the persistent copy for recovery.
pub async fn persist_single_in_flight_request(
    request: &super::types::InFlightMeltRequest,
) -> Result<(), String> {
    {
        let mut requests = IN_FLIGHT_MELT_REQUESTS.write();
        if requests
            .iter()
            .any(|r| r.transaction_id == request.transaction_id)
        {
            log::debug!(
                "In-flight request {} already exists, skipping",
                request.transaction_id
            );
            return Ok(());
        }
        requests.push(request.clone());
    }
    ACTIVE_OPERATIONS
        .write()
        .insert(request.transaction_id.clone());
    let localstore = SHARED_LOCALSTORE
        .read()
        .as_ref()
        .ok_or("Localstore not initialized")?
        .clone();
    let requests = IN_FLIGHT_MELT_REQUESTS.read().clone();
    if let Err(e) = localstore.save_in_flight_melt_requests(&requests).await {
        {
            let mut requests = IN_FLIGHT_MELT_REQUESTS.write();
            requests.retain(|r| r.transaction_id != request.transaction_id);
        }
        ACTIVE_OPERATIONS.write().remove(&request.transaction_id);
        log::warn!(
            "Rolled back in-memory state after persist failure for {}",
            request.transaction_id
        );
        return Err(format!("Failed to save in-flight request: {}", e));
    }
    log::debug!(
        "Persisted in-flight melt request {}",
        request.transaction_id
    );
    Ok(())
}
/// Atomically update tokens and recalculate balance
///
/// Holds WALLET_TOKENS lock during entire operation and updates
/// WALLET_BALANCES before releasing, preventing race conditions.
/// Following CDK pattern: balance is always computed from proof state.
///
/// Safety: Mutates a copy first; only replaces on success (no partial mutation on error).
pub fn atomic_token_update<F>(mutate_fn: F) -> Result<u64, String>
where
    F: FnOnce(&mut Vec<super::types::TokenData>) -> Result<(), String>,
{
    let store = WALLET_TOKENS.read();
    let mut data = store.data();
    let mut tokens_write = data.write();
    let mut working_copy = tokens_write.clone();
    mutate_fn(&mut working_copy)?;
    *tokens_write = working_copy;
    let (available, pending) =
        tokens_write
            .iter()
            .flat_map(|t| &t.proofs)
            .fold((0u64, 0u64), |(avail, pend), proof| {
                if proof.state.is_spendable() {
                    (avail + proof.amount, pend)
                } else if proof.state.is_pending() {
                    (avail, pend + proof.amount)
                } else {
                    (avail, pend)
                }
            });
    *crate::stores::cashu_cdk_bridge::WALLET_BALANCES.write() =
        crate::stores::cashu_cdk_bridge::WalletBalances {
            total: available + pending,
            available,
            pending,
        };
    Ok(available)
}
/// Crash-safe token replacement: add new tokens BEFORE deleting old ones
///
/// Ensures worst case on crash is duplicate tokens (recoverable), never lost tokens.
pub fn atomic_token_replace(
    tokens_to_add: Vec<super::types::TokenData>,
    event_ids_to_delete: &[String],
) -> Result<u64, String> {
    atomic_token_update(|tokens| {
        for token in tokens_to_add {
            tokens.push(token);
        }
        if !event_ids_to_delete.is_empty() {
            tokens.retain(|t| !event_ids_to_delete.contains(&t.event_id));
        }
        Ok(())
    })
}
/// Recalculate wallet balances from WALLET_TOKENS
///
/// This performs a standalone recomputation of balances from WALLET_TOKENS.
/// Note: `atomic_token_update` also updates WALLET_BALANCES as part of atomic
/// token mutations. Use this function for explicit balance refresh; use
/// atomic_token_update/atomic_token_replace for transactional token changes.
pub fn update_wallet_balances() {
    let store = WALLET_TOKENS.read();
    let data = store.data();
    let tokens = data.read();
    let (available, pending) =
        tokens
            .iter()
            .flat_map(|t| &t.proofs)
            .fold((0u64, 0u64), |(avail, pend), proof| {
                if proof.state.is_spendable() {
                    (avail + proof.amount, pend)
                } else if proof.state.is_pending() {
                    (avail, pend + proof.amount)
                } else {
                    (avail, pend)
                }
            });
    let total = available + pending;
    *crate::stores::cashu_cdk_bridge::WALLET_BALANCES.write() =
        crate::stores::cashu_cdk_bridge::WalletBalances {
            total,
            available,
            pending,
        };
}
/// Load in-flight melt requests from IndexedDB (call on startup)
///
/// Restores the in-flight melt state from the previous session for recovery.
pub async fn load_in_flight_melt_requests() {
    let localstore = match SHARED_LOCALSTORE.read().as_ref() {
        Some(store) => store.clone(),
        None => {
            log::debug!("Skipping in-flight melt load: localstore not initialized");
            return;
        }
    };
    match localstore.load_in_flight_melt_requests().await {
        Ok(Some(requests)) => {
            if !requests.is_empty() {
                log::info!(
                    "Loaded {} in-flight melt requests from storage",
                    requests.len()
                );
                {
                    let mut active_ops = ACTIVE_OPERATIONS.write();
                    for req in &requests {
                        active_ops.insert(req.transaction_id.clone());
                    }
                }
                *IN_FLIGHT_MELT_REQUESTS.write() = requests;
            }
        }
        Ok(None) => {
            log::debug!("No in-flight melt requests found in storage");
        }
        Err(e) => {
            log::warn!("Failed to load in-flight melt requests: {}", e);
        }
    }
}
/// Schedule persistence of pending secrets to IndexedDB.
///
/// NOTE: This only schedules persistence on WASM/IndexedDB targets.
/// On non-WASM targets, this is a no-op (pending secrets are not persisted).
///
/// This ensures pending-at-mint state survives app restarts.
/// Uses spawn to avoid blocking the caller.
pub fn schedule_persist_pending_secrets() {
    #[cfg(feature = "web")]
    {
        use dioxus::prelude::spawn;
        spawn(async move {
            persist_pending_secrets_impl().await;
        });
    }
    #[cfg(not(feature = "web"))]
    {
        log::debug!("schedule_persist_pending_secrets: no-op on non-WASM target");
    }
}
/// Internal implementation for persisting pending secrets
async fn persist_pending_secrets_impl() {
    let localstore = match SHARED_LOCALSTORE.read().as_ref() {
        Some(store) => store.clone(),
        None => {
            log::debug!("Skipping pending secrets persistence: localstore not initialized");
            return;
        }
    };
    let secrets = PENDING_BY_MINT_SECRETS.read().clone();
    if let Err(e) = localstore.save_pending_mint_secrets(&secrets).await {
        log::warn!("Failed to persist pending secrets: {}", e);
    } else {
        log::debug!("Persisted {} pending secrets to IndexedDB", secrets.len());
    }
}
/// Load pending secrets from IndexedDB (call on startup)
///
/// Restores the pending-at-mint state from the previous session.
pub async fn load_pending_secrets() {
    let localstore = match SHARED_LOCALSTORE.read().as_ref() {
        Some(store) => store.clone(),
        None => {
            log::debug!("Skipping pending secrets load: localstore not initialized");
            return;
        }
    };
    match localstore.load_pending_mint_secrets().await {
        Ok(Some(secrets)) => {
            if !secrets.is_empty() {
                log::info!("Loaded {} pending mint secrets from storage", secrets.len());
                *PENDING_BY_MINT_SECRETS.write() = secrets;
            }
        }
        Ok(None) => {
            log::debug!("No pending secrets found in storage");
        }
        Err(e) => {
            log::warn!("Failed to load pending secrets: {}", e);
        }
    }
}
/// Clean up expired pending secrets (older than 1 hour)
///
/// Pending proofs should resolve within a reasonable time.
/// This prevents stale entries from accumulating indefinitely.
pub async fn cleanup_expired_pending_secrets() {
    const ONE_HOUR_SECS: u64 = 60 * 60;
    let now = super::utils::now_secs();
    let (before_count, removed_count) = {
        let mut secrets = PENDING_BY_MINT_SECRETS.write();
        let before = secrets.len();
        secrets.retain(|_, timestamp| now.saturating_sub(*timestamp) < ONE_HOUR_SECS);
        let after = secrets.len();
        (before, before - after)
    };
    if removed_count > 0 {
        log::info!(
            "Cleaned up {} expired pending secrets ({} remaining)",
            removed_count,
            before_count - removed_count
        );
        persist_pending_secrets_impl().await;
    }
}
/// Sync state for incremental Nostr event fetching
/// Tracks last sync timestamps to avoid fetching all events every time
pub static SYNC_STATE: GlobalSignal<Option<SyncState>> = Signal::global(|| None);
/// Shared IndexedDB database instance for all wallet operations
/// Using a single connection is more efficient than creating one per operation
pub static SHARED_LOCALSTORE: GlobalSignal<
    Option<Arc<crate::stores::indexeddb_database::IndexedDbDatabase>>,
> = Signal::global(|| None);
/// Pending Nostr events (offline queue)
pub static PENDING_NOSTR_EVENTS: GlobalSignal<Vec<PendingNostrEvent>> = Signal::global(Vec::new);
/// Global signal for pending mint quotes (lightning receive)
pub static PENDING_MINT_QUOTES: GlobalSignal<Store<PendingMintQuotesStore>> =
    Signal::global(|| Store::new(PendingMintQuotesStore::default()));
/// Global signal for pending melt quotes (lightning send)
pub static PENDING_MELT_QUOTES: GlobalSignal<Store<PendingMeltQuotesStore>> =
    Signal::global(|| Store::new(PendingMeltQuotesStore::default()));
/// Global signal for melt progress (lightning payment progress)
pub static MELT_PROGRESS: GlobalSignal<Option<MeltProgress>> = Signal::global(|| None);
/// Global signal for transfer progress (cross-mint transfer)
pub static TRANSFER_PROGRESS: GlobalSignal<Option<TransferProgress>> = Signal::global(|| None);
/// Global signal for payment request progress (NUT-18)
pub static PAYMENT_REQUEST_PROGRESS: GlobalSignal<Option<PaymentRequestProgress>> =
    Signal::global(|| None);
/// Global signal for pending payment requests waiting for payment
pub static PENDING_PAYMENT_REQUESTS: GlobalSignal<HashMap<String, NostrPaymentWaitInfo>> =
    Signal::global(HashMap::new);
/// Counter backups for mint removal/re-addition
/// When a mint is removed, its proof counters are backed up here
/// When the same mint is re-added, counters are restored
pub static COUNTER_BACKUPS: GlobalSignal<Vec<CounterBackup>> = Signal::global(Vec::new);
/// NIP-78 d-tag identifier for Cashu wallet terms agreement
pub const TERMS_D_TAG: &str = "nostr.blue/cashu/terms";
/// Counter for generating unique transaction IDs
pub static TRANSACTION_ID_COUNTER: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(1);
/// Maximum number of proofs to sync at once (NUT-07 state check)
pub const MAX_SYNC_INPUT_SIZE: usize = 200;
/// Maximum number of proofs to swap at once
pub const MAX_SWAP_INPUT_SIZE: usize = 100;
/// Maximum number of proofs to receive in a single batch
pub const MAX_RECEIVE_BATCH_SIZE: usize = 100;
/// RAII guard for in-flight send/swap tracking cleanup
/// Follows CDK pattern: decrement on all exit paths (success, error, panic)
pub struct InFlightGuard {
    tx_id: Option<String>,
}
impl InFlightGuard {
    pub fn new(tx_id: String) -> Self {
        Self { tx_id: Some(tx_id) }
    }
    /// Dismiss guard - call on success when manual cleanup is done
    pub fn dismiss(&mut self) {
        self.tx_id = None;
    }
}
impl Drop for InFlightGuard {
    fn drop(&mut self) {
        if let Some(ref tx_id) = self.tx_id {
            remove_in_flight_send_request(tx_id);
            log::debug!("In-flight guard auto-cleaned tx_id: {}", tx_id);
        }
    }
}
/// Guard that releases the mint lock when dropped (RAII pattern)
pub struct MintOperationGuard {
    mint_url: String,
}
impl MintOperationGuard {
    pub fn new(mint_url: String) -> Self {
        Self { mint_url }
    }
}
impl Drop for MintOperationGuard {
    fn drop(&mut self) {
        MINT_OPERATION_LOCK.write().remove(&self.mint_url);
        log::debug!("Released operation lock for mint: {}", self.mint_url);
    }
}
/// Try to acquire an operation lock for a mint
/// Returns None if the mint is already being operated on
pub fn try_acquire_mint_lock(mint_url: &str) -> Option<MintOperationGuard> {
    let mut locks = MINT_OPERATION_LOCK.write();
    if locks.contains(mint_url) {
        log::warn!("Operation already in progress for mint: {}", mint_url);
        None
    } else {
        locks.insert(mint_url.to_string());
        log::debug!("Acquired operation lock for mint: {}", mint_url);
        Some(MintOperationGuard::new(mint_url.to_string()))
    }
}
/// Clear shared database connection (e.g., on logout)
pub fn clear_shared_localstore() {
    *SHARED_LOCALSTORE.write() = None;
    log::info!("Cleared shared localstore");
}
/// Reset all wallet state (for logout or wallet reset)
pub fn reset_wallet_state() {
    *WALLET_STATE.write() = None;
    {
        let store = WALLET_TOKENS.read();
        let mut data = store.data();
        data.write().clear();
    }
    {
        let store = WALLET_HISTORY.read();
        let mut data = store.data();
        data.write().clear();
    }
    *WALLET_STATUS.write() = WalletStatus::Uninitialized;
    *crate::stores::cashu_cdk_bridge::WALLET_BALANCES.write() =
        crate::stores::cashu_cdk_bridge::WalletBalances::default();
    *TERMS_ACCEPTED.write() = None;
    MINT_OPERATION_LOCK.write().clear();
    PROOF_EVENT_MAP.write().clear();
    ACTIVE_TRANSACTIONS.write().clear();
    PENDING_BY_MINT_SECRETS.write().clear();
    IN_FLIGHT_MELT_REQUESTS.write().clear();
    IN_FLIGHT_SEND_REQUESTS.write().clear();
    ACTIVE_OPERATIONS.write().clear();
    *SYNC_STATE.write() = None;
    clear_shared_localstore();
    PENDING_NOSTR_EVENTS.write().clear();
    *MELT_PROGRESS.write() = None;
    *TRANSFER_PROGRESS.write() = None;
    *PAYMENT_REQUEST_PROGRESS.write() = None;
    PENDING_PAYMENT_REQUESTS.write().clear();
    {
        let store = PENDING_MINT_QUOTES.read();
        let mut data = store.data();
        data.write().clear();
    }
    {
        let store = PENDING_MELT_QUOTES.read();
        let mut data = store.data();
        data.write().clear();
    }
    super::nutzap_signals::clear_nutzap_state();
    log::info!("Reset all wallet state");
}
