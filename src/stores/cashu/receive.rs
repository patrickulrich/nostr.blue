//! Receive operations
//!
//! Functions for receiving ecash tokens with optional DLEQ verification.

use dioxus::prelude::*;
use nostr_sdk::signer::NostrSigner;
use nostr_sdk::{Kind, PublicKey};

use super::internal::{
    cleanup_spent_proofs_internal, collect_p2pk_signing_keys, create_ephemeral_wallet,
    is_token_already_spent_error,
};
use super::proofs::{cdk_proof_to_proof_data, register_proofs_in_event_map};
use super::signals::{try_acquire_mint_lock, WALLET_BALANCE, WALLET_TOKENS};
use super::types::{ExtendedCashuProof, ExtendedTokenEvent, ProofData, TokenData, WalletTokensStoreStoreExt};
use super::utils::normalize_mint_url;
use crate::stores::{auth_store, cashu_cdk_bridge, nostr_client};

// =============================================================================
// Types (re-exported from types module)
// =============================================================================

// Re-export ReceiveOptions from types module for backwards compatibility
pub use super::types::ReceiveOptions as ReceiveTokensOptions;

// =============================================================================
// Public API
// =============================================================================

/// Receive ecash from a token string (default options - no DLEQ verification)
#[allow(dead_code)]
pub async fn receive_tokens(token_string: String) -> Result<u64, String> {
    receive_tokens_with_options(token_string, ReceiveTokensOptions::default()).await
}

/// Receive ecash from a token string with options
///
/// If `options.verify_dleq` is true, will verify DLEQ proofs (NUT-12) before accepting.
/// This provides offline verification that the mint's signatures are valid.
pub async fn receive_tokens_with_options(
    token_string: String,
    options: ReceiveTokensOptions,
) -> Result<u64, String> {
    use cdk::nuts::Token;
    use cdk::wallet::ReceiveOptions;
    use std::str::FromStr;

    log::info!("Receiving token (verify_dleq: {})...", options.verify_dleq);

    // Sanitize token string - remove ALL whitespace (spaces, tabs, newlines)
    let token_string = token_string
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect::<String>();

    if token_string.is_empty() {
        return Err("Token string is empty".to_string());
    }

    log::info!(
        "Token string length: {}, starts with: {}",
        token_string.len(),
        token_string.chars().take(10).collect::<String>()
    );

    // Validate token format
    if !token_string.starts_with("cashuA") && !token_string.starts_with("cashuB") {
        return Err(format!(
            "Invalid token format. Cashu tokens must start with 'cashuA' or 'cashuB'. Your token starts with: '{}'",
            token_string.chars().take(10).collect::<String>()
        ));
    }

    // Check for control characters that might indicate encoding issues
    if token_string.chars().any(|c| c.is_control()) {
        log::warn!("Token contains control characters");
        return Err(
            "Token contains invalid control characters. Please copy the token again.".to_string(),
        );
    }

    // Extract and validate the base64 portion
    let base64_part = if token_string.starts_with("cashuA") || token_string.starts_with("cashuB") {
        &token_string[6..]
    } else {
        ""
    };

    log::info!(
        "Base64 portion length: {}, last 20 chars: {}",
        base64_part.len(),
        base64_part
            .chars()
            .rev()
            .take(20)
            .collect::<String>()
            .chars()
            .rev()
            .collect::<String>()
    );

    // Check if base64 length is valid and try auto-correction
    let remainder = base64_part.len() % 4;
    let token_to_parse = if remainder != 0 {
        log::warn!(
            "Base64 portion length {} is not a multiple of 4. Remainder: {}",
            base64_part.len(),
            remainder
        );

        // Try adding padding if it's close to being valid
        if remainder == 2 || remainder == 3 {
            let padding_needed = 4 - remainder;
            log::warn!(
                "Auto-correcting malformed token: adding {} padding character(s)",
                padding_needed
            );
            format!("{}{}", token_string, "=".repeat(padding_needed))
        } else {
            token_string.clone()
        }
    } else {
        token_string.clone()
    };

    // Parse token
    let token = Token::from_str(&token_to_parse).map_err(|e| {
        log::error!("Token parse error: {:?}", e);
        let error_str = e.to_string();

        if error_str.contains("6-bit remainder") || error_str.contains("InvalidLength") {
            return format!(
                "Token appears to be incomplete or corrupted (base64 length: {}, remainder: {}). Please ensure you copied the entire token.",
                base64_part.len(),
                remainder
            );
        } else if error_str.contains("InvalidByte") {
            return "Token contains invalid characters. Please copy the token again carefully."
                .to_string();
        }

        format!("Invalid token format: {}", e)
    })?;

    if token_to_parse != token_string {
        log::info!("Successfully parsed token after adding padding!");
    }

    let mint_url = normalize_mint_url(
        &token
            .mint_url()
            .map_err(|e| format!("Failed to get mint URL: {}", e))?
            .to_string(),
    );

    log::info!("Token from mint: {}", mint_url);

    // Acquire mint operation lock to prevent concurrent operations
    let _lock_guard = try_acquire_mint_lock(&mint_url)
        .ok_or_else(|| format!("Another operation is in progress for mint: {}", mint_url))?;

    // Create ephemeral wallet
    let wallet = create_ephemeral_wallet(&mint_url, vec![]).await?;

    // NUT-13: Validate keyset is active before receiving
    // Inactive keysets indicate the mint has rotated keys - tokens should be
    // migrated to a new keyset rather than accepted as-is
    // Note: We check the keyset by examining the token's proofs directly
    // since CDK's Token type requires keysets for proofs() call
    if let Ok(proofs) = token.proofs(&[]) {
        if let Some(first_proof) = proofs.first() {
            let keyset_id = first_proof.keyset_id;

            // Try to get keyset info by fetching all keysets and finding ours
            match wallet.get_mint_keysets().await {
                Ok(keysets) => {
                    if let Some(keyset_info) = keysets.iter().find(|k| k.id == keyset_id) {
                        if !keyset_info.active {
                            log::warn!(
                                "Token uses inactive keyset {} - mint has rotated keys",
                                keyset_id
                            );
                            // Don't reject outright - the receive will still work (swap to new keyset)
                            // But log a warning so we can track this
                        }
                    } else {
                        log::warn!("Keyset {} not found on mint {}", keyset_id, mint_url);
                        // Proceed - mint will reject if truly invalid
                    }
                }
                Err(e) => {
                    log::debug!("Could not fetch keysets to verify status: {}", e);
                    // Proceed - mint will reject if truly invalid
                }
            }
        }
    }

    // NUT-12: Verify DLEQ proofs if requested
    if options.verify_dleq {
        log::info!("Verifying DLEQ proofs (NUT-12)...");
        match wallet.verify_token_dleq(&token).await {
            Ok(()) => {
                log::info!("DLEQ verification successful - token signatures are valid");
            }
            Err(e) => {
                // Use CDK error enum matching instead of string matching
                use cdk::Error as CdkError;
                match &e {
                    CdkError::DleqProofNotProvided => {
                        log::warn!("Token does not contain DLEQ proofs - cannot verify offline");
                        return Err("Token verification failed: This token does not contain DLEQ proofs for offline verification. The mint may not support NUT-12.".to_string());
                    }
                    CdkError::CouldNotVerifyDleq => {
                        log::error!("DLEQ verification failed: invalid signature");
                        return Err("Token verification failed: Invalid DLEQ proof signature.".to_string());
                    }
                    _ => {
                        // Other errors (e.g., keyset not found, network issues)
                        log::error!("DLEQ verification error: {}", e);
                        return Err(format!("Token verification failed: {}", e));
                    }
                }
            }
        }
    }

    // Collect P2PK signing keys for unlock (NUT-11)
    let p2pk_signing_keys = collect_p2pk_signing_keys().await;
    log::debug!(
        "Using {} P2PK signing keys for receive",
        p2pk_signing_keys.len()
    );

    // Log HTLC preimages if provided (NUT-14)
    if !options.preimages.is_empty() {
        log::info!(
            "Using {} HTLC preimages for receive (NUT-14)",
            options.preimages.len()
        );
    }

    let receive_opts = ReceiveOptions {
        p2pk_signing_keys,
        preimages: options.preimages.clone(),
        ..Default::default()
    };

    // Receive token (contacts mint to swap proofs)
    let amount_received = match wallet.receive(&token_to_parse, receive_opts).await {
        Ok(amount) => amount,
        Err(e) => {
            if is_token_already_spent_error(&e) {
                log::warn!("Token already spent or redeemed, checking for spent proofs in wallet");

                // Cleanup any spent proofs in our wallet
                match cleanup_spent_proofs_internal(&mint_url).await {
                    Ok((cleaned_count, cleaned_amount)) if cleaned_count > 0 => {
                        log::info!(
                            "Cleaned up {} spent proofs worth {} sats",
                            cleaned_count,
                            cleaned_amount
                        );
                        return Err(format!(
                            "This token has already been spent. However, we cleaned up {} spent proofs ({} sats) from your wallet.",
                            cleaned_count, cleaned_amount
                        ));
                    }
                    Ok(_) => {
                        return Err(
                            "This token has already been spent and cannot be redeemed.".to_string(),
                        );
                    }
                    Err(cleanup_err) => {
                        log::error!("Cleanup failed: {}", cleanup_err);
                        return Err(
                            "This token has already been spent and cannot be redeemed.".to_string(),
                        );
                    }
                }
            }
            return Err(format!("Failed to receive token: {}", e));
        }
    };

    log::info!("Received {} sats", u64::from(amount_received));

    // Get received proofs from wallet
    let new_proofs = wallet
        .get_unspent_proofs()
        .await
        .map_err(|e| format!("Failed to get proofs: {}", e))?;

    // Convert to ProofData
    let proof_data: Vec<ProofData> = new_proofs.iter().map(|p| cdk_proof_to_proof_data(p)).collect();

    // Create extended token event with P2PK support
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

    // Get signer and publish event
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

        // Register proofs in event map for fast lookup
        register_proofs_in_event_map(&event_id, &proof_data);

        // Recalculate balance
        let new_balance: u64 = tokens
            .iter()
            .flat_map(|t| &t.proofs)
            .map(|p| p.amount)
            .try_fold(0u64, |acc, amount| acc.checked_add(amount))
            .ok_or_else(|| "Balance calculation overflow".to_string())?;

        *WALLET_BALANCE.write() = new_balance;
        log::info!("Balance after receive: {} sats", new_balance);
    }

    let amount = u64::from(amount_received);

    // Create history event (kind 7376)
    if let Err(e) =
        super::events::create_history_event("in", amount, vec![event_id.clone()], vec![]).await
    {
        log::error!("Failed to create history event: {}", e);
        // Don't fail the whole operation
    }

    // Sync MultiMintWallet state (non-critical)
    if let Err(e) = cashu_cdk_bridge::sync_wallet_state().await {
        log::warn!("Failed to sync MultiMintWallet state after receive: {}", e);
    }

    Ok(amount)
}
