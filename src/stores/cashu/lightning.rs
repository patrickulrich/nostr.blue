//! Lightning integration
//!
//! Functions for mint/melt operations (lightning topup and withdrawal).

use dioxus::prelude::*;
use nostr_sdk::signer::NostrSigner;
use nostr_sdk::{EventId, Kind, PublicKey};

use super::events::{publish_quote_event, queue_event_for_retry};
use super::recovery::is_quote_about_to_expire;
use super::internal::{
    cleanup_spent_proofs_internal, create_ephemeral_wallet, is_token_spent_error_string,
    remove_melt_quote_from_db,
};
use super::proofs::{cdk_proof_to_proof_data, proof_data_to_cdk_proof, register_proofs_in_event_map};
use super::signals::{
    try_acquire_mint_lock, MELT_PROGRESS, PENDING_MELT_QUOTES, PENDING_MINT_QUOTES, WALLET_BALANCE,
    WALLET_TOKENS,
};
use super::types::{
    ExtendedCashuProof, ExtendedTokenEvent, MeltProgress, MeltQuoteInfo, MintQuoteInfo,
    MintQuoteState, MeltQuoteState, ProofData, TokenData,
    PendingMintQuotesStoreStoreExt, PendingMeltQuotesStoreStoreExt, WalletTokensStoreStoreExt,
};
use super::utils::{mint_matches, normalize_mint_url};
use crate::stores::{auth_store, cashu_cdk_bridge, nostr_client};
use super::types::PendingEventType;

// =============================================================================
// Mint Quote Operations (Lightning → Ecash)
// =============================================================================

/// Create a mint quote (request lightning invoice to receive sats)
pub async fn create_mint_quote(
    mint_url: String,
    amount_sats: u64,
    description: Option<String>,
) -> Result<MintQuoteInfo, String> {
    use cdk::Amount;

    log::info!("Creating mint quote for {} sats at {}", amount_sats, mint_url);

    // Create ephemeral wallet
    let wallet = create_ephemeral_wallet(&mint_url, vec![]).await?;

    // Create mint quote
    let quote = wallet
        .mint_quote(Amount::from(amount_sats), description)
        .await
        .map_err(|e| format!("Failed to create mint quote: {}", e))?;

    log::info!("Mint quote created: {}", quote.id);

    let quote_info = MintQuoteInfo::from_cdk(&quote, mint_url.clone());

    // Store in global state for tracking
    PENDING_MINT_QUOTES
        .read()
        .data()
        .write()
        .push(quote_info.clone());

    // Publish quote event to Nostr (NIP-60 kind 7374) for cross-device sync
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

    // Verify the quote is paid and ready to mint
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

    // CDK best practice: Check quote expiry with safety margin before operations
    // to avoid race conditions where the quote expires mid-operation
    if is_quote_about_to_expire(quote_response.expiry) {
        return Err(format!(
            "Mint quote {} has expired or is expiring soon. Please create a new quote.",
            quote_id
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

    // Mint tokens
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

            // Clean up the quote from database on failure
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
                    error_msg
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

    // Convert to ProofData
    let proof_data: Vec<ProofData> = proofs.iter().map(|p| cdk_proof_to_proof_data(p)).collect();

    // Create token event
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
        .send_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to publish event: {}", e))?;

    let event_id = event_output.id().to_hex();

    log::info!("Published token event: {}", event_id);

    // Update local state
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

    // Update balance
    let current_balance = *WALLET_BALANCE.read();
    let new_balance = current_balance
        .checked_add(amount_minted)
        .ok_or_else(|| "Balance overflow".to_string())?;
    *WALLET_BALANCE.write() = new_balance;

    // Create history event
    create_history_event_with_type(
        "in",
        amount_minted,
        vec![event_id.clone()],
        vec![],
        Some("lightning_mint"),
        None,
    )
    .await?;

    // Clean up quote
    if let Err(e) = wallet.localstore.remove_mint_quote(&quote_id).await {
        log::warn!("Failed to remove mint quote from database: {}", e);
    }
    PENDING_MINT_QUOTES
        .read()
        .data()
        .write()
        .retain(|q| q.quote_id != quote_id);

    // Sync state
    if let Err(e) = cashu_cdk_bridge::sync_wallet_state().await {
        log::warn!("Failed to sync MultiMintWallet state after mint: {}", e);
    }

    log::info!("Mint complete: {} sats", amount_minted);

    Ok(amount_minted)
}

// =============================================================================
// Melt Quote Operations (Ecash → Lightning)
// =============================================================================

/// Create a melt quote (request to pay a lightning invoice)
pub async fn create_melt_quote(mint_url: String, invoice: String) -> Result<MeltQuoteInfo, String> {
    log::info!("Creating melt quote for invoice at {}", mint_url);

    *MELT_PROGRESS.write() = Some(MeltProgress::CreatingQuote);

    let wallet = create_ephemeral_wallet(&mint_url, vec![]).await?;

    let quote = wallet.melt_quote(invoice.clone(), None).await.map_err(|e| {
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

    // Publish quote event
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

    // Acquire mint operation lock
    let _lock_guard = try_acquire_mint_lock(&mint_url).ok_or_else(|| {
        *MELT_PROGRESS.write() = Some(MeltProgress::Failed {
            error: format!("Another operation is in progress for mint: {}", mint_url),
        });
        format!("Another operation is in progress for mint: {}", mint_url)
    })?;

    // Get melt quote details
    let quote_info = PENDING_MELT_QUOTES
        .read()
        .data()
        .read()
        .iter()
        .find(|q| q.quote_id == quote_id)
        .cloned()
        .ok_or("Melt quote not found")?;

    // CDK best practice: Check quote expiry with safety margin before operations
    if is_quote_about_to_expire(quote_info.expiry) {
        let error = format!(
            "Melt quote {} has expired or is expiring soon. Please create a new quote.",
            quote_id
        );
        *MELT_PROGRESS.write() = Some(MeltProgress::Failed {
            error: error.clone(),
        });
        return Err(error);
    }

    let amount_needed = quote_info.amount
        .checked_add(quote_info.fee_reserve)
        .ok_or("Amount + fee overflow")?;

    // Get available proofs
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
            amount_needed, quote_info.amount, quote_info.fee_reserve, total_available
        ));
    }

    *MELT_PROGRESS.write() = Some(MeltProgress::PayingInvoice);

    // Execute melt with auto-retry
    let (melted, keep_proofs) =
        execute_melt_with_retry(&mint_url, &quote_id, all_proofs, amount_needed).await?;

    let paid = melted.state == cdk::nuts::MeltQuoteState::Paid;
    let preimage = melted.preimage;
    let fee_paid = u64::from(melted.fee_paid);

    log::info!("Melt result: paid={}, fee_paid={}", paid, fee_paid);

    // Validate fee didn't exceed reserve (warn but don't fail - transaction already completed)
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

    // Publish events and update state
    let new_event_id =
        publish_melt_events(&mint_url, &keep_proofs, &event_ids_to_delete).await?;

    // Update local state
    update_local_state_after_melt(&mint_url, &keep_proofs, &event_ids_to_delete, &new_event_id)?;

    // Create history event
    let valid_created: Vec<String> = new_event_id.iter().cloned().collect();
    let valid_destroyed: Vec<String> = event_ids_to_delete
        .iter()
        .filter(|id| EventId::from_hex(id).is_ok())
        .cloned()
        .collect();

    create_history_event_with_type(
        "out",
        quote_info.amount + fee_paid,
        valid_created,
        valid_destroyed,
        Some("lightning_melt"),
        Some(&quote_info.invoice),
    )
    .await?;

    // Clean up quote
    if let Err(e) = remove_melt_quote_from_db(&quote_id).await {
        log::warn!("Failed to remove melt quote from database: {}", e);
    }
    PENDING_MELT_QUOTES
        .read()
        .data()
        .write()
        .retain(|q| q.quote_id != quote_id);

    // Sync state
    if let Err(e) = cashu_cdk_bridge::sync_wallet_state().await {
        log::warn!("Failed to sync MultiMintWallet state after melt: {}", e);
    }

    log::info!(
        "Melt complete: paid={}, amount={}, fee={}",
        paid,
        quote_info.amount,
        fee_paid
    );

    Ok((paid, preimage, fee_paid))
}

// =============================================================================
// Internal Helpers
// =============================================================================

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
        let keep_proofs = wallet.get_unspent_proofs().await.map_err(|e| e.to_string())?;

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

                // Get fresh proofs
                let (fresh_proofs, _) = get_proofs_and_events_for_mint(mint_url)?;

                let fresh_total: u64 = fresh_proofs
                    .iter()
                    .map(|p| u64::from(p.amount))
                    .try_fold(0u64, |acc, amt| acc.checked_add(amt))
                    .ok_or("Fresh proofs balance overflow")?;
                if fresh_total < amount_needed {
                    return Err(format!(
                        "Insufficient funds after cleanup. Need: {} sats, have: {} sats",
                        amount_needed, fresh_total
                    ));
                }

                // Retry
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
                // Clean up quote on failure
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

                Err(format!(
                    "Failed to melt: {}. Quote has been cleaned up.",
                    e
                ))
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

    // Publish token event with remaining proofs
    if !keep_proofs.is_empty() {
        let proof_data: Vec<ProofData> = keep_proofs.iter().map(|p| cdk_proof_to_proof_data(p)).collect();

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

        match client.send_event_builder(builder.clone()).await {
            Ok(event_output) => {
                let real_id = event_output.id().to_hex();
                log::info!("Published new token event: {}", real_id);
                new_event_id = Some(real_id);
            }
            Err(e) => {
                log::warn!("Failed to publish token event, queuing for retry: {}", e);
                queue_event_for_retry(builder, PendingEventType::TokenEvent).await;
            }
        }
    }

    // Publish deletion event
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

            match client.send_event_builder(deletion_builder.clone()).await {
                Ok(_) => {
                    log::info!(
                        "Published deletion events for {} token events",
                        valid_event_ids.len()
                    );
                }
                Err(e) => {
                    log::warn!("Failed to publish deletion event, queuing for retry: {}", e);
                    queue_event_for_retry(deletion_builder, PendingEventType::DeletionEvent).await;
                }
            }
        }
    }

    Ok(new_event_id)
}

/// Update local state after melt
fn update_local_state_after_melt(
    mint_url: &str,
    keep_proofs: &[cdk::nuts::Proof],
    event_ids_to_delete: &[String],
    new_event_id: &Option<String>,
) -> Result<(), String> {
    let store = WALLET_TOKENS.read();
    let mut data = store.data();
    let mut tokens_write = data.write();

    // Remove old token events
    tokens_write.retain(|t| !event_ids_to_delete.contains(&t.event_id));

    // Add new token with remaining proofs
    if let Some(ref event_id) = new_event_id {
        let proof_data: Vec<ProofData> = keep_proofs.iter().map(|p| cdk_proof_to_proof_data(p)).collect();

        tokens_write.push(TokenData {
            event_id: event_id.clone(),
            mint: mint_url.to_string(),
            unit: "sat".to_string(),
            proofs: proof_data,
            created_at: chrono::Utc::now().timestamp() as u64,
        });
    }

    // Update balance
    let new_balance: u64 = tokens_write
        .iter()
        .flat_map(|t| &t.proofs)
        .map(|p| p.amount)
        .try_fold(0u64, |acc, amount| acc.checked_add(amount))
        .ok_or_else(|| "Balance calculation overflow".to_string())?;

    *WALLET_BALANCE.write() = new_balance;

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

    // Build content array
    let mut content_array = vec![
        vec!["direction".to_string(), direction.to_string()],
        vec!["amount".to_string(), amount.to_string()],
    ];

    // Extension fields
    content_array.push(vec!["unit".to_string(), "sat".to_string()]);

    if let Some(op_type) = operation_type {
        content_array.push(vec!["type".to_string(), op_type.to_string()]);
    }

    if let Some(inv) = invoice {
        content_array.push(vec!["invoice".to_string(), inv.to_string()]);
    }

    // Standard NIP-60 event references
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
        .send_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to publish history event: {}", e))?;

    log::info!("Published history event: {}", event_output.id().to_hex());

    Ok(())
}
