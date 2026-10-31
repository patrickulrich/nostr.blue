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
use super::signals::{PENDING_MELT_QUOTES, PENDING_MINT_QUOTES};
use super::types::{
    MeltQuoteInfo, MintQuoteInfo, PendingMeltQuotesStoreStoreExt, PendingMintQuotesStoreStoreExt,
};
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

/// Does a freshly-fetched on-chain melt quote correspond to the fee option
/// the user picked in the UI?
///
/// `quote_onchain_melt_options` POSTs a **fresh** quote per call (new quote
/// id each time), so the stale id captured when the fee menu was rendered can
/// never identify the re-fetched quote — identity is the
/// `(fee_index, fee_reserve, estimated_blocks)` triple. The `fee_reserve`
/// equality is the safety catch: if the mint's fees moved while the menu was
/// open, we refuse instead of silently paying a different fee.
fn fresh_quote_matches_fee_option(
    fresh: &cdk::wallet::MeltQuote,
    selected: &OnchainFeeOption,
) -> bool {
    fresh.fee_index.unwrap_or(0) as u64 == selected.fee_index
        && u64::from(fresh.fee_reserve) == selected.fee_reserve_sats
        && fresh.estimated_blocks.is_none_or(|blocks| {
            // Compare only when both sides carry the estimate; `0` is the
            // "unknown" placeholder from `onchain_withdraw_fee_options`.
            selected.estimated_blocks == 0 || blocks == selected.estimated_blocks
        })
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
    // Every re-fetch returns brand-new quote ids, so match the fresh quote by
    // its fee identity (never the stale `fee_option.quote_id`) and fail
    // loudly when the mint's fee menu moved.
    let selected = match quotes
        .iter()
        .find(|q| fresh_quote_matches_fee_option(q, fee_option))
    {
        Some(q) => q.clone(),
        None => {
            // Distinguish "fee drifted" from "option gone" for a clearer error.
            let same_index = quotes
                .iter()
                .find(|q| q.fee_index.unwrap_or(0) as u64 == fee_option.fee_index);
            return Err(match same_index {
                Some(q) => format!(
                    "On-chain fee changed: option {} was {} sats when selected, is {} sats now. \
                     Fetch fee options again and retry.",
                    fee_option.fee_index,
                    fee_option.fee_reserve_sats,
                    u64::from(q.fee_reserve)
                ),
                None => format!(
                    "Selected fee option (index {}) is no longer offered by the mint. Fetch fee \
                     options again and retry.",
                    fee_option.fee_index
                ),
            });
        }
    };
    // Persist the FRESH quote (cdk's melt machinery reads it back by id from
    // the shared localstore).
    wallet
        .select_onchain_melt_quote(selected.clone())
        .await
        .map_err(|e| format!("Failed to select on-chain quote: {}", e))?;
    // `melt_tokens` resolves quotes from the in-memory PENDING_MELT_QUOTES
    // signal by id (hard-failing "Melt quote not found" otherwise); the fresh
    // id only exists in the localstore, so mirror it into the signal. The
    // `invoice` field holds the destination BTC address here (the history
    // event labels it "lightning_melt", accepted for this flow).
    PENDING_MELT_QUOTES
        .read()
        .data()
        .write()
        .push(MeltQuoteInfo::from_cdk(&selected, mint_url.to_string()));
    // The standard melt flow takes it from here (proof reservation, change,
    // payment_proof = outpoint) — keyed by the FRESH quote id.
    melt_tokens(mint_url.to_string(), selected.id).await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn melt_quote(
        fee_index: Option<u32>,
        fee_reserve_sats: u64,
        estimated_blocks: Option<u32>,
    ) -> cdk::wallet::MeltQuote {
        cdk::wallet::MeltQuote {
            id: "fresh-quote-id".to_string(),
            mint_url: None,
            unit: cdk::nuts::CurrencyUnit::Sat,
            amount: cdk::Amount::from(1000),
            request: "bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4".to_string(),
            fee_reserve: cdk::Amount::from(fee_reserve_sats),
            state: cdk::nuts::MeltQuoteState::Unpaid,
            expiry: 0,
            payment_proof: None,
            estimated_blocks,
            fee_index,
            payment_method: PaymentMethod::Known(KnownMethod::Onchain),
            used_by_operation: None,
            version: 0,
        }
    }

    fn fee_option(fee_index: u64, fee_reserve_sats: u64, estimated_blocks: u32) -> OnchainFeeOption {
        // `quote_id` is deliberately stale — matching must ignore it.
        OnchainFeeOption {
            quote_id: "stale-quote-id".to_string(),
            fee_index,
            fee_reserve_sats,
            estimated_blocks,
        }
    }

    #[test]
    fn fresh_quote_match_ignores_stale_id_and_matches_fee_identity() {
        let selected = fee_option(2, 150, 6);
        // Same fee identity under a brand-new id → match.
        assert!(fresh_quote_matches_fee_option(
            &melt_quote(Some(2), 150, Some(6)),
            &selected
        ));
        // `None` fee_index matches the normalized 0 placeholder.
        assert!(fresh_quote_matches_fee_option(
            &melt_quote(None, 150, Some(6)),
            &fee_option(0, 150, 6)
        ));
    }

    #[test]
    fn fresh_quote_match_rejects_drifted_fee_reserve() {
        let selected = fee_option(2, 150, 6);
        // Same index, higher fee since the menu was rendered → refuse.
        assert!(!fresh_quote_matches_fee_option(
            &melt_quote(Some(2), 300, Some(6)),
            &selected
        ));
        // Same fee, different index → different option, refuse.
        assert!(!fresh_quote_matches_fee_option(
            &melt_quote(Some(1), 150, Some(6)),
            &selected
        ));
    }

    #[test]
    fn fresh_quote_match_tolerates_missing_estimated_blocks() {
        let selected = fee_option(2, 150, 6);
        // Fresh quote without the estimate (or selection without one) is not
        // a mismatch — fee_index + fee_reserve carry the identity.
        assert!(fresh_quote_matches_fee_option(
            &melt_quote(Some(2), 150, None),
            &selected
        ));
        assert!(fresh_quote_matches_fee_option(
            &melt_quote(Some(2), 150, Some(9)),
            &fee_option(2, 150, 0)
        ));
        // But when both carry estimates they must agree.
        assert!(!fresh_quote_matches_fee_option(
            &melt_quote(Some(2), 150, Some(9)),
            &selected
        ));
    }
}
