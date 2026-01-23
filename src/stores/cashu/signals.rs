//! Cashu wallet global signals
//!
//! All Dioxus GlobalSignal definitions for wallet state management.

// Allow dead_code for planned features not yet wired to UI
#![allow(dead_code)]

use dioxus::prelude::*;
use dioxus_stores::Store;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use super::types::*;

// =============================================================================
// Core Wallet Signals
// =============================================================================

/// Global signal for wallet state (privkey, mints list)
pub static WALLET_STATE: GlobalSignal<Option<WalletState>> = Signal::global(|| None);

/// Global signal for tokens (NIP-60 token events)
pub static WALLET_TOKENS: GlobalSignal<Store<WalletTokensStore>> =
    Signal::global(|| Store::new(WalletTokensStore::default()));

/// Global signal for transaction history
pub static WALLET_HISTORY: GlobalSignal<Store<WalletHistoryStore>> =
    Signal::global(|| Store::new(WalletHistoryStore::default()));

/// Global signal for total balance (computed from tokens)
pub static WALLET_BALANCE: GlobalSignal<u64> = Signal::global(|| 0);

/// Global signal for wallet status (loading state)
pub static WALLET_STATUS: GlobalSignal<WalletStatus> =
    Signal::global(|| WalletStatus::Uninitialized);

/// Global signal for detailed balance breakdown
pub static WALLET_BALANCES: GlobalSignal<WalletBalances> =
    Signal::global(WalletBalances::default);

/// Global signal for terms acceptance status
/// None = not yet checked, Some(true) = accepted, Some(false) = not accepted
pub static TERMS_ACCEPTED: GlobalSignal<Option<bool>> = Signal::global(|| None);

// =============================================================================
// Operation Tracking Signals
// =============================================================================

/// Operation lock to prevent concurrent wallet operations on the same mint
/// Uses GlobalSignal with HashSet to track mints currently being operated on
pub static MINT_OPERATION_LOCK: GlobalSignal<HashSet<String>> =
    Signal::global(HashSet::new);

/// Proof-to-EventId mapping for fast lookup of which Nostr event contains each proof
/// Key: proof secret, Value: event_id of the token event containing this proof
/// This enables correct deletion events when spending proofs
pub static PROOF_EVENT_MAP: GlobalSignal<HashMap<String, String>> =
    Signal::global(HashMap::new);

/// Global signal for active transactions (local-only tracking)
pub static ACTIVE_TRANSACTIONS: GlobalSignal<Vec<ActiveTransaction>> =
    Signal::global(Vec::new);

/// Proof secrets that the mint has reported as PENDING state
/// Different from local pending (is_pending flag) - this tracks proofs
/// where the mint itself says they're pending (e.g., lightning payment in-flight)
///
/// CDK best practice: Track timestamps for TTL cleanup to prevent stale entries
/// Key: proof secret, Value: timestamp when registered (seconds since epoch)
pub static PENDING_BY_MINT_SECRETS: GlobalSignal<HashMap<String, u64>> =
    Signal::global(HashMap::new);

// =============================================================================
// Pending Secrets Persistence Functions
// =============================================================================

/// Persist pending secrets to IndexedDB (call after modifications)
///
/// This ensures pending-at-mint state survives app restarts.
/// Uses spawn to avoid blocking the caller.
pub fn schedule_persist_pending_secrets() {
    #[cfg(target_arch = "wasm32")]
    {
        use dioxus::prelude::spawn;
        spawn(async move {
            persist_pending_secrets_impl().await;
        });
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

    // Snapshot the current state
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

    let now = {
        #[cfg(target_arch = "wasm32")]
        {
            (js_sys::Date::now() / 1000.0) as u64
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            use std::time::{SystemTime, UNIX_EPOCH};
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0)
        }
    };

    let (before_count, removed_count) = {
        let mut secrets = PENDING_BY_MINT_SECRETS.write();
        let before = secrets.len();
        secrets.retain(|_, timestamp| now.saturating_sub(*timestamp) < ONE_HOUR_SECS);
        let after = secrets.len();
        (before, before - after)
    };

    if removed_count > 0 {
        log::info!("Cleaned up {} expired pending secrets ({} remaining)", removed_count, before_count - removed_count);
        // Persist after cleanup
        persist_pending_secrets_impl().await;
    }
}

// =============================================================================
// Sync State Signals
// =============================================================================

/// Sync state for incremental Nostr event fetching
/// Tracks last sync timestamps to avoid fetching all events every time
pub static SYNC_STATE: GlobalSignal<Option<SyncState>> = Signal::global(|| None);

// =============================================================================
// Caching Signals
// =============================================================================

/// Shared IndexedDB database instance for all wallet operations
/// Using a single connection is more efficient than creating one per operation
pub static SHARED_LOCALSTORE: GlobalSignal<
    Option<Arc<crate::stores::indexeddb_database::IndexedDbDatabase>>,
> = Signal::global(|| None);

// =============================================================================
// Pending Events & Quotes Signals
// =============================================================================

/// Pending Nostr events (offline queue)
pub static PENDING_NOSTR_EVENTS: GlobalSignal<Vec<PendingNostrEvent>> =
    Signal::global(Vec::new);

/// Global signal for pending mint quotes (lightning receive)
pub static PENDING_MINT_QUOTES: GlobalSignal<Store<PendingMintQuotesStore>> =
    Signal::global(|| Store::new(PendingMintQuotesStore::default()));

/// Global signal for pending melt quotes (lightning send)
pub static PENDING_MELT_QUOTES: GlobalSignal<Store<PendingMeltQuotesStore>> =
    Signal::global(|| Store::new(PendingMeltQuotesStore::default()));

// =============================================================================
// Progress Tracking Signals
// =============================================================================

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

// =============================================================================
// Counter Backup Signal
// =============================================================================

/// Counter backups for mint removal/re-addition
/// When a mint is removed, its proof counters are backed up here
/// When the same mint is re-added, counters are restored
pub static COUNTER_BACKUPS: GlobalSignal<Vec<CounterBackup>> = Signal::global(Vec::new);

// =============================================================================
// Constants
// =============================================================================

/// NIP-78 d-tag identifier for Cashu wallet terms agreement
pub const TERMS_D_TAG: &str = "nostr.blue/cashu/terms";

/// Counter for generating unique transaction IDs
pub static TRANSACTION_ID_COUNTER: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(1);

// =============================================================================
// Batch Pagination Constants
// =============================================================================

/// Maximum number of proofs to sync at once (NUT-07 state check)
pub const MAX_SYNC_INPUT_SIZE: usize = 200;

/// Maximum number of proofs to swap at once
pub const MAX_SWAP_INPUT_SIZE: usize = 100;

/// Maximum number of proofs to receive in a single batch
pub const MAX_RECEIVE_BATCH_SIZE: usize = 100;

// =============================================================================
// Helper Functions
// =============================================================================

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

    *WALLET_BALANCE.write() = 0;
    *WALLET_STATUS.write() = WalletStatus::Uninitialized;
    *WALLET_BALANCES.write() = WalletBalances::default();
    *TERMS_ACCEPTED.write() = None;

    MINT_OPERATION_LOCK.write().clear();
    PROOF_EVENT_MAP.write().clear();
    ACTIVE_TRANSACTIONS.write().clear();
    PENDING_BY_MINT_SECRETS.write().clear();
    *SYNC_STATE.write() = None;

    clear_shared_localstore();

    PENDING_NOSTR_EVENTS.write().clear();

    *MELT_PROGRESS.write() = None;
    *TRANSFER_PROGRESS.write() = None;
    *PAYMENT_REQUEST_PROGRESS.write() = None;
    PENDING_PAYMENT_REQUESTS.write().clear();

    // Clear pending quote stores
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

    log::info!("Reset all wallet state");
}
