//! Cashu wallet data types
//!
//! All data structures used throughout the cashu wallet module.

// Allow dead_code for planned features not yet wired to UI
#![allow(dead_code)]

use dioxus_stores::Store;
use nostr_sdk::nips::nip60::TransactionDirection;
use serde::{Deserialize, Serialize};

/// Default unit for Cashu proofs (per NIP-60 spec, defaults to "sat")
pub fn default_unit() -> String {
    "sat".to_string()
}

// =============================================================================
// Proof Types
// =============================================================================

/// Proof state enum aligned with CDK's State
/// This replaces the dual is_pending/is_spent boolean flags to provide a single source of truth
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ProofState {
    #[default]
    Unspent,      // Available for spending
    Pending,      // Receive operation in progress
    Reserved,     // Locked for current send operation (PreparedSend)
    PendingSpent, // Sent but not yet confirmed by mint
    Spent,        // Confirmed spent
}

impl ProofState {
    /// Returns true if the proof is available for spending
    pub fn is_spendable(&self) -> bool {
        matches!(self, ProofState::Unspent)
    }

    /// Returns true if the proof is in any pending state
    pub fn is_pending(&self) -> bool {
        matches!(self, ProofState::Pending | ProofState::Reserved | ProofState::PendingSpent)
    }

    /// Returns true if the proof is spent
    pub fn is_spent(&self) -> bool {
        matches!(self, ProofState::Spent)
    }
}

// =============================================================================
// Send Options (CDK-compliant)
// =============================================================================

/// Send mode determines how the wallet handles proof selection
///
/// Follows CDK's SendKind pattern for offline/online operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SendMode {
    /// Online mode with exact amount (default)
    /// Wallet can swap proofs with mint to get exact amount
    #[default]
    OnlineExact,
    /// Online mode with tolerance
    /// Prefer exact, but allow overpaying up to tolerance
    OnlineTolerance(u64),
    /// Offline mode with exact amount
    /// Must use locally available proofs that sum to exact amount
    OfflineExact,
    /// Offline mode with tolerance
    /// Use locally available proofs, allow overpaying up to tolerance
    OfflineTolerance(u64),
}

impl SendMode {
    /// Check if this mode requires online connectivity
    pub fn is_online(&self) -> bool {
        matches!(self, Self::OnlineExact | Self::OnlineTolerance(_))
    }

    /// Check if this mode can work offline
    pub fn is_offline(&self) -> bool {
        matches!(self, Self::OfflineExact | Self::OfflineTolerance(_))
    }

    /// Get the tolerance amount (0 for exact modes)
    pub fn tolerance(&self) -> u64 {
        match self {
            Self::OnlineTolerance(t) | Self::OfflineTolerance(t) => *t,
            _ => 0,
        }
    }

    /// Convert to CDK's SendKind
    pub fn to_cdk_send_kind(&self) -> cdk::wallet::SendKind {
        use cdk::wallet::SendKind;
        use cdk::Amount;

        match self {
            Self::OnlineExact => SendKind::OnlineExact,
            Self::OnlineTolerance(t) => SendKind::OnlineTolerance(Amount::from(*t)),
            Self::OfflineExact => SendKind::OfflineExact,
            Self::OfflineTolerance(t) => SendKind::OfflineTolerance(Amount::from(*t)),
        }
    }
}

/// Options for receive operations
///
/// Follows CDK's ReceiveOptions pattern for P2PK/HTLC unlocking.
#[derive(Debug, Clone, Default)]
pub struct ReceiveOptions {
    /// Whether to verify DLEQ proofs before accepting tokens (NUT-12)
    pub verify_dleq: bool,
    /// HTLC preimages for unlocking hash-locked tokens (NUT-14)
    /// Map of hash -> preimage (both as hex strings)
    pub preimages: Vec<String>,
}

impl ReceiveOptions {
    /// Create options with DLEQ verification enabled
    pub fn with_dleq_verification() -> Self {
        Self {
            verify_dleq: true,
            ..Default::default()
        }
    }

    /// Create options with HTLC preimages
    pub fn with_preimages(preimages: Vec<String>) -> Self {
        Self {
            preimages,
            ..Default::default()
        }
    }

    /// Add a preimage for HTLC unlocking
    pub fn add_preimage(mut self, preimage: String) -> Self {
        self.preimages.push(preimage);
        self
    }

    /// Convert to CDK's ReceiveOptions
    pub fn to_cdk_options(&self, p2pk_signing_keys: Vec<cdk::nuts::SecretKey>) -> cdk::wallet::ReceiveOptions {
        cdk::wallet::ReceiveOptions {
            p2pk_signing_keys,
            preimages: self.preimages.clone(),
            ..Default::default()
        }
    }
}

/// Options for send operations
///
/// Follows CDK's SendOptions pattern for configuring send behavior.
#[derive(Debug, Clone, Default)]
pub struct SendOptions {
    /// Send mode (online/offline, exact/tolerance)
    pub mode: SendMode,
    /// Include fee in the sent amount (recipient pays fee)
    pub include_fee: bool,
    /// P2PK recipient pubkey (locks tokens to this key)
    pub p2pk_pubkey: Option<String>,
    /// Optional memo for the token
    pub memo: Option<String>,
    /// Maximum number of proofs to include
    pub max_proofs: Option<usize>,
}

impl SendOptions {
    /// Create options for a simple send
    pub fn simple() -> Self {
        Self::default()
    }

    /// Create options for offline send
    pub fn offline() -> Self {
        Self {
            mode: SendMode::OfflineExact,
            ..Default::default()
        }
    }

    /// Create options for offline send with tolerance
    pub fn offline_with_tolerance(tolerance_sats: u64) -> Self {
        Self {
            mode: SendMode::OfflineTolerance(tolerance_sats),
            ..Default::default()
        }
    }

    /// Create options for P2PK send
    pub fn p2pk(pubkey: String) -> Self {
        Self {
            p2pk_pubkey: Some(pubkey),
            ..Default::default()
        }
    }

    /// Set memo
    pub fn with_memo(mut self, memo: String) -> Self {
        self.memo = Some(memo);
        self
    }

    /// Set include_fee flag
    pub fn with_include_fee(mut self, include: bool) -> Self {
        self.include_fee = include;
        self
    }

    /// Convert to CDK's SendOptions
    pub fn to_cdk_options(&self) -> cdk::wallet::SendOptions {
        use cdk::wallet::SendOptions as CdkSendOptions;
        use cdk::nuts::SpendingConditions;
        use cdk::nuts::PublicKey;

        let conditions = self.p2pk_pubkey.as_ref().and_then(|pk| {
            PublicKey::from_hex(pk).ok().map(|key| {
                SpendingConditions::new_p2pk(key, None)
            })
        });

        CdkSendOptions {
            conditions,
            include_fee: self.include_fee,
            send_kind: self.mode.to_cdk_send_kind(),
            ..Default::default()
        }
    }
}

/// DLEQ proof data (preserves P2PK verification capability)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DleqData {
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

    // Local-only state tracking (not serialized to Nostr events)
    /// Proof state aligned with CDK's State enum
    #[serde(skip)]
    pub state: ProofState,
    /// Transaction ID this proof is associated with (for Reserved/PendingSpent states)
    #[serde(skip)]
    pub transaction_id: Option<u64>,
}

/// Extended Cashu proof with P2PK support (superset of nostr_sdk::nips::nip60::CashuProof)
/// Preserves witness and DLEQ fields for P2PK verification while maintaining NIP-60 compatibility
/// Uses uppercase "C" per NIP-60 spec, with alias for backward compatibility
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtendedCashuProof {
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

// =============================================================================
// Token Event Types (NIP-60)
// =============================================================================

/// Custom deserialization structure for token events (more lenient than rust-nostr)
/// Includes unit field per NIP-60 spec
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenEventData {
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
pub struct ExtendedTokenEvent {
    pub mint: String,
    #[serde(default = "default_unit")]
    pub unit: String,
    pub proofs: Vec<ExtendedCashuProof>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub del: Vec<String>,
}

// =============================================================================
// Wallet State Types
// =============================================================================

/// Wallet state containing configuration
#[derive(Clone, Debug, PartialEq)]
pub struct WalletState {
    /// Wallet private key (None when no wallet exists)
    pub privkey: Option<String>,
    pub mints: Vec<String>,
    pub initialized: bool,
}

/// Sync state for incremental Nostr event fetching (Chorus pattern)
/// Tracks last sync timestamps to avoid fetching all events every time
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SyncState {
    /// Last successful Kind 7375 (token) fetch timestamp
    pub last_token_sync: u64,
    /// Last successful Kind 7376 (history) fetch timestamp
    pub last_history_sync: u64,
    /// Last successful Kind 5 (deletion) fetch timestamp
    pub last_deletion_sync: u64,
    /// Set of known token event IDs for reconciliation
    #[serde(default)]
    pub known_token_event_ids: std::collections::HashSet<String>,
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
    /// Wallet initialized, background recovery/sync in progress
    Recovering,
    Ready,
    Error(String),
}

impl WalletStatus {
    pub fn is_ready(&self) -> bool {
        matches!(self, WalletStatus::Ready)
    }

    pub fn is_recovering(&self) -> bool {
        matches!(self, WalletStatus::Recovering)
    }

    /// Returns true if wallet is usable (Ready or Recovering)
    pub fn is_usable(&self) -> bool {
        matches!(self, WalletStatus::Ready | WalletStatus::Recovering)
    }
}

/// Wallet balance breakdown
#[derive(Clone, Debug, Default, PartialEq)]
pub struct WalletBalances {
    pub total: u64,
    pub available: u64,
    pub pending: u64,
}

// =============================================================================
// Transaction Types
// =============================================================================

/// Transaction lifecycle status (following minibits pattern)
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum TransactionStatus {
    /// Transaction created but not yet prepared
    Draft,
    /// Proofs selected/swapped, ready for execution
    Prepared,
    /// Offline receive prepared
    PreparedOffline,
    /// Transaction submitted, waiting for confirmation
    Pending,
    /// Transaction failed, proofs returned to spendable
    Reverted,
    /// Proofs recovered after failure
    Recovered,
    /// Transaction completed successfully
    Completed,
    /// Transaction failed with error
    Error(String),
    /// Transaction blocked (e.g., P2PK locktime not reached)
    Blocked,
    /// Quote or token expired
    Expired,
}

/// Transaction type for lifecycle tracking
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum TransactionType {
    /// Ecash send
    Send,
    /// Ecash receive
    Receive,
    /// Offline receive
    ReceiveOffline,
    /// Lightning receive (mint)
    Topup,
    /// Lightning send (melt)
    Transfer,
}

/// Transaction data entry for state accumulation (Minibits pattern)
/// Each transaction maintains a full history of state changes
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TransactionDataEntry {
    pub status: TransactionStatus,
    pub created_at: u64,
    pub message: Option<String>,
    pub amount: Option<u64>,
    pub fee_paid: Option<u64>,
}

/// Status update entry for transaction history (legacy)
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TransactionStatusUpdate {
    pub status: TransactionStatus,
    pub timestamp: u64,
    pub message: Option<String>,
    pub fee_paid: Option<u64>,
}

/// Active transaction tracking (local-only, not persisted to Nostr)
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ActiveTransaction {
    pub id: u64,
    pub tx_type: TransactionType,
    pub amount: u64,
    pub unit: String,
    pub mint_url: String,
    pub status: TransactionStatus,
    pub proof_secrets: Vec<String>,
    pub quote_id: Option<String>,
    pub memo: Option<String>,
    pub expires_at: Option<u64>,
    pub created_at: u64,
    pub updated_at: u64,
    pub history: Vec<TransactionStatusUpdate>,
}

// =============================================================================
// Quote Types (CDK-aligned)
// =============================================================================

// Re-export CDK quote state enums for direct use
pub use cdk::nuts::{MintQuoteState, MeltQuoteState};

/// Mint quote information (lightning receive)
/// Wraps CDK's quote response with mint_url context
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MintQuoteInfo {
    pub quote_id: String,
    pub invoice: String,
    pub amount: u64,
    pub expiry: Option<u64>,
    pub mint_url: String,
}

impl MintQuoteInfo {
    /// Create from CDK MintQuote response
    pub fn from_cdk(quote: &cdk::wallet::MintQuote, mint_url: String) -> Self {
        Self {
            quote_id: quote.id.clone(),
            invoice: quote.request.clone(),
            amount: quote.amount.map(u64::from).unwrap_or(0),
            expiry: Some(quote.expiry),
            mint_url,
        }
    }
}

/// Melt quote information (lightning send)
/// Wraps CDK's quote response with mint_url context
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MeltQuoteInfo {
    pub quote_id: String,
    pub invoice: String,
    pub amount: u64,
    pub fee_reserve: u64,
    pub mint_url: String,
    pub expiry: Option<u64>,
}

impl MeltQuoteInfo {
    /// Create from CDK MeltQuote response
    pub fn from_cdk(quote: &cdk::wallet::MeltQuote, mint_url: String) -> Self {
        Self {
            quote_id: quote.id.clone(),
            invoice: quote.request.clone(),
            amount: u64::from(quote.amount),
            fee_reserve: u64::from(quote.fee_reserve),
            mint_url,
            expiry: Some(quote.expiry),
        }
    }
}

// =============================================================================
// Progress Types
// =============================================================================

/// Melt progress tracking
#[derive(Clone, Debug, PartialEq)]
pub enum MeltProgress {
    CreatingQuote,
    QuoteCreated {
        quote_id: String,
        amount: u64,
        fee_reserve: u64,
    },
    PreparingPayment,
    PayingInvoice,
    WaitingForConfirmation,
    Completed {
        total_paid: u64,
        fee_paid: u64,
        preimage: Option<String>,
    },
    Failed {
        error: String,
    },
}

/// Transfer progress tracking
#[derive(Clone, Debug, PartialEq)]
pub enum TransferProgress {
    CreatingMintQuote,
    CreatingMeltQuote,
    QuotesReady {
        amount: u64,
        fee_estimate: u64,
    },
    Melting,
    WaitingForPayment,
    Minting,
    Completed {
        amount_received: u64,
        fees_paid: u64,
    },
    Failed {
        error: String,
    },
}

/// Transfer result
#[derive(Clone, Debug)]
pub struct TransferResult {
    pub amount_sent: u64,
    pub amount_received: u64,
    pub fees_paid: u64,
}

/// Revert result
#[derive(Clone, Debug)]
pub struct RevertResult {
    pub amount_recovered: u64,
    pub fee_paid: u64,
    pub original_amount: u64,
}

/// Sync result
#[derive(Clone, Debug, Default)]
pub struct SyncResult {
    pub spent_found: usize,
    pub pending_found: usize,
    pub proofs_cleaned: usize,
    pub sats_cleaned: u64,
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

// =============================================================================
// Mint Discovery Types
// =============================================================================

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

/// NIP-87: Mint recommendation from kind:38000 events
#[derive(Clone, Debug, PartialEq)]
pub struct MintRecommendation {
    /// Recommender pubkey
    pub recommender: String,
    /// Review/comment content
    pub content: String,
}

// =============================================================================
// Payment Request Types (NUT-18) - DEPRECATED
// Use cdk::nuts::{PaymentRequest, PaymentRequestPayload, Transport} instead
// =============================================================================

/// Payment request data for creating requests
/// DEPRECATED: Use cdk::nuts::PaymentRequest instead
#[deprecated(note = "Use cdk::nuts::PaymentRequest instead")]
#[allow(deprecated)] // Using deprecated PaymentTransport within deprecated struct
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
/// DEPRECATED: Use cdk::nuts::Transport instead
#[deprecated(note = "Use cdk::nuts::Transport instead")]
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

/// Payment request progress
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

/// Payment request payload (what's sent via transport)
/// DEPRECATED: Use cdk::nuts::PaymentRequestPayload instead
#[deprecated(note = "Use cdk::nuts::PaymentRequestPayload instead")]
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

// =============================================================================
// Store Types (Dioxus)
// =============================================================================

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

/// Store for pending mint quotes
#[derive(Clone, Debug, Default, Store)]
pub struct PendingMintQuotesStore {
    pub data: Vec<MintQuoteInfo>,
}

/// Store for pending melt quotes
#[derive(Clone, Debug, Default, Store)]
pub struct PendingMeltQuotesStore {
    pub data: Vec<MeltQuoteInfo>,
}

// =============================================================================
// Pending Event Types
// =============================================================================

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
    pub builder_json: String,
    pub event_type: PendingEventType,
    pub created_at: u64,
    pub retry_count: u32,
}

// =============================================================================
// Counter Backup Types (Minibits pattern)
// =============================================================================

/// Counter backup for mint removal/re-addition
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CounterBackup {
    pub mint_url: String,
    pub counters: Vec<(String, u64)>, // keyset_id -> counter
    pub created_at: u64,
}

// =============================================================================
// Recovery Types
// =============================================================================

/// Recovery result
#[derive(Clone, Debug, Default)]
pub struct RecoveryResult {
    pub recovered_amount: u64,
    pub message: Option<String>,
}

impl RecoveryResult {
    pub fn none() -> Self {
        Self::default()
    }

    pub fn recovered(amount: u64) -> Self {
        Self {
            recovered_amount: amount,
            message: Some(format!("Recovered {} sats", amount)),
        }
    }
}
