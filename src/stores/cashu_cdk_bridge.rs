//! CDK Bridge Module
//!
//! This module bridges CDK's MultiMintWallet with Dioxus reactive signals.
//! It provides synchronization between CDK's internal state and Dioxus GlobalSignals
//! for UI reactivity.
#![allow(dead_code)]
use super::cashu::signals::PENDING_NOSTR_EVENTS;
use super::cashu::{
    DleqData, ProofData, ProofState, TokenData, WalletStatus, WalletTokensStoreStoreExt,
    WALLET_STATE, WALLET_STATUS, WALLET_TOKENS,
};
use cdk::nuts::CurrencyUnit;
use cdk::wallet::multi_mint_wallet::MultiMintWallet;
use dioxus::prelude::*;
use std::sync::Arc;
/// Global MultiMintWallet instance
/// Replaces the previous WALLET_CACHE HashMap approach
pub static MULTI_WALLET: GlobalSignal<Option<Arc<MultiMintWallet>>> = Signal::global(|| None);
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
/// Initialize the MultiMintWallet with the given seed and localstore
pub async fn init_multi_wallet(
    localstore: Arc<super::indexeddb_database::IndexedDbDatabase>,
    seed: [u8; 64],
) -> Result<Arc<MultiMintWallet>, String> {
    *MULTI_WALLET.write() = None;
    *WALLET_BALANCES.write() = WalletBalances::default();
    log::info!("Initializing MultiMintWallet");
    let multi_wallet = MultiMintWallet::new(localstore, seed, CurrencyUnit::Sat)
        .await
        .map_err(|e| format!("Failed to create MultiMintWallet: {}", e))?;
    let wallet_arc = Arc::new(multi_wallet);
    *MULTI_WALLET.write() = Some(wallet_arc.clone());
    log::info!("MultiMintWallet initialized successfully");
    Ok(wallet_arc)
}
/// Add a mint to the MultiMintWallet
pub async fn add_mint(mint_url: &str) -> Result<(), String> {
    let multi_wallet = MULTI_WALLET
        .read()
        .as_ref()
        .ok_or("MultiMintWallet not initialized")?
        .clone();
    let mint_url = mint_url
        .parse()
        .map_err(|e| format!("Invalid mint URL: {}", e))?;
    multi_wallet
        .add_mint(mint_url)
        .await
        .map_err(|e| format!("Failed to add mint: {}", e))?;
    sync_wallet_state().await?;
    Ok(())
}
/// Remove a mint from the MultiMintWallet
pub async fn remove_mint(mint_url: &str) -> Result<(), String> {
    let multi_wallet = MULTI_WALLET
        .read()
        .as_ref()
        .ok_or("MultiMintWallet not initialized")?
        .clone();
    let mint_url = mint_url
        .parse()
        .map_err(|e| format!("Invalid mint URL: {}", e))?;
    multi_wallet.remove_mint(&mint_url).await;
    sync_wallet_state().await?;
    Ok(())
}
/// Get a wallet for a specific mint
pub async fn get_wallet(mint_url: &str) -> Result<cdk::Wallet, String> {
    let multi_wallet = MULTI_WALLET
        .read()
        .as_ref()
        .ok_or("MultiMintWallet not initialized")?
        .clone();
    let mint_url = mint_url
        .parse()
        .map_err(|e| format!("Invalid mint URL: {}", e))?;
    multi_wallet
        .get_wallet(&mint_url)
        .await
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
    let multi_wallet = MULTI_WALLET
        .read()
        .as_ref()
        .ok_or("MultiMintWallet not initialized")?
        .clone();
    let balance = multi_wallet
        .total_balance()
        .await
        .map_err(|e| format!("Failed to get balance: {}", e))?;
    Ok(u64::from(balance))
}
/// Sync CDK state to Dioxus signals
///
/// This should be called after any CDK operation that changes wallet state.
/// It updates WALLET_TOKENS and WALLET_BALANCES.
pub async fn sync_wallet_state() -> Result<(), String> {
    let multi_wallet = match MULTI_WALLET.read().as_ref() {
        Some(w) => w.clone(),
        None => {
            log::debug!("sync_wallet_state: MultiMintWallet not initialized");
            return Ok(());
        }
    };
    let proofs_by_mint = multi_wallet
        .list_proofs()
        .await
        .map_err(|e| format!("Failed to list proofs: {}", e))?;
    let mut tokens: Vec<TokenData> = Vec::new();
    for (mint_url, proofs) in proofs_by_mint {
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
            pending_publish: false,
            mint: mint_url.to_string(),
            unit: "sat".to_string(),
            proofs: proof_data,
            created_at: 0,
        });
    }
    let existing_tokens = WALLET_TOKENS.read().data().read().clone();
    let durable_pending_tokens = PENDING_NOSTR_EVENTS.read().clone();
    for token in &mut tokens {
        if let Some(existing) = existing_tokens
            .iter()
            .filter(|t| t.mint == token.mint)
            .max_by_key(|t| (t.created_at, t.pending_publish))
        {
            token.event_id = existing.event_id.clone();
            token.created_at = existing.created_at;
            token.pending_publish = existing.pending_publish
                && durable_pending_tokens.iter().any(|pending| {
                    pending.pending_token_id.as_deref() == Some(existing.event_id.as_str())
                        && pending
                            .mint_url
                            .as_deref()
                            .is_none_or(|mint| mint == existing.mint)
                });
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
/// Clear the MultiMintWallet and all related UI signals (for logout)
#[allow(dead_code)]
pub fn clear_multi_wallet() {
    *MULTI_WALLET.write() = None;
    *WALLET_BALANCES.write() = WalletBalances::default();
    *WALLET_TOKENS.read().data().write() = Vec::new();
    *WALLET_STATUS.write() = WalletStatus::Uninitialized;
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
    let multi_wallet = MULTI_WALLET
        .read()
        .as_ref()
        .ok_or("MultiMintWallet not initialized")?
        .clone();
    let wallets = multi_wallet.get_wallets().await;
    Ok(wallets.iter().map(|w| w.mint_url.to_string()).collect())
}
