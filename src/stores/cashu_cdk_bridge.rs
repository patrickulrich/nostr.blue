//! CDK Bridge Module
//!
//! This module bridges CDK's WalletRepository with Dioxus reactive signals.
//! It provides synchronization between CDK's internal state and Dioxus GlobalSignals
//! for UI reactivity.
#![allow(dead_code)]
use super::cashu::{
    DleqData, ProofData, ProofState, TokenData, WalletStatus, WalletTokensStoreStoreExt,
    WALLET_STATE, WALLET_STATUS, WALLET_TOKENS,
};
use cdk::nuts::CurrencyUnit;
use cdk::wallet::{WalletRepository, WalletRepositoryBuilder};
use cdk_common::wallet::WalletKey;
use dioxus::prelude::*;
use std::collections::BTreeMap;
use std::sync::Arc;
/// Global WalletRepository instance
/// Replaces the previous WALLET_CACHE HashMap approach
pub static MULTI_WALLET: GlobalSignal<Option<Arc<WalletRepository>>> = Signal::global(|| None);
#[allow(unused_imports)]
pub use super::cashu::mpp::{
    calculate_mpp_split, create_mpp_melt_quotes, execute_mpp_melt, get_balances_per_mint,
    mint_supports_mpp, MintBalance, MppMeltResult, MppQuoteContribution, MppQuoteInfo,
};
/// Balance breakdown for UI display
#[derive(Clone, Debug, Default, PartialEq)]
pub struct WalletBalances {
    pub total: u64,
    pub available: u64,
    pub pending: u64,
}
/// Global signal for balance breakdown
pub static WALLET_BALANCES: GlobalSignal<WalletBalances> = Signal::global(WalletBalances::default);
/// Initialize the WalletRepository with the given seed and localstore
pub async fn init_multi_wallet(
    localstore: Arc<super::indexeddb_database::IndexedDbDatabase>,
    seed: [u8; 64],
) -> Result<Arc<WalletRepository>, String> {
    *MULTI_WALLET.write() = None;
    *WALLET_BALANCES.write() = WalletBalances::default();
    log::info!("Initializing WalletRepository");
    let repo = WalletRepositoryBuilder::new()
        .localstore(localstore)
        .seed(seed)
        .build()
        .await
        .map_err(|e| format!("Failed to create WalletRepository: {}", e))?;
    let repo_arc = Arc::new(repo);
    *MULTI_WALLET.write() = Some(repo_arc.clone());
    log::info!("WalletRepository initialized successfully");
    Ok(repo_arc)
}
/// Add a mint to the WalletRepository
pub async fn add_mint(mint_url: &str) -> Result<(), String> {
    let repo = MULTI_WALLET
        .read()
        .as_ref()
        .ok_or("WalletRepository not initialized")?
        .clone();
    let mint_url = mint_url
        .parse()
        .map_err(|e| format!("Invalid mint URL: {}", e))?;
    let config = cdk::wallet::WalletConfig::new()
        .with_target_proof_count(5)
        .with_metadata_cache_ttl(Some(std::time::Duration::from_secs(3600)));
    repo.add_wallet_with_config(mint_url, Some(config))
        .await
        .map_err(|e| format!("Failed to add mint: {}", e))?;
    sync_wallet_state().await?;
    Ok(())
}
/// Remove a mint from the WalletRepository
pub async fn remove_mint(mint_url: &str) -> Result<(), String> {
    let repo = MULTI_WALLET
        .read()
        .as_ref()
        .ok_or("WalletRepository not initialized")?
        .clone();
    let mint_url = mint_url
        .parse()
        .map_err(|e| format!("Invalid mint URL: {}", e))?;
    repo.remove_wallet(mint_url, CurrencyUnit::Sat)
        .await
        .map_err(|e| format!("Failed to remove mint: {}", e))?;
    sync_wallet_state().await?;
    Ok(())
}
/// Get a wallet for a specific mint
pub async fn get_wallet(mint_url: &str) -> Result<cdk::Wallet, String> {
    let repo = MULTI_WALLET
        .read()
        .as_ref()
        .ok_or("WalletRepository not initialized")?
        .clone();
    let mint_url = mint_url
        .parse()
        .map_err(|e| format!("Invalid mint URL: {}", e))?;
    repo.get_wallet(&mint_url, &CurrencyUnit::Sat)
        .await
        .map_err(|e| format!("Mint not found {}: {}", mint_url, e))
}
/// Check if a mint exists in the wallet
pub async fn has_mint(mint_url: &str) -> bool {
    let repo = match MULTI_WALLET.read().as_ref() {
        Some(w) => w.clone(),
        None => return false,
    };
    let mint_url = match mint_url.parse() {
        Ok(url) => url,
        Err(_) => return false,
    };
    repo.has_mint(&mint_url).await
}
/// Get total balance across all mints
#[allow(dead_code)]
pub async fn get_total_balance() -> Result<u64, String> {
    let repo = MULTI_WALLET
        .read()
        .as_ref()
        .ok_or("WalletRepository not initialized")?
        .clone();
    let balances = repo
        .total_balance()
        .await
        .map_err(|e| format!("Failed to get balance: {}", e))?;
    let balance = balances
        .get(&CurrencyUnit::Sat)
        .copied()
        .unwrap_or(cdk::Amount::ZERO);
    Ok(u64::from(balance))
}
/// Sync CDK state to Dioxus signals
///
/// This should be called after any CDK operation that changes wallet state.
/// It updates WALLET_TOKENS and WALLET_BALANCES.
pub async fn sync_wallet_state() -> Result<(), String> {
    let repo = match MULTI_WALLET.read().as_ref() {
        Some(w) => w.clone(),
        None => {
            log::debug!("sync_wallet_state: WalletRepository not initialized");
            return Ok(());
        }
    };
    let proofs_map: BTreeMap<WalletKey, Vec<cdk_common::Proof>> = repo
        .list_proofs()
        .await
        .map_err(|e| format!("Failed to list proofs: {}", e))?;
    let mut tokens: Vec<TokenData> = Vec::new();
    for (wallet_key, proofs) in proofs_map {
        if proofs.is_empty() {
            continue;
        }
        let proof_data: Vec<ProofData> = proofs
            .iter()
            .map(|p| ProofData {
                id: p.keyset_id.to_string(),
                amount: u64::from(p.amount),
                secret: p.secret.to_string(),
                c: p.c.to_string(),
                witness: p
                    .witness
                    .as_ref()
                    .and_then(|w| match serde_json::to_string(w) {
                        Ok(s) => Some(s),
                        Err(e) => {
                            log::warn!(
                                "Failed to serialize witness for proof {}: {}",
                                p.keyset_id,
                                e
                            );
                            None
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
            })
            .collect();
        tokens.push(TokenData {
            event_id: String::new(),
            mint: wallet_key.mint_url.to_string(),
            unit: "sat".to_string(),
            proofs: proof_data,
            created_at: 0,
        });
    }
    let existing_tokens = WALLET_TOKENS.read().data().read().clone();
    for token in &mut tokens {
        if let Some(existing) = existing_tokens.iter().find(|t| t.mint == token.mint) {
            token.event_id = existing.event_id.clone();
            token.created_at = existing.created_at;
        }
    }
    *WALLET_TOKENS.read().data().write() = tokens;
    super::cashu::signals::update_wallet_balances();
    *WALLET_STATUS.write() = WalletStatus::Ready;
    let balance = WALLET_BALANCES.read().available;
    log::debug!("Synced wallet state: {} sats available", balance);
    Ok(())
}
/// Recompute wallet balances from cached WALLET_TOKENS
///
/// **IMPORTANT**: This function does NOT query CDK or mint state.
/// It only recomputes balances from the in-memory WALLET_TOKENS cache.
/// Call `sync_wallet_state()` first to ensure WALLET_TOKENS is fresh.
///
/// This is a lightweight, synchronous, no-network operation suitable for frequent calls.
#[allow(dead_code)]
pub fn sync_balance_only() -> u64 {
    super::cashu::signals::update_wallet_balances();
    WALLET_BALANCES.read().available
}
/// Clear the WalletRepository and all related UI signals (for logout)
#[allow(dead_code)]
pub fn clear_multi_wallet() {
    *MULTI_WALLET.write() = None;
    *WALLET_BALANCES.write() = WalletBalances::default();
    *WALLET_TOKENS.read().data().write() = Vec::new();
    *WALLET_STATUS.write() = WalletStatus::Uninitialized;
    *WALLET_STATE.write() = None;
    log::info!("Cleared WalletRepository and all wallet signals");
}
/// Check if WalletRepository is initialized
pub fn is_initialized() -> bool {
    MULTI_WALLET.read().is_some()
}
/// Get all mint URLs from the wallet
#[allow(dead_code)]
pub async fn get_mint_urls() -> Result<Vec<String>, String> {
    let repo = MULTI_WALLET
        .read()
        .as_ref()
        .ok_or("WalletRepository not initialized")?
        .clone();
    let wallets = repo.get_wallets().await;
    Ok(wallets.iter().map(|w| w.mint_url.to_string()).collect())
}
