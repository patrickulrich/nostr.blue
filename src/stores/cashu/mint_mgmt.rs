//! Mint management
//!
//! Functions for adding, removing, and managing mints.
//! Includes counter backup/restore for mint re-addition (Minibits pattern).

// Allow dead_code for planned features not yet wired to UI
#![allow(dead_code)]

use std::str::FromStr;
use std::collections::HashMap;
use std::time::Duration;
use dioxus::prelude::*;
use nostr_sdk::{Kind, PublicKey, Filter};
use super::errors::CashuResult;
use super::internal::create_ephemeral_wallet;
use super::proofs::{proof_data_to_cdk_proof, cdk_proof_to_proof_data};
use super::signals::{
    COUNTER_BACKUPS, SHARED_LOCALSTORE, WALLET_STATE, WALLET_TOKENS, WALLET_BALANCE,
    try_acquire_mint_lock,
};
use super::types::{
    CounterBackup, MintInfoDisplay, DiscoveredMint, MintRecommendation, ConsolidationResult,
    ProofData, TokenData, ExtendedCashuProof, ExtendedTokenEvent, WalletTokensStoreStoreExt,
};
use super::utils::{mint_matches, normalize_mint_url};
use crate::stores::{auth_store, nostr_client};

/// Maximum number of proofs to swap in a single batch (CDK pattern)
/// Mints may reject requests with too many input proofs
const BATCH_PROOF_SIZE: usize = 100;

// =============================================================================
// Keyset Collision Detection
// =============================================================================

/// Result of keyset collision check
#[derive(Debug, Clone)]
pub struct KeysetCollision {
    pub keyset_id: String,
    pub existing_mint: String,
    pub new_mint: String,
}

/// Check if a new mint has any keysets that collide with existing mints
///
/// Keyset IDs are supposed to be globally unique (derived from keyset pubkeys),
/// but in theory two mints could have colliding IDs. This would cause proofs
/// to be misattributed to the wrong mint.
///
/// Returns a list of collisions found (empty if no collisions).
pub async fn check_keyset_collision(new_mint_url: &str) -> Result<Vec<KeysetCollision>, String> {
    use crate::stores::cashu_cdk_bridge;

    log::debug!("Checking for keyset collisions with {}", new_mint_url);

    // Get existing keyset IDs from all current mints
    let mut existing_keyset_to_mint: HashMap<String, String> = HashMap::new();

    // Collect keysets from all existing mints in the MultiMintWallet
    if let Some(ref multi_wallet) = *cashu_cdk_bridge::MULTI_WALLET.read() {
        for wallet in multi_wallet.get_wallets().await {
            let mint_url = wallet.mint_url.to_string();
            if let Ok(keysets) = wallet.get_mint_keysets().await {
                for keyset in keysets {
                    existing_keyset_to_mint.insert(keyset.id.to_string(), mint_url.clone());
                }
            }
        }
    }

    // Also check keysets from proofs we have stored (in case MultiMintWallet doesn't have all mints)
    {
        let store = WALLET_TOKENS.read();
        let data = store.data();
        let tokens = data.read();

        for token in tokens.iter() {
            for proof in &token.proofs {
                // Extract keyset ID from proof if available
                // The proof's `id` field contains the keyset ID
                if let Some(keyset_id) = extract_keyset_id_from_proof(proof) {
                    if !existing_keyset_to_mint.contains_key(&keyset_id) {
                        existing_keyset_to_mint.insert(keyset_id, token.mint.clone());
                    }
                }
            }
        }
    }

    if existing_keyset_to_mint.is_empty() {
        log::debug!("No existing keysets to check against");
        return Ok(vec![]);
    }

    // Fetch keysets from the new mint
    let new_mint_wallet = super::internal::create_ephemeral_wallet(new_mint_url, vec![]).await?;
    let new_keysets = new_mint_wallet
        .get_mint_keysets()
        .await
        .map_err(|e| format!("Failed to fetch keysets from {}: {}", new_mint_url, e))?;

    // Check for collisions
    let mut collisions = Vec::new();
    for keyset in new_keysets {
        let keyset_id = keyset.id.to_string();
        if let Some(existing_mint) = existing_keyset_to_mint.get(&keyset_id) {
            // Don't report collision with self (in case of re-adding same mint)
            if existing_mint != new_mint_url {
                log::warn!(
                    "Keyset collision detected! Keyset {} exists on both {} and {}",
                    keyset_id,
                    existing_mint,
                    new_mint_url
                );
                collisions.push(KeysetCollision {
                    keyset_id,
                    existing_mint: existing_mint.clone(),
                    new_mint: new_mint_url.to_string(),
                });
            }
        }
    }

    if collisions.is_empty() {
        log::debug!("No keyset collisions found for {}", new_mint_url);
    }

    Ok(collisions)
}

/// Extract keyset ID from a proof's id field
fn extract_keyset_id_from_proof(proof: &ProofData) -> Option<String> {
    // The proof's `id` field in our storage is formatted as "{secret}_{amount}"
    // The actual keyset ID should come from parsing the proof's C value
    // For now, we rely on the CDK proofs which have the proper keyset ID

    // Try to parse as a keyset ID directly (if stored that way)
    if !proof.id.is_empty() && proof.id.len() <= 16 && !proof.id.contains('_') {
        return Some(proof.id.clone());
    }

    // Keyset ID not available in our proof storage format
    // This is a limitation - we should ideally store keyset_id separately
    None
}

// =============================================================================
// Basic Mint Queries
// =============================================================================

/// Get total number of mints
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

/// Get balance for a specific mint
pub fn get_mint_balance(mint_url: &str) -> u64 {
    let store = WALLET_TOKENS.read();
    let data = store.data();
    let tokens = data.read();

    tokens.iter()
        .filter(|t| mint_matches(&t.mint, mint_url))
        .flat_map(|t| t.proofs.iter())
        .map(|p| p.amount)
        .fold(0u64, |acc, amt| acc.saturating_add(amt))
}

/// Get proof count for a specific mint
pub fn get_mint_proof_count(mint_url: &str) -> usize {
    let store = WALLET_TOKENS.read();
    let data = store.data();
    let tokens = data.read();

    tokens.iter()
        .filter(|t| mint_matches(&t.mint, mint_url))
        .map(|t| t.proofs.len())
        .fold(0usize, |acc, count| acc.saturating_add(count))
}

/// Get total proof count across all mints
pub fn get_total_proof_count() -> usize {
    let store = WALLET_TOKENS.read();
    let data = store.data();
    let tokens = data.read();

    tokens.iter()
        .map(|t| t.proofs.len())
        .fold(0usize, |acc, count| acc.saturating_add(count))
}

// =============================================================================
// Mint Info
// =============================================================================

/// Get mint info by connecting to the mint and fetching its info endpoint
pub async fn get_mint_info(mint_url: &str) -> Result<MintInfoDisplay, String> {
    log::info!("Fetching mint info for: {}", mint_url);

    // Create ephemeral wallet to fetch mint info
    let wallet = create_ephemeral_wallet(mint_url, vec![]).await?;

    let mint_info = wallet.fetch_mint_info().await
        .map_err(|e| format!("Failed to fetch mint info: {}", e))?
        .ok_or("Mint info not available")?;

    // Extract supported NUTs
    let mut supported_nuts: Vec<u8> = Vec::new();

    if !mint_info.nuts.nut04.methods.is_empty() { supported_nuts.push(4); }
    if !mint_info.nuts.nut05.methods.is_empty() { supported_nuts.push(5); }
    if mint_info.nuts.nut07.supported { supported_nuts.push(7); }
    if mint_info.nuts.nut08.supported { supported_nuts.push(8); }
    if mint_info.nuts.nut09.supported { supported_nuts.push(9); }
    if mint_info.nuts.nut10.supported { supported_nuts.push(10); }
    if mint_info.nuts.nut11.supported { supported_nuts.push(11); }
    if mint_info.nuts.nut12.supported { supported_nuts.push(12); }
    if mint_info.nuts.nut14.supported { supported_nuts.push(14); }
    if mint_info.nuts.nut20.supported { supported_nuts.push(20); }

    supported_nuts.sort();

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

// =============================================================================
// Counter Backup/Restore (Minibits pattern)
// =============================================================================

/// Save keyset counters before removing a mint
///
/// Implements Minibits pattern: when a mint is removed, its counters are backed up
/// so they can be restored if the same mint is re-added. This prevents proof reuse.
pub async fn backup_mint_counters(mint_url: &str) -> CashuResult<()> {
    use crate::stores::cashu_cdk_bridge;
    use cdk_common::database::WalletDatabase;

    log::info!("Backing up counters for mint: {}", mint_url);

    // Get the wallet for this mint (to access its keysets)
    let wallet = match cashu_cdk_bridge::get_wallet(mint_url).await {
        Ok(w) => w,
        Err(e) => {
            log::warn!("Cannot backup counters, failed to get wallet: {}", e);
            return Ok(()); // Non-fatal - we can proceed without backup
        }
    };

    // Get all keysets for this mint
    let keysets = match wallet.get_mint_keysets().await {
        Ok(ks) => ks,
        Err(e) => {
            log::warn!("Cannot backup counters, failed to get keysets: {}", e);
            return Ok(());
        }
    };

    if keysets.is_empty() {
        log::debug!("No keysets to backup for mint {}", mint_url);
        return Ok(());
    }

    // Get database to read counters
    let db = SHARED_LOCALSTORE
        .read()
        .as_ref()
        .cloned()
        .ok_or_else(|| super::errors::CashuWalletError::Database(
            "Localstore not initialized".to_string()
        ))?;

    // Read current counter values for each keyset
    let mut counters = Vec::new();
    for keyset in &keysets {
        // increment_keyset_counter(id, 0) returns current value without changing it
        match db.increment_keyset_counter(&keyset.id, 0).await {
            Ok(counter) => {
                if counter > 0 {
                    counters.push((keyset.id.to_string(), counter as u64));
                    log::debug!("Backed up counter for keyset {}: {}", keyset.id, counter);
                }
            }
            Err(e) => {
                log::warn!("Failed to read counter for keyset {}: {}", keyset.id, e);
            }
        }
    }

    if counters.is_empty() {
        log::debug!("No non-zero counters to backup for mint {}", mint_url);
        return Ok(());
    }

    // Create backup entry
    let backup = CounterBackup {
        mint_url: mint_url.to_string(),
        counters,
        created_at: js_sys::Date::now() as u64 / 1000,
    };

    // Store in global signal (update existing or add new)
    let mut backups = COUNTER_BACKUPS.write();
    if let Some(existing) = backups.iter_mut().find(|b| b.mint_url == mint_url) {
        *existing = backup;
        log::info!("Updated counter backup for mint {}", mint_url);
    } else {
        backups.push(backup);
        log::info!("Created counter backup for mint {}", mint_url);
    }

    Ok(())
}

/// Restore keyset counters after re-adding a mint
///
/// Called after a mint is added - if there's a backup, restores the counters.
/// This ensures proof secret derivation continues from where it left off.
pub async fn restore_mint_counters(mint_url: &str) -> CashuResult<()> {
    use cdk_common::database::WalletDatabase;

    // Check if we have a backup for this mint
    let backup = {
        let backups = COUNTER_BACKUPS.read();
        backups.iter().find(|b| b.mint_url == mint_url).cloned()
    };

    let backup = match backup {
        Some(b) => b,
        None => {
            log::debug!("No counter backup found for mint {}", mint_url);
            return Ok(());
        }
    };

    log::info!(
        "Restoring {} counter(s) for mint {}",
        backup.counters.len(),
        mint_url
    );

    // Get database to set counters
    let db = SHARED_LOCALSTORE
        .read()
        .as_ref()
        .cloned()
        .ok_or_else(|| super::errors::CashuWalletError::Database(
            "Localstore not initialized".to_string()
        ))?;

    // Restore each counter
    for (keyset_id_str, target_value) in &backup.counters {
        let keyset_id = match cdk::nuts::Id::from_str(keyset_id_str) {
            Ok(id) => id,
            Err(e) => {
                log::warn!("Invalid keyset ID in backup '{}': {}", keyset_id_str, e);
                continue;
            }
        };

        // Get current counter value
        let current = match db.increment_keyset_counter(&keyset_id, 0).await {
            Ok(c) => c as u64,
            Err(e) => {
                log::warn!("Failed to read current counter for {}: {}", keyset_id, e);
                continue;
            }
        };

        // Only restore if backup is higher than current
        if *target_value > current {
            let increment = (*target_value - current) as u32;
            match db.increment_keyset_counter(&keyset_id, increment).await {
                Ok(new_val) => {
                    log::info!(
                        "Restored counter for keyset {}: {} → {}",
                        keyset_id,
                        current,
                        new_val
                    );
                }
                Err(e) => {
                    log::error!("Failed to restore counter for keyset {}: {}", keyset_id, e);
                }
            }
        } else {
            log::debug!(
                "Counter for keyset {} already at {} (backup was {})",
                keyset_id,
                current,
                target_value
            );
        }
    }

    log::info!("Counter restore complete for mint {}", mint_url);
    Ok(())
}

/// Remove counter backup for a mint
///
/// Called when we're sure we don't need the backup anymore
/// (e.g., mint successfully re-added and counters restored)
pub fn remove_counter_backup(mint_url: &str) {
    let mut backups = COUNTER_BACKUPS.write();
    let len_before = backups.len();
    backups.retain(|b| b.mint_url != mint_url);
    if backups.len() < len_before {
        log::debug!("Removed counter backup for mint {}", mint_url);
    }
}

/// Get counter backup for a mint (if exists)
pub fn get_counter_backup(mint_url: &str) -> Option<CounterBackup> {
    COUNTER_BACKUPS
        .read()
        .iter()
        .find(|b| b.mint_url == mint_url)
        .cloned()
}

// =============================================================================
// Enhanced Mint Operations with Counter Backup
// =============================================================================

/// Add a mint with counter restoration and automatic proof recovery
///
/// Full implementation that:
/// 1. Validates URL and connectivity
/// 2. Verifies NUT support
/// 3. Updates wallet state and publishes to Nostr
/// 4. Restores counters if we previously had this mint
/// 5. Runs background proof restoration (Harbor pattern)
pub async fn add_mint(mint_url: &str) -> Result<(), String> {
    use nostr_sdk::signer::NostrSigner;
    use url::Url;

    // Normalize the URL to prevent duplicates
    let mint_url = normalize_mint_url(mint_url);

    log::info!("Adding mint: {}", mint_url);

    // Validate URL format
    let url = Url::parse(&mint_url)
        .map_err(|e| format!("Invalid URL format: {}", e))?;

    // Ensure it's https (or http for localhost testing)
    if url.scheme() != "https" && !url.host_str().unwrap_or("").contains("localhost") {
        return Err("Mint URL must use HTTPS".to_string());
    }

    // Early check if mint already exists (fast path, but we re-check atomically later)
    let existing_mints = get_mints();
    let normalized_existing: Vec<String> = existing_mints.iter().map(|m| normalize_mint_url(m)).collect();
    if normalized_existing.contains(&mint_url) {
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

    // Check for keyset collisions (safety check - extremely rare in practice)
    match check_keyset_collision(&mint_url).await {
        Ok(collisions) if !collisions.is_empty() => {
            // Log warning but allow user to proceed
            // In practice, keyset collisions are astronomically unlikely
            log::warn!(
                "Keyset collision warning for {}: {} collision(s) detected. \
                This is extremely rare and could indicate a configuration issue.",
                mint_url,
                collisions.len()
            );
            for collision in &collisions {
                log::warn!(
                    "  - Keyset {} already exists on {}",
                    collision.keyset_id,
                    collision.existing_mint
                );
            }
        }
        Ok(_) => {
            // No collisions - normal case
            log::debug!("No keyset collisions detected for {}", mint_url);
        }
        Err(e) => {
            // Couldn't check - log warning but continue
            log::warn!("Could not check for keyset collisions: {}", e);
        }
    }

    // Atomically check-and-insert to prevent race condition
    {
        let mut state = WALLET_STATE.write();
        if let Some(ref mut wallet_state) = *state {
            // Re-check under write lock to prevent concurrent additions
            let normalized_existing: Vec<String> = wallet_state.mints.iter()
                .map(|m| normalize_mint_url(m))
                .collect();
            if normalized_existing.contains(&mint_url) {
                return Err("Mint already exists in wallet".to_string());
            }
            wallet_state.mints.push(mint_url.clone());
        } else {
            return Err("Wallet not initialized".to_string());
        }
    }

    // Update wallet event on relays
    let wallet_state = WALLET_STATE.read().clone()
        .ok_or("Wallet state not available")?;
    let privkey = wallet_state.privkey.as_ref()
        .ok_or("Wallet private key not available")?;

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
    let mut content_array: Vec<Vec<&str>> = vec![vec!["privkey", privkey]];
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

    // Restore counters if we have a backup
    if let Err(e) = restore_mint_counters(&mint_url).await {
        log::warn!("Failed to restore counters for {}: {}", mint_url, e);
        // Non-fatal - mint is still added
    }

    // Run background proof restoration (Harbor pattern)
    let mint_url_owned = mint_url.clone();
    wasm_bindgen_futures::spawn_local(async move {
        if let Err(e) = restore_proofs_from_mint(&mint_url_owned).await {
            log::warn!("Background restoration failed for {}: {}", mint_url_owned, e);
        }
    });

    Ok(())
}

/// Restore proofs from a mint using CDK's restore function
///
/// This implements the Harbor pattern of automatic restoration when adding a mint.
/// It checks the mint for any proofs we might have derived but not recorded locally.
pub async fn restore_proofs_from_mint(mint_url: &str) -> CashuResult<u64> {
    use crate::stores::cashu_cdk_bridge;

    log::info!("Starting proof restoration for mint: {}", mint_url);

    // Get wallet for this mint
    let wallet = cashu_cdk_bridge::get_wallet(mint_url).await.map_err(|e| {
        super::errors::CashuWalletError::MintConnection {
            mint_url: mint_url.to_string(),
            message: e,
        }
    })?;

    // Run CDK's restore function
    // This iterates through counter values checking for unrecorded proofs
    match wallet.restore().await {
        Ok(amount) => {
            let restored_sats = u64::from(amount);
            if restored_sats > 0 {
                log::info!(
                    "Restored {} sats from mint {}",
                    restored_sats,
                    mint_url
                );

                // Sync wallet state to pick up restored proofs
                if let Err(e) = cashu_cdk_bridge::sync_wallet_state().await {
                    log::warn!("Failed to sync after restoration: {}", e);
                }
            } else {
                log::debug!("No proofs to restore from mint {}", mint_url);
            }
            Ok(restored_sats)
        }
        Err(e) => {
            log::warn!("Restore failed for mint {}: {}", mint_url, e);
            Err(super::errors::CashuWalletError::Cdk(e))
        }
    }
}

/// Remove a mint with counter backup
///
/// Full implementation that:
/// 1. Backs up counters before removal
/// 2. Removes all tokens for the mint
/// 3. Publishes deletion events
/// 4. Updates wallet state
/// Returns (event_count, total_amount) on success.
pub async fn remove_mint(mint_url: &str) -> Result<(usize, u64), String> {
    use nostr_sdk::signer::NostrSigner;

    // Backup counters before removing
    if let Err(e) = backup_mint_counters(mint_url).await {
        log::warn!("Failed to backup counters for {}: {}", mint_url, e);
        // Non-fatal - continue with removal
    }

    log::info!("Removing mint: {}", mint_url);

    // Get all token events for this mint (scoped read)
    let (event_ids_to_delete, total_amount, token_count) = {
        let store = WALLET_TOKENS.read();
        let data = store.data();
        let tokens = data.read();
        let mint_tokens: Vec<_> = tokens.iter()
            .filter(|t| mint_matches(&t.mint, mint_url))
            .collect();

        let event_ids: Vec<String> = mint_tokens.iter()
            .map(|t| t.event_id.clone())
            .collect();

        let amount: u64 = mint_tokens.iter()
            .flat_map(|t| &t.proofs)
            .map(|p| p.amount)
            .fold(0u64, |acc, amt| acc.saturating_add(amt));

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
    // Use mint_matches for normalized comparison (handles URL variants)
    {
        let store = WALLET_TOKENS.read();
        let mut data = store.data();
        let mut tokens_write = data.write();
        tokens_write.retain(|t| !mint_matches(&t.mint, mint_url));
    }

    // Remove mint from wallet state
    // Use mint_matches for normalized comparison
    {
        let mut state_write = WALLET_STATE.write();
        if let Some(ref mut state) = *state_write {
            state.mints.retain(|m| !mint_matches(m, mint_url));
        }
    }

    // Update wallet event on relays to persist the mint removal
    {
        let wallet_state = WALLET_STATE.read().clone();
        if let Some(ref state) = wallet_state {
            // Ensure privkey exists before publishing
            let Some(ref privkey) = state.privkey else {
                log::warn!("Cannot update wallet event: no private key available");
                // Continue with local state changes - wallet event update is non-critical
                return Ok((token_count, total_amount));
            };

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
            let mut content_array: Vec<Vec<&str>> = vec![vec!["privkey", privkey]];
            for mint in state.mints.iter() {
                content_array.push(vec!["mint", mint.as_str()]);
            }

            let json_content = serde_json::to_string(&content_array)
                .map_err(|e| format!("Failed to serialize wallet data: {}", e))?;

            let encrypted = signer.nip44_encrypt(&pubkey, &json_content).await
                .map_err(|e| format!("Failed to encrypt: {}", e))?;

            let builder = nostr_sdk::EventBuilder::new(Kind::CashuWallet, encrypted);

            match client.send_event_builder(builder).await {
                Ok(_) => log::info!("Published updated wallet event after mint removal"),
                Err(e) => log::warn!("Failed to publish wallet event: {}", e),
            }
        }
    }

    // Recalculate balance
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

    log::info!("Removed mint {} ({} sats)", mint_url, total_amount);

    Ok((token_count, total_amount))
}

// =============================================================================
// Proof Consolidation
// =============================================================================

/// Consolidate proofs for a mint via swap
///
/// This reduces the number of proofs by swapping them for a smaller set
/// with the same total value. Useful for optimizing wallet size.
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
            .filter(|t| mint_matches(&t.mint, &mint_url))
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
        .fold(0u64, |acc, amt| acc.saturating_add(amt));

    log::info!("Consolidating {} proofs worth {} sats", proofs_before, total_amount);

    // Create wallet for swaps
    let wallet = create_ephemeral_wallet(&mint_url, all_proofs.clone()).await?;

    // Batch proofs to avoid exceeding mint limits (CDK pattern)
    let mut new_proofs: Vec<cdk::nuts::Proof> = Vec::new();
    for (batch_idx, proof_batch) in all_proofs.chunks(BATCH_PROOF_SIZE).enumerate() {
        let batch_amount: u64 = proof_batch.iter()
            .map(|p| u64::from(p.amount))
            .fold(0u64, |acc, amt| acc.saturating_add(amt));

        log::debug!("Swapping batch {} with {} proofs ({} sats)",
            batch_idx + 1, proof_batch.len(), batch_amount);

        let batch_result = wallet.swap(
            Some(Amount::from(batch_amount)),
            SplitTarget::default(),  // PowerOfTwo split
            proof_batch.to_vec(),
            None,   // No spending conditions
            false,  // Don't add fees to amount
        ).await
            .map_err(|e| format!("Swap failed on batch {}: {}", batch_idx + 1, e))?;

        if let Some(proofs) = batch_result {
            new_proofs.extend(proofs);
        }
    }

    if new_proofs.is_empty() {
        return Err("Swap returned no proofs".to_string());
    }

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

// =============================================================================
// Mint Discovery (NIP-87)
// =============================================================================

/// Discover mints via NIP-87 announcements and recommendations
pub async fn discover_mints() -> Result<Vec<DiscoveredMint>, String> {
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
