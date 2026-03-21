//! Mint management
//!
//! Functions for adding, removing, and managing mints.
//! Includes counter backup/restore for mint re-addition.
#![allow(dead_code)]
use super::errors::CashuResult;
use super::internal::create_ephemeral_wallet;
use super::proofs::{cdk_proof_to_proof_data, proof_data_to_cdk_proof};
use super::signals::{
    try_acquire_mint_lock, COUNTER_BACKUPS, SHARED_LOCALSTORE, WALLET_STATE, WALLET_TOKENS,
};
use super::types::{
    ConsolidationResult, CounterBackup, DiscoveredMint, ExtendedCashuProof, ExtendedTokenEvent,
    InFlightSendRequest, MintInfoDisplay, MintRecommendation, OperationType, ProofData, TokenData,
    WalletTokensStoreStoreExt,
};
use super::utils::{mint_matches, normalize_mint_url, now_secs};
use crate::stores::{auth_store, nostr_client};
use dioxus::prelude::*;
use nostr_sdk::{Filter, Kind, PublicKey};
use std::collections::HashMap;
use std::str::FromStr;
use std::time::Duration;
/// Maximum number of proofs to swap in a single batch (CDK pattern)
/// Mints may reject requests with too many input proofs
const BATCH_PROOF_SIZE: usize = 100;
/// Exponential backoff delays for CDK persistence retry (milliseconds)
const PERSISTENCE_RETRY_DELAYS_MS: [u32; 3] = [1000, 2000, 4000];
/// Maximum jitter to add to retry delays (prevents synchronized retries)
const PERSISTENCE_RETRY_JITTER_MS: u32 = 200;
use super::signals::InFlightGuard;
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
    let mut existing_keyset_to_mint: HashMap<String, String> = HashMap::new();
    let multi_wallet_opt = cashu_cdk_bridge::MULTI_WALLET.read().clone();
    if let Some(ref multi_wallet) = multi_wallet_opt {
        let wallets = multi_wallet.get_wallets().await;
        let futures: Vec<_> = wallets
            .iter()
            .map(|wallet| {
                let mint_url = wallet.mint_url.to_string();
                let wallet = wallet.clone();
                async move {
                    match wallet.get_mint_keysets().await {
                        Ok(keysets) => Some((mint_url, keysets)),
                        Err(e) => {
                            log::warn!("Failed to get keysets for {}: {}", mint_url, e);
                            None
                        }
                    }
                }
            })
            .collect();
        let results = futures::future::join_all(futures).await;
        for result in results.into_iter().flatten() {
            let (mint_url, keysets) = result;
            for keyset in keysets {
                existing_keyset_to_mint.insert(keyset.id.to_string(), mint_url.clone());
            }
        }
    }
    {
        let store = WALLET_TOKENS.read();
        let data = store.data();
        let tokens = data.read();
        for token in tokens.iter() {
            for proof in &token.proofs {
                if let Some(keyset_id) = extract_keyset_id_from_proof(proof) {
                    existing_keyset_to_mint
                        .entry(keyset_id)
                        .or_insert_with(|| token.mint.clone());
                }
            }
        }
    }
    if existing_keyset_to_mint.is_empty() {
        log::debug!("No existing keysets to check against");
        return Ok(vec![]);
    }
    let new_mint_wallet = super::internal::create_ephemeral_wallet(new_mint_url, vec![]).await?;
    let new_keysets = new_mint_wallet
        .get_mint_keysets()
        .await
        .map_err(|e| format!("Failed to fetch keysets from {}: {}", new_mint_url, e))?;
    let mut collisions = Vec::new();
    for keyset in new_keysets {
        let keyset_id = keyset.id.to_string();
        if let Some(existing_mint) = existing_keyset_to_mint.get(&keyset_id) {
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
    let len = proof.id.len();
    if (len == 14 || len == 64)
        && !proof.id.contains('_')
        && proof.id.chars().all(|c| c.is_ascii_hexdigit())
    {
        return Some(proof.id.clone());
    }
    None
}
/// Get total number of mints
pub fn get_mint_count() -> usize {
    WALLET_STATE
        .read()
        .as_ref()
        .map(|w| w.mints.len())
        .unwrap_or(0)
}
/// Get mints list
pub fn get_mints() -> Vec<String> {
    WALLET_STATE
        .read()
        .as_ref()
        .map(|w| w.mints.clone())
        .unwrap_or_default()
}
/// Get balance for a specific mint (includes all proofs regardless of state)
/// For spendable balance, use get_mint_spendable_balance instead
pub fn get_mint_balance(mint_url: &str) -> u64 {
    let store = WALLET_TOKENS.read();
    let data = store.data();
    let tokens = data.read();
    tokens
        .iter()
        .filter(|t| mint_matches(&t.mint, mint_url))
        .flat_map(|t| t.proofs.iter())
        .map(|p| p.amount)
        .fold(0u64, |acc, amt| acc.saturating_add(amt))
}
/// Get spendable balance for a specific mint (only Unspent proofs)
/// CDK pattern: filter by state to exclude pending/reserved/spent proofs
pub fn get_mint_spendable_balance(mint_url: &str) -> u64 {
    use super::types::ProofState;
    let store = WALLET_TOKENS.read();
    let data = store.data();
    let tokens = data.read();
    tokens
        .iter()
        .filter(|t| mint_matches(&t.mint, mint_url))
        .flat_map(|t| t.proofs.iter())
        .filter(|p| p.state == ProofState::Unspent)
        .map(|p| p.amount)
        .fold(0u64, |acc, amt| acc.saturating_add(amt))
}
/// Get spendable balance for a specific mint AND unit (CDK pattern)
/// Filters by both mint URL and unit for multi-unit mint support
/// Issue #1: Unit-aware balance check for nutzaps
pub fn get_mint_unit_spendable_balance(mint_url: &str, unit: &str) -> u64 {
    use super::types::ProofState;
    let store = WALLET_TOKENS.read();
    let data = store.data();
    let tokens = data.read();
    tokens
        .iter()
        .filter(|t| mint_matches(&t.mint, mint_url) && t.unit == unit)
        .flat_map(|t| t.proofs.iter())
        .filter(|p| p.state == ProofState::Unspent)
        .map(|p| p.amount)
        .fold(0u64, |acc, amt| acc.saturating_add(amt))
}
/// Get proof count for a specific mint
pub fn get_mint_proof_count(mint_url: &str) -> usize {
    let store = WALLET_TOKENS.read();
    let data = store.data();
    let tokens = data.read();
    tokens
        .iter()
        .filter(|t| mint_matches(&t.mint, mint_url))
        .map(|t| t.proofs.len())
        .fold(0usize, |acc, count| acc.saturating_add(count))
}
/// Get total proof count across all mints
pub fn get_total_proof_count() -> usize {
    let store = WALLET_TOKENS.read();
    let data = store.data();
    let tokens = data.read();
    tokens
        .iter()
        .map(|t| t.proofs.len())
        .fold(0usize, |acc, count| acc.saturating_add(count))
}
/// Get mint info by connecting to the mint and fetching its info endpoint
pub async fn get_mint_info(mint_url: &str) -> Result<MintInfoDisplay, String> {
    log::info!("Fetching mint info for: {}", mint_url);
    let wallet = create_ephemeral_wallet(mint_url, vec![]).await?;
    let mint_info = wallet
        .fetch_mint_info()
        .await
        .map_err(|e| format!("Failed to fetch mint info: {}", e))?
        .ok_or("Mint info not available")?;
    let mut supported_nuts: Vec<u8> = Vec::new();
    if !mint_info.nuts.nut04.methods.is_empty() {
        supported_nuts.push(4);
    }
    if !mint_info.nuts.nut05.methods.is_empty() {
        supported_nuts.push(5);
    }
    if mint_info.nuts.nut07.supported {
        supported_nuts.push(7);
    }
    if mint_info.nuts.nut08.supported {
        supported_nuts.push(8);
    }
    if mint_info.nuts.nut09.supported {
        supported_nuts.push(9);
    }
    if mint_info.nuts.nut10.supported {
        supported_nuts.push(10);
    }
    if mint_info.nuts.nut11.supported {
        supported_nuts.push(11);
    }
    if mint_info.nuts.nut12.supported {
        supported_nuts.push(12);
    }
    if mint_info.nuts.nut14.supported {
        supported_nuts.push(14);
    }
    if mint_info.nuts.nut20.supported {
        supported_nuts.push(20);
    }
    supported_nuts.sort();
    let contact: Vec<(String, String)> = mint_info
        .contact
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
/// Save keyset counters before removing a mint
///
/// When a mint is removed, its counters are backed up
/// so they can be restored if the same mint is re-added. This prevents proof reuse.
pub async fn backup_mint_counters(mint_url: &str) -> CashuResult<()> {
    use crate::stores::cashu_cdk_bridge;
    use cdk_common::database::WalletDatabase;
    log::info!("Backing up counters for mint: {}", mint_url);
    let wallet = match cashu_cdk_bridge::get_wallet(mint_url).await {
        Ok(w) => w,
        Err(e) => {
            log::warn!("Cannot backup counters, failed to get wallet: {}", e);
            return Ok(());
        }
    };
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
    let db = SHARED_LOCALSTORE.read().as_ref().cloned().ok_or_else(|| {
        super::errors::CashuWalletError::Database("Localstore not initialized".to_string())
    })?;
    let mut counters = Vec::new();
    for keyset in &keysets {
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
    let backup = CounterBackup {
        mint_url: mint_url.to_string(),
        counters,
        created_at: crate::platform::timestamp::now_secs(),
    };
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
    let db = SHARED_LOCALSTORE.read().as_ref().cloned().ok_or_else(|| {
        super::errors::CashuWalletError::Database("Localstore not initialized".to_string())
    })?;
    for (keyset_id_str, target_value) in &backup.counters {
        let keyset_id = match cdk::nuts::Id::from_str(keyset_id_str) {
            Ok(id) => id,
            Err(e) => {
                log::warn!("Invalid keyset ID in backup '{}': {}", keyset_id_str, e);
                continue;
            }
        };
        let current = match db.increment_keyset_counter(&keyset_id, 0).await {
            Ok(c) => c as u64,
            Err(e) => {
                log::warn!("Failed to read current counter for {}: {}", keyset_id, e);
                continue;
            }
        };
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
/// Add a mint with counter restoration and automatic proof recovery
///
/// Full implementation that:
/// 1. Validates URL and connectivity
/// 2. Verifies NUT support
/// 3. Updates wallet state and publishes to Nostr
/// 4. Restores counters if we previously had this mint
/// 5. Runs background proof restoration
pub async fn add_mint(mint_url: &str) -> Result<(), String> {
    use nostr_sdk::signer::NostrSigner;
    use url::Url;
    let mint_url = normalize_mint_url(mint_url);
    log::info!("Adding mint: {}", mint_url);
    let url = Url::parse(&mint_url).map_err(|e| format!("Invalid URL format: {}", e))?;
    if url.scheme() != "https" && !url.host_str().unwrap_or("").contains("localhost") {
        return Err("Mint URL must use HTTPS".to_string());
    }
    let existing_mints = get_mints();
    let normalized_existing: Vec<String> = existing_mints
        .iter()
        .map(|m| normalize_mint_url(m))
        .collect();
    if normalized_existing.contains(&mint_url) {
        return Err("Mint already exists in wallet".to_string());
    }
    let mint_info = get_mint_info(&mint_url).await?;
    let has_nut4 = mint_info.supported_nuts.contains(&4);
    let has_nut5 = mint_info.supported_nuts.contains(&5);
    if !has_nut4 || !has_nut5 {
        return Err(format!(
            "Mint doesn't support required features. NUT-4: {}, NUT-5: {}",
            has_nut4, has_nut5,
        ));
    }
    match check_keyset_collision(&mint_url).await {
        Ok(collisions) if !collisions.is_empty() => {
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
            log::debug!("No keyset collisions detected for {}", mint_url);
        }
        Err(e) => {
            log::warn!("Could not check for keyset collisions: {}", e);
        }
    }
    {
        let mut state = WALLET_STATE.write();
        if let Some(ref mut wallet_state) = *state {
            let normalized_existing: Vec<String> = wallet_state
                .mints
                .iter()
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
    let wallet_state = WALLET_STATE
        .read()
        .clone()
        .ok_or("Wallet state not available")?;
    let privkey = wallet_state
        .privkey
        .as_ref()
        .ok_or("Wallet private key not available")?;
    let signer = crate::stores::signer::get_signer()
        .ok_or("No signer available")?
        .as_nostr_signer();
    let pubkey_str = auth_store::get_pubkey().ok_or("Not authenticated")?;
    let pubkey = PublicKey::parse(&pubkey_str).map_err(|e| format!("Invalid pubkey: {}", e))?;
    let client = nostr_client::NOSTR_CLIENT
        .read()
        .as_ref()
        .ok_or("Client not initialized")?
        .clone();
    let mut content_array: Vec<Vec<&str>> = vec![vec!["privkey", privkey]];
    for mint in wallet_state.mints.iter() {
        content_array.push(vec!["mint", mint.as_str()]);
    }
    let json_content = serde_json::to_string(&content_array)
        .map_err(|e| format!("Failed to serialize wallet data: {}", e))?;
    let encrypted = signer
        .nip44_encrypt(&pubkey, &json_content)
        .await
        .map_err(|e| format!("Failed to encrypt: {}", e))?;
    let builder = nostr_sdk::EventBuilder::new(Kind::CashuWallet, encrypted);
    match client
        .send_event_builder(crate::utils::nips::nip89::tag_event_builder(builder))
        .await
    {
        Ok(output) if !output.success.is_empty() => {
            log::info!("Published updated wallet event with new mint")
        }
        Ok(_) => {
            log::error!("Failed to publish wallet event: no relays accepted the event");
            {
                let mut state = WALLET_STATE.write();
                if let Some(ref mut wallet_state) = *state {
                    wallet_state
                        .mints
                        .retain(|m| normalize_mint_url(m) != mint_url);
                }
            }
            return Err("Failed to publish wallet event: no relays accepted the event".to_string());
        }
        Err(e) => {
            log::error!("Failed to publish wallet event: {}", e);
            {
                let mut state = WALLET_STATE.write();
                if let Some(ref mut wallet_state) = *state {
                    wallet_state
                        .mints
                        .retain(|m| normalize_mint_url(m) != mint_url);
                }
            }
            return Err(format!("Failed to publish wallet event: {}", e));
        }
    }
    log::info!("Successfully added mint: {}", mint_url);
    if let Err(e) = restore_mint_counters(&mint_url).await {
        log::warn!("Failed to restore counters for {}: {}", mint_url, e);
    }
    let mint_url_owned = mint_url.clone();
    #[cfg(feature = "web")]
    wasm_bindgen_futures::spawn_local(async move {
        if let Err(e) = restore_proofs_from_mint(&mint_url_owned).await {
            log::warn!(
                "Background restoration failed for {}: {}",
                mint_url_owned,
                e
            );
        }
    });
    #[cfg(feature = "native")]
    tokio::task::spawn(async move {
        if let Err(e) = restore_proofs_from_mint(&mint_url_owned).await {
            log::warn!(
                "Background restoration failed for {}: {}",
                mint_url_owned,
                e
            );
        }
    });
    Ok(())
}
/// Restore proofs from a mint using CDK's restore function
///
/// Implements automatic restoration when adding a mint.
/// It checks the mint for any proofs we might have derived but not recorded locally.
pub async fn restore_proofs_from_mint(mint_url: &str) -> CashuResult<u64> {
    use crate::stores::cashu_cdk_bridge;
    log::info!("Starting proof restoration for mint: {}", mint_url);
    let wallet = cashu_cdk_bridge::get_wallet(mint_url).await.map_err(|e| {
        super::errors::CashuWalletError::MintConnection {
            mint_url: mint_url.to_string(),
            message: e,
        }
    })?;
    match wallet.restore().await {
        Ok(amount) => {
            let restored_sats = u64::from(amount);
            if restored_sats > 0 {
                log::info!("Restored {} sats from mint {}", restored_sats, mint_url);
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
///
/// Returns (event_count, total_amount) on success.
pub async fn remove_mint(mint_url: &str) -> Result<(usize, u64), String> {
    use nostr_sdk::signer::NostrSigner;
    if let Err(e) = backup_mint_counters(mint_url).await {
        log::warn!("Failed to backup counters for {}: {}", mint_url, e);
    }
    log::info!("Removing mint: {}", mint_url);
    let (event_ids_to_delete, total_amount, token_count) = {
        let store = WALLET_TOKENS.read();
        let data = store.data();
        let tokens = data.read();
        let mint_tokens: Vec<_> = tokens
            .iter()
            .filter(|t| mint_matches(&t.mint, mint_url))
            .collect();
        let event_ids: Vec<String> = mint_tokens.iter().map(|t| t.event_id.clone()).collect();
        let amount: u64 = mint_tokens
            .iter()
            .flat_map(|t| &t.proofs)
            .map(|p| p.amount)
            .fold(0u64, |acc, amt| acc.saturating_add(amt));
        (event_ids, amount, mint_tokens.len())
    };
    log::info!(
        "Found {} token events worth {} sats to remove",
        token_count,
        total_amount
    );
    if !event_ids_to_delete.is_empty() {
        let mut tags = Vec::new();
        for event_id in &event_ids_to_delete {
            tags.push(nostr_sdk::Tag::event(
                nostr_sdk::EventId::parse(event_id)
                    .map_err(|e| format!("Invalid event ID: {}", e))?,
            ));
        }
        tags.push(nostr_sdk::Tag::custom(
            nostr_sdk::TagKind::custom("k"),
            ["7375"],
        ));
        let deletion_builder =
            nostr_sdk::EventBuilder::new(Kind::from(5), format!("Removed mint: {}", mint_url))
                .tags(tags);
        let client = nostr_client::NOSTR_CLIENT
            .read()
            .as_ref()
            .ok_or("Client not initialized")?
            .clone();
        let output = client
            .send_event_builder(crate::utils::nips::nip89::tag_event_builder(
                deletion_builder,
            ))
            .await
            .map_err(|e| format!("Failed to publish deletion event: {}", e))?;
        if output.success.is_empty() {
            return Err(
                "Failed to publish deletion event: no relays accepted the event".to_string(),
            );
        }
        log::info!(
            "Published deletion event for {} token events",
            event_ids_to_delete.len()
        );
    }
    let staged_wallet_state = WALLET_STATE.read().clone().map(|mut state| {
        state.mints.retain(|m| !mint_matches(m, mint_url));
        state
    });
    if let Some(ref state) = staged_wallet_state {
        if let Some(ref privkey) = state.privkey {
            let signer = crate::stores::signer::get_signer()
                .ok_or("No signer available")?
                .as_nostr_signer();
            let pubkey_str = auth_store::get_pubkey().ok_or("Not authenticated")?;
            let pubkey =
                PublicKey::parse(&pubkey_str).map_err(|e| format!("Invalid pubkey: {}", e))?;
            let client = nostr_client::NOSTR_CLIENT
                .read()
                .as_ref()
                .ok_or("Client not initialized")?
                .clone();
            let mut content_array: Vec<Vec<&str>> = vec![vec!["privkey", privkey]];
            for mint in state.mints.iter() {
                content_array.push(vec!["mint", mint.as_str()]);
            }
            let json_content = serde_json::to_string(&content_array)
                .map_err(|e| format!("Failed to serialize wallet data: {}", e))?;
            let encrypted = signer
                .nip44_encrypt(&pubkey, &json_content)
                .await
                .map_err(|e| format!("Failed to encrypt: {}", e))?;
            let builder = nostr_sdk::EventBuilder::new(Kind::CashuWallet, encrypted);
            let output = client
                .send_event_builder(crate::utils::nips::nip89::tag_event_builder(builder))
                .await
                .map_err(|e| format!("Failed to publish wallet event: {}", e))?;
            if output.success.is_empty() {
                return Err("Failed to publish wallet event: no relays accepted the event".into());
            }
            log::info!("Published updated wallet event after mint removal");
        }
    }
    {
        let store = WALLET_TOKENS.read();
        let mut data = store.data();
        let mut tokens_write = data.write();
        tokens_write.retain(|t| !mint_matches(&t.mint, mint_url));
    }
    if let Some(state) = staged_wallet_state {
        *WALLET_STATE.write() = Some(state);
    }
    super::signals::update_wallet_balances();
    log::info!("Removed mint {} ({} sats)", mint_url, total_amount);
    Ok((token_count, total_amount))
}
/// Consolidate proofs for a mint via swap
///
/// This reduces the number of proofs by swapping them for a smaller set
/// with the same total value. Useful for optimizing wallet size.
pub async fn consolidate_proofs(mint_url: String) -> Result<ConsolidationResult, String> {
    use cdk::amount::SplitTarget;
    use cdk::Amount;
    use nostr_sdk::signer::NostrSigner;
    log::info!("Consolidating proofs for mint: {}", mint_url);
    let _lock_guard = try_acquire_mint_lock(&mint_url)
        .ok_or_else(|| format!("Another operation is in progress for mint: {}", mint_url))?;
    let localstore = SHARED_LOCALSTORE
        .read()
        .as_ref()
        .ok_or("Localstore not initialized - cannot safely persist proofs")?
        .clone();
    let (all_proofs, event_ids_to_delete, unit_str) = {
        let store = WALLET_TOKENS.read();
        let data = store.data();
        let tokens = data.read();
        let mint_tokens: Vec<_> = tokens
            .iter()
            .filter(|t| mint_matches(&t.mint, &mint_url))
            .collect();
        if mint_tokens.is_empty() {
            return Err("No tokens found for this mint".to_string());
        }
        let mut all_proofs = Vec::new();
        let mut event_ids = Vec::new();
        let mut detected_unit: Option<String> = None;
        for token in &mint_tokens {
            if let Some(ref existing) = detected_unit {
                if &token.unit != existing {
                    return Err(format!(
                        "Mixed units in mint proofs: '{}' and '{}' - cannot consolidate",
                        existing, token.unit,
                    ));
                }
            } else {
                detected_unit = Some(token.unit.clone());
            }
            event_ids.push(token.event_id.clone());
            for proof in &token.proofs {
                all_proofs.push(proof_data_to_cdk_proof(proof)?);
            }
        }
        let unit = detected_unit.ok_or("No proofs found to determine unit")?;
        (all_proofs, event_ids, unit)
    };
    let proofs_before = all_proofs.len();
    if proofs_before <= 8 {
        log::info!("Wallet already optimized with {} proofs", proofs_before);
        return Ok(ConsolidationResult {
            proofs_before,
            proofs_after: proofs_before,
            fee_paid: 0,
        });
    }
    let total_amount: u64 = all_proofs
        .iter()
        .map(|p| u64::from(p.amount))
        .fold(0u64, |acc, amt| acc.saturating_add(amt));
    log::info!(
        "Consolidating {} proofs worth {} sats",
        proofs_before,
        total_amount
    );
    let wallet = create_ephemeral_wallet(&mint_url, all_proofs.clone()).await?;
    let tx_id = format!("swap_{}", uuid::Uuid::new_v4());
    let proof_secrets: Vec<String> = all_proofs.iter().map(|p| p.secret.to_string()).collect();
    let in_flight = InFlightSendRequest {
        transaction_id: tx_id.clone(),
        mint_url: mint_url.clone(),
        proof_secrets,
        amount: total_amount,
        operation_type: OperationType::Swap,
        created_at: now_secs(),
    };
    super::signals::add_in_flight_send_request(in_flight);
    let mut in_flight_guard = InFlightGuard::new(tx_id.clone());
    let mint_url_parsed: cdk::mint_url::MintUrl = mint_url
        .parse()
        .map_err(|e| format!("Invalid mint URL: {}", e))?;
    let total_batches = all_proofs.chunks(BATCH_PROOF_SIZE).count();
    let mut new_proofs: Vec<cdk::nuts::Proof> = Vec::new();
    let mut persistence_failures: Vec<(usize, String)> = Vec::new();
    for (batch_idx, proof_batch) in all_proofs.chunks(BATCH_PROOF_SIZE).enumerate() {
        let batch_amount: u64 = proof_batch
            .iter()
            .map(|p| u64::from(p.amount))
            .fold(0u64, |acc, amt| acc.saturating_add(amt));
        log::debug!(
            "Swapping batch {}/{} with {} proofs ({} sats)",
            batch_idx + 1,
            total_batches,
            proof_batch.len(),
            batch_amount
        );
        let batch_result = match wallet
            .swap(
                Some(Amount::from(batch_amount)),
                SplitTarget::default(),
                proof_batch.to_vec(),
                None,
                false,
            )
            .await
        {
            Ok(result) => result,
            Err(e) => {
                log::warn!(
                    "Swap failed on batch {}/{}, syncing proof states with mint: {}",
                    batch_idx + 1,
                    total_batches,
                    e
                );
                match wallet.check_proofs_spent(proof_batch.to_vec()).await {
                    Ok(states) => {
                        let has_spent = states.iter().any(|s| s.state == cdk::nuts::State::Spent);
                        for (proof, state) in proof_batch.iter().zip(states.iter()) {
                            if state.state == cdk::nuts::State::Spent {
                                log::info!(
                                    "Proof (amount={}, keyset={}) was spent by mint despite error",
                                    proof.amount,
                                    &proof.keyset_id.to_string()[..8]
                                );
                            }
                        }
                        if has_spent {
                            if let Err(sync_err) =
                                crate::stores::cashu_cdk_bridge::sync_wallet_state().await
                            {
                                log::warn!("Failed to sync after spent proofs: {}", sync_err);
                            }
                            super::signals::update_wallet_balances();
                        }
                    }
                    Err(sync_err) => {
                        log::warn!("NUT-07 check failed: {}", sync_err);
                    }
                }
                if batch_idx > 0 {
                    log::warn!(
                        "Partial swap succeeded ({} batches); syncing wallet state",
                        batch_idx
                    );
                    if let Err(sync_err) =
                        crate::stores::cashu_cdk_bridge::sync_wallet_state().await
                    {
                        log::warn!("Failed to sync after partial swap: {}", sync_err);
                    }
                    super::signals::update_wallet_balances();
                }
                return Err(format!(
                    "Swap failed on batch {}/{}: {}",
                    batch_idx + 1,
                    total_batches,
                    e,
                ));
            }
        };
        if let Some(proofs) = batch_result {
            {
                use cdk_common::database::WalletDatabase;
                use std::str::FromStr;
                let currency_unit = cdk::nuts::CurrencyUnit::from_str(&unit_str)
                    .unwrap_or_else(|_| cdk::nuts::CurrencyUnit::Custom(unit_str.clone()));
                let proof_infos: Vec<cdk::types::ProofInfo> = proofs
                    .iter()
                    .enumerate()
                    .map(|(i, p)| {
                        cdk::types::ProofInfo::new(
                            p.clone(),
                            mint_url_parsed.clone(),
                            cdk::nuts::State::Unspent,
                            currency_unit.clone(),
                        )
                        .map_err(|e| {
                            format!(
                                "ProofInfo conversion failed in batch {}, proof {}: {}",
                                batch_idx + 1,
                                i,
                                e,
                            )
                        })
                    })
                    .collect::<Result<Vec<_>, String>>()?;
                if !proof_infos.is_empty() {
                    let consumed_ys: Vec<cdk::nuts::PublicKey> =
                        proof_batch.iter().filter_map(|p| p.y().ok()).collect();
                    if let Err(e) = localstore.update_proofs(proof_infos, consumed_ys).await {
                        log::error!(
                            "CRITICAL: Batch {}/{} persistence failed AFTER successful swap: {}",
                            batch_idx + 1,
                            total_batches,
                            e
                        );
                        persistence_failures.push((batch_idx + 1, e.to_string()));
                    } else {
                        log::info!(
                            "Batch {}/{} persisted {} proofs",
                            batch_idx + 1,
                            total_batches,
                            proofs.len()
                        );
                    }
                }
            }
            new_proofs.extend(proofs);
        }
    }
    let mut emergency_event_id: Option<String> = None;
    if !persistence_failures.is_empty() {
        log::warn!(
            "Consolidation had {} persistence failures. Updating in-memory state.",
            persistence_failures.len()
        );
        let emergency_proof_data: Vec<ProofData> =
            new_proofs.iter().map(cdk_proof_to_proof_data).collect();
        let temp_emergency_id = format!("recovery_{}", uuid::Uuid::new_v4());
        emergency_event_id = Some(temp_emergency_id.clone());
        let emergency_token = TokenData {
            event_id: temp_emergency_id.clone(),
            mint: mint_url.clone(),
            unit: unit_str.clone(),
            proofs: emergency_proof_data.clone(),
            created_at: now_secs(),
        };
        if let Err(e) =
            super::signals::atomic_token_replace(vec![emergency_token], &event_ids_to_delete)
        {
            log::error!("Emergency WALLET_TOKENS update failed: {}", e);
        } else {
            super::signals::update_wallet_balances();
            super::proofs::rebuild_proof_event_map();
            log::info!(
                "Emergency recovery: {} proofs now visible in UI",
                new_proofs.len()
            );
        }
        #[cfg(feature = "web")]
        {
            use cdk_common::database::WalletDatabase;
            use std::str::FromStr;
            let retry_proofs = new_proofs.clone();
            let retry_mint_url = mint_url_parsed.clone();
            let retry_unit_str = unit_str.clone();
            dioxus::prelude::spawn(async move {
                for (attempt, base_delay_ms) in PERSISTENCE_RETRY_DELAYS_MS.iter().enumerate() {
                    let jitter =
                        (js_sys::Math::random() * PERSISTENCE_RETRY_JITTER_MS as f64) as u32;
                    let delay =
                        base_delay_ms.saturating_sub(PERSISTENCE_RETRY_JITTER_MS / 2) + jitter;
                    crate::platform::timer::sleep_ms(delay).await;
                    let retry_localstore = match SHARED_LOCALSTORE.read().as_ref() {
                        Some(store) => store.clone(),
                        None => {
                            log::warn!("Background retry {}: localstore unavailable", attempt + 1);
                            continue;
                        }
                    };
                    let currency_unit = cdk::nuts::CurrencyUnit::from_str(&retry_unit_str)
                        .unwrap_or_else(|_| {
                            cdk::nuts::CurrencyUnit::Custom(retry_unit_str.clone())
                        });
                    let mut proof_infos: Vec<cdk::types::ProofInfo> =
                        Vec::with_capacity(retry_proofs.len());
                    for p in &retry_proofs {
                        match cdk::types::ProofInfo::new(
                            p.clone(),
                            retry_mint_url.clone(),
                            cdk::nuts::State::Unspent,
                            currency_unit.clone(),
                        ) {
                            Ok(info) => proof_infos.push(info),
                            Err(e) => {
                                log::error!(
                                    "Background retry {}: ProofInfo conversion failed: {} \
                                     (mint: {}, unit: {}, proof_amount: {})",
                                    attempt + 1,
                                    e,
                                    retry_mint_url,
                                    retry_unit_str,
                                    p.amount
                                );
                            }
                        }
                    }
                    if let Err(e) = retry_localstore.update_proofs(proof_infos, vec![]).await {
                        log::warn!("Background retry {} failed: {}", attempt + 1, e);
                    } else {
                        log::info!("Background retry {}: persistence succeeded", attempt + 1);
                        return;
                    }
                }
                log::error!(
                    "All background persistence retries failed. \
                     Proofs are in WALLET_TOKENS but not fully persisted. \
                     Run sync_wallet_state() to reconcile."
                );
            });
        }
    }
    super::signals::remove_in_flight_send_request(&tx_id);
    in_flight_guard.dismiss();
    if new_proofs.is_empty() {
        return Err("Swap returned no proofs".to_string());
    }
    let proofs_after = new_proofs.len();
    if persistence_failures.is_empty() {
        log::info!(
            "Consolidated to {} proofs (all batches persisted)",
            proofs_after
        );
    } else {
        log::info!(
            "Consolidated to {} proofs ({} batch failures; emergency recovery performed)",
            proofs_after,
            persistence_failures.len()
        );
    }
    let signer = crate::stores::signer::get_signer()
        .ok_or("No signer available")?
        .as_nostr_signer();
    let pubkey_str = auth_store::get_pubkey().ok_or("Not authenticated")?;
    let pubkey = PublicKey::parse(&pubkey_str).map_err(|e| format!("Invalid pubkey: {}", e))?;
    let client = nostr_client::NOSTR_CLIENT
        .read()
        .as_ref()
        .ok_or("Client not initialized")?
        .clone();
    let proof_data: Vec<ProofData> = new_proofs.iter().map(cdk_proof_to_proof_data).collect();
    let extended_proofs: Vec<ExtendedCashuProof> = proof_data
        .iter()
        .map(|p| ExtendedCashuProof::from(p.clone()))
        .collect();
    let token_event_data = ExtendedTokenEvent {
        mint: mint_url.clone(),
        unit: unit_str.clone(),
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
    let mut unsigned = crate::utils::nips::nip89::tag_event_builder(builder.clone()).build(pubkey);
    let pre_signed_event_id = unsigned.id().to_hex();
    let signed_event = unsigned
        .sign(&signer)
        .await
        .map_err(|e| format!("Failed to sign token event: {}", e))?;
    let mut publish_succeeded = false;
    let mut last_error = String::new();
    let mut retryable = true;
    let delays = [500u32, 1000, 2000];
    for (attempt, delay_ms) in std::iter::once(0).chain(delays.iter().copied()).enumerate() {
        if attempt > 0 {
            #[cfg(feature = "web")]
            {
                let jitter = (js_sys::Math::random() * 200.0) as u32;
                let effective_delay = delay_ms.saturating_sub(100) + jitter;
                crate::platform::timer::sleep_ms(effective_delay).await;
            }
            #[cfg(feature = "native")]
            {
                crate::platform::timer::sleep_ms(delay_ms).await;
            }
            log::info!("Retrying token event publish (attempt {})", attempt + 1);
        }
        match client.send_event(&signed_event).await {
            Ok(output) => {
                if !output.success.is_empty() {
                    log::info!(
                        "Published token event {} to {}/{} relays",
                        pre_signed_event_id,
                        output.success.len(),
                        output.success.len() + output.failed.len()
                    );
                    publish_succeeded = true;
                    break;
                } else {
                    last_error = format!("All {} relays failed", output.failed.len());
                    log::warn!("Publish attempt {} - all relays failed", attempt + 1);
                }
            }
            Err(e) => {
                last_error = e.to_string();
                let err_str = last_error.to_lowercase();
                if err_str.contains("banned")
                    || err_str.contains("invalid")
                    || err_str.contains("malformed")
                    || err_str.contains("too large")
                {
                    log::error!("Non-retryable error: {}", last_error);
                    retryable = false;
                    break;
                }
                log::warn!("Publish attempt {} failed: {}", attempt + 1, e);
            }
        }
    }
    let new_event_id = if publish_succeeded {
        pre_signed_event_id
    } else if retryable {
        log::error!(
            "All publish attempts failed, using pending ID for background retry: {}",
            last_error
        );
        let pending_id = format!("pending_{}", uuid::Uuid::new_v4());
        super::events::queue_token_event_for_retry(builder, pending_id.clone(), mint_url.clone())
            .await;
        pending_id
    } else {
        log::error!("Non-retryable publish error: {}", last_error);
        let local_only_id = format!("local_{}", uuid::Uuid::new_v4());
        let local_token = TokenData {
            event_id: local_only_id.clone(),
            mint: mint_url.clone(),
            unit: unit_str.clone(),
            proofs: proof_data.clone(),
            created_at: now_secs(),
        };
        if let Err(e) =
            super::signals::atomic_token_replace(vec![local_token], &event_ids_to_delete)
        {
            log::error!("Failed to persist local-only token: {}", e);
        } else {
            super::proofs::register_proofs_in_event_map(&local_only_id, &proof_data);
            super::proofs::rebuild_proof_event_map();
            log::info!(
                "Persisted {} proofs as local-only token {} (publish_error={})",
                proof_data.len(),
                &local_only_id[..16.min(local_only_id.len())],
                &last_error[..50.min(last_error.len())]
            );
        }
        super::signals::update_wallet_balances();
        return Err(format!("Non-retryable publish error: {}", last_error));
    };
    if let Some(ref emergency_id) = emergency_event_id {
        let replacement_token = TokenData {
            event_id: new_event_id.clone(),
            mint: mint_url.clone(),
            unit: unit_str.clone(),
            proofs: proof_data.clone(),
            created_at: now_secs(),
        };
        if let Err(e) = super::signals::atomic_token_replace(
            vec![replacement_token],
            std::slice::from_ref(emergency_id),
        ) {
            log::error!(
                "Failed to replace emergency token with real event_id: {}",
                e
            );
        } else {
            super::proofs::rebuild_proof_event_map();
            log::info!(
                "Replaced emergency token {} with published event {}",
                &emergency_id[..16.min(emergency_id.len())],
                &new_event_id[..16.min(new_event_id.len())]
            );
        }
    } else {
        let new_token = TokenData {
            event_id: new_event_id.clone(),
            mint: mint_url.clone(),
            unit: unit_str.clone(),
            proofs: proof_data,
            created_at: now_secs(),
        };
        if let Err(e) = super::signals::atomic_token_replace(vec![new_token], &event_ids_to_delete)
        {
            log::error!("Failed to update WALLET_TOKENS: {}", e);
        } else {
            super::proofs::rebuild_proof_event_map();
        }
    }
    use nostr::nips::nip09::EventDeletionRequest;
    let mut deletion_request = EventDeletionRequest::new();
    for event_id_str in &event_ids_to_delete {
        if let Ok(event_id) = nostr_sdk::EventId::from_hex(event_id_str) {
            deletion_request = deletion_request.id(event_id);
        }
    }
    let delete_builder = nostr_sdk::EventBuilder::delete(deletion_request);
    match client
        .send_event_builder(crate::utils::nips::nip89::tag_event_builder(delete_builder))
        .await
    {
        Ok(output) if !output.success.is_empty() => {}
        Ok(_) => {
            log::warn!("Failed to publish deletion event: no relays accepted the event");
        }
        Err(e) => {
            log::warn!("Failed to publish deletion event: {}", e);
        }
    }
    super::signals::update_wallet_balances();
    if let Err(e) = crate::stores::cashu_cdk_bridge::sync_wallet_state().await {
        log::warn!(
            "Failed to sync MultiMintWallet state after consolidation: {}",
            e
        );
    }
    log::info!(
        "Consolidation complete: {} -> {} proofs",
        proofs_before,
        proofs_after
    );
    Ok(ConsolidationResult {
        proofs_before,
        proofs_after,
        fee_paid: 0,
    })
}
/// Consolidate proofs across all mints (parallel execution)
pub async fn consolidate_all_mints() -> Result<Vec<(String, ConsolidationResult)>, String> {
    let mints = get_mints();
    if mints.is_empty() {
        return Ok(vec![]);
    }
    let futures: Vec<_> = mints
        .iter()
        .map(|mint| {
            let mint = mint.clone();
            async move {
                let result = consolidate_proofs(mint.clone()).await;
                (mint, result)
            }
        })
        .collect();
    let results = futures::future::join_all(futures).await;
    let mut output = Vec::new();
    for (mint, result) in results {
        match result {
            Ok(r) => output.push((mint, r)),
            Err(e) => log::warn!("Failed to consolidate {}: {}", mint, e),
        }
    }
    Ok(output)
}
/// Discover mints via NIP-87 announcements and recommendations
pub async fn discover_mints() -> Result<Vec<DiscoveredMint>, String> {
    log::info!("Discovering mints via NIP-87");
    let client = nostr_client::NOSTR_CLIENT
        .read()
        .as_ref()
        .ok_or("Client not initialized")?
        .clone();
    nostr_client::ensure_relays_ready(&client).await;
    let mint_filter = Filter::new().kind(Kind::from(38172)).limit(50);
    let recommendation_filter = Filter::new()
        .kind(Kind::from(38000))
        .custom_tag(
            nostr_sdk::SingleLetterTag::lowercase(nostr_sdk::Alphabet::K),
            "38172",
        )
        .limit(100);
    let (mint_events, recommendation_events) = futures::join!(
        client.fetch_events(mint_filter, Duration::from_secs(10)),
        client.fetch_events(recommendation_filter, Duration::from_secs(10))
    );
    let mint_events = mint_events.map_err(|e| format!("Failed to fetch mint events: {}", e))?;
    let recommendation_events =
        recommendation_events.map_err(|e| format!("Failed to fetch recommendations: {}", e))?;
    log::info!(
        "Found {} mint announcements, {} recommendations",
        mint_events.len(),
        recommendation_events.len()
    );
    let mut mints_by_url: HashMap<String, DiscoveredMint> = HashMap::new();
    for event in mint_events.iter() {
        let url = event.tags.iter().find_map(|tag| {
            let values: Vec<&str> = tag.as_slice().iter().map(|s| s.as_str()).collect();
            if values.first() == Some(&"u") {
                values.get(1).map(|s| s.to_string())
            } else {
                None
            }
        });
        let Some(url) = url else { continue };
        let mint_pubkey = event.tags.iter().find_map(|tag| {
            let values: Vec<&str> = tag.as_slice().iter().map(|s| s.as_str()).collect();
            if values.first() == Some(&"d") {
                values.get(1).map(|s| s.to_string())
            } else {
                None
            }
        });
        let nuts = event.tags.iter().find_map(|tag| {
            let values: Vec<&str> = tag.as_slice().iter().map(|s| s.as_str()).collect();
            if values.first() == Some(&"nuts") {
                values.get(1).map(|s| s.to_string())
            } else {
                None
            }
        });
        let network = event.tags.iter().find_map(|tag| {
            let values: Vec<&str> = tag.as_slice().iter().map(|s| s.as_str()).collect();
            if values.first() == Some(&"n") {
                values.get(1).map(|s| s.to_string())
            } else {
                None
            }
        });
        let (name, description) = if !event.content.is_empty() {
            if let Ok(metadata) = serde_json::from_str::<serde_json::Value>(&event.content) {
                (
                    metadata
                        .get("name")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string()),
                    metadata
                        .get("about")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string()),
                )
            } else {
                (None, None)
            }
        } else {
            (None, None)
        };
        mints_by_url.insert(
            url.clone(),
            DiscoveredMint {
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
            },
        );
    }
    for event in recommendation_events.iter() {
        for tag in event.tags.iter() {
            let values: Vec<&str> = tag.as_slice().iter().map(|s| s.as_str()).collect();
            if values.first() == Some(&"u") {
                if let Some(url) = values.get(1) {
                    let is_cashu = values.get(2).map(|t| *t == "cashu").unwrap_or(true);
                    if is_cashu && url.starts_with("http") {
                        let recommender = event.pubkey.to_hex();
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
                            mints_by_url.insert(
                                url.to_string(),
                                DiscoveredMint {
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
                                },
                            );
                        }
                    }
                }
            }
        }
    }
    let mut mints: Vec<DiscoveredMint> = mints_by_url.into_values().collect();
    mints.sort_by(|a, b| b.recommendation_count.cmp(&a.recommendation_count));
    log::info!("Discovered {} unique mints", mints.len());
    Ok(mints)
}
