//! Lightning integration
//!
//! Functions for mint/melt operations (lightning topup and withdrawal).
use super::events::{publish_quote_event, queue_event_for_retry};
use super::internal::{
    cleanup_spent_proofs_internal, create_ephemeral_wallet, is_token_spent_error_string,
    remove_melt_quote_from_db,
};
use super::proofs::{
    cdk_proof_to_proof_data, proof_data_to_cdk_proof, register_proofs_in_event_map,
};
use super::recovery::is_quote_about_to_expire;
use super::signals::{
    add_in_flight_melt_request, persist_in_flight_melt_requests, persist_single_in_flight_request,
    remove_in_flight_melt_request, try_acquire_mint_lock, MELT_PROGRESS, PENDING_MELT_QUOTES,
    PENDING_MINT_QUOTES, WALLET_TOKENS,
};
use super::types::PendingEventType;
use super::types::{
    ExtendedCashuProof, ExtendedTokenEvent, InFlightMeltRequest, MeltProgress, MeltQuoteInfo,
    MeltQuoteState, MintQuoteInfo, MintQuoteState, PendingMeltQuotesStoreStoreExt,
    PendingMintQuotesStoreStoreExt, ProofData, TokenData, WalletTokensStoreStoreExt,
};
use super::utils::{mint_matches, normalize_mint_url};
use crate::stores::{auth_store, cashu_cdk_bridge, nostr_client};
use dioxus::prelude::*;
use nostr_sdk::signer::NostrSigner;
use nostr_sdk::{EventId, Kind, PublicKey};
/// Create a mint quote (request lightning invoice to receive sats)
pub async fn create_mint_quote(
    mint_url: String,
    amount_sats: u64,
    description: Option<String>,
) -> Result<MintQuoteInfo, String> {
    use cdk::Amount;
    log::info!(
        "Creating mint quote for {} sats at {}",
        amount_sats,
        mint_url
    );
    let wallet = create_ephemeral_wallet(&mint_url, vec![]).await?;
    let quote = wallet
        .mint_quote(Amount::from(amount_sats), description)
        .await
        .map_err(|e| format!("Failed to create mint quote: {}", e))?;
    log::info!("Mint quote created: {}", quote.id);
    let quote_info = MintQuoteInfo::from_cdk(&quote, mint_url.clone());
    PENDING_MINT_QUOTES
        .read()
        .data()
        .write()
        .push(quote_info.clone());
    match publish_quote_event(&quote.id, &mint_url, 14).await {
        Ok(event_id) => {
            log::info!("Quote event published: {}", event_id);
        }
        Err(e) => {
            log::warn!("Failed to publish quote event: {}", e);
        }
    }
    Ok(quote_info)
}
/// Check mint quote payment status
/// Returns CDK's MintQuoteState directly for better type safety
pub async fn check_mint_quote_status(
    mint_url: String,
    quote_id: String,
) -> Result<MintQuoteState, String> {
    log::info!("Checking mint quote status: {}", quote_id);
    let wallet = create_ephemeral_wallet(&mint_url, vec![]).await?;
    let response = wallet
        .mint_quote_state(&quote_id)
        .await
        .map_err(|e| format!("Failed to check mint quote status: {}", e))?;
    log::info!("Quote {} status: {:?}", quote_id, response.state);
    Ok(response.state)
}
/// Mint tokens from a paid quote
pub async fn mint_tokens_from_quote(mint_url: String, quote_id: String) -> Result<u64, String> {
    use cdk::nuts::MintQuoteState;
    let mint_url = normalize_mint_url(&mint_url);
    log::info!("Minting tokens from quote: {}", quote_id);
    let wallet = create_ephemeral_wallet(&mint_url, vec![]).await?;
    let quote_response = wallet
        .mint_quote_state(&quote_id)
        .await
        .map_err(|e| format!("Failed to fetch quote state: {}", e))?;
    log::info!(
        "Quote state: {:?}, amount: {:?}, expiry: {:?}",
        quote_response.state,
        quote_response.amount,
        quote_response.expiry
    );
    if is_quote_about_to_expire(quote_response.expiry) {
        return Err(format!(
            "Mint quote {} has expired or is expiring soon. Please create a new quote.",
            quote_id,
        ));
    }
    match quote_response.state {
        MintQuoteState::Paid => {}
        MintQuoteState::Issued => {
            return Err(
                "Quote has already been minted. Tokens were already issued for this payment."
                    .to_string(),
            );
        }
        MintQuoteState::Unpaid => {
            return Err(
                "Quote has not been paid yet. Please pay the lightning invoice first.".to_string(),
            );
        }
    }
    log::info!("Quote is paid, proceeding to mint tokens");
    let proofs = match wallet
        .mint(&quote_id, cdk::amount::SplitTarget::default(), None)
        .await
    {
        Ok(proofs) => {
            log::info!("Mint succeeded, received {} proofs", proofs.len());
            proofs
        }
        Err(e) => {
            let error_msg = e.to_string();
            log::error!("Mint failed: {}", error_msg);
            if let Err(cleanup_err) = wallet.localstore.remove_mint_quote(&quote_id).await {
                log::warn!("Failed to remove mint quote after error: {}", cleanup_err);
            }
            PENDING_MINT_QUOTES
                .read()
                .data()
                .write()
                .retain(|q| q.quote_id != quote_id);
            if error_msg.contains("missing field `signatures`") {
                return Err(format!(
                    "Mint returned an error. The quote has been cleaned up. \
                    Please generate a NEW invoice and try again. Error: {}",
                    error_msg,
                ));
            }
            return Err(format!("Failed to mint tokens: {}", error_msg));
        }
    };
    let amount_minted: u64 = proofs
        .iter()
        .map(|p| u64::from(p.amount))
        .try_fold(0u64, |acc, amt| acc.checked_add(amt))
        .ok_or("Minted amount overflow")?;
    log::info!("Minted {} sats", amount_minted);
    let proof_data: Vec<ProofData> = proofs.iter().map(cdk_proof_to_proof_data).collect();
    let extended_proofs: Vec<ExtendedCashuProof> = proof_data
        .iter()
        .map(|p| ExtendedCashuProof::from(p.clone()))
        .collect();
    let token_event_data = ExtendedTokenEvent {
        mint: mint_url.clone(),
        unit: "sat".to_string(),
        proofs: extended_proofs,
        del: vec![],
    };
    let signer = crate::stores::signer::get_signer()
        .ok_or("No signer available")?
        .as_nostr_signer();
    let pubkey_str = auth_store::get_pubkey().ok_or("Not authenticated")?;
    let pubkey = PublicKey::parse(&pubkey_str).map_err(|e| format!("Invalid pubkey: {}", e))?;
    let json_content = serde_json::to_string(&token_event_data)
        .map_err(|e| format!("Failed to serialize token event: {}", e))?;
    let encrypted = signer
        .nip44_encrypt(&pubkey, &json_content)
        .await
        .map_err(|e| format!("Failed to encrypt token event: {}", e))?;
    let builder = nostr_sdk::EventBuilder::new(Kind::CashuWalletUnspentProof, encrypted);
    let client = nostr_client::NOSTR_CLIENT
        .read()
        .as_ref()
        .ok_or("Client not initialized")?
        .clone();
    let event_output = client
        .send_event_builder(crate::utils::nips::nip89::tag_event_builder(builder))
        .await
        .map_err(|e| format!("Failed to publish event: {}", e))?;
    if event_output.success.is_empty() {
        return Err("Failed to publish event: no relays accepted the event".to_string());
    }
    let event_id = event_output.id().to_hex();
    log::info!("Published token event: {}", event_id);
    {
        let store = WALLET_TOKENS.read();
        let mut data = store.data();
        let mut tokens = data.write();
        tokens.push(TokenData {
            event_id: event_id.clone(),
            mint: mint_url.clone(),
            unit: "sat".to_string(),
            proofs: proof_data.clone(),
            created_at: chrono::Utc::now().timestamp() as u64,
        });
        register_proofs_in_event_map(&event_id, &proof_data);
    }
    super::signals::update_wallet_balances();
    create_history_event_with_type(
        "in",
        amount_minted,
        vec![event_id.clone()],
        vec![],
        Some("lightning_mint"),
        None,
    )
    .await?;
    if let Err(e) = wallet.localstore.remove_mint_quote(&quote_id).await {
        log::warn!("Failed to remove mint quote from database: {}", e);
    }
    PENDING_MINT_QUOTES
        .read()
        .data()
        .write()
        .retain(|q| q.quote_id != quote_id);
    if let Err(e) = cashu_cdk_bridge::sync_wallet_state().await {
        log::warn!("Failed to sync MultiMintWallet state after mint: {}", e);
    }
    log::info!("Mint complete: {} sats", amount_minted);
    Ok(amount_minted)
}
/// Create a melt quote (request to pay a lightning invoice)
pub async fn create_melt_quote(mint_url: String, invoice: String) -> Result<MeltQuoteInfo, String> {
    log::info!("Creating melt quote for invoice at {}", mint_url);
    *MELT_PROGRESS.write() = Some(MeltProgress::CreatingQuote);
    let wallet = create_ephemeral_wallet(&mint_url, vec![]).await?;
    let quote = wallet
        .melt_quote(invoice.clone(), None)
        .await
        .map_err(|e| {
            *MELT_PROGRESS.write() = Some(MeltProgress::Failed {
                error: e.to_string(),
            });
            format!("Failed to create melt quote: {}", e)
        })?;
    log::info!("Melt quote created: {}", quote.id);
    *MELT_PROGRESS.write() = Some(MeltProgress::QuoteCreated {
        quote_id: quote.id.clone(),
        amount: u64::from(quote.amount),
        fee_reserve: u64::from(quote.fee_reserve),
    });
    let quote_info = MeltQuoteInfo::from_cdk(&quote, mint_url.clone());
    PENDING_MELT_QUOTES
        .read()
        .data()
        .write()
        .push(quote_info.clone());
    match publish_quote_event(&quote.id, &mint_url, 14).await {
        Ok(event_id) => {
            log::info!("Melt quote event published: {}", event_id);
        }
        Err(e) => {
            log::warn!("Failed to publish melt quote event: {}", e);
        }
    }
    Ok(quote_info)
}
/// Check melt quote status
/// Returns CDK's MeltQuoteState directly for better type safety
#[allow(dead_code)]
pub async fn check_melt_quote_status(
    mint_url: String,
    quote_id: String,
) -> Result<MeltQuoteState, String> {
    log::info!("Checking melt quote status: {}", quote_id);
    let wallet = create_ephemeral_wallet(&mint_url, vec![]).await?;
    let response = wallet
        .melt_quote_status(&quote_id)
        .await
        .map_err(|e| format!("Failed to check melt quote status: {}", e))?;
    log::info!("Melt quote {} status: {:?}", quote_id, response.state);
    Ok(response.state)
}
/// Melt tokens to pay a lightning invoice
pub async fn melt_tokens(
    mint_url: String,
    quote_id: String,
) -> Result<(bool, Option<String>, u64), String> {
    let mint_url = normalize_mint_url(&mint_url);
    log::info!("Melting tokens to pay invoice via quote: {}", quote_id);
    *MELT_PROGRESS.write() = Some(MeltProgress::PreparingPayment);
    let _lock_guard = try_acquire_mint_lock(&mint_url).ok_or_else(|| {
        *MELT_PROGRESS.write() = Some(MeltProgress::Failed {
            error: format!("Another operation is in progress for mint: {}", mint_url),
        });
        format!("Another operation is in progress for mint: {}", mint_url)
    })?;
    let quote_info = PENDING_MELT_QUOTES
        .read()
        .data()
        .read()
        .iter()
        .find(|q| q.quote_id == quote_id)
        .cloned()
        .ok_or("Melt quote not found")?;
    if is_quote_about_to_expire(quote_info.expiry) {
        let error = format!(
            "Melt quote {} has expired or is expiring soon. Please create a new quote.",
            quote_id,
        );
        *MELT_PROGRESS.write() = Some(MeltProgress::Failed {
            error: error.clone(),
        });
        return Err(error);
    }
    let amount_needed = quote_info
        .amount
        .checked_add(quote_info.fee_reserve)
        .ok_or("Amount + fee overflow")?;
    let (all_proofs, event_ids_to_delete) = get_proofs_and_events_for_mint(&mint_url)?;
    if all_proofs.is_empty() {
        return Err("No tokens found for this mint".to_string());
    }
    let total_available: u64 = all_proofs
        .iter()
        .map(|p| u64::from(p.amount))
        .try_fold(0u64, |acc, amt| acc.checked_add(amt))
        .ok_or("Available balance overflow")?;
    if total_available < amount_needed {
        return Err(format!(
            "Insufficient funds. Need {} sats (amount: {}, fee: {}), have: {} sats",
            amount_needed, quote_info.amount, quote_info.fee_reserve, total_available,
        ));
    }
    *MELT_PROGRESS.write() = Some(MeltProgress::PayingInvoice);
    let tx_id = format!("melt_{}", uuid::Uuid::new_v4());
    let in_flight = InFlightMeltRequest {
        transaction_id: tx_id.clone(),
        mint_url: mint_url.clone(),
        quote_id: quote_id.clone(),
        proofs_used: all_proofs.iter().map(cdk_proof_to_proof_data).collect(),
        amount: quote_info.amount,
        fee_reserve: quote_info.fee_reserve,
        created_at: super::utils::now_secs(),
    };
    if let Err(e) = persist_single_in_flight_request(&in_flight).await {
        *MELT_PROGRESS.write() = Some(MeltProgress::Failed {
            error: format!("Failed to persist recovery data: {}", e),
        });
        return Err(format!(
            "Cannot proceed with melt: failed to persist recovery data. {}",
            e
        ));
    }
    add_in_flight_melt_request(in_flight);
    let (melted, keep_proofs) = match super::internal::try_operation_or_recover(
        &mint_url,
        all_proofs.clone(),
        execute_melt_with_retry(&mint_url, &quote_id, all_proofs, amount_needed),
    )
    .await
    {
        Ok(result) => result,
        Err(e) => {
            remove_in_flight_melt_request(&tx_id);
            if let Err(persist_err) = persist_in_flight_melt_requests().await {
                log::warn!(
                    "Failed to persist in-flight melt cleanup for tx_id={}: {} (state may diverge, recovery will handle on restart)",
                    tx_id, persist_err
                );
            }
            return Err(e);
        }
    };
    let paid = melted.state == cdk::nuts::MeltQuoteState::Paid;
    let preimage = melted.preimage;
    let fee_paid = u64::from(melted.fee_paid);
    log::info!("Melt result: paid={}, fee_paid={}", paid, fee_paid);
    if fee_paid > quote_info.fee_reserve {
        log::warn!(
            "Fee overcharge detected: paid {} sats but reserve was {} sats ({}% over)",
            fee_paid,
            quote_info.fee_reserve,
            ((fee_paid as f64 / quote_info.fee_reserve as f64) - 1.0) * 100.0
        );
    }
    if paid {
        *MELT_PROGRESS.write() = Some(MeltProgress::Completed {
            total_paid: quote_info.amount.saturating_add(fee_paid),
            fee_paid,
            preimage: preimage.clone(),
        });
    } else {
        *MELT_PROGRESS.write() = Some(MeltProgress::WaitingForConfirmation);
    }
    let pending_event_id = format!("pending_{}", uuid::Uuid::new_v4());
    update_local_state_after_melt(
        &mint_url,
        &keep_proofs,
        &event_ids_to_delete,
        &Some(pending_event_id.clone()),
    )?;
    let new_event_id =
        match publish_melt_events(&mint_url, &keep_proofs, &event_ids_to_delete).await {
            Ok(Some(real_event_id)) => {
                super::events::update_token_event_id(&pending_event_id, &real_event_id);
                Some(real_event_id)
            }
            Ok(None) => None,
            Err(e) => {
                log::warn!("Nostr melt publish failed, queued for retry: {}", e);
                Some(pending_event_id.clone())
            }
        };
    let valid_created: Vec<String> = new_event_id
        .iter()
        .filter(|id| !id.starts_with("pending_"))
        .cloned()
        .collect();
    let valid_destroyed: Vec<String> = event_ids_to_delete
        .iter()
        .filter(|id| !id.starts_with("pending_") && EventId::from_hex(id).is_ok())
        .cloned()
        .collect();
    let total_amount = quote_info
        .amount
        .checked_add(fee_paid)
        .ok_or_else(|| "Overflow adding quote amount and fee".to_string())?;
    if !valid_created.is_empty() || !valid_destroyed.is_empty() {
        if let Err(e) = create_history_event_with_type(
            "out",
            total_amount,
            valid_created,
            valid_destroyed,
            Some("lightning_melt"),
            Some(&quote_info.invoice),
        )
        .await
        {
            log::error!("Failed to create melt history event: {}", e);
        }
    }
    if let Err(e) = remove_melt_quote_from_db(&quote_id).await {
        log::warn!("Failed to remove melt quote from database: {}", e);
    }
    PENDING_MELT_QUOTES
        .read()
        .data()
        .write()
        .retain(|q| q.quote_id != quote_id);
    if let Err(e) = cashu_cdk_bridge::sync_wallet_state().await {
        log::warn!("Failed to sync MultiMintWallet state after melt: {}", e);
    }
    remove_in_flight_melt_request(&tx_id);
    if let Err(e) = persist_in_flight_melt_requests().await {
        log::error!(
            "Failed to persist in-flight melt cleanup for tx_id={}: {}. \
             May cause spurious recovery on next startup.",
            tx_id,
            e
        );
    }
    log::info!(
        "Melt complete: paid={}, amount={}, fee={}",
        paid,
        quote_info.amount,
        fee_paid
    );
    Ok((paid, preimage, fee_paid))
}
/// Get proofs and event IDs for a specific mint
fn get_proofs_and_events_for_mint(
    mint_url: &str,
) -> Result<(Vec<cdk::nuts::Proof>, Vec<String>), String> {
    let store = WALLET_TOKENS.read();
    let data = store.data();
    let tokens = data.read();
    let mint_tokens: Vec<_> = tokens
        .iter()
        .filter(|t| mint_matches(&t.mint, mint_url))
        .collect();
    let mut all_proofs = Vec::new();
    let mut event_ids_to_delete = Vec::new();
    for token in &mint_tokens {
        event_ids_to_delete.push(token.event_id.clone());
        for proof in &token.proofs {
            all_proofs.push(proof_data_to_cdk_proof(proof)?);
        }
    }
    Ok((all_proofs, event_ids_to_delete))
}
/// Execute melt with auto-retry on spent proofs
async fn execute_melt_with_retry(
    mint_url: &str,
    quote_id: &str,
    all_proofs: Vec<cdk::nuts::Proof>,
    amount_needed: u64,
) -> Result<(cdk::types::Melted, Vec<cdk::nuts::Proof>), String> {
    let result = async {
        let wallet = create_ephemeral_wallet(mint_url, all_proofs.clone()).await?;
        let melted = wallet.melt(quote_id).await.map_err(|e| e.to_string())?;
        let keep_proofs = wallet
            .get_unspent_proofs()
            .await
            .map_err(|e| e.to_string())?;
        Ok::<(cdk::types::Melted, Vec<cdk::nuts::Proof>), String>((melted, keep_proofs))
    }
    .await;
    match result {
        Ok((melted, proofs)) => Ok((melted, proofs)),
        Err(e) => {
            if is_token_spent_error_string(&e) {
                log::warn!("Some proofs already spent, cleaning up and retrying...");
                let (cleaned_count, cleaned_amount) =
                    cleanup_spent_proofs_internal(mint_url).await?;
                log::info!(
                    "Cleaned up {} spent proofs worth {} sats, retrying melt",
                    cleaned_count,
                    cleaned_amount
                );
                let (fresh_proofs, _) = get_proofs_and_events_for_mint(mint_url)?;
                let fresh_total: u64 = fresh_proofs
                    .iter()
                    .map(|p| u64::from(p.amount))
                    .try_fold(0u64, |acc, amt| acc.checked_add(amt))
                    .ok_or("Fresh proofs balance overflow")?;
                if fresh_total < amount_needed {
                    return Err(format!(
                        "Insufficient funds after cleanup. Need: {} sats, have: {} sats",
                        amount_needed, fresh_total,
                    ));
                }
                let wallet = create_ephemeral_wallet(mint_url, fresh_proofs).await?;
                let melted = wallet
                    .melt(quote_id)
                    .await
                    .map_err(|e| format!("Retry failed: {}", e))?;
                let keep_proofs = wallet
                    .get_unspent_proofs()
                    .await
                    .map_err(|e| format!("Failed to get remaining proofs: {}", e))?;
                log::info!("Melt succeeded after cleanup and retry");
                Ok((melted, keep_proofs))
            } else {
                if let Err(cleanup_err) = remove_melt_quote_from_db(quote_id).await {
                    log::error!("Failed to remove melt quote: {}", cleanup_err);
                }
                PENDING_MELT_QUOTES
                    .read()
                    .data()
                    .write()
                    .retain(|q| q.quote_id != quote_id);
                *MELT_PROGRESS.write() = Some(MeltProgress::Failed {
                    error: e.to_string(),
                });
                Err(format!("Failed to melt: {}. Quote has been cleaned up.", e))
            }
        }
    }
}
/// Publish token and deletion events after melt
async fn publish_melt_events(
    mint_url: &str,
    keep_proofs: &[cdk::nuts::Proof],
    event_ids_to_delete: &[String],
) -> Result<Option<String>, String> {
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
    let mut new_event_id: Option<String> = None;
    if !keep_proofs.is_empty() {
        let proof_data: Vec<ProofData> = keep_proofs.iter().map(cdk_proof_to_proof_data).collect();
        let extended_proofs: Vec<ExtendedCashuProof> = proof_data
            .iter()
            .map(|p| ExtendedCashuProof::from(p.clone()))
            .collect();
        let token_event_data = ExtendedTokenEvent {
            mint: mint_url.to_string(),
            unit: "sat".to_string(),
            proofs: extended_proofs,
            del: event_ids_to_delete.to_vec(),
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
                let real_id = event_output.id().to_hex();
                log::info!("Published new token event: {}", real_id);
                new_event_id = Some(real_id);
            }
            Ok(_) => {
                log::warn!("No relays accepted token event, queuing for retry");
                let pending_id = format!("pending_{}", uuid::Uuid::new_v4());
                queue_event_for_retry(
                    builder,
                    PendingEventType::TokenEvent,
                    Some(pending_id.clone()),
                    Some(mint_url.to_string()),
                )
                .await;
                new_event_id = Some(pending_id);
            }
            Err(e) => {
                log::warn!("Failed to publish token event, queuing for retry: {}", e);
                let pending_id = format!("pending_{}", uuid::Uuid::new_v4());
                queue_event_for_retry(
                    builder,
                    PendingEventType::TokenEvent,
                    Some(pending_id.clone()),
                    Some(mint_url.to_string()),
                )
                .await;
                new_event_id = Some(pending_id);
            }
        }
    }
    if !event_ids_to_delete.is_empty() {
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
                nostr_sdk::EventBuilder::new(Kind::from(5), "Melted token").tags(tags);
            match client
                .send_event_builder(crate::utils::nips::nip89::tag_event_builder(
                    deletion_builder.clone(),
                ))
                .await
            {
                Ok(output) if !output.success.is_empty() => {
                    log::info!(
                        "Published deletion events for {} token events",
                        valid_event_ids.len()
                    );
                }
                Ok(_) => {
                    log::warn!("No relays accepted deletion event, queuing for retry");
                    queue_event_for_retry(
                        deletion_builder,
                        PendingEventType::DeletionEvent,
                        None,
                        None,
                    )
                    .await;
                }
                Err(e) => {
                    log::warn!("Failed to publish deletion event, queuing for retry: {}", e);
                    queue_event_for_retry(
                        deletion_builder,
                        PendingEventType::DeletionEvent,
                        None,
                        None,
                    )
                    .await;
                }
            }
        }
    }
    Ok(new_event_id)
}
/// Update local state after melt using atomic token replacement
///
/// Uses crash-safe atomic replacement: add new tokens BEFORE deleting old ones.
/// This ensures worst case on crash is duplicate tokens (recoverable), never lost tokens.
fn update_local_state_after_melt(
    mint_url: &str,
    keep_proofs: &[cdk::nuts::Proof],
    event_ids_to_delete: &[String],
    new_event_id: &Option<String>,
) -> Result<(), String> {
    let tokens_to_add = if let Some(ref event_id) = new_event_id {
        if keep_proofs.is_empty() {
            vec![]
        } else {
            let proof_data: Vec<ProofData> =
                keep_proofs.iter().map(cdk_proof_to_proof_data).collect();
            vec![TokenData {
                event_id: event_id.clone(),
                mint: mint_url.to_string(),
                unit: "sat".to_string(),
                proofs: proof_data,
                created_at: chrono::Utc::now().timestamp() as u64,
            }]
        }
    } else {
        vec![]
    };
    let new_balance = super::signals::atomic_token_replace(tokens_to_add, event_ids_to_delete)?;
    super::proofs::rebuild_proof_event_map();
    log::info!("Local state updated. New balance: {} sats", new_balance);
    Ok(())
}
/// Create a history event with operation type metadata
///
/// Extension fields (type, invoice) are non-standard but safe per JSON parsing.
pub async fn create_history_event_with_type(
    direction: &str,
    amount: u64,
    created_tokens: Vec<String>,
    destroyed_tokens: Vec<String>,
    operation_type: Option<&str>,
    invoice: Option<&str>,
) -> Result<(), String> {
    let signer = crate::stores::signer::get_signer()
        .ok_or("No signer available")?
        .as_nostr_signer();
    let pubkey_str = auth_store::get_pubkey().ok_or("Not authenticated")?;
    let pubkey = PublicKey::parse(&pubkey_str).map_err(|e| format!("Invalid pubkey: {}", e))?;
    let mut content_array = vec![
        vec!["direction".to_string(), direction.to_string()],
        vec!["amount".to_string(), amount.to_string()],
    ];
    content_array.push(vec!["unit".to_string(), "sat".to_string()]);
    if let Some(op_type) = operation_type {
        content_array.push(vec!["type".to_string(), op_type.to_string()]);
    }
    if let Some(inv) = invoice {
        content_array.push(vec!["invoice".to_string(), inv.to_string()]);
    }
    for event_id in created_tokens {
        content_array.push(vec![
            "e".to_string(),
            event_id,
            "".to_string(),
            "created".to_string(),
        ]);
    }
    for event_id in destroyed_tokens {
        content_array.push(vec![
            "e".to_string(),
            event_id,
            "".to_string(),
            "destroyed".to_string(),
        ]);
    }
    let json_content =
        serde_json::to_string(&content_array).map_err(|e| format!("Failed to serialize: {}", e))?;
    let encrypted = signer
        .nip44_encrypt(&pubkey, &json_content)
        .await
        .map_err(|e| format!("Failed to encrypt: {}", e))?;
    let builder = nostr_sdk::EventBuilder::new(Kind::CashuWalletSpendingHistory, encrypted);
    let client = nostr_client::NOSTR_CLIENT
        .read()
        .as_ref()
        .ok_or("Client not initialized")?
        .clone();
    let event_output = client
        .send_event_builder(crate::utils::nips::nip89::tag_event_builder(builder))
        .await
        .map_err(|e| format!("Failed to publish history event: {}", e))?;
    if event_output.success.is_empty() {
        return Err("Failed to publish history event: no relays accepted the event".to_string());
    }
    log::info!("Published history event: {}", event_output.id().to_hex());
    Ok(())
}
