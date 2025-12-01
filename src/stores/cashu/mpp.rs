//! Multi-path payments (MPP)
//!
//! Functions for splitting payments across multiple mints (NUT-15).
//!
//! This module provides:
//! - Balance queries per mint
//! - Optimal MPP split calculation (greedy algorithm)
//! - MPP quote creation via CDK's `mpp_melt_quote()`
//! - MPP execution via CDK's `mpp_melt()`
//! - Mint MPP support detection with caching

use dioxus::prelude::*;
use std::collections::HashMap;
use nostr_sdk::signer::NostrSigner;
use nostr_sdk::{Kind, PublicKey, EventId};

use crate::stores::cashu_cdk_bridge::{MULTI_WALLET, sync_wallet_state};
use super::signals::{WALLET_BALANCE, WALLET_TOKENS};
use super::types::{
    TokenData, ProofData, ExtendedCashuProof, ExtendedTokenEvent,
    PendingEventType, WalletTokensStoreStoreExt,
};
use super::proofs::cdk_proof_to_proof_data;
use super::events::queue_event_for_retry;
use super::lightning::create_history_event_with_type;
use crate::stores::{auth_store, nostr_client};

// =============================================================================
// Cache Configuration
// =============================================================================

/// Cache for mint MPP support: mint_url -> (timestamp_ms, supports_mpp)
/// TTL is 5 minutes (300,000 ms)
pub static MINT_MPP_CACHE: GlobalSignal<HashMap<String, (f64, bool)>> = Signal::global(|| HashMap::new());
pub const MINT_INFO_CACHE_TTL_MS: f64 = 300_000.0; // 5 minutes

// =============================================================================
// Types
// =============================================================================

/// Balance info for a single mint
#[derive(Clone, Debug)]
pub struct MintBalance {
    pub mint_url: String,
    pub balance: u64,
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

// =============================================================================
// Balance Query
// =============================================================================

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

// =============================================================================
// MPP Split Calculation
// =============================================================================

/// Calculate optimal MPP split across mints to pay an invoice
///
/// Uses a greedy algorithm: allocates from mints with largest balances first.
/// Returns a list of (mint_url, amount) pairs that sum to the target amount.
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

    // Calculate total available with checked arithmetic
    let total_available: u64 = available.iter()
        .map(|b| b.balance)
        .try_fold(0u64, |acc, v| acc.checked_add(v))
        .ok_or("Balance sum overflow in MPP split calculation")?;
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

// =============================================================================
// MPP Quote Creation
// =============================================================================

/// Create MPP melt quotes from multiple mints
///
/// Uses CDK's `mpp_melt_quote()` which creates quotes in parallel with
/// `MeltOptions::new_mpp(amount_msat)` for each mint.
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

// =============================================================================
// MPP Execution
// =============================================================================

/// Execute MPP melts using previously obtained quotes
///
/// Uses CDK's `mpp_melt()` which executes melts in parallel.
///
/// This function handles NIP-60 compliant Nostr event publishing for multi-mint payments:
/// - Publishes new token events (Kind 7375) with remaining proofs after melt
/// - Publishes deletion events (Kind 5) for spent token events
/// - Creates history event tracking the MPP operation
/// - Updates local WALLET_TOKENS state
pub async fn execute_mpp_melt(
    quote_contributions: Vec<MppQuoteContribution>,
) -> Result<MppMeltResult, String> {
    use cdk::mint_url::MintUrl;

    let multi_wallet = MULTI_WALLET.read()
        .as_ref()
        .ok_or("MultiMintWallet not initialized")?
        .clone();

    // STEP 1: Collect event IDs to delete for each affected mint BEFORE melt
    let affected_mints: Vec<String> = quote_contributions.iter()
        .map(|c| c.mint_url.clone())
        .collect();

    let event_ids_by_mint: HashMap<String, Vec<String>> = {
        let store = WALLET_TOKENS.read();
        let data = store.data();
        let tokens = data.read();

        let mut map: HashMap<String, Vec<String>> = HashMap::new();
        for mint_url in &affected_mints {
            let event_ids: Vec<String> = tokens.iter()
                .filter(|t| &t.mint == mint_url)
                .map(|t| t.event_id.clone())
                .collect();
            if !event_ids.is_empty() {
                map.insert(mint_url.clone(), event_ids);
            }
        }
        map
    };

    // Convert to CDK format: (MintUrl, quote_id)
    let quotes: Vec<(MintUrl, String)> = quote_contributions.iter()
        .map(|c| {
            let mint_url: MintUrl = c.mint_url.parse()
                .map_err(|e| format!("Invalid mint URL {}: {}", c.mint_url, e))?;
            Ok((mint_url, c.quote_id.clone()))
        })
        .collect::<Result<Vec<_>, String>>()?;

    // STEP 2: Execute MPP melts in parallel using CDK
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

    // Only proceed with Nostr publishing if payment was successful
    if all_paid {
        // STEP 3: Get remaining proofs per mint from MultiMintWallet
        let remaining_proofs = multi_wallet.list_proofs().await
            .map_err(|e| format!("Failed to get remaining proofs: {}", e))?;

        // Prepare for Nostr publishing
        let signer = crate::stores::signer::get_signer()
            .ok_or("No signer available")?
            .as_nostr_signer();

        let pubkey_str = auth_store::get_pubkey()
            .ok_or("Not authenticated")?;
        let pubkey = PublicKey::parse(&pubkey_str)
            .map_err(|e| format!("Invalid pubkey: {}", e))?;

        let client = nostr_client::NOSTR_CLIENT.read().as_ref()
            .ok_or("Client not initialized")?.clone();

        // Track new event IDs for history and all event IDs to delete
        let mut new_event_ids: Vec<String> = Vec::new();
        let mut all_event_ids_to_delete: Vec<String> = Vec::new();
        let mut new_tokens: Vec<TokenData> = Vec::new();

        // STEP 4: For each affected mint, publish token event with remaining proofs
        //
        // NIP-60 best practice note: These publications are done with retry queuing.
        // If publishing fails, events are queued and will be retried later.
        // Local state is updated optimistically to prevent double-spending.
        // The retry queue ensures eventual consistency with relays.
        let mut publish_failures = 0;
        for mint_url in &affected_mints {
            let mint_url_parsed: MintUrl = mint_url.parse()
                .map_err(|e| format!("Invalid mint URL: {}", e))?;

            // Get event IDs to delete for this mint
            if let Some(event_ids) = event_ids_by_mint.get(mint_url) {
                all_event_ids_to_delete.extend(event_ids.clone());
            }

            // Get remaining proofs for this mint
            if let Some(proofs) = remaining_proofs.get(&mint_url_parsed) {
                if !proofs.is_empty() {
                    let proof_data: Vec<ProofData> = proofs.iter()
                        .map(|p| cdk_proof_to_proof_data(p))
                        .collect();

                    let extended_proofs: Vec<ExtendedCashuProof> = proof_data.iter()
                        .map(|p| ExtendedCashuProof::from(p.clone()))
                        .collect();

                    let event_ids_for_mint = event_ids_by_mint.get(mint_url)
                        .cloned()
                        .unwrap_or_default();

                    let token_event_data = ExtendedTokenEvent {
                        mint: mint_url.clone(),
                        unit: "sat".to_string(),
                        proofs: extended_proofs,
                        del: event_ids_for_mint,
                    };

                    let json_content = serde_json::to_string(&token_event_data)
                        .map_err(|e| format!("Failed to serialize token event: {}", e))?;

                    let encrypted = signer.nip44_encrypt(&pubkey, &json_content).await
                        .map_err(|e| format!("Failed to encrypt token event: {}", e))?;

                    let builder = nostr_sdk::EventBuilder::new(Kind::CashuWalletUnspentProof, encrypted);

                    // Publish immediately to get real event ID
                    match client.send_event_builder(builder.clone()).await {
                        Ok(event_output) => {
                            let real_id = event_output.id().to_hex();
                            log::info!("Published MPP token event for {}: {}", mint_url, real_id);
                            new_event_ids.push(real_id.clone());

                            // Prepare new token data for local state
                            new_tokens.push(TokenData {
                                event_id: real_id,
                                mint: mint_url.clone(),
                                unit: "sat".to_string(),
                                proofs: proof_data,
                                created_at: chrono::Utc::now().timestamp() as u64,
                            });
                        }
                        Err(e) => {
                            log::warn!("Failed to publish MPP token event for {}, queuing for retry: {}", mint_url, e);
                            queue_event_for_retry(builder, PendingEventType::TokenEvent).await;
                            publish_failures += 1;
                        }
                    }
                }
            }
        }

        // STEP 5: Update local WALLET_TOKENS state
        {
            let store = WALLET_TOKENS.read();
            let mut data = store.data();
            let mut tokens_write = data.write();

            // Remove old token events for affected mints
            tokens_write.retain(|t| !all_event_ids_to_delete.contains(&t.event_id));

            // Add new tokens with remaining proofs
            for token in new_tokens {
                tokens_write.push(token);
            }

            // Update balance atomically
            let new_balance: u64 = tokens_write.iter()
                .flat_map(|t| &t.proofs)
                .map(|p| p.amount)
                .try_fold(0u64, |acc, amount| acc.checked_add(amount))
                .ok_or_else(|| "Balance calculation overflow in execute_mpp_melt".to_string())?;

            *WALLET_BALANCE.write() = new_balance;
            drop(tokens_write);

            log::info!("MPP melt: local state updated. New balance: {} sats", new_balance);
            if publish_failures > 0 {
                log::warn!("MPP melt: {} token event(s) queued for retry", publish_failures);
            }
        }

        // STEP 6: Publish deletion events for old token events (NIP-60 compliant)
        if !all_event_ids_to_delete.is_empty() {
            let valid_event_ids: Vec<_> = all_event_ids_to_delete.iter()
                .filter(|id| EventId::from_hex(id).is_ok())
                .collect();

            if !valid_event_ids.is_empty() {
                let mut tags = Vec::new();
                for event_id in &valid_event_ids {
                    tags.push(nostr_sdk::Tag::event(
                        EventId::from_hex(event_id).unwrap()
                    ));
                }

                // Add NIP-60 required tag
                tags.push(nostr_sdk::Tag::custom(
                    nostr_sdk::TagKind::custom("k"),
                    ["7375"]
                ));

                let deletion_builder = nostr_sdk::EventBuilder::new(
                    Kind::from(5),
                    "MPP melted tokens"
                ).tags(tags);

                match client.send_event_builder(deletion_builder.clone()).await {
                    Ok(_) => {
                        log::info!("Published MPP deletion events for {} token events", valid_event_ids.len());
                    }
                    Err(e) => {
                        log::warn!("Failed to publish MPP deletion event, will queue for retry: {}", e);
                        queue_event_for_retry(deletion_builder, PendingEventType::DeletionEvent).await;
                    }
                }
            }
        }

        // STEP 7: Create history event
        let valid_destroyed: Vec<String> = all_event_ids_to_delete.iter()
            .filter(|id| EventId::from_hex(id).is_ok())
            .cloned()
            .collect();

        if let Err(e) = create_history_event_with_type(
            "out",
            total_paid + total_fee,
            new_event_ids,
            valid_destroyed,
            Some("mpp_lightning_melt"),
            None, // MPP doesn't have a single invoice to reference
        ).await {
            log::warn!("Failed to create MPP history event: {}", e);
        }
    }

    // Sync wallet state after melt (updates balance from CDK's perspective)
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

// =============================================================================
// MPP Support Detection
// =============================================================================

/// Check if a mint supports MPP (NUT-15) with caching
///
/// Checks the mint info's `nuts.nut15.methods` array. Results are cached
/// for 5 minutes to reduce network requests.
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
            !info.nuts.nut15.methods.is_empty()
        }
        _ => false,
    }
}
