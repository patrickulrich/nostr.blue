//! CDK Bridge Module
//!
//! This module bridges CDK's MultiMintWallet with Dioxus reactive signals.
//! It provides synchronization between CDK's internal state and Dioxus GlobalSignals
//! for UI reactivity.

// Allow dead_code for planned features not yet wired to UI
#![allow(dead_code)]

use dioxus::prelude::*;
use std::sync::Arc;
use cdk::wallet::multi_mint_wallet::MultiMintWallet;
use cdk::nuts::CurrencyUnit;

use super::cashu::{
    WALLET_BALANCE, WALLET_TOKENS, WALLET_STATUS, WalletStatus,
    TokenData, ProofData, ProofState, WalletTokensStoreStoreExt,
    DleqData, PENDING_BY_MINT_SECRETS, WALLET_STATE,
};
use super::indexeddb_database::IndexedDbDatabase;

/// Global MultiMintWallet instance
/// Replaces the previous WALLET_CACHE HashMap approach
pub static MULTI_WALLET: GlobalSignal<Option<Arc<MultiMintWallet>>> = Signal::global(|| None);

// Re-export MPP types and functions for backward compatibility
// The implementation has been moved to cashu/mpp.rs
#[allow(unused_imports)]
pub use super::cashu::mpp::{
    MintBalance, MppQuoteContribution, MppQuoteInfo, MppMeltResult,
    get_balances_per_mint, calculate_mpp_split, create_mpp_melt_quotes,
    execute_mpp_melt, mint_supports_mpp,
};

/// Balance breakdown for UI display
#[derive(Clone, Debug, Default, PartialEq)]
pub struct WalletBalances {
    pub total: u64,
    pub available: u64,
    pub pending: u64,
}

/// Global signal for balance breakdown
pub static WALLET_BALANCES: GlobalSignal<WalletBalances> = Signal::global(|| WalletBalances::default());

/// Initialize the MultiMintWallet with the given seed and localstore
pub async fn init_multi_wallet(
    localstore: Arc<IndexedDbDatabase>,
    seed: [u8; 64],
) -> Result<Arc<MultiMintWallet>, String> {
    // Clear CDK-specific state (but NOT WALLET_STATUS - that would trigger init loop)
    // The clear_multi_wallet() function is for logout; here we only reset CDK internals
    *MULTI_WALLET.write() = None;
    *WALLET_BALANCES.write() = WalletBalances::default();

    log::info!("Initializing MultiMintWallet");

    // Create MultiMintWallet - it automatically loads existing mints from the database
    let multi_wallet = MultiMintWallet::new(
        localstore,
        seed,
        CurrencyUnit::Sat,
    ).await.map_err(|e| format!("Failed to create MultiMintWallet: {}", e))?;

    let wallet_arc = Arc::new(multi_wallet);

    // Store in global signal
    *MULTI_WALLET.write() = Some(wallet_arc.clone());

    log::info!("MultiMintWallet initialized successfully");

    Ok(wallet_arc)
}

/// Add a mint to the MultiMintWallet
pub async fn add_mint(mint_url: &str) -> Result<(), String> {
    let multi_wallet = MULTI_WALLET.read()
        .as_ref()
        .ok_or("MultiMintWallet not initialized")?
        .clone();

    let mint_url = mint_url.parse()
        .map_err(|e| format!("Invalid mint URL: {}", e))?;

    multi_wallet.add_mint(mint_url, None).await
        .map_err(|e| format!("Failed to add mint: {}", e))?;

    // Sync state after adding mint
    sync_wallet_state().await?;

    Ok(())
}

/// Remove a mint from the MultiMintWallet
pub async fn remove_mint(mint_url: &str) -> Result<(), String> {
    let multi_wallet = MULTI_WALLET.read()
        .as_ref()
        .ok_or("MultiMintWallet not initialized")?
        .clone();

    let mint_url = mint_url.parse()
        .map_err(|e| format!("Invalid mint URL: {}", e))?;

    multi_wallet.remove_mint(&mint_url).await;

    // Sync state after removing mint
    sync_wallet_state().await?;

    Ok(())
}

/// Get a wallet for a specific mint
pub async fn get_wallet(mint_url: &str) -> Result<cdk::Wallet, String> {
    let multi_wallet = MULTI_WALLET.read()
        .as_ref()
        .ok_or("MultiMintWallet not initialized")?
        .clone();

    let mint_url = mint_url.parse()
        .map_err(|e| format!("Invalid mint URL: {}", e))?;

    multi_wallet.get_wallet(&mint_url).await
        .ok_or_else(|| format!("Mint not found: {}", mint_url))
}

/// Check if a mint exists in the wallet
pub async fn has_mint(mint_url: &str) -> bool {
    let multi_wallet = match MULTI_WALLET.read().as_ref() {
        Some(w) => w.clone(),
        None => return false,
    };

    let mint_url = match mint_url.parse() {
        Ok(url) => url,
        Err(_) => return false,
    };

    multi_wallet.has_mint(&mint_url).await
}

/// Get total balance across all mints
#[allow(dead_code)]
pub async fn get_total_balance() -> Result<u64, String> {
    let multi_wallet = MULTI_WALLET.read()
        .as_ref()
        .ok_or("MultiMintWallet not initialized")?
        .clone();

    let balance = multi_wallet.total_balance().await
        .map_err(|e| format!("Failed to get balance: {}", e))?;

    Ok(u64::from(balance))
}

/// Sync CDK state to Dioxus signals
///
/// This should be called after any CDK operation that changes wallet state.
/// It updates WALLET_BALANCE, WALLET_TOKENS, and WALLET_BALANCES.
pub async fn sync_wallet_state() -> Result<(), String> {
    let multi_wallet = match MULTI_WALLET.read().as_ref() {
        Some(w) => w.clone(),
        None => {
            log::debug!("sync_wallet_state: MultiMintWallet not initialized");
            return Ok(());
        }
    };

    // Get balances per mint
    let balances = multi_wallet.get_balances().await
        .map_err(|e| format!("Failed to get balances: {}", e))?;

    // Calculate total balance with checked arithmetic
    let total: u64 = balances.values()
        .try_fold(0u64, |acc, amount| acc.checked_add(u64::from(*amount)))
        .ok_or("Balance overflow")?;

    // Update WALLET_BALANCE
    *WALLET_BALANCE.write() = total;

    // Get proofs per mint for token list
    let proofs_by_mint = multi_wallet.list_proofs().await
        .map_err(|e| format!("Failed to list proofs: {}", e))?;

    // Convert to TokenData format for WALLET_TOKENS
    let mut tokens: Vec<TokenData> = Vec::new();
    for (mint_url, proofs) in proofs_by_mint {
        if proofs.is_empty() {
            continue;
        }

        // Convert CDK proofs to our ProofData format
        // Note: list_proofs() returns all unspent proofs from CDK's perspective
        let proof_data: Vec<ProofData> = proofs.iter().map(|p| ProofData {
            id: p.keyset_id.to_string(),
            amount: u64::from(p.amount),
            secret: p.secret.to_string(),
            c: p.c.to_string(),
            witness: p.witness.as_ref().and_then(|w| {
                match serde_json::to_string(w) {
                    Ok(s) => Some(s),
                    Err(e) => {
                        log::warn!("Failed to serialize witness for proof {}: {}", p.keyset_id, e);
                        None
                    }
                }
            }),
            dleq: p.dleq.as_ref().map(|d| DleqData {
                e: d.e.to_string(),
                s: d.s.to_string(),
                r: d.r.to_string(),
            }),
            state: ProofState::Unspent,
            transaction_id: None,
            state_set_at: None,
        }).collect();

        tokens.push(TokenData {
            event_id: String::new(), // Will be populated from NIP-60 events
            mint: mint_url.to_string(),
            unit: "sat".to_string(),
            proofs: proof_data,
            created_at: 0,
        });
    }

    // Update WALLET_TOKENS - need to preserve event_ids from existing data
    let existing_tokens = WALLET_TOKENS.read().data().read().clone();

    // Merge - keep event_ids from existing, update proofs from CDK
    for token in &mut tokens {
        if let Some(existing) = existing_tokens.iter().find(|t| t.mint == token.mint) {
            token.event_id = existing.event_id.clone();
            token.created_at = existing.created_at;
        }
    }

    *WALLET_TOKENS.read().data().write() = tokens;

    // Calculate pending balance from proof state flags and mint-reported pending
    let pending: u64 = {
        let store = WALLET_TOKENS.read();
        let data = store.data();
        let tokens = data.read();

        // Get proofs that are pending at mint level
        let pending_at_mint = PENDING_BY_MINT_SECRETS.read();

        tokens.iter()
            .flat_map(|t| &t.proofs)
            .filter(|p| {
                // Proof is pending if:
                // 1. Local state is pending (user initiated operation)
                // 2. Mint reports it as pending (lightning payment in-flight)
                p.state.is_pending() || pending_at_mint.contains_key(&p.secret)
            })
            .map(|p| p.amount)
            .try_fold(0u64, |acc, amount| acc.checked_add(amount))
            .unwrap_or(0)
    };

    // Update balance breakdown
    let available = total.saturating_sub(pending);
    *WALLET_BALANCES.write() = WalletBalances {
        total,
        available,
        pending,
    };

    // Update status to Ready
    *WALLET_STATUS.write() = WalletStatus::Ready;

    log::debug!("Synced wallet state: {} sats total", total);

    Ok(())
}

/// Sync balance only (lighter weight than full sync)
#[allow(dead_code)]
pub async fn sync_balance_only() -> Result<u64, String> {
    let total = get_total_balance().await?;
    *WALLET_BALANCE.write() = total;
    Ok(total)
}

/// Clear the MultiMintWallet and all related UI signals (for logout)
#[allow(dead_code)]
pub fn clear_multi_wallet() {
    // Clear CDK wallet (triggers Drop which zeroizes the seed)
    *MULTI_WALLET.write() = None;

    // Clear balance signals
    *WALLET_BALANCES.write() = WalletBalances::default();
    *WALLET_BALANCE.write() = 0;

    // Clear token data
    *WALLET_TOKENS.read().data().write() = Vec::new();

    // Reset status to uninitialized
    *WALLET_STATUS.write() = WalletStatus::Uninitialized;

    // Clear wallet state (mints, etc.)
    *WALLET_STATE.write() = None;

    log::info!("Cleared MultiMintWallet and all wallet signals");
}

/// Check if MultiMintWallet is initialized
pub fn is_initialized() -> bool {
    MULTI_WALLET.read().is_some()
}

/// Get all mint URLs from the wallet
#[allow(dead_code)]
pub async fn get_mint_urls() -> Result<Vec<String>, String> {
    let multi_wallet = MULTI_WALLET.read()
        .as_ref()
        .ok_or("MultiMintWallet not initialized")?
        .clone();

    let wallets = multi_wallet.get_wallets().await;
    Ok(wallets.iter().map(|w| w.mint_url.to_string()).collect())
}



