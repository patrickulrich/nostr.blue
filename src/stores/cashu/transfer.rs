//! Cross-mint transfers
//!
//! Functions for transferring tokens between mints via Lightning.

use dioxus::prelude::*;
use nostr_sdk::{EventId, Kind, PublicKey};

use super::events::queue_event_for_retry;
use super::types::PendingEventType;
use super::internal::{create_ephemeral_wallet, get_or_create_wallet};
use super::utils::normalize_mint_url;
use super::mint_mgmt::get_mint_balance;
use super::proofs::{cdk_proof_to_proof_data, proof_data_to_cdk_proof, register_proofs_in_event_map};
use super::signals::{try_acquire_mint_lock, TRANSFER_PROGRESS, WALLET_BALANCE, WALLET_TOKENS};
use super::types::{
    ExtendedCashuProof, ExtendedTokenEvent, ProofData, TokenData, TransferProgress, TransferResult,
    WalletTokensStoreStoreExt,
};
use crate::stores::{auth_store, nostr_client};

/// Transfer tokens from one mint to another via Lightning
///
/// This performs a melt at the source mint and mint at the target mint,
/// effectively moving tokens between mints via Lightning payment.
pub async fn transfer_between_mints(
    source_mint: String,
    target_mint: String,
    amount: u64,
) -> Result<TransferResult, String> {
    use cdk::Amount;
    use nostr_sdk::signer::NostrSigner;

    // Normalize mint URLs to ensure consistent comparison and storage
    let source_mint = normalize_mint_url(&source_mint);
    let target_mint = normalize_mint_url(&target_mint);

    log::info!(
        "Starting cross-mint transfer: {} sats from {} to {}",
        amount,
        source_mint,
        target_mint
    );

    // Reset progress
    *TRANSFER_PROGRESS.write() = Some(TransferProgress::CreatingMintQuote);

    // Validate inputs
    if source_mint == target_mint {
        *TRANSFER_PROGRESS.write() = Some(TransferProgress::Failed {
            error: "Source and target mints must be different".to_string(),
        });
        return Err("Source and target mints must be different".to_string());
    }

    if amount == 0 {
        *TRANSFER_PROGRESS.write() = Some(TransferProgress::Failed {
            error: "Amount must be greater than 0".to_string(),
        });
        return Err("Amount must be greater than 0".to_string());
    }

    // Check source balance
    let source_balance = get_mint_balance(&source_mint);
    if source_balance < amount {
        let error = format!(
            "Insufficient balance at source mint. Have: {} sats, need: {} sats",
            source_balance, amount
        );
        *TRANSFER_PROGRESS.write() = Some(TransferProgress::Failed {
            error: error.clone(),
        });
        return Err(error);
    }

    // Acquire lock for source mint
    let _source_lock = try_acquire_mint_lock(&source_mint).ok_or_else(|| {
        let error = format!(
            "Another operation is in progress for source mint: {}",
            source_mint
        );
        *TRANSFER_PROGRESS.write() = Some(TransferProgress::Failed {
            error: error.clone(),
        });
        error
    })?;

    // Acquire lock for target mint to prevent concurrent operations
    let _target_lock = try_acquire_mint_lock(&target_mint).ok_or_else(|| {
        let error = format!(
            "Another operation is in progress for target mint: {}",
            target_mint
        );
        *TRANSFER_PROGRESS.write() = Some(TransferProgress::Failed {
            error: error.clone(),
        });
        error
    })?;

    // STEP 1: Create mint quote at target mint (get Lightning invoice)
    log::info!("Creating mint quote at target mint for {} sats", amount);
    *TRANSFER_PROGRESS.write() = Some(TransferProgress::CreatingMintQuote);

    let target_wallet = get_or_create_wallet(&target_mint).await?;
    let mint_quote = target_wallet
        .mint_quote(Amount::from(amount), None)
        .await
        .map_err(|e| {
            let error = format!("Failed to create mint quote at target: {}", e);
            *TRANSFER_PROGRESS.write() = Some(TransferProgress::Failed {
                error: error.clone(),
            });
            error
        })?;

    log::info!("Mint quote created: {}", mint_quote.id);

    // STEP 2: Create melt quote at source mint for that invoice
    log::info!("Creating melt quote at source mint");
    *TRANSFER_PROGRESS.write() = Some(TransferProgress::CreatingMeltQuote);

    let source_wallet = get_or_create_wallet(&source_mint).await?;
    let melt_quote = source_wallet
        .melt_quote(mint_quote.request.clone(), None)
        .await
        .map_err(|e| {
            let error = format!("Failed to create melt quote at source: {}", e);
            *TRANSFER_PROGRESS.write() = Some(TransferProgress::Failed {
                error: error.clone(),
            });
            error
        })?;

    let fee_estimate = u64::from(melt_quote.fee_reserve);
    let total_needed = amount + fee_estimate;

    log::info!(
        "Melt quote created: {}, fee reserve: {} sats",
        melt_quote.id,
        fee_estimate
    );

    // Verify we have enough balance including fees
    if source_balance < total_needed {
        let error = format!(
            "Insufficient balance including fees. Have: {} sats, need: {} sats (amount: {} + fee: {})",
            source_balance, total_needed, amount, fee_estimate
        );
        *TRANSFER_PROGRESS.write() = Some(TransferProgress::Failed {
            error: error.clone(),
        });
        return Err(error);
    }

    *TRANSFER_PROGRESS.write() = Some(TransferProgress::QuotesReady {
        amount,
        fee_estimate,
    });

    // STEP 3: Get proofs from source mint and execute melt
    log::info!("Preparing proofs for melt");
    *TRANSFER_PROGRESS.write() = Some(TransferProgress::Melting);

    let (all_proofs, event_ids_to_delete) = {
        let store = WALLET_TOKENS.read();
        let data = store.data();
        let tokens = data.read();
        let mint_tokens: Vec<_> = tokens
            .iter()
            .filter(|t| t.mint == source_mint)
            .collect();

        let mut all_proofs = Vec::new();
        let mut event_ids = Vec::new();

        for token in &mint_tokens {
            event_ids.push(token.event_id.clone());
            for proof in &token.proofs {
                all_proofs.push(proof_data_to_cdk_proof(proof)?);
            }
        }

        (all_proofs, event_ids)
    };

    // Create wallet with proofs and execute melt
    let wallet_with_proofs = create_ephemeral_wallet(&source_mint, all_proofs).await?;

    let melted = wallet_with_proofs
        .melt(&melt_quote.id)
        .await
        .map_err(|e| {
            let error = format!("Failed to melt tokens: {}", e);
            *TRANSFER_PROGRESS.write() = Some(TransferProgress::Failed {
                error: error.clone(),
            });
            error
        })?;

    let paid = melted.state == cdk::nuts::MeltQuoteState::Paid;
    let fee_paid = u64::from(melted.fee_paid);

    log::info!("Melt result: paid={}, fee_paid={} sats", paid, fee_paid);

    if !paid {
        let error = "Lightning payment failed".to_string();
        *TRANSFER_PROGRESS.write() = Some(TransferProgress::Failed {
            error: error.clone(),
        });
        return Err(error);
    }

    // Get remaining proofs at source after melt
    let source_keep_proofs = wallet_with_proofs
        .get_unspent_proofs()
        .await
        .map_err(|e| format!("Failed to get remaining proofs: {}", e))?;

    // STEP 4: Wait for payment confirmation and mint at target
    log::info!("Payment sent, waiting for mint quote to be paid");
    *TRANSFER_PROGRESS.write() = Some(TransferProgress::WaitingForPayment);

    // Poll for mint quote status (will be replaced with WebSocket in Phase 3)
    let max_attempts = 60; // 2 minutes with 2-second intervals
    let mut mint_quote_paid = false;

    for attempt in 0..max_attempts {
        let state = target_wallet
            .mint_quote_state(&mint_quote.id)
            .await
            .map_err(|e| format!("Failed to check mint quote status: {}", e))?;

        if state.state == cdk::nuts::MintQuoteState::Paid {
            mint_quote_paid = true;
            log::info!("Mint quote is paid after {} attempts", attempt + 1);
            break;
        }

        if state.state == cdk::nuts::MintQuoteState::Issued {
            // Already minted elsewhere? This shouldn't happen in normal flow
            let error = "Mint quote was already issued".to_string();
            *TRANSFER_PROGRESS.write() = Some(TransferProgress::Failed {
                error: error.clone(),
            });
            return Err(error);
        }

        // Wait 2 seconds before next check
        #[cfg(target_arch = "wasm32")]
        {
            use gloo_timers::future::TimeoutFuture;
            TimeoutFuture::new(2000).await;
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        }
    }

    if !mint_quote_paid {
        let error = "Timeout waiting for Lightning payment confirmation".to_string();
        *TRANSFER_PROGRESS.write() = Some(TransferProgress::Failed {
            error: error.clone(),
        });
        return Err(error);
    }

    // STEP 5: Mint tokens at target
    log::info!("Minting tokens at target mint");
    *TRANSFER_PROGRESS.write() = Some(TransferProgress::Minting);

    let target_proofs = target_wallet
        .mint(&mint_quote.id, cdk::amount::SplitTarget::default(), None)
        .await
        .map_err(|e| {
            let error = format!("Failed to mint tokens at target: {}", e);
            *TRANSFER_PROGRESS.write() = Some(TransferProgress::Failed {
                error: error.clone(),
            });
            error
        })?;

    let amount_received: u64 = target_proofs
        .iter()
        .map(|p| u64::from(p.amount))
        .try_fold(0u64, |acc, amt| acc.checked_add(amt))
        .ok_or("Amount overflow")?;

    log::info!(
        "Minted {} sats at target mint ({} proofs)",
        amount_received,
        target_proofs.len()
    );

    // STEP 6: Update local state and publish to Nostr
    let signer = crate::stores::signer::get_signer()
        .ok_or("No signer available")?
        .as_nostr_signer();

    let pubkey_str = auth_store::get_pubkey().ok_or("Not authenticated")?;
    let pubkey =
        PublicKey::parse(&pubkey_str).map_err(|e| format!("Invalid pubkey: {}", e))?;

    let client = nostr_client::NOSTR_CLIENT
        .read()
        .as_ref()
        .ok_or("Client not initialized")?
        .clone();

    // Publish source mint token event (remaining proofs)
    let mut source_new_event_id: Option<String> = None;
    if !source_keep_proofs.is_empty() {
        let proof_data: Vec<ProofData> = source_keep_proofs
            .iter()
            .map(cdk_proof_to_proof_data)
            .collect();

        let extended_proofs: Vec<ExtendedCashuProof> = proof_data
            .iter()
            .map(|p| ExtendedCashuProof::from(p.clone()))
            .collect();

        let token_event_data = ExtendedTokenEvent {
            mint: source_mint.clone(),
            unit: "sat".to_string(),
            proofs: extended_proofs,
            del: event_ids_to_delete.clone(),
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
                source_new_event_id = Some(event_output.id().to_hex());
                log::info!("Published source token event: {:?}", source_new_event_id);
            }
            Err(e) => {
                log::warn!("Failed to publish source token event: {}", e);
                queue_event_for_retry(builder, PendingEventType::TokenEvent).await;
            }
        }
    } else {
        // No remaining proofs - publish deletion for all old events
        if !event_ids_to_delete.is_empty() {
            use nostr::nips::nip09::EventDeletionRequest;
            let mut deletion_request = EventDeletionRequest::new();
            for event_id_str in &event_ids_to_delete {
                if let Ok(event_id) = EventId::parse(event_id_str) {
                    deletion_request = deletion_request.id(event_id);
                }
            }

            let builder = nostr_sdk::EventBuilder::delete(deletion_request);
            if let Err(e) = client.send_event_builder(builder.clone()).await {
                log::warn!("Failed to publish deletion event: {}", e);
                queue_event_for_retry(builder, PendingEventType::DeletionEvent).await;
            }
        }
    }

    // Publish target mint token event (new proofs)
    let mut target_new_event_id: Option<String> = None;
    {
        let proof_data: Vec<ProofData> = target_proofs
            .iter()
            .map(cdk_proof_to_proof_data)
            .collect();

        let extended_proofs: Vec<ExtendedCashuProof> = proof_data
            .iter()
            .map(|p| ExtendedCashuProof::from(p.clone()))
            .collect();

        let token_event_data = ExtendedTokenEvent {
            mint: target_mint.clone(),
            unit: "sat".to_string(),
            proofs: extended_proofs,
            del: vec![],
        };

        let json_content = serde_json::to_string(&token_event_data)
            .map_err(|e| format!("Failed to serialize target token event: {}", e))?;

        let encrypted = signer
            .nip44_encrypt(&pubkey, &json_content)
            .await
            .map_err(|e| format!("Failed to encrypt target token event: {}", e))?;

        let builder = nostr_sdk::EventBuilder::new(Kind::CashuWalletUnspentProof, encrypted);

        match client.send_event_builder(builder.clone()).await {
            Ok(event_output) => {
                target_new_event_id = Some(event_output.id().to_hex());
                log::info!("Published target token event: {:?}", target_new_event_id);
            }
            Err(e) => {
                log::warn!("Failed to publish target token event: {}", e);
                queue_event_for_retry(builder, PendingEventType::TokenEvent).await;
            }
        }
    }

    // Update local state
    {
        let store = WALLET_TOKENS.read();
        let mut data = store.data();
        let mut tokens = data.write();

        // Remove old source tokens
        tokens.retain(|t| !event_ids_to_delete.contains(&t.event_id));

        // Add new source tokens (if any remain)
        if !source_keep_proofs.is_empty() {
            let proof_data: Vec<ProofData> = source_keep_proofs
                .iter()
                .map(cdk_proof_to_proof_data)
                .collect();

            let event_id = source_new_event_id
                .unwrap_or_else(|| format!("local-{}-src-{:08x}", chrono::Utc::now().timestamp_millis(), rand::random::<u32>()));
            tokens.push(TokenData {
                event_id: event_id.clone(),
                mint: source_mint.clone(),
                unit: "sat".to_string(),
                proofs: proof_data.clone(),
                created_at: chrono::Utc::now().timestamp() as u64,
            });

            // Register source proofs in event map
            register_proofs_in_event_map(&event_id, &proof_data);
        }

        // Add new target tokens
        let target_proof_data: Vec<ProofData> = target_proofs
            .iter()
            .map(cdk_proof_to_proof_data)
            .collect();

        let target_event_id = target_new_event_id
            .unwrap_or_else(|| format!("local-{}-tgt-{:08x}", chrono::Utc::now().timestamp_millis(), rand::random::<u32>()));
        tokens.push(TokenData {
            event_id: target_event_id.clone(),
            mint: target_mint.clone(),
            unit: "sat".to_string(),
            proofs: target_proof_data.clone(),
            created_at: chrono::Utc::now().timestamp() as u64,
        });

        // Register target proofs in event map
        register_proofs_in_event_map(&target_event_id, &target_proof_data);
    }

    // Update balance
    {
        let store = WALLET_TOKENS.read();
        let data = store.data();
        let tokens = data.read();
        let new_balance: u64 = tokens
            .iter()
            .flat_map(|t| &t.proofs)
            .map(|p| p.amount)
            .fold(0u64, |acc, amount| acc.saturating_add(amount));
        *WALLET_BALANCE.write() = new_balance;
    }

    // Calculate result
    let amount_sent = amount + fee_paid;
    let result = TransferResult {
        amount_sent,
        amount_received,
        fees_paid: fee_paid,
    };

    log::info!(
        "Transfer complete: sent {} sats, received {} sats, fees {} sats",
        amount_sent,
        amount_received,
        fee_paid
    );

    *TRANSFER_PROGRESS.write() = Some(TransferProgress::Completed {
        amount_received,
        fees_paid: fee_paid,
    });

    Ok(result)
}

/// Estimate fees for a cross-mint transfer
///
/// Creates quotes at both mints to determine the fee reserve that would be required.
/// Returns (fee_estimate, amount).
pub async fn estimate_transfer_fees(
    source_mint: String,
    target_mint: String,
    amount: u64,
) -> Result<(u64, u64), String> {
    use cdk::Amount;

    // Normalize mint URLs to ensure consistent comparison
    let source_mint = normalize_mint_url(&source_mint);
    let target_mint = normalize_mint_url(&target_mint);

    log::info!("Estimating transfer fees: {} sats from {} to {}",
        amount, source_mint, target_mint);

    if source_mint == target_mint {
        return Err("Source and target mints must be different".to_string());
    }

    if amount == 0 {
        return Err("Amount must be greater than 0".to_string());
    }

    // Create wallet for target mint to get a mint quote (Lightning invoice)
    let target_wallet = get_or_create_wallet(&target_mint).await?;

    // Create mint quote at target to get invoice amount
    let mint_quote = target_wallet.mint_quote(Amount::from(amount), None).await
        .map_err(|e| format!("Failed to create mint quote: {}", e))?;

    // Create wallet for source mint to get melt quote (fee estimate)
    let source_wallet = get_or_create_wallet(&source_mint).await?;

    // Create melt quote to see what fees would be
    let melt_quote = source_wallet.melt_quote(mint_quote.request.clone(), None).await
        .map_err(|e| format!("Failed to create melt quote: {}", e))?;

    let fee_estimate = u64::from(melt_quote.fee_reserve);
    let total_needed = amount + fee_estimate;

    log::info!("Transfer fee estimate: {} sats (total needed: {} sats)",
        fee_estimate, total_needed);

    Ok((fee_estimate, amount))
}
