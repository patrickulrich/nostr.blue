//! CDK Bridge Module
//!
//! This module bridges CDK's MultiMintWallet with Dioxus reactive signals.
//! It provides synchronization between CDK's internal state and Dioxus GlobalSignals
//! for UI reactivity.

use dioxus::prelude::*;
use std::sync::Arc;
use std::collections::HashMap;
use cdk::wallet::multi_mint_wallet::MultiMintWallet;
use cdk::nuts::CurrencyUnit;

use super::cashu_wallet::{
    WALLET_BALANCE, WALLET_TOKENS, WALLET_STATUS, WalletStatus,
    TokenData, ProofData, WalletTokensStoreStoreExt,
};
use super::indexeddb_database::IndexedDbDatabase;

/// Global MultiMintWallet instance
/// Replaces the previous WALLET_CACHE HashMap approach
pub static MULTI_WALLET: GlobalSignal<Option<Arc<MultiMintWallet>>> = Signal::global(|| None);

/// Cache for mint MPP support: mint_url -> (timestamp_ms, supports_mpp)
/// TTL is 5 minutes (300,000 ms)
static MINT_MPP_CACHE: GlobalSignal<HashMap<String, (f64, bool)>> = Signal::global(|| HashMap::new());
const MINT_INFO_CACHE_TTL_MS: f64 = 300_000.0; // 5 minutes

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
            dleq: p.dleq.as_ref().map(|d| super::cashu_wallet::DleqData {
                e: d.e.to_string(),
                s: d.s.to_string(),
                r: d.r.to_string(),
            }),
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

    // Update balance breakdown (for now, all is available - pending detection comes later)
    *WALLET_BALANCES.write() = WalletBalances {
        total,
        available: total,
        pending: 0,
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

/// Clear the MultiMintWallet (for logout)
#[allow(dead_code)]
pub fn clear_multi_wallet() {
    *MULTI_WALLET.write() = None;
    *WALLET_BALANCES.write() = WalletBalances::default();
    log::info!("Cleared MultiMintWallet");
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

// ============================================================================
// NUT-15: Multi-Path Payment (MPP) Support
// ============================================================================

/// Balance info for a single mint
#[derive(Clone, Debug)]
pub struct MintBalance {
    pub mint_url: String,
    pub balance: u64,
}

/// Get balance breakdown per mint
pub async fn get_balances_per_mint() -> Result<Vec<MintBalance>, String> {
    let multi_wallet = MULTI_WALLET.read()
        .as_ref()
        .ok_or("MultiMintWallet not initialized")?
        .clone();

    let balances = multi_wallet.get_balances().await
        .map_err(|e| format!("Failed to get balances: {}", e))?;

    Ok(balances.iter()
        .map(|(url, amount)| MintBalance {
            mint_url: url.to_string(),
            balance: u64::from(*amount),
        })
        .collect())
}

/// MPP quote info for a single mint's contribution
#[derive(Clone, Debug)]
pub struct MppQuoteContribution {
    pub mint_url: String,
    pub quote_id: String,
    pub amount: u64,
    pub fee_reserve: u64,
}

/// MPP combined quote for the full payment
#[derive(Clone, Debug)]
pub struct MppQuoteInfo {
    pub contributions: Vec<MppQuoteContribution>,
    pub total_amount: u64,
    pub total_fee_reserve: u64,
}

/// Calculate optimal MPP split across mints to pay an invoice
///
/// Returns a list of (mint_url, amount) pairs that sum to the target amount
/// If `include_mints` is provided, only those mints will be considered.
pub async fn calculate_mpp_split(
    target_amount: u64,
    include_mints: Option<Vec<String>>,
) -> Result<Vec<(String, u64)>, String> {
    let balances = get_balances_per_mint().await?;

    // Filter to only included mints (if specified)
    let available: Vec<_> = balances.into_iter()
        .filter(|b| {
            if let Some(ref included) = include_mints {
                included.contains(&b.mint_url)
            } else {
                true
            }
        })
        .filter(|b| b.balance > 0)
        .collect();

    if available.is_empty() {
        return Err("No mints with available balance".to_string());
    }

    // Calculate total available
    let total_available: u64 = available.iter().map(|b| b.balance).sum();
    if total_available < target_amount {
        return Err(format!(
            "Insufficient total balance: {} sats available, {} sats needed",
            total_available, target_amount
        ));
    }

    // Greedy allocation: use mints with largest balances first
    let mut sorted = available;
    sorted.sort_by(|a, b| b.balance.cmp(&a.balance));

    let mut remaining = target_amount;
    let mut allocations = Vec::new();

    for mint in sorted {
        if remaining == 0 {
            break;
        }

        let contribution = mint.balance.min(remaining);
        if contribution > 0 {
            allocations.push((mint.mint_url, contribution));
            remaining -= contribution;
        }
    }

    if remaining > 0 {
        return Err("Could not allocate enough balance across mints".to_string());
    }

    Ok(allocations)
}

/// Create MPP melt quotes from multiple mints
pub async fn create_mpp_melt_quotes(
    bolt11: String,
    mint_amounts: Vec<(String, u64)>,
) -> Result<MppQuoteInfo, String> {
    use cdk::Amount;
    use cdk::mint_url::MintUrl;

    let multi_wallet = MULTI_WALLET.read()
        .as_ref()
        .ok_or("MultiMintWallet not initialized")?
        .clone();

    // Convert to CDK types
    let mint_amounts_cdk: Vec<(MintUrl, Amount)> = mint_amounts.iter()
        .map(|(url, amount)| {
            let mint_url: MintUrl = url.parse()
                .map_err(|e| format!("Invalid mint URL {}: {}", url, e))?;
            Ok((mint_url, Amount::from(*amount)))
        })
        .collect::<Result<Vec<_>, String>>()?;

    // Create quotes in parallel using CDK's MPP function
    let quotes = multi_wallet.mpp_melt_quote(bolt11, mint_amounts_cdk).await
        .map_err(|e| format!("Failed to create MPP quotes: {}", e))?;

    // Convert to our types
    let contributions: Vec<MppQuoteContribution> = quotes.iter()
        .map(|(url, quote)| MppQuoteContribution {
            mint_url: url.to_string(),
            quote_id: quote.id.clone(),
            amount: u64::from(quote.amount),
            fee_reserve: u64::from(quote.fee_reserve),
        })
        .collect();

    let total_amount = contributions.iter()
        .map(|c| c.amount)
        .try_fold(0u64, |acc, v| acc.checked_add(v))
        .ok_or("MPP quote total amount overflow")?;
    let total_fee_reserve = contributions.iter()
        .map(|c| c.fee_reserve)
        .try_fold(0u64, |acc, v| acc.checked_add(v))
        .ok_or("MPP quote fee reserve overflow")?;

    Ok(MppQuoteInfo {
        contributions,
        total_amount,
        total_fee_reserve,
    })
}

/// Execute MPP melts using previously obtained quotes
pub async fn execute_mpp_melt(
    quote_contributions: Vec<MppQuoteContribution>,
) -> Result<MppMeltResult, String> {
    use cdk::mint_url::MintUrl;

    let multi_wallet = MULTI_WALLET.read()
        .as_ref()
        .ok_or("MultiMintWallet not initialized")?
        .clone();

    // Convert to CDK format: (MintUrl, quote_id)
    let quotes: Vec<(MintUrl, String)> = quote_contributions.iter()
        .map(|c| {
            let mint_url: MintUrl = c.mint_url.parse()
                .map_err(|e| format!("Invalid mint URL {}: {}", c.mint_url, e))?;
            Ok((mint_url, c.quote_id.clone()))
        })
        .collect::<Result<Vec<_>, String>>()?;

    // Execute MPP melts in parallel
    let results = multi_wallet.mpp_melt(quotes).await
        .map_err(|e| format!("MPP melt failed: {}", e))?;

    // Aggregate results
    let mut total_paid = 0u64;
    let mut total_fee = 0u64;
    let mut preimage: Option<String> = None;
    let mut all_paid = true;

    for (url, melted) in &results {
        log::info!("MPP contribution from {}: paid={}, fee={}",
            url, u64::from(melted.amount), u64::from(melted.fee_paid));

        total_paid = total_paid
            .checked_add(u64::from(melted.amount))
            .ok_or("MPP total amount overflow")?;
        total_fee = total_fee
            .checked_add(u64::from(melted.fee_paid))
            .ok_or("MPP total fee overflow")?;

        // Get preimage from any contribution (they should all have the same)
        if preimage.is_none() && melted.preimage.is_some() {
            preimage = melted.preimage.clone();
        }

        // Check if all paid - for MPP, state should be Paid for success
        if melted.state != cdk::nuts::MeltQuoteState::Paid {
            all_paid = false;
        }
    }

    // Sync wallet state after melt
    if let Err(e) = sync_wallet_state().await {
        log::warn!("Failed to sync wallet state after MPP melt: {}", e);
    }

    Ok(MppMeltResult {
        paid: all_paid,
        preimage,
        total_amount_paid: total_paid,
        total_fee_paid: total_fee,
        contributions: results.len(),
    })
}

/// Result of MPP melt operation
#[derive(Clone, Debug)]
#[allow(dead_code)]
pub struct MppMeltResult {
    pub paid: bool,
    pub preimage: Option<String>,
    pub total_amount_paid: u64,
    pub total_fee_paid: u64,
    pub contributions: usize,
}

/// Check if a mint supports MPP (NUT-15) with caching
pub async fn mint_supports_mpp(mint_url: &str) -> bool {
    let now = js_sys::Date::now();

    // Check cache first
    {
        let cache = MINT_MPP_CACHE.read();
        if let Some((timestamp, supports)) = cache.get(mint_url) {
            if now - timestamp < MINT_INFO_CACHE_TTL_MS {
                return *supports;
            }
        }
    }

    // Cache miss or expired - fetch from network
    let supports = fetch_mint_mpp_support(mint_url).await;

    // Update cache
    MINT_MPP_CACHE.write().insert(mint_url.to_string(), (now, supports));

    supports
}

/// Internal function to fetch MPP support from network
async fn fetch_mint_mpp_support(mint_url: &str) -> bool {
    let multi_wallet = match MULTI_WALLET.read().as_ref() {
        Some(w) => w.clone(),
        None => return false,
    };

    let mint_url: cdk::mint_url::MintUrl = match mint_url.parse() {
        Ok(url) => url,
        Err(_) => return false,
    };

    // Get wallet for this mint
    let wallet = match multi_wallet.get_wallet(&mint_url).await {
        Some(w) => w,
        None => return false,
    };

    // Try to get mint info and check NUT-15 support
    match wallet.fetch_mint_info().await {
        Ok(Some(info)) => {
            // Check if NUT-15 methods list is not empty (mint supports MPP)
            !info.nuts.nut15.is_empty()
        }
        _ => false,
    }
}
