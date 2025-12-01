//! Cashu NIP-60 wallet implementation
//!
//! This module implements a Cashu ecash wallet with NIP-60 Nostr integration.
//! It provides:
//! - Multi-mint wallet management via CDK
//! - NIP-60 token/history event publishing
//! - Lightning integration (mint/melt quotes)
//! - P2PK support with DLEQ verification
//! - Offline event queue with retry
//! - Dual persistence: IndexedDB cache + NIP-60 source of truth
//! - Keyset migration and rotation detection
//! - Quote lifecycle management
//! - Adaptive proof pagination
//! - Fee estimation including P2PK overhead
//! - Dust consolidation

pub mod types;
pub mod errors;
pub mod signals;
pub mod proofs;
pub mod events;
pub mod init;
pub mod send;
pub mod receive;
pub mod lightning;
pub mod mpp;
pub mod mint_mgmt;
pub mod history;
pub mod recovery;
pub mod transfer;
pub mod payment_request;
pub mod auth;
pub mod auth_cache;
pub mod cache;
pub mod token;
pub mod address;
pub mod keyset;
pub mod spending_conditions;
pub mod denomination;
pub mod swap;
pub mod capabilities;
pub mod quotes;
pub mod proof_recovery;
pub mod fees;
pub mod pagination;
pub mod dust;
pub mod enriched_history;
pub mod ws;

// Re-export commonly used types
pub use types::*;
// Error types available internally via super::errors
// pub use errors::{CashuWalletError, CashuResult};
pub use signals::*;


// Re-export main public functions
pub use init::{
    init_wallet,
    create_wallet,
    check_terms_accepted,
    accept_terms,
};
pub use send::{
    send_tokens, send_tokens_p2pk, get_wallet_pubkey, estimate_send_fee,
};
pub use send::{watch_sent_token_claims, extract_y_values_from_token};
#[allow(unused_imports)] // receive_tokens is simpler API for future use
pub use receive::{receive_tokens, receive_tokens_with_options, ReceiveTokensOptions};
pub use lightning::{
    create_mint_quote,
    check_mint_quote_status,
    mint_tokens_from_quote,
    create_melt_quote,
    melt_tokens,
};
pub use mpp::{
    get_balances_per_mint,
    calculate_mpp_split,
    create_mpp_melt_quotes,
    execute_mpp_melt,
    mint_supports_mpp,
    MppQuoteInfo,
};
pub use mint_mgmt::{
    add_mint,
    remove_mint,
    get_mints,
    get_mint_balance,
    get_mint_info,
    get_mint_proof_count,
    get_total_proof_count,
    discover_mints,
    consolidate_all_mints,
};
#[allow(unused_imports)]
pub use mint_mgmt::{check_keyset_collision, KeysetCollision};
pub use recovery::{
    cleanup_spent_proofs,
    refresh_wallet,
};
pub use transfer::{transfer_between_mints, estimate_transfer_fees};
pub use payment_request::{
    create_payment_request,
    parse_payment_request,
    pay_payment_request,
    cancel_payment_request,
    wait_for_nostr_payment,
};
// Internal helpers used by cashu_cdk_bridge and other modules
#[allow(unused_imports)]
pub use proofs::cdk_proof_to_proof_data;
#[allow(unused_imports)]
pub use events::queue_event_for_retry;
#[allow(unused_imports)]
pub use lightning::create_history_event_with_type;
// =============================================================================
// NUT-17 WebSocket Subscriptions (Real-time state sync)
// =============================================================================

// NUT-17 support detection and proof state functions
// Note: ProofState renamed to MintProofState to avoid conflict with types::ProofState
#[allow(unused_imports)]
pub use self::ws::{
    mint_supports_websocket,
    subscribe_to_proof_states,
    poll_proof_states,
    ProofState as MintProofState,
    ProofStateNotification,
};

// =============================================================================
// Advanced CDK features (planned, not yet wired to UI)
// =============================================================================

// Auth types for protected mints (NUT-21/22)
// Core types are re-exported from CDK
#[allow(unused_imports)]
pub use auth::{
    // CDK types
    AuthRequired, AuthToken, BlindAuthToken, AuthProof,
    HttpMethod, RoutePath, ProtectedEndpoint,
    ClearAuthSettings, BlindAuthSettings,
    // Local state management
    MintAuthState, MINT_AUTH_STATES,
    get_mint_auth_state, set_mint_auth_state, mint_requires_auth, clear_mint_auth_state,
    // Parsing
    parse_auth_from_mint_info, is_protected_mint,
    // Header helpers
    add_auth_header, add_auth_header_for, is_auth_required_error,
    // Token management
    get_blind_auth_for_request, has_blind_auth_tokens, blind_auth_token_count,
    get_auth_for_endpoint,
    // Discovery and validation
    discover_mint_auth, check_operation_auth, ensure_auth_available,
};
// Keyset migration (NUT-13 compliant)
#[allow(unused_imports)]
pub use keyset::{
    refresh_keysets, migrate_inactive_proofs, should_migrate,
    get_migration_recommendation, get_active_keyset_ids,
    KeysetRefreshResult, KeysetMigrationResult,
};
// Spending conditions with SIG_ALL (NUT-10/11)
#[allow(unused_imports)]
pub use spending_conditions::{
    SigFlag, ExtendedConditions,
    build_sig_all_message_for_swap, build_sig_all_message_for_melt,
    sign_sig_all_message, verify_sig_all_signatures,
    create_p2pk_sig_all, create_multisig_sig_all,
};
// Denomination strategies
#[allow(unused_imports)]
pub use denomination::{
    DenominationStrategy, OperationType as DenomOperationType,
    estimate_proof_count, CONSOLIDATION_THRESHOLD,
};
// Direct swap operations
#[allow(unused_imports)]
pub use swap::{
    SwapOptions, SwapResult,
    execute_swap, swap_optimize_denominations, swap_refresh,
};
// Mint capabilities
#[allow(unused_imports)]
pub use capabilities::{
    Nut, MintCapabilities, OperationKind,
    get_mint_capabilities, check_operation_supported,
    supports_p2pk, supports_mpp, supports_restore,
    check_mint_limits, check_melt_limits,
};
// Quote management
#[allow(unused_imports)]
pub use quotes::{
    QuoteValidity, QuoteStats,
    is_quote_expired, check_quote_validity,
    format_expiry, get_quote_stats,
    cleanup_all_expired_quotes, cleanup_expired_mint_quotes, cleanup_expired_melt_quotes,
    find_mint_quote, find_melt_quote,
};
// Proof state recovery
#[allow(unused_imports)]
pub use proof_recovery::{
    ProofRecoveryResult,
    detect_stuck_proofs, find_reserved_proofs, find_pending_spent_proofs,
    recover_reserved_proofs, recover_pending_spent_proofs, run_full_recovery,
    get_recovery_stats,
};
// Fee estimation
#[allow(unused_imports)]
pub use fees::{
    FeeEstimate, P2pkComplexity, MintFeeSummary,
    get_mint_fee_ppk, calculate_proof_fee,
    estimate_simple_send_fee, estimate_p2pk_send_fee,
    estimate_multisig_fee, estimate_htlc_fee,
    estimate_p2pk_receive_fee, estimate_swap_fee,
    compare_mint_fees, find_cheapest_mint,
};
// Adaptive pagination
#[allow(unused_imports)]
pub use pagination::{
    MintLimits, ProofPaginator,
    fetch_mint_limits, get_batch_size, get_optimal_batch_size,
    batch_proofs, batch_proofs_for_mint, batch_proofs_adaptive,
};
// Dust consolidation
#[allow(unused_imports)]
pub use dust::{
    DustStats, DustConsolidationResult,
    find_dust_proofs, get_dust_stats, get_all_dust_stats,
    consolidate_dust, consolidate_all_dust,
    should_consolidate_dust, get_total_dust_stats,
    DEFAULT_DUST_THRESHOLD,
};
// Enriched history
#[allow(unused_imports)]
pub use enriched_history::{
    EnrichedHistoryItem, TransactionType as HistoryTransactionType,
    Direction as HistoryDirection, KeysetInfo, SwapDetails, SwapReason,
    enrich_history_item,
    create_lightning_receive_history, create_lightning_send_history,
    create_p2pk_send_history, create_swap_history,
};
// Auth token caching
#[allow(unused_imports)]
pub use auth_cache::{
    CachedAuthToken, BlindAuthCache, AuthCacheStats,
    cache_tokens, get_cached_token, has_cached_tokens,
    cached_token_count, needs_token_replenishment,
    cleanup_auth_cache, clear_mint_tokens, get_auth_cache_stats,
};

// Internal helpers (shared by submodules, not exported)
pub(crate) mod internal;

// Utility functions
mod utils;
pub use utils::normalize_mint_url;
