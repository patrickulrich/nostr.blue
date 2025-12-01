//! Internal helper functions for cashu wallet operations
//!
//! These are shared by multiple cashu submodules but not exported publicly.
//! The module provides:
//! - Database access (shared localstore)
//! - Wallet creation and caching
//! - Seed derivation
//! - Error detection helpers
//! - Proof validation
//! - P2PK key collection

// Allow dead_code for planned features not yet wired to UI
#![allow(dead_code)]

use std::sync::Arc;

use dioxus::prelude::*;
use cdk::nuts::{CurrencyUnit, State};
use cdk::Wallet;
use cdk_common::database::WalletDatabase;

use super::proofs::proof_data_to_cdk_proof;
use super::signals::{SHARED_LOCALSTORE, WALLET_STATE, WALLET_TOKENS};
use super::types::{ProofData, WalletTokensStoreStoreExt};
use super::utils::mint_matches;
use crate::stores::{auth_store, cashu_cdk_bridge};

// =============================================================================
// Database Access
// =============================================================================

/// Get or create the shared IndexedDB localstore
pub(crate) async fn get_shared_localstore(
) -> Result<Arc<crate::stores::indexeddb_database::IndexedDbDatabase>, String> {
    // Check if we already have a cached localstore
    if let Some(store) = SHARED_LOCALSTORE.read().clone() {
        return Ok(store);
    }

    // Create new localstore
    let localstore = Arc::new(
        crate::stores::indexeddb_database::IndexedDbDatabase::new()
            .await
            .map_err(|e| format!("Failed to create IndexedDB: {}", e))?,
    );

    // Cache it
    *SHARED_LOCALSTORE.write() = Some(localstore.clone());
    log::info!("Created shared IndexedDB localstore");

    Ok(localstore)
}

// =============================================================================
// Wallet Creation and Caching
// =============================================================================

/// Get or create a wallet for a mint via MultiMintWallet
///
/// Uses CDK's MultiMintWallet for all wallet management.
/// If the mint isn't already in the wallet, it will be added.
pub(crate) async fn get_or_create_wallet(mint_url: &str) -> Result<Arc<Wallet>, String> {
    // Require MultiMintWallet to be initialized
    if !cashu_cdk_bridge::is_initialized() {
        return Err("Wallet not initialized. Please set up wallet first.".to_string());
    }

    // Try to get existing wallet
    if let Ok(wallet) = cashu_cdk_bridge::get_wallet(mint_url).await {
        log::debug!("Using wallet from MultiMintWallet for {}", mint_url);
        return Ok(Arc::new(wallet));
    }

    // Mint not in wallet - add it
    log::info!("Adding mint {} to MultiMintWallet", mint_url);
    cashu_cdk_bridge::add_mint(mint_url).await?;

    // Get the newly added wallet
    cashu_cdk_bridge::get_wallet(mint_url)
        .await
        .map(Arc::new)
        .map_err(|e| format!("Failed to get wallet after adding mint: {}", e))
}

/// Create an ephemeral wallet and optionally inject proofs
///
/// This function bridges NIP-60 (proofs stored in Nostr events) with CDK
/// (proofs stored in its own database). When performing operations, proofs
/// from NIP-60 events need to be in the CDK database.
///
/// Architecture Note: This pattern is necessary because:
/// - NIP-60 treats Nostr events as the source of truth for proofs
/// - CDK expects to manage proofs in its internal database
/// - For operations like send/melt, we inject NIP-60 proofs into CDK's IndexedDB
///
/// Uses the cached wallet system for performance. Proofs are injected into
/// the shared IndexedDB store which all wallets share.
pub(crate) async fn create_ephemeral_wallet(
    mint_url: &str,
    proofs: Vec<cdk::nuts::Proof>,
) -> Result<Arc<Wallet>, String> {
    use cdk::mint_url::MintUrl as CdkMintUrl;
    use cdk::types::ProofInfo;

    // Get or create cached wallet
    let wallet = get_or_create_wallet(mint_url).await?;

    // Inject proofs if any provided
    if !proofs.is_empty() {
        // CDK best practice: Validate proof format before injection
        // to prevent malformed proofs from causing mid-operation failures
        for (idx, proof) in proofs.iter().enumerate() {
            validate_proof_format(proof).map_err(|e| {
                format!("Invalid proof at index {}: {}", idx, e)
            })?;
        }

        let mint_url_parsed: CdkMintUrl = mint_url
            .parse()
            .map_err(|e| format!("Invalid mint URL: {}", e))?;

        let proof_infos: Vec<_> = proofs
            .into_iter()
            .map(|p| ProofInfo::new(p, mint_url_parsed.clone(), State::Unspent, CurrencyUnit::Sat))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("Failed to create proof info: {}", e))?;

        // Inject proofs via shared localstore
        let localstore = get_shared_localstore().await?;
        localstore
            .update_proofs(proof_infos, vec![])
            .await
            .map_err(|e| format!("Failed to inject proofs: {}", e))?;
    }

    Ok(wallet)
}

// =============================================================================
// Seed Derivation
// =============================================================================

/// Derive deterministic wallet seed from Nostr private key or signer
#[cfg(target_arch = "wasm32")]
pub(crate) async fn derive_wallet_seed() -> Result<[u8; 64], String> {
    use sha2::{Digest, Sha256};

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
    let sig_bytes = challenge_event.sig.serialize();

    let mut seed = [0u8; 64];
    seed.copy_from_slice(&sig_bytes);

    log::info!("Wallet seed derived from NIP-07 signature");
    Ok(seed)
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) async fn derive_wallet_seed() -> Result<[u8; 64], String> {
    Err("Seed derivation only available in WASM".to_string())
}

// =============================================================================
// Error Detection Helpers
// =============================================================================

/// Check if an error message indicates tokens are already spent
pub(crate) fn is_token_spent_error_string(error_msg: &str) -> bool {
    let msg = error_msg.to_lowercase();
    msg.contains("already spent")
        || msg.contains("already redeemed")
        || msg.contains("token pending")
}

/// Check if an error message indicates insufficient funds
pub(crate) fn is_insufficient_funds_error_string(error_msg: &str) -> bool {
    error_msg.to_lowercase().contains("insufficient")
}

/// Check if a CDK error indicates tokens are already spent
pub(crate) fn is_token_already_spent_error(error: &cdk::Error) -> bool {
    match error {
        cdk::Error::TokenAlreadySpent => true,
        _ => is_token_spent_error_string(&error.to_string()),
    }
}

/// Check if a CDK error indicates insufficient funds
pub(crate) fn is_insufficient_funds_error(error: &cdk::Error) -> bool {
    match error {
        cdk::Error::InsufficientFunds => true,
        _ => is_insufficient_funds_error_string(&error.to_string()),
    }
}

// =============================================================================
// Database Cleanup Helpers
// =============================================================================

/// Remove a melt quote from the database
pub(crate) async fn remove_melt_quote_from_db(quote_id: &str) -> Result<(), String> {
    use cdk_common::database::WalletDatabase;

    let localstore = get_shared_localstore().await?;

    localstore
        .remove_melt_quote(quote_id)
        .await
        .map_err(|e| format!("Failed to remove melt quote: {}", e))?;

    log::info!("Removed melt quote {} from database", quote_id);
    Ok(())
}

// =============================================================================
// Proof Validation
// =============================================================================

/// Validate proof format before injection into CDK
///
/// CDK best practice: Validate proof format before injection to prevent
/// malformed proofs from causing failures during multi-step operations
/// (which could leave proofs in Reserved state).
fn validate_proof_format(proof: &cdk::nuts::Proof) -> Result<(), String> {
    // Validate keyset_id format (8 bytes = 16 hex chars)
    let keyset_str = proof.keyset_id.to_string();
    if keyset_str.len() != 16 {
        return Err(format!(
            "Invalid keyset_id length: expected 16, got {}",
            keyset_str.len()
        ));
    }

    // Validate secret is not empty
    let secret_str = proof.secret.to_string();
    if secret_str.is_empty() {
        return Err("Empty proof secret".to_string());
    }

    // Validate C point format (33-byte compressed pubkey = 66 hex chars)
    let c_hex = proof.c.to_hex();
    if c_hex.len() != 66 {
        return Err(format!(
            "Invalid C point length: expected 66, got {}",
            c_hex.len()
        ));
    }

    // Validate amount is non-zero
    if u64::from(proof.amount) == 0 {
        return Err("Proof amount is zero".to_string());
    }

    Ok(())
}

/// Validate proofs with the mint before sending (NUT-07)
///
/// Checks proof states with the mint to detect stale proofs that
/// were spent elsewhere. Cleans up any spent proofs from local storage.
pub(crate) async fn validate_proofs_with_mint(
    mint_url: &str,
    proofs: Vec<cdk::nuts::Proof>,
) -> Result<Vec<cdk::nuts::Proof>, String> {
    if proofs.is_empty() {
        return Ok(proofs);
    }

    log::debug!(
        "Validating {} proofs with mint: {}",
        proofs.len(),
        mint_url
    );

    // Create wallet with proofs to check
    let wallet = create_ephemeral_wallet(mint_url, proofs.clone()).await?;

    // NUT-07: Check proof states with mint
    let states = wallet
        .check_proofs_spent(proofs.clone().into())
        .await
        .map_err(|e| format!("Failed to check proof states: {}", e))?;

    // Count spent proofs
    let spent_count = states.iter().filter(|s| s.state == State::Spent).count();

    if spent_count > 0 {
        log::warn!(
            "Found {} spent proofs in wallet, cleaning up...",
            spent_count
        );

        // Clean up spent proofs from our local store
        cleanup_spent_proofs_internal(mint_url).await?;

        // Re-fetch fresh proofs after cleanup
        let fresh_proofs = {
            let store = WALLET_TOKENS.read();
            let data = store.data();
            let tokens = data.read();
            let mut proofs = Vec::new();

            for token in tokens.iter().filter(|t| mint_matches(&t.mint, mint_url)) {
                for proof in &token.proofs {
                    proofs.push(proof_data_to_cdk_proof(proof)?);
                }
            }
            proofs
        };

        log::info!(
            "After cleanup: {} proofs remaining for mint {}",
            fresh_proofs.len(),
            mint_url
        );
        return Ok(fresh_proofs);
    }

    log::debug!("All {} proofs validated as unspent", proofs.len());
    Ok(proofs)
}

/// Internal cleanup function for spent proofs
///
/// Checks proof states with mint and removes spent/reserved/pending proofs.
/// Returns (count_cleaned, sats_cleaned).
pub(crate) async fn cleanup_spent_proofs_internal(mint_url: &str) -> Result<(usize, u64), String> {
    use nostr_sdk::signer::NostrSigner;
    use nostr_sdk::{EventId, Kind, PublicKey};

    log::info!("Checking for spent proofs on {}", mint_url);

    // Get all token events and proofs for this mint
    let (cdk_proofs, event_ids_to_delete, all_mint_proofs) = {
        let store = WALLET_TOKENS.read();
        let data = store.data();
        let tokens = data.read();
        let mint_tokens: Vec<_> = tokens.iter().filter(|t| mint_matches(&t.mint, mint_url)).collect();

        if mint_tokens.is_empty() {
            log::info!("No proofs to check");
            return Ok((0, 0));
        }

        let event_ids: Vec<String> = mint_tokens.iter().map(|t| t.event_id.clone()).collect();

        let all_proofs: Vec<ProofData> = mint_tokens
            .iter()
            .flat_map(|t| &t.proofs)
            .cloned()
            .collect();

        let cdk_proofs: Result<Vec<_>, _> =
            all_proofs.iter().map(|p| proof_data_to_cdk_proof(p)).collect();

        (cdk_proofs?, event_ids, all_proofs)
    };

    // Create ephemeral wallet
    let wallet = create_ephemeral_wallet(mint_url, vec![]).await?;

    // Check states at mint
    let states = wallet
        .check_proofs_spent(cdk_proofs.clone())
        .await
        .map_err(|e| format!("Failed to check proof states: {}", e))?;

    // Find unavailable proofs (spent, reserved, or pending)
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
    log::info!(
        "Found {} unavailable proofs worth {} sats, cleaning up",
        unavailable_count,
        unavailable_amount
    );

    // Filter to keep only available proofs
    let available_proofs: Vec<ProofData> = all_mint_proofs
        .into_iter()
        .filter(|p| !unavailable_secrets.contains(&p.secret))
        .collect();

    // Get signer and pubkey for creating events
    let signer = crate::stores::signer::get_signer()
        .ok_or("No signer available")?
        .as_nostr_signer();

    let pubkey_str = auth_store::get_pubkey().ok_or("Not authenticated")?;
    let pubkey =
        PublicKey::parse(&pubkey_str).map_err(|e| format!("Invalid pubkey: {}", e))?;

    let client = crate::stores::nostr_client::NOSTR_CLIENT
        .read()
        .as_ref()
        .ok_or("Client not initialized")?
        .clone();

    // Publish new token event with remaining proofs (if any)
    let mut new_event_id: Option<String> = None;

    if !available_proofs.is_empty() {
        use super::types::ExtendedCashuProof;
        use super::types::ExtendedTokenEvent;

        let extended_proofs: Vec<ExtendedCashuProof> = available_proofs
            .iter()
            .map(|p| ExtendedCashuProof::from(p.clone()))
            .collect();

        let token_event_data = ExtendedTokenEvent {
            mint: mint_url.to_string(),
            unit: "sat".to_string(),
            proofs: extended_proofs,
            del: event_ids_to_delete.clone(),
        };

        let json_content = serde_json::to_string(&token_event_data)
            .map_err(|e| format!("Failed to serialize token event: {}", e))?;

        let encrypted = signer
            .nip44_encrypt(&pubkey, &json_content)
            .await
            .map_err(|e| format!("Failed to encrypt token event: {}", e))?;

        let builder = nostr_sdk::EventBuilder::new(Kind::CashuWalletUnspentProof, encrypted);

        match client.send_event_builder(builder).await {
            Ok(event_output) => {
                new_event_id = Some(event_output.id().to_hex());
                log::info!("Published cleanup token event: {}", new_event_id.as_ref().unwrap());
            }
            Err(e) => {
                log::warn!("Failed to publish cleanup token event: {}", e);
            }
        }
    }

    // Publish deletion event for old token events
    // NIP-60 best practice: Queue deletion for retry if publish fails to prevent
    // orphaned tokens on other devices that sync from relays
    if !event_ids_to_delete.is_empty() {
        let valid_event_ids: Vec<_> = event_ids_to_delete
            .iter()
            .filter(|id| EventId::from_hex(id).is_ok())
            .collect();

        if !valid_event_ids.is_empty() {
            let mut tags = Vec::new();
            for event_id in &valid_event_ids {
                tags.push(nostr_sdk::Tag::event(EventId::from_hex(event_id).unwrap()));
            }
            tags.push(nostr_sdk::Tag::custom(
                nostr_sdk::TagKind::custom("k"),
                ["7375"],
            ));

            let deletion_builder =
                nostr_sdk::EventBuilder::new(Kind::from(5), "Spent proofs cleanup").tags(tags);

            match client.send_event_builder(deletion_builder.clone()).await {
                Ok(_) => {
                    log::info!("Published deletion event for {} token events", valid_event_ids.len());
                }
                Err(e) => {
                    log::warn!("Failed to publish deletion event, queuing for retry: {}", e);
                    // Queue for retry to ensure deletion eventually propagates
                    super::events::queue_event_for_retry(
                        deletion_builder,
                        super::types::PendingEventType::DeletionEvent,
                    ).await;
                }
            }
        }
    }

    // Update local state
    {
        let store = WALLET_TOKENS.read();
        let mut data = store.data();
        let mut tokens_write = data.write();

        // Remove old token events
        tokens_write.retain(|t| !event_ids_to_delete.contains(&t.event_id));

        // Add new token with remaining proofs
        if let Some(ref event_id) = new_event_id {
            use super::types::TokenData;

            tokens_write.push(TokenData {
                event_id: event_id.clone(),
                mint: mint_url.to_string(),
                unit: "sat".to_string(),
                proofs: available_proofs,
                created_at: chrono::Utc::now().timestamp() as u64,
            });
        }

        // Update balance with overflow protection
        let new_balance: u64 = tokens_write
            .iter()
            .flat_map(|t| &t.proofs)
            .map(|p| p.amount)
            .fold(0u64, |acc, amt| acc.saturating_add(amt));

        *super::signals::WALLET_BALANCE.write() = new_balance;
    }

    log::info!(
        "Cleanup complete: removed {} proofs worth {} sats",
        unavailable_count,
        unavailable_amount
    );

    Ok((unavailable_count, unavailable_amount))
}

// =============================================================================
// P2PK Key Collection
// =============================================================================

/// Collect all P2PK signing keys that can be used to unlock tokens
///
/// Includes the wallet's private key and the Nostr identity key.
pub(crate) async fn collect_p2pk_signing_keys() -> Vec<cdk::nuts::SecretKey> {
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
    if let Some(nostr_keys) = auth_store::get_keys() {
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

    // Remove duplicates
    let mut seen_keys = std::collections::HashSet::new();
    keys.retain(|key| {
        let key_hex = key.to_string();
        seen_keys.insert(key_hex)
    });

    log::info!(
        "Collected {} unique P2PK signing keys for token receive",
        keys.len()
    );
    keys
}

/// Convert a Nostr public key to a CDK public key for P2PK spending conditions
///
/// Nostr uses 32-byte x-only public keys, while CDK/Cashu uses 33-byte compressed
/// secp256k1 public keys. This handles the conversion by assuming even Y parity.
pub(crate) fn nostr_pubkey_to_cdk_pubkey(nostr_pubkey: &str) -> Result<cdk::nuts::PublicKey, String> {
    // Parse the Nostr pubkey (supports npub, hex, NIP-21)
    let parsed = nostr_sdk::PublicKey::parse(nostr_pubkey)
        .map_err(|e| format!("Invalid Nostr pubkey: {}", e))?;

    // Get the 32-byte hex (x-only format)
    let x_only_hex = parsed.to_hex();

    // Convert to 33-byte compressed format by prepending 02 (even Y parity)
    let compressed_hex = format!("02{}", x_only_hex);

    // Parse as CDK PublicKey
    cdk::nuts::PublicKey::from_hex(&compressed_hex)
        .map_err(|e| format!("Failed to create CDK pubkey: {}", e))
}

// =============================================================================
// MultiMintWallet Initialization
// =============================================================================

/// Initialize the MultiMintWallet with all mints from the wallet event
pub(crate) async fn init_multi_mint_wallet(mints: &[nostr_sdk::Url]) -> Result<(), String> {
    log::info!("Initializing MultiMintWallet with {} mints", mints.len());

    // Get shared localstore
    let localstore = get_shared_localstore().await?;

    // Derive wallet seed
    let seed = derive_wallet_seed().await?;

    // Initialize MultiMintWallet
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

    log::info!(
        "MultiMintWallet initialized with {} wallets",
        multi_wallet.get_wallets().await.len()
    );

    Ok(())
}

// =============================================================================
// Atomic Recovery Wrapper (CDK pattern: try_proof_operation_or_reclaim)
// =============================================================================

use std::sync::atomic::{AtomicBool, Ordering};

/// Global flag to prevent concurrent recovery operations
static IN_ERROR_RECOVERY: AtomicBool = AtomicBool::new(false);

/// Execute an operation with automatic proof state recovery on failure
///
/// This wraps any operation that uses proofs and automatically syncs proof states
/// with the mint if the operation fails. This prevents proof "limbo" where proofs
/// might be spent at the mint but our local state doesn't reflect this.
///
/// Based on CDK's `try_proof_operation_or_reclaim` pattern.
pub(crate) async fn try_operation_or_recover<F, R>(
    mint_url: &str,
    proofs: Vec<cdk::nuts::Proof>,
    operation: F,
) -> Result<R, String>
where
    F: std::future::Future<Output = Result<R, String>>,
{
    match operation.await {
        Ok(result) => Ok(result),
        Err(err) => {
            log::error!(
                "Operation failed with '{}', attempting to recover {} proofs",
                err,
                proofs.len()
            );

            // Try to acquire recovery lock to prevent concurrent recovery
            if IN_ERROR_RECOVERY
                .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
                .is_ok()
            {
                log::info!("Syncing proof states with mint after failure");

                // Sync proof states with mint
                if let Err(sync_err) = sync_proofs_with_mint_after_failure(mint_url, &proofs).await {
                    log::warn!("Failed to sync proof states: {}", sync_err);
                }

                // Release recovery lock
                IN_ERROR_RECOVERY.store(false, Ordering::SeqCst);
            } else {
                log::debug!("Recovery already in progress, skipping duplicate sync");
            }

            Err(err)
        }
    }
}

/// Sync proof states with mint after a failed operation
///
/// Checks which proofs are actually spent/pending at the mint and updates
/// our local state accordingly.
async fn sync_proofs_with_mint_after_failure(
    mint_url: &str,
    proofs: &[cdk::nuts::Proof],
) -> Result<(), String> {
    if proofs.is_empty() {
        return Ok(());
    }

    // Get or create wallet for this mint
    let wallet = cashu_cdk_bridge::get_wallet(mint_url)
        .await
        .map_err(|e| format!("Failed to get wallet: {}", e))?;

    // Check proof states at mint (NUT-07)
    let states = wallet
        .check_proofs_spent(proofs.to_vec())
        .await
        .map_err(|e| format!("Failed to check proof states: {}", e))?;

    // Process state updates
    let mut spent_count = 0;
    let mut pending_count = 0;

    for (state, proof) in states.iter().zip(proofs.iter()) {
        match state.state {
            State::Spent => {
                spent_count += 1;
                // Mark as spent in our local state
                super::proofs::move_proofs_to_spent(&[proof.secret.to_string()]);
            }
            State::Pending => {
                pending_count += 1;
                // Mark as pending at mint
                super::proofs::register_proofs_pending_at_mint(&[proof.secret.to_string()]);
            }
            State::Unspent => {
                // Proof is still unspent, revert any local pending state
                super::proofs::revert_proofs_to_spendable(&[proof.secret.to_string()]);
            }
            State::Reserved => {
                // Reserved at mint - treat as pending
                pending_count += 1;
            }
            _ => {}
        }
    }

    if spent_count > 0 || pending_count > 0 {
        log::info!(
            "Recovery sync: {} spent, {} pending out of {} proofs",
            spent_count,
            pending_count,
            proofs.len()
        );

        // Sync wallet state to update UI
        let _ = cashu_cdk_bridge::sync_wallet_state().await;
    }

    Ok(())
}

/// Execute a wallet swap operation with automatic recovery
///
/// Convenience wrapper for swap/send operations that handles proof recovery.
pub(crate) async fn try_swap_or_recover(
    _wallet: &Wallet,
    mint_url: &str,
    input_proofs: Vec<cdk::nuts::Proof>,
    swap_fn: impl std::future::Future<Output = Result<Vec<cdk::nuts::Proof>, cdk::Error>>,
) -> Result<Vec<cdk::nuts::Proof>, String> {
    let proofs_clone = input_proofs.clone();

    match swap_fn.await {
        Ok(result) => Ok(result),
        Err(err) => {
            let err_str = err.to_string();
            log::error!("Swap failed: {}", err_str);

            // Sync proofs with mint
            if IN_ERROR_RECOVERY
                .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
                .is_ok()
            {
                let _ = sync_proofs_with_mint_after_failure(mint_url, &proofs_clone).await;
                IN_ERROR_RECOVERY.store(false, Ordering::SeqCst);
            }

            Err(err_str)
        }
    }
}
