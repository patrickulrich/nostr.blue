//! Multi-path payments (MPP)
//!
//! Functions for splitting payments across multiple mints (NUT-15).
//!
//! This module provides:
//! - Balance queries per mint
//! - Optimal MPP split calculation (greedy algorithm)
//! - MPP quote creation with `MeltOptions::new_mpp(amount_msat)` per mint
//! - MPP melt execution (sequential per-mint, no cross-mint atomicity)
//! - Mint MPP support detection with caching
use super::events::queue_event_for_retry;
use super::lightning::create_history_event_with_type;
use super::proofs::cdk_proof_to_proof_data;
use super::signals::WALLET_TOKENS;
use super::types::{
    ExtendedCashuProof, ExtendedTokenEvent, PendingEventType, ProofData, TokenData,
    WalletTokensStoreStoreExt,
};
use crate::stores::cashu_cdk_bridge::{sync_wallet_state, MULTI_WALLET};
use crate::stores::{auth_store, nostr_client};
use dioxus::prelude::*;
use nostr_sdk::signer::NostrSigner;
use nostr_sdk::{EventId, Kind, PublicKey};
use std::collections::HashMap;
/// Cache for mint MPP support: mint_url -> (timestamp_ms, supports_mpp)
/// TTL is 5 minutes (300,000 ms)
pub static MINT_MPP_CACHE: GlobalSignal<HashMap<String, (f64, bool)>> =
    Signal::global(HashMap::new);
pub const MINT_INFO_CACHE_TTL_MS: f64 = 300_000.0;
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
/// Get balance breakdown per mint
pub async fn get_balances_per_mint() -> Result<Vec<MintBalance>, String> {
    let multi_wallet = MULTI_WALLET
        .read()
        .as_ref()
        .ok_or("Wallet not initialized")?
        .clone();
    let balances = multi_wallet
        .get_balances()
        .await
        .map_err(|e| format!("Failed to get balances: {}", e))?;
    Ok(balances
        .iter()
        .map(|(url, amount)| MintBalance {
            mint_url: url.to_string(),
            balance: u64::from(*amount),
        })
        .collect())
}
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
    let available: Vec<_> = balances
        .into_iter()
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
    let total_available: u64 = available
        .iter()
        .map(|b| b.balance)
        .try_fold(0u64, |acc, v| acc.checked_add(v))
        .ok_or("Balance sum overflow in MPP split calculation")?;
    if total_available < target_amount {
        return Err(format!(
            "Insufficient total balance: {} sats available, {} sats needed",
            total_available, target_amount,
        ));
    }
    let mut sorted = available;
    sorted.sort_by_key(|b| std::cmp::Reverse(b.balance));
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
///
/// Creates individual melt quotes per mint, each with `MeltOptions::new_mpp(amount_msat)`
/// so the mint's Lightning backend knows it is only paying a partial amount of the invoice.
pub async fn create_mpp_melt_quotes(
    bolt11: String,
    mint_amounts: Vec<(String, u64)>,
) -> Result<MppQuoteInfo, String> {
    use cdk::mint_url::MintUrl;
    let multi_wallet = MULTI_WALLET
        .read()
        .as_ref()
        .ok_or("Wallet not initialized")?
        .clone();
    let mut contributions: Vec<MppQuoteContribution> = Vec::new();
    for (mint_url_str, amount) in &mint_amounts {
        let mint_url: MintUrl = mint_url_str
            .parse()
            .map_err(|e| format!("Invalid mint URL {}: {}", mint_url_str, e))?;
        let wallet = multi_wallet.get_wallet(&mint_url, &cdk::nuts::CurrencyUnit::Sat).await
            .map_err(|e| format!("MPP: wallet not found for {}: {}", mint_url_str, e))?;
        let amount_msat = *amount * 1000;
        let options = cdk::nuts::MeltOptions::new_mpp(amount_msat);
        let quote = wallet.melt_quote(cdk::nuts::PaymentMethod::BOLT11, bolt11.clone(), Some(options), None).await
            .map_err(|e| format!("MPP: melt quote failed for {}: {}", mint_url_str, e))?;
        contributions.push(MppQuoteContribution {
            mint_url: mint_url_str.clone(),
            quote_id: quote.id.clone(),
            amount: *amount,
            fee_reserve: u64::from(quote.fee_reserve),
        });
    }
    let total_amount = contributions
        .iter()
        .map(|c| c.amount)
        .try_fold(0u64, |acc, v| acc.checked_add(v))
        .ok_or("MPP quote total amount overflow")?;
    let total_fee_reserve = contributions
        .iter()
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
///
/// Executes melts sequentially per mint. If one mint's melt fails after another
/// succeeds, there is no rollback — the successful mint's proofs are spent.
/// This matches the behavior of the former CDK `MultiMintWallet::mpp_melt()`.
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
    let multi_wallet = MULTI_WALLET
        .read()
        .as_ref()
        .ok_or("Wallet not initialized")?
        .clone();
    let affected_mints: Vec<String> = quote_contributions
        .iter()
        .map(|c| c.mint_url.clone())
        .collect();
    let event_ids_by_mint: HashMap<String, Vec<String>> = {
        let store = WALLET_TOKENS.read();
        let data = store.data();
        let tokens = data.read();
        let mut map: HashMap<String, Vec<String>> = HashMap::new();
        for mint_url in &affected_mints {
            let event_ids: Vec<String> = tokens
                .iter()
                .filter(|t| &t.mint == mint_url)
                .map(|t| t.event_id.clone())
                .collect();
            if !event_ids.is_empty() {
                map.insert(mint_url.clone(), event_ids);
            }
        }
        map
    };
    let quotes: Vec<(MintUrl, String)> = quote_contributions
        .iter()
        .map(|c| {
            let mint_url: MintUrl = c
                .mint_url
                .parse()
                .map_err(|e| format!("Invalid mint URL {}: {}", c.mint_url, e))?;
            Ok((mint_url, c.quote_id.clone()))
        })
        .collect::<Result<Vec<_>, String>>()?;
    let mut melt_results: Vec<(MintUrl, cdk::types::FinalizedMelt)> = Vec::new();
    // NOTE: Melts execute sequentially per-mint with no cross-mint rollback.
    // If mint N+1 fails after mint N succeeds, mint N's proofs are already spent.
    // Individual mint melts are crash-safe via CDK's saga system.
    for (mint_url, quote_id) in quotes {
        let wallet = multi_wallet.get_wallet(&mint_url, &cdk::nuts::CurrencyUnit::Sat).await
            .map_err(|e| format!("MPP: wallet not found for {}: {}", mint_url, e))?;
        let prepared = wallet.prepare_melt(&quote_id, std::collections::HashMap::new()).await
            .map_err(|e| format!("MPP prepare failed for {}: {}", mint_url, e))?;
        let finalized = prepared.confirm().await
            .map_err(|e| format!("MPP confirm failed for {}: {}", mint_url, e))?;
        melt_results.push((mint_url, finalized));
    }
    let mut total_paid = 0u64;
    let mut total_fee = 0u64;
    let mut preimage: Option<String> = None;
    let mut all_paid = true;
    for (url, finalized) in &melt_results {
        log::info!(
            "MPP contribution from {}: paid={}, fee={}",
            url,
            u64::from(finalized.amount()),
            u64::from(finalized.fee_paid())
        );
        total_paid = total_paid
            .checked_add(u64::from(finalized.amount()))
            .ok_or("MPP total amount overflow")?;
        total_fee = total_fee
            .checked_add(u64::from(finalized.fee_paid()))
            .ok_or("MPP total fee overflow")?;
        if preimage.is_none() && finalized.payment_proof().is_some() {
            preimage = finalized.payment_proof().map(|s| s.to_string());
        }
        if finalized.state() != cdk::nuts::MeltQuoteState::Paid {
            all_paid = false;
        }
    }
    if all_paid {
        let remaining_proofs = multi_wallet
            .list_proofs()
            .await
            .map_err(|e| format!("Failed to get remaining proofs: {}", e))?;
        let signer = crate::stores::signer::get_signer()
            .ok_or("No signer available")?
            .as_nostr_signer();
        let pubkey_str = auth_store::get_pubkey().ok_or("Not authenticated")?;
        let pubkey = PublicKey::parse(&pubkey_str).map_err(|e| format!("Invalid pubkey: {}", e))?;
        let _client = nostr_client::NOSTR_CLIENT
            .read()
            .as_ref()
            .ok_or("Client not initialized")?
            .clone();
        let mut new_event_ids: Vec<String> = Vec::new();
        let mut all_event_ids_to_delete: Vec<String> = Vec::new();
        let mut new_tokens: Vec<TokenData> = Vec::new();
        let mut publish_failures = 0;
        for mint_url in &affected_mints {
            let mint_url_parsed: MintUrl = mint_url
                .parse()
                .map_err(|e| format!("Invalid mint URL: {}", e))?;
            if let Some(event_ids) = event_ids_by_mint.get(mint_url) {
                all_event_ids_to_delete.extend(event_ids.clone());
            }
            let key = cdk_common::wallet::WalletKey::new(mint_url_parsed.clone(), cdk::nuts::CurrencyUnit::Sat);
            if let Some(proofs) = remaining_proofs.get(&key) {
                if !proofs.is_empty() {
                    let proof_data: Vec<ProofData> =
                        proofs.iter().map(cdk_proof_to_proof_data).collect();
                    let extended_proofs: Vec<ExtendedCashuProof> = proof_data
                        .iter()
                        .map(|p| ExtendedCashuProof::from(p.clone()))
                        .collect();
                    let event_ids_for_mint =
                        event_ids_by_mint.get(mint_url).cloned().unwrap_or_default();
                    let filtered_del: Vec<String> = event_ids_for_mint
                        .into_iter()
                        .filter(|id| nostr_sdk::EventId::from_hex(id).is_ok())
                        .collect();
                    let token_event_data = ExtendedTokenEvent {
                        mint: mint_url.clone(),
                        unit: "sat".to_string(),
                        proofs: extended_proofs,
                        del: filtered_del,
                    };
                    let json_content = serde_json::to_string(&token_event_data)
                        .map_err(|e| format!("Failed to serialize token event: {}", e))?;
                    let encrypted = signer
                        .nip44_encrypt(&pubkey, &json_content)
                        .await
                        .map_err(|e| format!("Failed to encrypt token event: {}", e))?;
                    let builder =
                        nostr_sdk::EventBuilder::new(Kind::CashuWalletUnspentProof, encrypted);
                    match crate::stores::publish_queue::signing::sign_event_builder(builder.clone()).await {
                        Ok(signed_event) => {
                            let real_id = signed_event.id.to_hex();
                            log::info!("Queued MPP token event for {}: {}", mint_url, real_id);
                            let mut metadata = std::collections::HashMap::new();
                            metadata.insert("pending_token_id".to_string(), real_id.clone());
                            metadata.insert("mint_url".to_string(), mint_url.clone());
                            crate::stores::publish_queue::enqueue(
                                signed_event,
                                crate::stores::publish_queue::types::QueueEventType::Cashu,
                                None,
                                metadata,
                            ).await;
                            new_event_ids.push(real_id.clone());
                            new_tokens.push(TokenData {
                                event_id: real_id,
                                mint: mint_url.clone(),
                                unit: "sat".to_string(),
                                proofs: proof_data,
                                created_at: super::proofs::now_secs(),
                            });
                        }
                        Err(e) => {
                            log::warn!(
                                "Failed to sign MPP token event for {}, queuing unsigned: {}",
                                mint_url,
                                e
                            );
                            let pending_id = format!("pending_{}", uuid::Uuid::new_v4());
                            if let Err(queue_err) = queue_event_for_retry(
                                builder,
                                PendingEventType::TokenEvent,
                                Some(pending_id.clone()),
                                Some(mint_url.clone()),
                            )
                            .await
                            {
                                log::error!(
                                    "Failed to queue MPP token event for retry: {}",
                                    queue_err
                                );
                            }
                            new_tokens.push(TokenData {
                                event_id: pending_id,
                                mint: mint_url.clone(),
                                unit: "sat".to_string(),
                                proofs: proof_data,
                                created_at: super::proofs::now_secs(),
                            });
                            publish_failures += 1;
                        }
                    }
                }
            }
        }
        {
            let store = WALLET_TOKENS.read();
            let mut data = store.data();
            let mut tokens_write = data.write();
            tokens_write.retain(|t| !all_event_ids_to_delete.contains(&t.event_id));
            for token in new_tokens {
                tokens_write.push(token);
            }
            drop(tokens_write);
            super::signals::update_wallet_balances();
            let new_balance = crate::stores::cashu_cdk_bridge::WALLET_BALANCES
                .read()
                .available;
            log::info!(
                "MPP melt: local state updated. New balance: {} sats",
                new_balance
            );
            if publish_failures > 0 {
                log::warn!(
                    "MPP melt: {} token event(s) queued for retry",
                    publish_failures
                );
            }
        }
        if !all_event_ids_to_delete.is_empty() {
        let valid_event_ids: Vec<EventId> = all_event_ids_to_delete
            .iter()
            .filter_map(|id| EventId::from_hex(id).ok())
            .collect();
        if !valid_event_ids.is_empty() {
            let mut tags: Vec<nostr_sdk::Tag> = valid_event_ids
                .iter()
                .map(|id| nostr_sdk::Tag::event(*id))
                .collect();
                tags.push(nostr_sdk::Tag::custom(
                    nostr_sdk::TagKind::custom("k"),
                    ["7375"],
                ));
                let deletion_builder =
                    nostr_sdk::EventBuilder::new(Kind::from(5), "MPP melted tokens").tags(tags);
                match crate::stores::publish_queue::signing::sign_event_builder(deletion_builder.clone()).await {
                    Ok(signed_event) => {
                        crate::stores::publish_queue::enqueue(
                            signed_event,
                            crate::stores::publish_queue::types::QueueEventType::Cashu,
                            None,
                            std::collections::HashMap::new(),
                            ).await;
                        log::info!(
                            "Queued MPP deletion events for {} token events",
                            valid_event_ids.len()
                        );
                    }
                    Err(e) => {
                        log::warn!("Failed to sign MPP deletion event, queuing unsigned: {}", e);
                        if let Err(queue_err) = queue_event_for_retry(
                            deletion_builder,
                            PendingEventType::DeletionEvent,
                            None,
                            None,
                        )
                        .await
                        {
                            log::error!(
                                "Failed to queue MPP deletion event for retry: {}",
                                queue_err
                            );
                        }
                    }
                }
            }
        }
        let valid_destroyed: Vec<String> = all_event_ids_to_delete
            .iter()
            .filter(|id| EventId::from_hex(id).is_ok())
            .cloned()
            .collect();
        if let Err(e) = create_history_event_with_type(
            "out",
            total_paid.saturating_add(total_fee),
            new_event_ids,
            valid_destroyed,
            Some("mpp_lightning_melt"),
            None,
        )
        .await
        {
            log::warn!("Failed to create MPP history event: {}", e);
        }
    }
    if let Err(e) = sync_wallet_state().await {
        log::warn!("Failed to sync wallet state after MPP melt: {}", e);
    }
    Ok(MppMeltResult {
        paid: all_paid,
        preimage,
        total_amount_paid: total_paid,
        total_fee_paid: total_fee,
        contributions: melt_results.len(),
    })
}
/// Check if a mint supports MPP (NUT-15) with caching
///
/// Checks the mint info's `nuts.nut15.methods` array. Results are cached
/// for 5 minutes to reduce network requests.
pub async fn mint_supports_mpp(mint_url: &str) -> bool {
    let now = crate::platform::timestamp::now_millis() as f64;
    {
        let cache = MINT_MPP_CACHE.read();
        if let Some((timestamp, supports)) = cache.get(mint_url) {
            if now - timestamp < MINT_INFO_CACHE_TTL_MS {
                return *supports;
            }
        }
    }
    let supports = fetch_mint_mpp_support(mint_url).await;
    MINT_MPP_CACHE
        .write()
        .insert(mint_url.to_string(), (now, supports));
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
    let wallet = match multi_wallet.get_wallet(&mint_url, &cdk::nuts::CurrencyUnit::Sat).await {
        Ok(w) => w,
        Err(_) => return false,
    };
    match wallet.fetch_mint_info().await {
        Ok(Some(info)) => !info.nuts.nut15.methods.is_empty(),
        _ => false,
    }
}
