//! Receive operations
//!
//! Functions for receiving ecash tokens with optional DLEQ verification.
use super::internal::{
    cleanup_spent_proofs_internal, collect_p2pk_signing_keys, create_ephemeral_wallet,
    is_token_already_spent_error,
};
use super::proofs::{cdk_proof_to_proof_data, register_proofs_in_event_map};
use super::signals::{try_acquire_mint_lock, WALLET_TOKENS};
pub use super::types::ReceiveOptions as ReceiveTokensOptions;
use super::types::{
    ExtendedCashuProof, ExtendedTokenEvent, ProofData, TokenData, WalletTokensStoreStoreExt,
};
use super::utils::{normalize_mint_url, sanitize_and_validate_token};
use crate::stores::{auth_store, cashu_cdk_bridge, nostr_client};
use dioxus::prelude::*;
use nostr_sdk::signer::NostrSigner;
use nostr_sdk::{Kind, PublicKey};
/// Token preview data extracted using CDK's get_token_data()
#[derive(Clone, Debug)]
pub struct TokenPreview {
    /// Mint URL
    pub mint_url: String,
    /// Total value in sats
    pub value: u64,
    /// Currency unit (usually "sat")
    pub unit: String,
    /// Optional memo from sender
    pub memo: Option<String>,
    /// Estimated fee to redeem (if known)
    /// Note: CDK 0.14.2 doesn't expose this yet, reserved for future use
    #[allow(dead_code)]
    pub redeem_fee: Option<u64>,
    /// Number of proofs in token
    pub proof_count: usize,
}
/// Preview a token without receiving it
///
/// Uses CDK's get_token_data() to extract token information including:
/// - Mint URL and value
/// - Memo from sender
/// - Estimated redemption fee
///
/// This allows showing token details to the user before they decide to receive.
pub async fn preview_token(token_string: String) -> Result<TokenPreview, String> {
    use cdk::nuts::Token;
    use std::str::FromStr;
    let token_string = sanitize_and_validate_token(&token_string)?;
    let token =
        Token::from_str(&token_string).map_err(|e| format!("Failed to parse token: {}", e))?;
    let multi_wallet = cashu_cdk_bridge::MULTI_WALLET
        .read()
        .as_ref()
        .ok_or("Wallet not initialized")?
        .clone();
    let token_data = multi_wallet
        .get_token_data(&token)
        .await
        .map_err(|e| format!("Failed to get token data: {}", e))?;
    let value: u64 = token_data.proofs.iter().map(|p| u64::from(p.amount)).sum();
    let unit = token
        .unit()
        .map(|u| u.to_string())
        .unwrap_or_else(|| "sat".to_string());
    let memo = token_data.memo.map(|m| {
        if m.len() > 256 {
            let mut end = 0;
            for (i, c) in m.char_indices() {
                let char_end = i + c.len_utf8();
                if char_end > 253 {
                    break;
                }
                end = char_end;
            }
            format!("{}...", &m[..end])
        } else {
            m
        }
    });
    Ok(TokenPreview {
        mint_url: token_data.mint_url.to_string(),
        value,
        unit,
        memo,
        redeem_fee: None,
        proof_count: token_data.proofs.len(),
    })
}
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
    let token_string = sanitize_and_validate_token(&token_string)?;
    log::info!(
        "Token string length: {}, starts with: {}",
        token_string.len(),
        token_string.chars().take(10).collect::<String>()
    );
    if token_string.chars().any(|c| c.is_control()) {
        log::warn!("Token contains control characters");
        return Err(
            "Token contains invalid control characters. Please copy the token again.".to_string(),
        );
    }
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
    let remainder = base64_part.len() % 4;
    let token_to_parse = if remainder != 0 {
        log::warn!(
            "Base64 portion length {} is not a multiple of 4. Remainder: {}",
            base64_part.len(),
            remainder
        );
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
    let token = Token::from_str(&token_to_parse)
        .map_err(|e| {
            log::error!("Token parse error: {:?}", e);
            let error_str = e.to_string();
            if error_str.contains("6-bit remainder")
                || error_str.contains("InvalidLength")
            {
                return format!(
                    "Token appears to be incomplete or corrupted (base64 length: {}, remainder: {}). Please ensure you copied the entire token.",
                    base64_part.len(),
                    remainder,
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
    let _lock_guard = try_acquire_mint_lock(&mint_url)
        .ok_or_else(|| format!("Another operation is in progress for mint: {}", mint_url))?;
    let wallet = create_ephemeral_wallet(&mint_url, vec![]).await?;
    match wallet.get_mint_keysets().await {
        Ok(keysets) => {
            if let Ok(proofs) = token.proofs(&keysets) {
                if let Some(first_proof) = proofs.first() {
                    let keyset_id = first_proof.keyset_id;
                    if let Some(keyset_info) = keysets.iter().find(|k| k.id == keyset_id) {
                        if !keyset_info.active {
                            log::info!(
                                "Token uses inactive keyset {} - will be migrated to active keyset during receive",
                                keyset_id
                            );
                        }
                    } else {
                        log::warn!("Keyset {} not found on mint {}", keyset_id, mint_url);
                    }
                }
            }
        }
        Err(e) => {
            log::debug!("Could not fetch keysets to verify status: {}", e);
        }
    }
    if options.verify_dleq {
        log::info!("Verifying DLEQ proofs (NUT-12)...");
        match wallet.verify_token_dleq(&token).await {
            Ok(()) => {
                log::info!("DLEQ verification successful - token signatures are valid");
            }
            Err(e) => {
                use cdk::Error as CdkError;
                match &e {
                    CdkError::DleqProofNotProvided => {
                        log::warn!("Token does not contain DLEQ proofs - cannot verify offline");
                        return Err(
                            "Token verification failed: This token does not contain DLEQ proofs for offline verification. The mint may not support NUT-12."
                                .to_string(),
                        );
                    }
                    CdkError::CouldNotVerifyDleq => {
                        log::error!("DLEQ verification failed: invalid signature");
                        return Err(
                            "Token verification failed: Invalid DLEQ proof signature.".to_string()
                        );
                    }
                    _ => {
                        log::error!("DLEQ verification error: {}", e);
                        return Err(format!("Token verification failed: {}", e));
                    }
                }
            }
        }
    }
    let p2pk_signing_keys = collect_p2pk_signing_keys().await;
    log::debug!(
        "Using {} P2PK signing keys for receive",
        p2pk_signing_keys.len()
    );
    if log::log_enabled!(log::Level::Debug) {
        if let Ok(keysets) = wallet.get_mint_keysets().await {
            if let Ok(proofs) = token.proofs(&keysets) {
                for proof in &proofs {
                    use cdk::nuts::SpendingConditions;
                    let spending_conditions: Option<SpendingConditions> =
                        (&proof.secret).try_into().ok();
                    if let Some(conditions) = spending_conditions {
                        if let Some(pubkeys) = conditions.pubkeys() {
                            for pubkey in pubkeys {
                                log::debug!(
                                    "Token P2PK locked to pubkey: {}",
                                    hex::encode(pubkey.x_only_public_key().serialize())
                                );
                            }
                        }
                    }
                }
            }
        }
    }
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
    let amount_received = match wallet.receive(&token_to_parse, receive_opts).await {
        Ok(amount) => amount,
        Err(e) => {
            if is_token_already_spent_error(&e) {
                log::warn!("Token already spent or redeemed, checking for spent proofs in wallet");
                match cleanup_spent_proofs_internal(&mint_url).await {
                    Ok((cleaned_count, cleaned_amount)) if cleaned_count > 0 => {
                        log::info!(
                            "Cleaned up {} spent proofs worth {} sats",
                            cleaned_count,
                            cleaned_amount
                        );
                        return Err(
                            format!(
                                "This token has already been spent. However, we cleaned up {} spent proofs ({} sats) from your wallet.",
                                cleaned_count,
                                cleaned_amount,
                            ),
                        );
                    }
                    Ok(_) => {
                        return Err(
                            "This token has already been spent and cannot be redeemed.".to_string()
                        );
                    }
                    Err(cleanup_err) => {
                        log::error!("Cleanup failed: {}", cleanup_err);
                        return Err(
                            "This token has already been spent and cannot be redeemed.".to_string()
                        );
                    }
                }
            }
            return Err(format!("Failed to receive token: {}", e));
        }
    };
    log::info!("Received {} sats", u64::from(amount_received));
    let new_proofs = wallet
        .get_unspent_proofs()
        .await
        .map_err(|e| format!("Failed to get proofs: {}", e))?;
    let proof_data: Vec<ProofData> = new_proofs.iter().map(cdk_proof_to_proof_data).collect();
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
    let builder = nostr_sdk::EventBuilder::new(Kind::CashuWalletUnspentProof, encrypted.clone());
    let client = nostr_client::NOSTR_CLIENT
        .read()
        .as_ref()
        .ok_or("Client not initialized")?
        .clone();
    let signed_event = crate::utils::nips::nip89::tag_event_builder(builder)
        .build(pubkey)
        .sign(&signer)
        .await
        .map_err(|e| format!("Failed to sign token event: {}", e))?;
    let pre_signed_event_id = signed_event.id.to_hex();
    let mut event_id: Option<String> = None;
    let mut last_error = String::new();
    let mut retryable = true;
    let delays_ms = [500u32, 1000, 2000];
    for (attempt, delay_ms) in std::iter::once(0u32)
        .chain(delays_ms.iter().copied())
        .enumerate()
    {
        if attempt > 0 {
            #[cfg(feature = "web")]
            {
                let jitter = (js_sys::Math::random() * 200.0) as u32;
                let actual_delay = delay_ms.saturating_sub(100) + jitter;
                crate::platform::timer::sleep_ms(actual_delay).await;
            }
            #[cfg(feature = "native")]
            {
                use rand::Rng;
                let jitter = rand::thread_rng().gen_range(0..200);
                let actual_delay = delay_ms.saturating_sub(100) + jitter;
                crate::platform::timer::sleep_ms(actual_delay).await;
            }
            log::info!("Retrying token event publish (attempt {})", attempt + 1);
        }
        match client.send_event(&signed_event).await {
            Ok(output) => {
                if !output.success.is_empty() {
                    event_id = Some(pre_signed_event_id.clone());
                    log::info!(
                        "Published token event to {}/{} relays",
                        output.success.len(),
                        output.success.len() + output.failed.len()
                    );
                    break;
                } else {
                    let all_duplicates = output
                        .failed
                        .values()
                        .all(|err| err.to_lowercase().starts_with("duplicate:"));
                    if all_duplicates && !output.failed.is_empty() {
                        log::debug!(
                            "Token event {} already exists on all relays (duplicate)",
                            pre_signed_event_id
                        );
                        event_id = Some(pre_signed_event_id.clone());
                        retryable = false;
                        break;
                    }
                    last_error = format!("All {} relays failed", output.failed.len());
                }
            }
            Err(e) => {
                last_error = e.to_string();
                let err_str = last_error.to_lowercase();
                if err_str.contains("banned") || err_str.contains("invalid") {
                    log::error!("Permanent error, stopping retries: {}", last_error);
                    retryable = false;
                    break;
                }
                if err_str.contains("duplicate") || err_str.contains("already exists") {
                    log::info!("Event already exists on relay, using pre-signed ID");
                    event_id = Some(pre_signed_event_id.clone());
                    retryable = false;
                    break;
                }
            }
        }
    }
    let event_id = match event_id {
        Some(id) => id,
        None => {
            if retryable {
                log::error!(
                    "All publish attempts failed, queueing for retry: {}",
                    last_error
                );
                let pending_id = format!("pending_{}", uuid::Uuid::new_v4());
                let retry_builder =
                    nostr_sdk::EventBuilder::new(Kind::CashuWalletUnspentProof, encrypted.clone());
                super::events::queue_token_event_for_retry(
                    retry_builder,
                    pending_id.clone(),
                    mint_url.clone(),
                )
                .await;
                pending_id
            } else {
                log::warn!("Permanent error - not queueing for retry: {}", last_error);
                format!("local_{}", uuid::Uuid::new_v4())
            }
        }
    };
    log::info!("Token event ID: {}", event_id);
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
        super::signals::update_wallet_balances();
        log::info!("Updated balance after receive");
    }
    let amount = u64::from(amount_received);
    if let Err(e) =
        super::events::create_history_event("in", amount, vec![event_id.clone()], vec![]).await
    {
        log::error!("Failed to create history event: {}", e);
    }
    if let Err(e) = cashu_cdk_bridge::sync_wallet_state().await {
        log::warn!("Failed to sync MultiMintWallet state after receive: {}", e);
    }
    Ok(amount)
}
