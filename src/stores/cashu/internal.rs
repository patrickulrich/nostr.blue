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
#![allow(dead_code)]
use super::proofs::proof_data_to_cdk_proof;
use super::signals::{SHARED_LOCALSTORE, WALLET_STATE, WALLET_TOKENS};
use super::types::{ProofData, WalletTokensStoreStoreExt};
use super::utils::{mint_matches, normalize_mint_url};
use crate::stores::{auth_store, cashu_cdk_bridge};
use cdk::nuts::{CurrencyUnit, State};
use cdk::Wallet;
use cdk_common::database::WalletDatabase;
use dioxus::prelude::*;
use std::sync::Arc;
/// Get or create the shared IndexedDB localstore
///
/// Uses double-checked locking to prevent race conditions where multiple
/// concurrent calls could create duplicate IndexedDB instances.
pub(crate) async fn get_shared_localstore(
) -> Result<Arc<crate::stores::indexeddb_database::IndexedDbDatabase>, String> {
    if let Some(store) = SHARED_LOCALSTORE.read().clone() {
        return Ok(store);
    }
    let mut store_guard = SHARED_LOCALSTORE.write();
    if let Some(store) = store_guard.clone() {
        return Ok(store);
    }
    let localstore = Arc::new(
        crate::stores::indexeddb_database::IndexedDbDatabase::new()
            .await
            .map_err(|e| format!("Failed to create IndexedDB: {}", e))?,
    );
    *store_guard = Some(localstore.clone());
    log::info!("Created shared IndexedDB localstore");
    Ok(localstore)
}
/// Get or create a wallet for a mint via MultiMintWallet
///
/// Uses CDK's MultiMintWallet for all wallet management.
/// If the mint isn't already in the wallet, it will be added.
pub(crate) async fn get_or_create_wallet(mint_url: &str) -> Result<Arc<Wallet>, String> {
    if !cashu_cdk_bridge::is_initialized() {
        return Err("Wallet not initialized. Please set up wallet first.".to_string());
    }
    if let Ok(wallet) = cashu_cdk_bridge::get_wallet(mint_url).await {
        log::debug!("Using wallet from MultiMintWallet for {}", mint_url);
        return Ok(Arc::new(wallet));
    }
    log::info!("Adding mint {} to MultiMintWallet", mint_url);
    cashu_cdk_bridge::add_mint(mint_url).await?;
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
///
/// Duplicate Check: Proofs that already exist in CDK database are skipped
/// to prevent balance corruption from duplicate entries.
///
/// TODO(multi-unit): Currently hardcodes CurrencyUnit::Sat. To support multi-unit
/// tokens (CDK pattern), add a `unit: CurrencyUnit` parameter and derive unit from
/// proof keysets or pass explicitly from callers.
pub(crate) async fn create_ephemeral_wallet(
    mint_url: &str,
    proofs: Vec<cdk::nuts::Proof>,
) -> Result<Arc<Wallet>, String> {
    use cdk::mint_url::MintUrl as CdkMintUrl;
    use cdk::types::ProofInfo;
    let wallet = get_or_create_wallet(mint_url).await?;
    if !proofs.is_empty() {
        for (idx, proof) in proofs.iter().enumerate() {
            validate_proof_format(proof)
                .map_err(|e| format!("Invalid proof at index {}: {}", idx, e))?;
        }
        let mint_url_parsed: CdkMintUrl = mint_url
            .parse()
            .map_err(|e| format!("Invalid mint URL: {}", e))?;
        let localstore = get_shared_localstore().await?;
        let existing_proofs = localstore
            .get_proofs(Some(mint_url_parsed.clone()), None, None, None)
            .await
            .map_err(|e| format!("Failed to query existing proofs: {}", e))?;
        let existing_secrets: std::collections::HashSet<String> = existing_proofs
            .iter()
            .map(|p| p.proof.secret.to_string())
            .collect();
        let input_len = proofs.len();
        let mut seen_secrets = std::collections::HashSet::new();
        let new_proofs: Vec<_> = proofs
            .into_iter()
            .filter(|p| {
                let secret = p.secret.to_string();
                if existing_secrets.contains(&secret) {
                    return false;
                }
                seen_secrets.insert(secret)
            })
            .collect();
        if new_proofs.is_empty() {
            log::debug!(
                "All {} proofs already exist in CDK database, skipping injection",
                input_len
            );
        } else {
            let skipped_count = input_len.saturating_sub(new_proofs.len());
            if skipped_count > 0 {
                log::debug!(
                    "Skipping {} duplicate proofs, injecting {} new proofs",
                    skipped_count,
                    new_proofs.len()
                );
            }
            let proof_infos: Vec<_> = new_proofs
                .into_iter()
                .map(|p| {
                    ProofInfo::new(
                        p,
                        mint_url_parsed.clone(),
                        State::Unspent,
                        CurrencyUnit::Sat,
                    )
                })
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| format!("Failed to create proof info: {}", e))?;
            localstore
                .update_proofs(proof_infos, vec![])
                .await
                .map_err(|e| format!("Failed to inject proofs: {}", e))?;
        }
    }
    Ok(wallet)
}
/// Derive deterministic wallet seed from Nostr private key or signer
#[cfg(feature = "web")]
pub(crate) async fn derive_wallet_seed() -> Result<[u8; 64], String> {
    use sha2::{Digest, Sha256};
    if let Some(keys) = auth_store::get_keys() {
        log::info!("Deriving seed from private key (nsec login)");
        let secret_key = keys.secret_key();
        let mut hasher = Sha256::new();
        hasher.update(secret_key.to_secret_bytes());
        hasher.update(b"cashu-wallet-seed-v1");
        let hash = hasher.finalize();
        let mut seed = [0u8; 64];
        seed[..32].copy_from_slice(&hash);
        let mut hasher = Sha256::new();
        hasher.update(hash);
        hasher.update(b"cashu-wallet-seed-v1-ext");
        let hash2 = hasher.finalize();
        seed[32..].copy_from_slice(&hash2);
        return Ok(seed);
    }
    log::info!("Using browser extension - deriving seed from NIP-07 signature");
    let signer = crate::stores::signer::get_signer()
        .ok_or("No signer available for extension user")?
        .as_nostr_signer();
    let challenge_content = "nostr.blue Cashu Wallet Seed Derivation - Sign this message to derive your wallet encryption key";
    use nostr_sdk::{EventBuilder, Timestamp};
    let challenge_event = EventBuilder::text_note(challenge_content)
        .custom_created_at(Timestamp::from(1700000000))
        .sign(&signer)
        .await
        .map_err(|e| format!("Failed to sign challenge for wallet derivation: {}", e))?;
    let sig_bytes = challenge_event.sig.serialize();
    let mut seed = [0u8; 64];
    seed.copy_from_slice(&sig_bytes);
    log::info!("Wallet seed derived from NIP-07 signature");
    Ok(seed)
}
#[cfg(not(feature = "web"))]
pub(crate) async fn derive_wallet_seed() -> Result<[u8; 64], String> {
    Err("Seed derivation requires the \"web\" feature (NIP-07 browser extension)".to_string())
}
pub(crate) use super::errors::{
    is_insufficient_funds_error_string, is_token_already_spent_error, is_token_spent_error_string,
};
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
/// Validate proof format before injection into CDK
///
/// CDK best practice: Validate proof format before injection to prevent
/// malformed proofs from causing failures during multi-step operations
/// (which could leave proofs in Reserved state).
fn validate_proof_format(proof: &cdk::nuts::Proof) -> Result<(), String> {
    let keyset_str = proof.keyset_id.to_string();
    if keyset_str.len() != 16 {
        return Err(format!(
            "Invalid keyset_id length: expected 16, got {}",
            keyset_str.len()
        ));
    }
    let secret_str = proof.secret.to_string();
    if secret_str.is_empty() {
        return Err("Empty proof secret".to_string());
    }
    let c_hex = proof.c.to_hex();
    if c_hex.len() != 66 {
        return Err(format!(
            "Invalid C point length: expected 66, got {}",
            c_hex.len()
        ));
    }
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
    log::debug!("Validating {} proofs with mint: {}", proofs.len(), mint_url);
    let wallet = create_ephemeral_wallet(mint_url, proofs.clone()).await?;
    let states = wallet
        .check_proofs_spent(proofs.clone())
        .await
        .map_err(|e| format!("Failed to check proof states: {}", e))?;
    let spent_count = states.iter().filter(|s| s.state == State::Spent).count();
    if spent_count > 0 {
        log::warn!(
            "Found {} spent proofs in wallet, cleaning up...",
            spent_count
        );
        cleanup_spent_proofs_internal(mint_url).await?;
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
    let (cdk_proofs, event_ids_to_delete, all_mint_proofs) = {
        let store = WALLET_TOKENS.read();
        let data = store.data();
        let tokens = data.read();
        let mint_tokens: Vec<_> = tokens
            .iter()
            .filter(|t| mint_matches(&t.mint, mint_url))
            .collect();
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
            all_proofs.iter().map(proof_data_to_cdk_proof).collect();
        (cdk_proofs?, event_ids, all_proofs)
    };
    let wallet = create_ephemeral_wallet(mint_url, vec![]).await?;
    let states = wallet
        .check_proofs_spent(cdk_proofs.clone())
        .await
        .map_err(|e| format!("Failed to check proof states: {}", e))?;
    let mut unavailable_secrets = std::collections::HashSet::new();
    let mut unavailable_amount = 0u64;
    for ((state, proof), local_proof) in states
        .iter()
        .zip(cdk_proofs.iter())
        .zip(all_mint_proofs.iter())
    {
        match state.state {
            State::Spent | State::Pending => {
                unavailable_secrets.insert(proof.secret.to_string());
                unavailable_amount += u64::from(proof.amount);
            }
            State::Reserved => {
                if local_proof.transaction_id.is_none() {
                    log::debug!(
                        "Removing orphaned reserved proof (no transaction_id): {} sats",
                        u64::from(proof.amount)
                    );
                    unavailable_secrets.insert(proof.secret.to_string());
                    unavailable_amount += u64::from(proof.amount);
                } else {
                    log::debug!(
                        "Preserving reserved proof with active transaction: {} sats",
                        u64::from(proof.amount)
                    );
                }
            }
            _ => {}
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
    let available_proofs: Vec<ProofData> = all_mint_proofs
        .into_iter()
        .filter(|p| !unavailable_secrets.contains(&p.secret))
        .collect();
    let signer = crate::stores::signer::get_signer()
        .ok_or("No signer available")?
        .as_nostr_signer();
    let pubkey_str = auth_store::get_pubkey().ok_or("Not authenticated")?;
    let pubkey = PublicKey::parse(&pubkey_str).map_err(|e| format!("Invalid pubkey: {}", e))?;
    let client = crate::stores::nostr_client::NOSTR_CLIENT
        .read()
        .as_ref()
        .ok_or("Client not initialized")?
        .clone();
    let mut new_event_id: Option<String> = None;
    let synthetic_pending_id = (!available_proofs.is_empty())
        .then(|| format!("local_pending_{}", chrono::Utc::now().timestamp_millis()));
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
        match client
            .send_event_builder(crate::utils::nips::nip89::tag_event_builder(
                builder.clone(),
            ))
            .await
        {
            Ok(event_output) if !event_output.success.is_empty() => {
                new_event_id = Some(event_output.id().to_hex());
                log::info!(
                    "Published cleanup token event: {}",
                    new_event_id.as_ref().unwrap()
                );
            }
            Ok(_) => {
                log::warn!("No relays accepted cleanup token event, queuing for retry");
                super::events::queue_event_for_retry(
                    builder,
                    super::types::PendingEventType::TokenEvent,
                    synthetic_pending_id.clone(),
                    Some(mint_url.to_string()),
                )
                .await;
            }
            Err(e) => {
                log::warn!(
                    "Failed to publish cleanup token event, queuing for retry: {}",
                    e
                );
                super::events::queue_event_for_retry(
                    builder,
                    super::types::PendingEventType::TokenEvent,
                    synthetic_pending_id.clone(),
                    Some(mint_url.to_string()),
                )
                .await;
            }
        }
    }
    let allow_deletion = available_proofs.is_empty() || new_event_id.is_some();
    if allow_deletion && !event_ids_to_delete.is_empty() {
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
            match client
                .send_event_builder(crate::utils::nips::nip89::tag_event_builder(
                    deletion_builder.clone(),
                ))
                .await
            {
                Ok(event_output) if !event_output.success.is_empty() => {
                    log::info!(
                        "Published deletion event for {} token events",
                        valid_event_ids.len()
                    );
                }
                Ok(_) => {
                    log::warn!("No relays accepted cleanup deletion event, queuing for retry");
                    super::events::queue_event_for_retry(
                        deletion_builder,
                        super::types::PendingEventType::DeletionEvent,
                        None,
                        None,
                    )
                    .await;
                }
                Err(e) => {
                    log::warn!("Failed to publish deletion event, queuing for retry: {}", e);
                    super::events::queue_event_for_retry(
                        deletion_builder,
                        super::types::PendingEventType::DeletionEvent,
                        None,
                        None,
                    )
                    .await;
                }
            }
        }
    }
    let available_proofs_is_empty = available_proofs.is_empty();
    let tokens_to_add = if let Some(ref event_id) = new_event_id {
        vec![super::types::TokenData {
            event_id: event_id.clone(),
            mint: mint_url.to_string(),
            unit: "sat".to_string(),
            proofs: available_proofs,
            created_at: chrono::Utc::now().timestamp() as u64,
        }]
    } else if !available_proofs.is_empty() {
        let synthetic_id = synthetic_pending_id
            .unwrap_or_else(|| format!("local_pending_{}", chrono::Utc::now().timestamp_millis()));
        log::warn!("Publish failed, using synthetic event_id: {}", synthetic_id);
        vec![super::types::TokenData {
            event_id: synthetic_id,
            mint: mint_url.to_string(),
            unit: "sat".to_string(),
            proofs: available_proofs,
            created_at: chrono::Utc::now().timestamp() as u64,
        }]
    } else {
        vec![]
    };
    if new_event_id.is_some() || available_proofs_is_empty || !tokens_to_add.is_empty() {
        if let Err(e) = super::signals::atomic_token_replace(tokens_to_add, &event_ids_to_delete) {
            log::error!("Failed atomic token replacement during cleanup: {}", e);
        } else {
            super::proofs::rebuild_proof_event_map();
        }
    }
    log::info!(
        "Cleanup complete: removed {} proofs worth {} sats",
        unavailable_count,
        unavailable_amount
    );
    Ok((unavailable_count, unavailable_amount))
}
/// Collect all P2PK signing keys that can be used to unlock tokens
///
/// Includes the wallet's private key and the Nostr identity key.
pub(crate) async fn collect_p2pk_signing_keys() -> Vec<cdk::nuts::SecretKey> {
    let mut keys = Vec::new();
    if let Some(state) = WALLET_STATE.read().as_ref() {
        if let Some(ref privkey) = state.privkey {
            match cdk::nuts::SecretKey::from_hex(privkey) {
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
    let mut seen_keys = std::collections::HashSet::new();
    keys.retain(|key| {
        let key_hex = key.to_string();
        seen_keys.insert(key_hex)
    });
    for key in &keys {
        let xonly = key.public_key().x_only_public_key();
        log::debug!(
            "P2PK signing key x-only pubkey: {}",
            hex::encode(xonly.serialize())
        );
    }
    log::debug!(
        "Collected {} unique P2PK signing keys for token receive",
        keys.len()
    );
    keys
}
/// Convert a Nostr public key to a CDK public key for P2PK spending conditions
///
/// Nostr uses 32-byte x-only public keys (BIP340), while CDK/Cashu uses 33-byte
/// compressed secp256k1 public keys. Per BIP340 convention, x-only pubkeys
/// implicitly represent the point with even Y parity, so we try 02 prefix first.
/// Falls back to 03 (odd parity) if even fails validation.
pub(crate) fn nostr_pubkey_to_cdk_pubkey(
    nostr_pubkey: &str,
) -> Result<cdk::nuts::PublicKey, String> {
    let parsed = nostr_sdk::PublicKey::parse(nostr_pubkey)
        .map_err(|e| format!("Invalid Nostr pubkey: {}", e))?;
    let x_only_hex = parsed.to_hex();
    let compressed_even = format!("02{}", x_only_hex);
    if let Ok(pk) = cdk::nuts::PublicKey::from_hex(&compressed_even) {
        return Ok(pk);
    }
    let compressed_odd = format!("03{}", x_only_hex);
    cdk::nuts::PublicKey::from_hex(&compressed_odd)
        .map_err(|e| format!("Failed to create CDK pubkey from either parity: {}", e))
}
/// Initialize the MultiMintWallet with all mints from the wallet event
pub(crate) async fn init_multi_mint_wallet(mints: &[nostr_sdk::Url]) -> Result<(), String> {
    log::info!("Initializing MultiMintWallet with {} mints", mints.len());
    let localstore = get_shared_localstore().await?;
    let seed = derive_wallet_seed().await?;
    let multi_wallet = cashu_cdk_bridge::init_multi_wallet(localstore, seed).await?;
    for mint_url in mints {
        let mint_str = mint_url.to_string();
        if !cashu_cdk_bridge::has_mint(&mint_str).await {
            if let Err(e) = cashu_cdk_bridge::add_mint(&mint_str).await {
                log::warn!("Failed to add mint {}: {}", mint_str, e);
            }
        }
    }
    log::info!(
        "MultiMintWallet initialized with {} wallets",
        multi_wallet.get_wallets().await.len()
    );
    Ok(())
}
/// Inject NIP-60 proofs into CDK's database
///
/// This function bridges the gap between NIP-60 (proofs stored in Nostr events)
/// and CDK (proofs stored in IndexedDB). After loading proofs from NIP-60 via
/// `fetch_tokens()`, we need to inject them into CDK's database so that
/// `sync_wallet_state()` and other CDK operations can find them.
///
/// Architecture Note:
/// - NIP-60 is the source of truth for cross-device wallet sync
/// - CDK is used for local operations (mint, melt, swap, NUT-07 checks)
/// - This injection bridges these two systems
pub(crate) async fn inject_nip60_proofs_to_cdk() -> Result<(), String> {
    use cdk::mint_url::MintUrl as CdkMintUrl;
    use cdk::types::ProofInfo;
    let localstore = get_shared_localstore().await?;
    let tokens = WALLET_TOKENS.read().data().read().clone();
    if tokens.is_empty() {
        log::debug!("No NIP-60 proofs to inject into CDK");
        return Ok(());
    }
    let mut total_proofs = 0;
    let mut failed_mints = Vec::new();
    for token in &tokens {
        let mint_url: CdkMintUrl = match token.mint.parse() {
            Ok(url) => url,
            Err(e) => {
                log::warn!("Invalid mint URL '{}': {}", token.mint, e);
                failed_mints.push(token.mint.clone());
                continue;
            }
        };
        let proof_infos: Vec<ProofInfo> = token
            .proofs
            .iter()
            .filter_map(|p| match proof_data_to_cdk_proof(p) {
                Ok(cdk_proof) => ProofInfo::new(
                    cdk_proof,
                    mint_url.clone(),
                    State::Unspent,
                    CurrencyUnit::Sat,
                )
                .ok(),
                Err(e) => {
                    log::warn!("Failed to convert proof: {}", e);
                    None
                }
            })
            .collect();
        if !proof_infos.is_empty() {
            total_proofs += proof_infos.len();
            if let Err(e) = localstore.update_proofs(proof_infos, vec![]).await {
                log::warn!("Failed to inject proofs for {}: {}", token.mint, e);
                failed_mints.push(token.mint.clone());
            }
        }
    }
    if total_proofs > 0 {
        log::info!("Injected {} NIP-60 proofs into CDK database", total_proofs);
    }
    if !failed_mints.is_empty() {
        log::warn!(
            "Failed to inject proofs for {} mints: {:?}",
            failed_mints.len(),
            failed_mints
        );
    }
    Ok(())
}
use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::sync::Mutex;
/// Per-mint recovery state - combines lock flag and pending queue atomically
/// CDK pattern: single lock per wallet, but we need per-mint for parallel recovery
#[derive(Default)]
struct MintRecoveryState {
    /// True if recovery is currently running for this mint
    in_recovery: bool,
    /// Proofs queued while recovery was in progress
    pending_proofs: Vec<cdk::nuts::Proof>,
}
/// Per-mint recovery coordination - mutex ensures atomic check-and-enqueue
/// Key is normalized mint URL
static MINT_RECOVERY_STATE: Lazy<Mutex<HashMap<String, MintRecoveryState>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));
/// CDK pattern: batch proof syncs to avoid overwhelming mint API
const BATCH_PROOF_SIZE: usize = 100;
/// Execute an operation with automatic proof state recovery on failure
///
/// This wraps any operation that uses proofs and automatically syncs proof states
/// with the mint if the operation fails. This prevents proof "limbo" where proofs
/// might be spent at the mint but our local state doesn't reflect this.
///
/// Based on CDK's `try_proof_operation_or_reclaim` pattern.
///
/// Uses per-mint locking to allow recovery for different mints to proceed in parallel
/// while preventing concurrent recovery for the same mint.
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
                "Operation failed with '{}', attempting to recover {} proofs for {}",
                err,
                proofs.len(),
                mint_url
            );
            let normalized_mint = normalize_mint_url(mint_url);
            let proofs_to_recover = {
                let mut states = MINT_RECOVERY_STATE.lock().unwrap();
                let state = states.entry(normalized_mint.clone()).or_default();
                if state.in_recovery {
                    log::debug!(
                        "Recovery in progress for {}, queuing {} proofs",
                        mint_url,
                        proofs.len()
                    );
                    state.pending_proofs.extend(proofs);
                    None
                } else {
                    state.in_recovery = true;
                    Some(proofs)
                }
            };
            if let Some(proofs) = proofs_to_recover {
                log::info!("Syncing proof states with mint {} after failure", mint_url);
                for chunk in proofs.chunks(BATCH_PROOF_SIZE) {
                    if let Err(sync_err) =
                        sync_proofs_with_mint_after_failure(mint_url, chunk).await
                    {
                        log::warn!("Failed to sync proof states for {}: {}", mint_url, sync_err);
                    }
                }
                loop {
                    let (queued_proofs, should_exit) = {
                        let mut states = MINT_RECOVERY_STATE.lock().unwrap();
                        let state = states.entry(normalized_mint.clone()).or_default();
                        let proofs = std::mem::take(&mut state.pending_proofs);
                        if proofs.is_empty() {
                            state.in_recovery = false;
                            states.remove(&normalized_mint);
                            (proofs, true)
                        } else {
                            (proofs, false)
                        }
                    };
                    if should_exit {
                        break;
                    }
                    log::info!(
                        "Processing {} queued proofs for {}",
                        queued_proofs.len(),
                        mint_url
                    );
                    for chunk in queued_proofs.chunks(BATCH_PROOF_SIZE) {
                        if let Err(e) = sync_proofs_with_mint_after_failure(mint_url, chunk).await {
                            log::warn!("Failed to sync queued proofs for {}: {}", mint_url, e);
                        }
                    }
                }
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
    let wallet = cashu_cdk_bridge::get_wallet(mint_url)
        .await
        .map_err(|e| format!("Failed to get wallet: {}", e))?;
    let states = wallet
        .check_proofs_spent(proofs.to_vec())
        .await
        .map_err(|e| format!("Failed to check proof states: {}", e))?;
    let mut spent_count = 0;
    let mut pending_count = 0;
    for (state, proof) in states.iter().zip(proofs.iter()) {
        match state.state {
            State::Spent => {
                spent_count += 1;
                super::proofs::move_proofs_to_spent(&[proof.secret.to_string()]);
            }
            State::Pending => {
                pending_count += 1;
                super::proofs::register_proofs_pending_at_mint(&[proof.secret.to_string()]);
            }
            State::Unspent => {
                super::proofs::revert_proofs_to_spendable(&[proof.secret.to_string()]);
            }
            State::Reserved => {
                pending_count += 1;
                super::proofs::register_proofs_pending_at_mint(&[proof.secret.to_string()]);
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
        let _ = cashu_cdk_bridge::sync_wallet_state().await;
    }
    Ok(())
}
/// Execute a wallet swap operation with automatic recovery
///
/// Convenience wrapper for swap/send operations that handles proof recovery.
/// Uses per-mint locking to allow recovery for different mints to proceed in parallel.
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
            log::error!("Swap failed for {}: {}", mint_url, err_str);
            let normalized_mint = normalize_mint_url(mint_url);
            let proofs_to_recover = {
                let mut states = MINT_RECOVERY_STATE.lock().unwrap();
                let state = states.entry(normalized_mint.clone()).or_default();
                if state.in_recovery {
                    log::debug!(
                        "Recovery in progress for {}, queuing {} proofs",
                        mint_url,
                        proofs_clone.len()
                    );
                    state.pending_proofs.extend(proofs_clone);
                    None
                } else {
                    state.in_recovery = true;
                    Some(proofs_clone)
                }
            };
            if let Some(proofs) = proofs_to_recover {
                for chunk in proofs.chunks(BATCH_PROOF_SIZE) {
                    match sync_proofs_with_mint_after_failure(mint_url, chunk).await {
                        Ok(_) => {
                            log::debug!("Recovery batch completed for mint {}", mint_url)
                        }
                        Err(e) => {
                            log::error!("Recovery failed for mint {}: {}", mint_url, e)
                        }
                    }
                }
                loop {
                    let (queued_proofs, should_exit) = {
                        let mut states = MINT_RECOVERY_STATE.lock().unwrap();
                        let state = states.entry(normalized_mint.clone()).or_default();
                        let proofs = std::mem::take(&mut state.pending_proofs);
                        if proofs.is_empty() {
                            state.in_recovery = false;
                            states.remove(&normalized_mint);
                            (proofs, true)
                        } else {
                            (proofs, false)
                        }
                    };
                    if should_exit {
                        break;
                    }
                    log::info!(
                        "Processing {} queued proofs for {}",
                        queued_proofs.len(),
                        mint_url
                    );
                    for chunk in queued_proofs.chunks(BATCH_PROOF_SIZE) {
                        if let Err(e) = sync_proofs_with_mint_after_failure(mint_url, chunk).await {
                            log::warn!("Failed to sync queued proofs for {}: {}", mint_url, e);
                        }
                    }
                }
            }
            Err(err_str)
        }
    }
}
