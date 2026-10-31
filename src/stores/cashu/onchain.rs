//! NUT-30: on-chain Bitcoin deposit and withdrawal.
//!
//! Deposit: an on-chain mint quote returns a BTC address (with a NUT-20
//! locking key managed internally by cdk via the derivation counter); as
//! confirmations arrive the mint reports `amount_paid`/`amount_issued`, and
//! the wallet mints the difference (partially or in full).
//!
//! Withdrawal: the mint returns a menu of fee options for a destination
//! address; the caller selects one and melts through the standard melt flow
//! (`select_onchain_melt_quote` persists the quote, then the existing
//! `melt_tokens` machinery pays it).
//!
//! Everything here is capability-gated by the UI on the mint advertising an
//! `onchain` entry in its nut04/nut05 method lists — no production mint runs
//! a BDK backend yet, so the flows activate only where supported.

use super::lightning::melt_tokens;
use super::signals::PENDING_MINT_QUOTES;
use super::types::{MintQuoteInfo, PendingMintQuotesStoreStoreExt};
use cdk::amount::{Amount, SplitTarget};
use cdk::nuts::PaymentMethod;
use cdk_common::nut00::KnownMethod;
use dioxus::prelude::*;

use super::internal::create_ephemeral_wallet;

/// An on-chain deposit quote (the address to pay to).
#[derive(Debug, Clone)]
pub struct OnchainDepositQuote {
    pub quote_id: String,
    pub mint_url: String,
    /// BTC address to pay to (`MintQuote::request`).
    pub address: String,
    /// Fixed amount for amount-fixed flows; on-chain deposits are typically
    /// variable-amount (`None`) and tracked via paid/issued.
    pub amount: Option<u64>,
    pub expiry: u64,
}

/// Live status of an on-chain deposit quote.
#[derive(Debug, Clone, Default)]
pub struct OnchainDepositStatus {
    pub state: String,
    pub amount_paid: u64,
    pub amount_issued: u64,
    /// `amount_paid - amount_issued` — what can be minted right now.
    pub mintable_sats: u64,
}

/// One fee option from the mint's on-chain melt menu.
#[derive(Debug, Clone, PartialEq)]
pub struct OnchainFeeOption {
    pub quote_id: String,
    pub fee_index: u64,
    pub fee_reserve_sats: u64,
    pub estimated_blocks: u32,
}

/// Create an on-chain deposit quote. `amount_sats` of `None` requests a
/// variable-amount (top-up style) address.
pub async fn create_onchain_deposit_quote(
    mint_url: &str,
    amount_sats: Option<u64>,
) -> Result<OnchainDepositQuote, String> {
    let wallet = create_ephemeral_wallet(mint_url, vec![]).await?;
    let quote = wallet
        .mint_quote(
            PaymentMethod::Known(KnownMethod::Onchain),
            amount_sats.map(Amount::from),
            None,
            None,
        )
        .await
        .map_err(|e| format!("Failed to create on-chain deposit quote: {}", e))?;
    let deposit = OnchainDepositQuote {
        quote_id: quote.id.clone(),
        mint_url: mint_url.to_string(),
        address: quote.request.clone(),
        amount: quote.amount.map(u64::from),
        expiry: quote.expiry,
    };
    // Reuse the pending-mint-quote tracking so startup recovery sweeps
    // unfinished on-chain deposits alongside lightning topups.
    PENDING_MINT_QUOTES.read().data().write().push(MintQuoteInfo {
        quote_id: deposit.quote_id.clone(),
        invoice: deposit.address.clone(),
        amount: deposit.amount.unwrap_or(0),
        expiry: Some(deposit.expiry),
        mint_url: deposit.mint_url.clone(),
    });
    Ok(deposit)
}

/// Check an on-chain deposit quote's confirmation progress.
pub async fn check_onchain_deposit(
    mint_url: &str,
    quote_id: &str,
) -> Result<OnchainDepositStatus, String> {
    let wallet = create_ephemeral_wallet(mint_url, vec![]).await?;
    let quote = wallet
        .check_mint_quote_status(quote_id)
        .await
        .map_err(|e| format!("Failed to check on-chain deposit: {}", e))?;
    let paid = u64::from(quote.amount_paid);
    let issued = u64::from(quote.amount_issued);
    Ok(OnchainDepositStatus {
        state: format!("{:?}", quote.state),
        amount_paid: paid,
        amount_issued: issued,
        mintable_sats: paid.saturating_sub(issued),
    })
}

/// Mint ecash for a confirmed on-chain deposit. Returns sats minted.
///
/// `max_sats: None` mints everything currently available; cdk's resumable
/// batch minting handles partial confirmations natively.
pub async fn mint_onchain_deposit(
    mint_url: &str,
    quote_id: &str,
    _max_sats: Option<u64>,
) -> Result<u64, String> {
    let wallet = create_ephemeral_wallet(mint_url, vec![]).await?;
    // `mint()` auto-resumes any crashed issue saga for this quote first —
    // the funds-safety net that makes a partial mint crash-safe. cdk mints
    // the full available difference; partial mints arrive with a follow-up
    // amount API upstream.
    let proofs = wallet
        .mint(quote_id, SplitTarget::default(), None)
        .await
        .map_err(|e| format!("Failed to mint on-chain deposit: {}", e))?;
    Ok(proofs.iter().map(|p| u64::from(p.amount)).sum())
}

/// Validate a BTC address (any network — testnet/regtest mints included).
pub fn validate_btc_address(address: &str) -> bool {
    use std::str::FromStr;
    bitcoin::Address::from_str(address).is_ok()
}

/// Fetch the mint's on-chain withdrawal fee menu for `amount_sats` to
/// `address`.
pub async fn onchain_withdraw_fee_options(
    mint_url: &str,
    address: &str,
    amount_sats: u64,
) -> Result<Vec<OnchainFeeOption>, String> {
    if !validate_btc_address(address) {
        return Err("Invalid Bitcoin address".to_string());
    }
    let wallet = create_ephemeral_wallet(mint_url, vec![]).await?;
    let quotes = wallet
        .quote_onchain_melt_options(address, Amount::from(amount_sats), None)
        .await
        .map_err(|e| format!("Failed to fetch on-chain fee options: {}", e))?;
    Ok(quotes
        .into_iter()
        .map(|q| OnchainFeeOption {
            quote_id: q.id.clone(),
            fee_index: q.fee_index.unwrap_or(0) as u64,
            fee_reserve_sats: u64::from(q.fee_reserve),
            estimated_blocks: q.estimated_blocks.unwrap_or(0),
        })
        .collect())
}

/// Execute an on-chain withdrawal with the selected fee option: persists the
/// selected quote and melts through the standard melt machinery (sagas,
/// change recovery, NIP-60 history events all apply). Returns the melt
/// result tuple `(paid, payment_proof, amount)` — the proof is the on-chain
/// outpoint for NUT-30.
pub async fn withdraw_onchain(
    mint_url: &str,
    address: &str,
    amount_sats: u64,
    fee_option: &OnchainFeeOption,
) -> Result<(bool, Option<String>, u64), String> {
    let wallet = create_ephemeral_wallet(mint_url, vec![]).await?;
    let quotes = wallet
        .quote_onchain_melt_options(address, Amount::from(amount_sats), None)
        .await
        .map_err(|e| format!("Failed to re-fetch fee options: {}", e))?;
    let selected = quotes
        .into_iter()
        .find(|q| {
            q.fee_index == Some(fee_option.fee_index as u32)
                && u64::from(q.fee_reserve) == fee_option.fee_reserve_sats
        })
        .ok_or_else(|| "Selected fee option is no longer available".to_string())?;
    wallet
        .select_onchain_melt_quote(selected)
        .await
        .map_err(|e| format!("Failed to select on-chain quote: {}", e))?;
    // The standard melt flow takes it from here (proof reservation, change,
    // payment_proof = outpoint).
    melt_tokens(mint_url.to_string(), fee_option.quote_id.clone()).await
}
