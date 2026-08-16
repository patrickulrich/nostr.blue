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
pub mod address;
pub mod auth;
pub mod auth_cache;
pub mod cache;
pub mod capabilities;
pub mod denomination;
pub mod dust;
pub mod enriched_history;
pub mod errors;
pub mod events;
pub mod fees;
pub mod history;
pub mod init;
pub mod keyset;
pub mod lightning;
pub mod mint_mgmt;
pub mod mpp;
pub mod nutzap;
pub mod nutzap_signals;
pub mod offline_receive;
pub mod pagination;
pub mod payment_request;
pub mod proof_recovery;
pub mod proofs;
pub mod quotes;
pub mod receive;
pub mod recovery;
pub mod send;
pub mod signals;
pub mod spending_conditions;
pub mod swap;
pub mod token;
pub mod transfer;
pub mod types;
pub mod ws;
#[allow(unused_imports)]
pub use self::ws::{
    mint_supports_websocket, poll_proof_states, subscribe_to_proof_states,
    ProofState as MintProofState, ProofStateNotification,
};
#[allow(unused_imports)]
pub use auth::{
    add_auth_header, add_auth_header_for, blind_auth_token_count, check_operation_auth,
    clear_mint_auth_state, discover_mint_auth, ensure_auth_available, get_auth_for_endpoint,
    get_blind_auth_for_request, get_mint_auth_state, has_blind_auth_tokens, is_auth_required_error,
    is_protected_mint, mint_requires_auth, parse_auth_from_mint_info, set_mint_auth_state,
    AuthProof, AuthRequired, AuthToken, BlindAuthSettings, BlindAuthToken, ClearAuthSettings,
    HttpMethod, MintAuthState, ProtectedEndpoint, RoutePath, MINT_AUTH_STATES,
};
#[allow(unused_imports)]
pub use auth_cache::{
    cache_tokens, cached_token_count, cleanup_auth_cache, clear_mint_tokens, get_auth_cache_stats,
    get_cached_token, has_cached_tokens, needs_token_replenishment, AuthCacheStats, BlindAuthCache,
    CachedAuthToken,
};
#[allow(unused_imports)]
pub use capabilities::{
    check_melt_limits, check_mint_limits, check_operation_supported, get_mint_capabilities,
    supports_mpp, supports_p2pk, supports_restore, MintCapabilities, Nut, OperationKind,
};
#[allow(unused_imports)]
pub use denomination::{
    estimate_proof_count, DenominationStrategy, OperationType as DenomOperationType,
    CONSOLIDATION_THRESHOLD,
};
#[allow(unused_imports)]
pub use dust::{
    consolidate_all_dust, consolidate_dust, find_dust_proofs, get_all_dust_stats, get_dust_stats,
    get_total_dust_stats, should_consolidate_dust, DustConsolidationResult, DustStats,
    DEFAULT_DUST_THRESHOLD,
};
#[allow(unused_imports)]
pub use enriched_history::{
    create_lightning_receive_history, create_lightning_send_history, create_p2pk_send_history,
    create_swap_history, enrich_history_item, Direction as HistoryDirection, EnrichedHistoryItem,
    KeysetInfo, SwapDetails, SwapReason, TransactionType as HistoryTransactionType,
};
#[allow(unused_imports)]
pub use events::queue_event_for_retry;
#[allow(unused_imports)]
pub use fees::{
    calculate_proof_fee, compare_mint_fees, estimate_htlc_fee, estimate_multisig_fee,
    estimate_p2pk_receive_fee, estimate_p2pk_send_fee, estimate_simple_send_fee, estimate_swap_fee,
    find_cheapest_mint, get_mint_fee_ppk, FeeEstimate, MintFeeSummary, P2pkComplexity,
};
pub use init::{accept_terms, check_terms_accepted, create_wallet, init_wallet};
#[allow(unused_imports)]
pub use keyset::{
    get_active_keyset_ids, get_migration_recommendation, migrate_inactive_proofs, refresh_keysets,
    should_migrate, KeysetMigrationResult, KeysetRefreshResult,
};
#[allow(unused_imports)]
pub use lightning::create_history_event_with_type;
#[allow(unused_imports)]
pub use lightning::{
    check_mint_quote_status, create_melt_quote, create_mint_quote, melt_tokens,
    mint_tokens_from_quote, preview_melt_fees, MeltFeePreview,
};
#[allow(unused_imports)]
pub use mint_mgmt::{
    add_mint, consolidate_all_mints, discover_mints, get_mint_balance, get_mint_info,
    get_mint_proof_count, get_mint_spendable_balance, get_mint_unit_spendable_balance, get_mints,
    get_total_proof_count, remove_mint,
};
#[allow(unused_imports)]
pub use mint_mgmt::{check_keyset_collision, KeysetCollision};
pub use mpp::{
    calculate_mpp_split, create_mpp_melt_quotes, execute_mpp_melt, get_balances_per_mint,
    mint_supports_mpp, MppQuoteInfo,
};
#[allow(unused_imports)]
pub use nutzap::{
    fetch_nutzap_info, fetch_pending_nutzaps, get_nutzap_p2pk_pubkey, process_nutzap_event,
    publish_nutzap_info, redeem_nutzap, restore_nutzap_state, send_nutzap,
    start_nutzap_subscription, validate_nutzap_recipient, validate_nutzap_recipient_with_info,
    NutzapInfo, NutzapMint, NutzapRedeemResult, NutzapSendResult, NutzapStatus, PendingNutzap,
};
#[allow(unused_imports)]
pub use nutzap_signals::{
    add_pending_nutzap, cache_nutzap_info, clear_nutzap_state, get_cached_nutzap_info,
    pending_nutzap_count, pending_nutzap_value, remove_pending_nutzap,
    update_pending_nutzap_status, MY_NUTZAP_INFO, NUTZAP_AUTO_REDEEM, NUTZAP_ENABLED,
    NUTZAP_INFO_CACHE, NUTZAP_SUBSCRIPTION_ACTIVE, PENDING_NUTZAPS,
};
#[allow(unused_imports)]
pub use pagination::{
    batch_proofs, batch_proofs_adaptive, batch_proofs_for_mint, fetch_mint_limits, get_batch_size,
    get_optimal_batch_size, MintLimits, ProofPaginator,
};
pub use payment_request::{
    cancel_payment_request, create_payment_request, parse_payment_request, pay_payment_request,
    wait_for_nostr_payment, PAYMENT_CANCELLED_MSG,
};
#[allow(unused_imports)]
pub use proof_recovery::{
    detect_stuck_proofs, find_pending_spent_proofs, find_reserved_proofs, get_recovery_stats,
    recover_pending_spent_proofs, recover_reserved_proofs, run_full_recovery, ProofRecoveryResult,
};
#[allow(unused_imports)]
pub use proofs::cdk_proof_to_proof_data;
#[allow(unused_imports)]
pub use quotes::{
    check_quote_validity, cleanup_all_expired_quotes, cleanup_expired_melt_quotes,
    cleanup_expired_mint_quotes, find_melt_quote, find_mint_quote, format_expiry, get_quote_stats,
    is_quote_expired, QuoteStats, QuoteValidity,
};
#[allow(unused_imports)]
pub use receive::{
    preview_token, receive_tokens, receive_tokens_with_options, P2PKLockInfo, ReceiveTokensOptions, TokenPreview,
};
#[allow(unused_imports)]
pub use recovery::{
    check_cdk_orphan_proofs, cleanup_spent_proofs, mint_all_pending_quotes, recover_cdk_sagas,
    refresh_wallet, CdkSagaRecoveryResult,
};
pub use send::{estimate_send_fee, get_wallet_pubkey, send_tokens, send_tokens_p2pk};
pub use send::{extract_y_values_from_token, watch_sent_token_claims};
pub use signals::*;
#[allow(unused_imports)]
pub use spending_conditions::{
    build_sig_all_message_for_melt, build_sig_all_message_for_swap, create_multisig_sig_all,
    create_p2pk_sig_all, sign_sig_all_message, verify_sig_all_signatures, ExtendedConditions,
    SigFlag,
};
#[allow(unused_imports)]
pub use swap::{execute_swap, swap_optimize_denominations, swap_refresh, SwapOptions, SwapResult};
pub use transfer::{estimate_transfer_fees, transfer_between_mints};
pub use types::*;
pub(crate) mod internal;
mod utils;
pub use utils::{mint_matches, normalize_mint_url};
