//! NIP-61 Nutzap Implementation
//!
//! Nutzaps are P2PK-locked Cashu tokens sent via Nostr events, enabling
//! privacy-preserving tips and payments.
//!
//! ## Event Kinds
//! - Kind 10019: Nutzap Info - User's P2PK pubkey, accepted mints, relays
//! - Kind 9321: Nutzap - Event containing P2PK-locked proofs with DLEQ
//! - Kind 7376: History - Tag redeemed nutzaps with "redeemed" marker
//!
//! ## Protocol Flow
//! 1. Recipient publishes kind:10019 declaring accepted mints and P2PK pubkey
//! 2. Sender fetches recipient's kind:10019 to get P2PK pubkey and mint info
//! 3. Sender creates P2PK-locked proofs and publishes kind:9321
//! 4. Recipient receives kind:9321, validates P2PK lock, redeems tokens
//! 5. Recipient creates kind:7376 history with "redeemed" tag
#![allow(dead_code)]
use super::internal::{collect_p2pk_signing_keys, create_ephemeral_wallet};
use super::nutzap_signals::*;
use super::proofs::cdk_proof_to_proof_data;
use super::signals::{try_acquire_mint_lock, WALLET_STATE, WALLET_TOKENS};
use super::types::{
    ExtendedCashuProof, ExtendedTokenEvent, ProofData, TokenData, WalletTokensStoreStoreExt,
};
use super::utils::{mint_matches, normalize_mint_url};
use crate::stores::{auth_store, cashu_cdk_bridge, nostr_client};
use crate::utils::format::safe_slice;
use dioxus::prelude::*;
use nostr_sdk::signer::NostrSigner;
use nostr_sdk::{EventId, Filter, Kind, PublicKey, Tag, TagKind, Timestamp};
use tokio::sync::broadcast::error::RecvError;
use serde::{Deserialize, Serialize};
use std::time::Duration;
/// User's nutzap configuration (from kind:10019)
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NutzapInfo {
    /// User's Nostr pubkey
    pub pubkey: String,
    /// P2PK pubkey for receiving nutzaps (02-prefixed hex)
    pub p2pk_pubkey: String,
    /// Accepted mints with their units
    pub mints: Vec<NutzapMint>,
    /// Relays for nutzap delivery
    pub relays: Vec<String>,
}
/// Mint info for nutzap configuration
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NutzapMint {
    /// Mint URL
    pub url: String,
    /// Accepted currency unit (typically "sat")
    pub unit: String,
}
/// Nutzap awaiting redemption
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PendingNutzap {
    /// Event ID of the kind:9321 event
    pub event_id: String,
    /// Sender's Nostr pubkey
    pub sender_pubkey: String,
    /// Amount in sats
    pub amount: u64,
    /// Mint URL
    pub mint_url: String,
    /// Currency unit (e.g., "sat", "usd")
    pub unit: String,
    /// Optional comment from sender
    pub comment: Option<String>,
    /// Event being nutzapped (optional)
    pub target_event_id: Option<String>,
    /// Target event kind (optional)
    pub target_kind: Option<u16>,
    /// Proofs as JSON strings
    pub proofs_json: Vec<String>,
    /// Current status
    pub status: NutzapStatus,
    /// Timestamp received
    pub received_at: u64,
}
/// Nutzap processing status
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum NutzapStatus {
    /// Awaiting redemption
    Pending,
    /// Currently being redeemed
    Redeeming,
    /// Successfully redeemed
    Redeemed,
    /// Redemption failed
    Failed(String),
}
/// Result of sending a nutzap
#[derive(Clone, Debug)]
pub struct NutzapSendResult {
    /// Event ID of the published kind:9321
    pub event_id: String,
    /// Amount sent
    pub amount: u64,
    /// Fee paid
    pub fee: u64,
    /// True if event was queued for retry (publish failed but tokens sent)
    pub is_pending_retry: bool,
}
/// Result of redeeming a nutzap
#[derive(Clone, Debug)]
pub struct NutzapRedeemResult {
    /// Amount redeemed
    pub amount: u64,
}
/// Get the P2PK pubkey for receiving nutzaps
///
/// Uses the wallet's NIP-60 privkey with "02" prefix per NIP-61 requirement.
/// The pubkey is formatted as 33-byte compressed secp256k1 (02 + 32-byte x).
pub fn get_nutzap_p2pk_pubkey() -> Result<String, String> {
    let wallet_state = WALLET_STATE.read();
    let state = wallet_state.as_ref().ok_or("Wallet not initialized")?;
    let privkey = state
        .privkey
        .as_ref()
        .ok_or("Wallet private key not available")?;
    let secret_key = cdk::nuts::SecretKey::from_hex(privkey)
        .map_err(|e| format!("Invalid wallet privkey: {}", e))?;
    let pubkey = secret_key.public_key();
    let x_only = pubkey.x_only_public_key();
    Ok(format!("02{}", hex::encode(x_only.serialize())))
}
/// Publish nutzap info event (kind:10019)
///
/// This declares the user's P2PK pubkey, accepted mints, and delivery relays
/// so others can send them nutzaps.
pub async fn publish_nutzap_info(
    mints: Vec<NutzapMint>,
    relays: Vec<String>,
) -> Result<String, String> {
    let _signer = crate::stores::signer::get_signer()
        .ok_or("No signer available")?
        .as_nostr_signer();
    let pubkey_str = auth_store::get_pubkey().ok_or("Not authenticated")?;
    let _pubkey = PublicKey::parse(&pubkey_str).map_err(|e| format!("Invalid pubkey: {}", e))?;
    let p2pk_pubkey = get_nutzap_p2pk_pubkey()?;
    let mut tags = Vec::new();
    for relay in &relays {
        tags.push(Tag::custom(TagKind::custom("relay"), [relay.as_str()]));
    }
    for mint in &mints {
        tags.push(Tag::custom(
            TagKind::custom("mint"),
            [mint.url.as_str(), mint.unit.as_str()],
        ));
    }
    tags.push(Tag::custom(
        TagKind::custom("pubkey"),
        [p2pk_pubkey.as_str()],
    ));
    let builder = nostr_sdk::EventBuilder::new(Kind::from(10019), "").tags(tags);
    let event = crate::stores::publish_queue::signing::sign_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to sign nutzap info: {}", e))?;
    let event_id = event.id.to_hex();
    crate::stores::publish_queue::enqueue(
        event,
        crate::stores::publish_queue::types::QueueEventType::Cashu,
        None,
        std::collections::HashMap::new(),
    ).await;
    log::info!("Queued nutzap info event: {}", event_id);
    let info = NutzapInfo {
        pubkey: pubkey_str.clone(),
        p2pk_pubkey,
        mints,
        relays,
    };
    *MY_NUTZAP_INFO.write() = Some(info.clone());
    *NUTZAP_ENABLED.write() = true;
    cache_nutzap_info(pubkey_str, info);
    Ok(event_id)
}
/// Fetch nutzap info for a user (kind:10019)
///
/// Checks cache first, fetches from relays if not cached.
pub async fn fetch_nutzap_info(pubkey: &str) -> Result<NutzapInfo, String> {
    if let Some(cached) = get_cached_nutzap_info(pubkey) {
        log::debug!("Using cached nutzap info for {}", pubkey);
        return Ok(cached);
    }
    let client = nostr_client::NOSTR_CLIENT
        .read()
        .as_ref()
        .ok_or("Client not initialized")?
        .clone();
    let pubkey_parsed = PublicKey::parse(pubkey).map_err(|e| format!("Invalid pubkey: {}", e))?;
    let filter = Filter::new().author(pubkey_parsed).kind(Kind::from(10019));
    let events = client
        .fetch_events(filter, Duration::from_secs(5))
        .await
        .map_err(|e| format!("Failed to fetch nutzap info: {}", e))?;
    let event = events
        .into_iter()
        .max_by_key(|e| e.created_at)
        .ok_or_else(|| format!("No nutzap info found for {}", pubkey))?;
    event
        .verify()
        .map_err(|e| format!("Invalid signature on nutzap info event: {}", e))?;
    let mut p2pk_pubkey = None;
    let mut mints = Vec::new();
    let mut relays = Vec::new();
    for tag in event.tags.iter() {
        let tag_kind = tag.kind().to_string();
        let tag_content = tag.content();
        match tag_kind.as_str() {
            "pubkey" => {
                if let Some(content) = tag_content {
                    p2pk_pubkey = Some(content.to_string());
                }
            }
            "mint" => {
                if let Some(content) = tag_content {
                    let url = normalize_mint_url(content);
                    let unit = tag
                        .as_slice()
                        .get(2)
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| "sat".to_string());
                    mints.push(NutzapMint { url, unit });
                }
            }
            "relay" => {
                if let Some(content) = tag_content {
                    relays.push(content.to_string());
                }
            }
            _ => {}
        }
    }
    let p2pk_pubkey =
        p2pk_pubkey.ok_or_else(|| format!("Nutzap info missing P2PK pubkey for {}", pubkey))?;
    if p2pk_pubkey.len() != 66 || !p2pk_pubkey.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(
            format!(
                "Invalid P2PK pubkey format for {}: expected 66 hex chars (02-prefixed), got '{}' ({} chars)",
                pubkey,
                &p2pk_pubkey[..p2pk_pubkey.len().min(16)],
                p2pk_pubkey.len(),
            ),
        );
    }
    if mints.is_empty() {
        return Err(format!("Nutzap info has no mints for {}", pubkey));
    }
    let info = NutzapInfo {
        pubkey: pubkey.to_string(),
        p2pk_pubkey,
        mints,
        relays,
    };
    cache_nutzap_info(pubkey.to_string(), info.clone());
    Ok(info)
}
/// Validate that we can send a nutzap to a recipient
///
/// Checks that the recipient has nutzap info and we have a compatible mint+unit.
pub async fn validate_nutzap_recipient(pubkey: &str) -> Result<NutzapMint, String> {
    let info = fetch_nutzap_info(pubkey).await?;
    validate_nutzap_recipient_with_info(&info)
}
/// Validate nutzap recipient using pre-fetched info (avoids duplicate network fetch)
///
/// CDK pattern: Always filter by unit when checking proof availability
pub fn validate_nutzap_recipient_with_info(info: &NutzapInfo) -> Result<NutzapMint, String> {
    let our_mints = super::get_mints();
    for their_mint in &info.mints {
        let their_url = normalize_mint_url(&their_mint.url);
        for our_mint in &our_mints {
            if mint_matches(our_mint, &their_url) {
                match get_proofs_for_mint(&their_url, &their_mint.unit) {
                    Ok(proofs) if !proofs.is_empty() => {
                        log::debug!(
                            "Found compatible mint+unit: {} ({}) with {} proofs",
                            their_url,
                            their_mint.unit,
                            proofs.len()
                        );
                        return Ok(their_mint.clone());
                    }
                    Ok(_) => {
                        log::debug!(
                            "Mint {} matches but no proofs for unit {}",
                            their_url,
                            their_mint.unit
                        );
                    }
                    Err(e) => {
                        log::debug!(
                            "Error checking proofs for {}/{}: {}",
                            their_url,
                            their_mint.unit,
                            e
                        );
                    }
                }
            }
        }
    }
    Err(format!(
        "No compatible mint+unit found. Recipient accepts: {:?}",
        info.mints
            .iter()
            .map(|m| format!("{}:{}", m.url, m.unit))
            .collect::<Vec<_>>(),
    ))
}
/// Send a nutzap to a recipient
///
/// Creates P2PK-locked proofs and publishes a kind:9321 event.
pub async fn send_nutzap(
    recipient_pubkey: &str,
    amount: u64,
    target_event_id: Option<&str>,
    target_kind: Option<u16>,
    comment: Option<&str>,
) -> Result<NutzapSendResult, String> {
    if amount == 0 {
        return Err("Cannot send zero amount nutzap".to_string());
    }
    use cdk::amount::SplitTarget;
    use cdk::nuts::SpendingConditions;
    use cdk::Amount;
    log::info!(
        "Sending {} sat nutzap to {}",
        amount,
        safe_slice(recipient_pubkey, 8)
    );
    let recipient_info = fetch_nutzap_info(recipient_pubkey).await?;
    let compatible_mint = validate_nutzap_recipient_with_info(&recipient_info)?;
    let mint_url = normalize_mint_url(&compatible_mint.url);
    let _lock_guard = try_acquire_mint_lock(&mint_url)
        .ok_or_else(|| format!("Another operation in progress for mint: {}", mint_url))?;
    let all_proofs = get_proofs_for_mint(&mint_url, &compatible_mint.unit)?;
    if all_proofs.is_empty() {
        return Err("No tokens found for compatible mint".to_string());
    }
    let total_available: u64 = all_proofs.iter().map(|p| u64::from(p.amount)).sum();
    if total_available < amount {
        return Err(format!(
            "Insufficient funds. Available: {} sats, Required: {} sats",
            total_available, amount,
        ));
    }
    let recipient_p2pk = cdk::nuts::PublicKey::from_hex(&recipient_info.p2pk_pubkey)
        .map_err(|e| format!("Invalid recipient P2PK pubkey: {}", e))?;
    let spending_conditions = SpendingConditions::new_p2pk(recipient_p2pk, None);
    let _signer =
        crate::stores::signer::get_signer().ok_or("No signer available - cannot send nutzap")?;
    let _pubkey = auth_store::get_pubkey().ok_or("Not authenticated - cannot send nutzap")?;
    let _client = nostr_client::NOSTR_CLIENT
        .read()
        .as_ref()
        .ok_or("Nostr client not initialized - cannot send nutzap")?
        .clone();
    let wallet = create_ephemeral_wallet(&mint_url, all_proofs.clone()).await?;
    let fee = wallet
        .get_proofs_fee(&all_proofs)
        .await
        .map_err(|e| format!("Failed to calculate fee: {}", e))?;
    let fee_u64 = u64::from(fee.total);
    let required_with_fee = amount
        .checked_add(fee_u64)
        .ok_or("Amount overflow when adding fee")?;
    if total_available < required_with_fee {
        return Err(format!(
            "Insufficient funds: need {} + {} fee = {}, have {}",
            amount, fee_u64, required_with_fee, total_available,
        ));
    }
    let pre_swap_secrets: std::collections::HashSet<String> = wallet
        .get_unspent_proofs()
        .await
        .map_err(|e| format!("Failed to get pre-swap proofs: {}", e))?
        .iter()
        .map(|p| p.secret.to_string())
        .collect();
    let send_proofs = wallet
        .swap(
            Some(Amount::from(amount)),
            SplitTarget::default(),
            all_proofs.clone(),
            Some(spending_conditions),
            true,
            false,
        )
        .await
        .map_err(|e| format!("Swap failed: {}", e))?
        .ok_or("Swap returned no proofs")?;
    let send_secrets: std::collections::HashSet<String> =
        send_proofs.iter().map(|p| p.secret.to_string()).collect();
    let keep_proofs: Vec<_> = wallet
        .get_unspent_proofs()
        .await
        .map_err(|e| format!("Failed to get change proofs: {}", e))?
        .into_iter()
        .filter(|p| {
            let secret = p.secret.to_string();
            !pre_swap_secrets.contains(&secret) && !send_secrets.contains(&secret)
        })
        .collect();
    let pending_event_id = format!("pending_{}", uuid::Uuid::new_v4());
    let event_ids_to_delete = get_event_ids_for_mint(&mint_url, &compatible_mint.unit);
    update_local_state_after_nutzap_send(
        &mint_url,
        &compatible_mint.unit,
        &keep_proofs,
        &event_ids_to_delete,
    )
    .await?;
    let mut tags = Vec::new();
    for proof in &send_proofs {
        let proof_json = serde_json::to_string(proof)
            .map_err(|e| format!("Failed to serialize proof: {}", e))?;
        tags.push(Tag::custom(TagKind::custom("proof"), [proof_json.as_str()]));
    }
    tags.push(Tag::custom(TagKind::custom("u"), [mint_url.as_str()]));
    tags.push(Tag::custom(
        TagKind::custom("unit"),
        [compatible_mint.unit.as_str()],
    ));
    let recipient_nostr_pubkey = PublicKey::parse(recipient_pubkey)
        .map_err(|e| format!("Invalid recipient pubkey: {}", e))?;
    tags.push(Tag::public_key(recipient_nostr_pubkey));
    if let Some(target_id) = target_event_id {
        if let Ok(event_id) = EventId::from_hex(target_id) {
            tags.push(Tag::event(event_id));
            if let Some(kind) = target_kind {
                tags.push(Tag::custom(
                    TagKind::custom("k"),
                    [kind.to_string().as_str()],
                ));
            }
        }
    }
    let content = comment.unwrap_or("");
    let builder = nostr_sdk::EventBuilder::new(Kind::from(9321), content).tags(tags.clone());
    let signed_event = match crate::stores::publish_queue::signing::sign_event_builder(builder.clone()).await {
        Ok(e) => e,
        Err(e) => {
            log::warn!("Failed to sign nutzap, queuing unsigned for retry: {}", e);
            super::events::queue_event_for_retry(
                builder,
                super::types::PendingEventType::NutzapEvent,
                Some(pending_event_id.clone()),
                Some(mint_url.to_string()),
            )
            .await
            .map_err(|queue_err| {
                format!(
                    "Failed to publish nutzap and persist retry state: {}",
                    queue_err
                )
            })?;
            if let Err(e) = super::events::create_history_event(
                "out",
                amount,
                vec![],
                event_ids_to_delete.clone(),
            )
            .await
            {
                log::error!("Failed to create history event: {}", e);
            }
            if let Err(e) = cashu_cdk_bridge::sync_wallet_state().await {
                log::warn!("Failed to sync wallet state after nutzap: {}", e);
            }
            return Ok(NutzapSendResult {
                event_id: pending_event_id,
                amount,
                fee: fee_u64,
                is_pending_retry: true,
            });
        }
    };
    let recipient_relay_urls: Vec<String> = recipient_info
        .relays
        .to_vec();
    let target_relays = if recipient_relay_urls.is_empty() {
        None
    } else {
        Some(recipient_relay_urls)
    };
    let event_id = signed_event.id.to_hex();
    crate::stores::publish_queue::enqueue(
        signed_event,
        crate::stores::publish_queue::types::QueueEventType::Cashu,
        target_relays,
        std::collections::HashMap::new(),
    ).await;
    log::info!("Queued nutzap event: {}", event_id);
    if let Err(e) =
        super::events::create_history_event("out", amount, vec![], event_ids_to_delete.clone())
            .await
    {
        log::error!("Failed to create history event: {}", e);
    }
    if let Err(e) = cashu_cdk_bridge::sync_wallet_state().await {
        log::warn!("Failed to sync wallet state after nutzap: {}", e);
    }
    Ok(NutzapSendResult {
        event_id,
        amount,
        fee: fee_u64,
        is_pending_retry: false,
    })
}
/// RAII guard that clears subscription flag on drop
/// Ensures cleanup regardless of how the subscription exits (normal, panic, early return)
struct SubscriptionGuard;
impl SubscriptionGuard {
    fn new() -> Self {
        Self
    }
}
impl Drop for SubscriptionGuard {
    fn drop(&mut self) {
        *NUTZAP_SUBSCRIPTION_ACTIVE.write() = false;
        log::debug!("Nutzap subscription guard dropped, flag cleared");
    }
}
/// Error classification for retry logic
///
/// Categories (following nostr-sdk Error pattern):
/// - NETWORK: Transient connectivity issues (retry)
/// - VALIDATION: Permanent proof/token failures (don't retry)
///
/// CDK-specific error identifiers are normalized to lowercase.
/// When CDK exposes structured error types, migrate to enum matching.
///
/// Network/connectivity errors - safe to retry
const TRANSIENT_PATTERNS: &[&str] = &[
    "network",
    "timeout",
    "connection",
    "unavailable",
    "temporary",
    "relay",
    "http",
    "fetch",
    "websocket",
];
/// Validation/permanent errors - do NOT retry
const PERMANENT_PATTERNS: &[&str] = &[
    "tokenalreadyspent",
    "invalidproofs",
    "keysetnotfound",
    "already spent",
    "invalid",
    "unknown",
    "keyset",
    "expired",
    "malformed",
];
/// Classify whether a redemption error is transient (retry-able) or permanent
fn is_transient_error(err: &str) -> bool {
    let err_lower = err.to_lowercase();
    if PERMANENT_PATTERNS.iter().any(|p| err_lower.contains(p)) {
        return false;
    }
    if TRANSIENT_PATTERNS.iter().any(|p| err_lower.contains(p)) {
        return true;
    }
    true
}
/// Start subscription for incoming nutzaps
///
/// Subscribes to kind:9321 events tagged with our pubkey and accepted mints.
pub async fn start_nutzap_subscription() -> Result<(), String> {
    let _my_info = MY_NUTZAP_INFO
        .read()
        .clone()
        .ok_or("Nutzap info not published - enable receiving first")?;
    let my_pubkey_str = auth_store::get_pubkey().ok_or("Not authenticated")?;
    let my_pubkey =
        PublicKey::parse(&my_pubkey_str).map_err(|e| format!("Invalid pubkey: {}", e))?;
    {
        let mut active = NUTZAP_SUBSCRIPTION_ACTIVE.write();
        if *active {
            log::debug!("Nutzap subscription already active");
            return Ok(());
        }
        *active = true;
    }
    let filter = Filter::new().kind(Kind::from(9321)).pubkey(my_pubkey);
    log::info!("Starting nutzap subscription with filter: {:?}", filter);
    spawn(async move {
        let _guard = SubscriptionGuard::new();
        let client = match nostr_client::NOSTR_CLIENT.read().as_ref() {
            Some(c) => c.clone(),
            None => {
                log::error!("Client not initialized for nutzap subscription");
                return;
            }
        };
        let sub_output = match client.subscribe(filter, None).await {
            Ok(output) => output,
            Err(e) => {
                log::error!("Failed to subscribe to nutzaps: {}", e);
                return;
            }
        };
        let sub_id = sub_output.val;
        log::info!("Nutzap subscription started: {:?}", sub_id);
        // Manual notification loop instead of `client.handle_notifications(...)`.
        // The SDK's `handle_notifications` uses `while let Ok(...)` which silently
        // dies on broadcast `Lagged` (a transient slow-consumer error). A manual
        // loop lets us log + continue on `Lagged`, keeping the nutzap watcher
        // alive even when the tab was idle and the consumer fell behind.
        let mut notifications = client.notifications();
        loop {
            match notifications.recv().await {
                Ok(nostr_sdk::RelayPoolNotification::Event {
                    subscription_id,
                    event,
                    ..
                }) => {
                    if subscription_id == sub_id && event.kind == Kind::from(9321) {
                        if let Err(e) = process_nutzap_event(&event).await {
                            log::warn!("Failed to process nutzap {}: {}", event.id.to_hex(), e);
                        }
                    }
                }
                Ok(nostr_sdk::RelayPoolNotification::Shutdown) => {
                    log::debug!("Nutzap watcher: pool shutdown");
                    break;
                }
                // Transient slow-consumer error — channel still alive.
                Err(RecvError::Lagged(skipped)) => {
                    log::warn!(
                        "Nutzap watcher lagged, skipped {} events, continuing",
                        skipped
                    );
                    continue;
                }
                Err(RecvError::Closed) => {
                    log::debug!("Nutzap watcher: channel closed");
                    break;
                }
                Ok(_) => {}
            }
        }
    });
    Ok(())
}
/// Process an incoming nutzap event
///
/// Validates the P2PK lock and adds to pending nutzaps for redemption.
/// Returns `Ok(true)` if newly added, `Ok(false)` if duplicate.
pub async fn process_nutzap_event(event: &nostr_sdk::Event) -> Result<bool, String> {
    log::info!("Processing nutzap event: {}", event.id.to_hex());
    event
        .verify()
        .map_err(|e| format!("Invalid signature on nutzap event: {}", e))?;
    let mut proofs_json = Vec::new();
    let mut mint_url = None;
    let mut unit = "sat".to_string();
    let mut target_event_id = None;
    let mut target_kind = None;
    for tag in event.tags.iter() {
        let tag_kind = tag.kind().to_string();
        let tag_content = tag.content();
        match tag_kind.as_str() {
            "proof" => {
                if let Some(content) = tag_content {
                    proofs_json.push(content.to_string());
                }
            }
            "u" => {
                if let Some(content) = tag_content {
                    mint_url = Some(normalize_mint_url(content));
                }
            }
            "unit" => {
                if let Some(content) = tag_content {
                    unit = content.to_string();
                }
            }
            "e" => {
                if let Some(content) = tag_content {
                    target_event_id = Some(content.to_string());
                }
            }
            "k" => {
                if let Some(content) = tag_content {
                    target_kind = content.parse().ok();
                }
            }
            _ => {}
        }
    }
    let mint_url = mint_url.ok_or("Nutzap missing mint URL")?;
    {
        let my_info = MY_NUTZAP_INFO.read();
        if let Some(info) = my_info.as_ref() {
            let accepted = info
                .mints
                .iter()
                .any(|m| super::utils::mint_matches(&m.url, &mint_url) && m.unit == unit);
            if !accepted {
                return Err(format!(
                    "Nutzap from unaccepted mint/unit: {} (unit: {})",
                    mint_url, unit,
                ));
            }
        } else {
            return Err("Nutzap info not configured - cannot validate mint".to_string());
        }
    }
    if proofs_json.is_empty() {
        return Err("Nutzap contains no proofs".to_string());
    }
    let mut amount = 0u64;
    for proof_json in &proofs_json {
        if let Ok(proof) = serde_json::from_str::<serde_json::Value>(proof_json) {
            if let Some(amt) = proof.get("amount").and_then(|a| a.as_u64()) {
                amount = amount.saturating_add(amt);
            }
        }
    }
    let my_p2pk = get_nutzap_p2pk_pubkey()?;
    validate_nutzap_p2pk(&proofs_json, &my_p2pk)?;
    let pending = PendingNutzap {
        event_id: event.id.to_hex(),
        sender_pubkey: event.pubkey.to_hex(),
        amount,
        mint_url,
        unit,
        comment: if event.content.is_empty() {
            None
        } else {
            Some(event.content.clone())
        },
        target_event_id,
        target_kind,
        proofs_json,
        status: NutzapStatus::Pending,
        received_at: event.created_at.as_secs(),
    };
    let was_added = add_pending_nutzap(pending.clone());
    if was_added && *NUTZAP_AUTO_REDEEM.read() {
        spawn(async move {
            match redeem_nutzap(&pending.event_id).await {
                Ok(result) => {
                    log::info!("Auto-redeemed nutzap: {} sats", result.amount);
                }
                Err(e) => {
                    let current_status = get_pending_nutzap_status(&pending.event_id);
                    if !matches!(current_status, Some(NutzapStatus::Pending)) {
                        log::debug!(
                            "Nutzap {} status changed during redeem (now {:?}), skipping update",
                            pending.event_id,
                            current_status
                        );
                        return;
                    }
                    if is_transient_error(&e) {
                        log::warn!(
                            "Transient error auto-redeeming nutzap {}, will retry: {}",
                            pending.event_id,
                            e
                        );
                    } else {
                        log::error!(
                            "Permanent error auto-redeeming nutzap {}: {}",
                            pending.event_id,
                            e
                        );
                        update_pending_nutzap_status(&pending.event_id, NutzapStatus::Failed(e));
                    }
                }
            }
        });
    }
    Ok(was_added)
}
/// Validate that proofs are P2PK-locked to our pubkey
fn validate_nutzap_p2pk(proofs_json: &[String], our_p2pk: &str) -> Result<(), String> {
    for proof_json in proofs_json {
        let proof: serde_json::Value =
            serde_json::from_str(proof_json).map_err(|e| format!("Invalid proof JSON: {}", e))?;
        let secret_str = proof
            .get("secret")
            .and_then(|s| s.as_str())
            .ok_or("Proof missing secret")?;
        let secret: serde_json::Value = serde_json::from_str(secret_str)
            .map_err(|e| format!("Invalid secret format: {}", e))?;
        let secret_kind = secret
            .get(0)
            .and_then(|k| k.as_str())
            .ok_or("Secret missing kind")?;
        if secret_kind != "P2PK" {
            return Err("Proof is not P2PK locked".to_string());
        }
        let nonce = secret
            .get(1)
            .and_then(|obj| obj.get("nonce"))
            .and_then(|n| n.as_str())
            .ok_or("P2PK secret missing nonce")?;
        if nonce.is_empty() {
            return Err("P2PK nonce is empty".to_string());
        }
        let locked_pubkey = secret
            .get(1)
            .and_then(|obj| obj.get("data"))
            .and_then(|d| d.as_str())
            .ok_or("P2PK secret missing pubkey data")?
            .to_lowercase();
        let our_p2pk_lower = our_p2pk.to_lowercase();
        if locked_pubkey.len() != 66 {
            return Err(format!(
                "Invalid pubkey length: {} (expected 66)",
                locked_pubkey.len()
            ));
        }
        if !locked_pubkey.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err("Invalid pubkey hex".to_string());
        }
        if !locked_pubkey.starts_with("02") {
            return Err(format!(
                "Invalid pubkey prefix: {} (NIP-61 requires '02' prefix for client-generated keys)",
                &locked_pubkey[0..2],
            ));
        }
        if locked_pubkey != our_p2pk_lower {
            return Err(format!(
                "P2PK lock doesn't match our pubkey. Expected: {}, Got: {}",
                our_p2pk_lower, locked_pubkey,
            ));
        }
    }
    Ok(())
}
/// Internal redemption logic - extracted for proper error handling
async fn redeem_nutzap_inner(
    pending: &PendingNutzap,
    event_id: &str,
) -> Result<NutzapRedeemResult, String> {
    use cdk::nuts::Token;
    use cdk::wallet::ReceiveOptions;
    use std::str::FromStr;
    let mint_url = &pending.mint_url;
    let _lock_guard = try_acquire_mint_lock(mint_url)
        .ok_or_else(|| format!("Another operation in progress for mint: {}", mint_url))?;
    let mut cdk_proofs = Vec::new();
    for proof_json in &pending.proofs_json {
        let proof: cdk::nuts::Proof = serde_json::from_str(proof_json)
            .map_err(|e| format!("Failed to parse proof: {}", e))?;
        cdk_proofs.push(proof);
    }
    let wallet = create_ephemeral_wallet(mint_url, vec![]).await?;
    let signing_keys = collect_p2pk_signing_keys().await;
    if signing_keys.is_empty() {
        return Err("No P2PK signing keys available - cannot redeem nutzap".to_string());
    }
    let mint_url_parsed: cdk::mint_url::MintUrl = mint_url
        .parse()
        .map_err(|e| format!("Invalid mint URL: {}", e))?;
    let currency_unit = cdk::nuts::CurrencyUnit::from_str(&pending.unit)
        .unwrap_or_else(|_| cdk::nuts::CurrencyUnit::Custom(pending.unit.clone()));
    let token = Token::new(mint_url_parsed, cdk_proofs, None, currency_unit);
    let receive_opts = ReceiveOptions {
        p2pk_signing_keys: signing_keys,
        ..Default::default()
    };
    let pre_receive_secrets: std::collections::HashSet<String> = wallet
        .get_unspent_proofs()
        .await
        .map_err(|e| format!("Failed to get pre-receive proofs: {}", e))?
        .iter()
        .map(|p| p.secret.to_string())
        .collect();
    let amount_received = wallet
        .receive(&token.to_string(), receive_opts)
        .await
        .map_err(|e| format!("Failed to receive nutzap: {}", e))?;
    let amount = u64::from(amount_received);
    log::info!("Received {} sats from nutzap", amount);
    let all_proofs = wallet
        .get_unspent_proofs()
        .await
        .map_err(|e| format!("Failed to get proofs: {}", e))?;
    let new_proofs: Vec<_> = all_proofs
        .into_iter()
        .filter(|p| !pre_receive_secrets.contains(&p.secret.to_string()))
        .collect();
    if new_proofs.is_empty() {
        log::warn!(
            "Receive returned {} sats but no new proofs found in database",
            amount
        );
        return Err("Receive succeeded but no new proofs found".to_string());
    }
    log::debug!(
        "Filtered to {} new proofs (excluded {} pre-existing)",
        new_proofs.len(),
        pre_receive_secrets.len()
    );
    let new_event_id = publish_redeemed_token_event(mint_url, &pending.unit, &new_proofs).await?;
    let proof_data: Vec<ProofData> = new_proofs.iter().map(cdk_proof_to_proof_data).collect();
    let token = TokenData {
        event_id: new_event_id.clone(),
        mint: mint_url.to_string(),
        unit: pending.unit.clone(),
        proofs: proof_data.clone(),
        created_at: chrono::Utc::now().timestamp() as u64,
    };
    if let Err(e) = super::signals::atomic_token_replace(vec![token], &[]) {
        log::error!("Failed atomic token replace: {}", e);
        if let Err(sync_err) = cashu_cdk_bridge::sync_wallet_state().await {
            log::warn!(
                "Failed to sync wallet state after atomic_token_replace error: {}",
                sync_err
            );
        }
        return Err(format!("Failed to update wallet tokens: {}", e));
    }
    super::proofs::register_proofs_in_event_map(&new_event_id, &proof_data);
    super::signals::update_wallet_balances();
    if let Err(e) = super::events::create_history_event_full(
        "in",
        amount,
        vec![new_event_id.clone()],
        vec![],
        vec![event_id.to_string()],
    )
    .await
    {
        log::error!("Failed to create history event: {}", e);
    }
    if let Err(e) = cashu_cdk_bridge::sync_wallet_state().await {
        log::warn!("Failed to sync wallet state after nutzap redeem: {}", e);
    }
    Ok(NutzapRedeemResult { amount })
}
/// Redeem a pending nutzap
///
/// Receives the P2PK-locked proofs and creates a history event with "redeemed" tag.
pub async fn redeem_nutzap(event_id: &str) -> Result<NutzapRedeemResult, String> {
    log::info!("Redeeming nutzap: {}", event_id);
    let pending = PENDING_NUTZAPS
        .read()
        .iter()
        .find(|n| n.event_id == event_id)
        .cloned()
        .ok_or_else(|| format!("Pending nutzap not found: {}", event_id))?;
    update_pending_nutzap_status(event_id, NutzapStatus::Redeeming);
    match redeem_nutzap_inner(&pending, event_id).await {
        Ok(result) => {
            update_pending_nutzap_status(event_id, NutzapStatus::Redeemed);
            remove_pending_nutzap(event_id);
            Ok(result)
        }
        Err(e) => {
            if is_transient_error(&e) {
                log::warn!(
                    "Transient error redeeming nutzap {}, reverting to Pending: {}",
                    event_id,
                    e
                );
                update_pending_nutzap_status(event_id, NutzapStatus::Pending);
            } else {
                log::error!("Permanent error redeeming nutzap {}: {}", event_id, e);
                update_pending_nutzap_status(event_id, NutzapStatus::Failed(e.clone()));
            }
            Err(e)
        }
    }
}
/// Get CDK proofs for a specific mint and unit (only spendable proofs)
fn get_proofs_for_mint(mint_url: &str, unit: &str) -> Result<Vec<cdk::nuts::Proof>, String> {
    let store = WALLET_TOKENS.read();
    let data = store.data();
    let tokens = data.read();
    let mut all_proofs = Vec::new();
    for token in tokens
        .iter()
        .filter(|t| mint_matches(&t.mint, mint_url) && t.unit == unit)
    {
        for proof in &token.proofs {
            if proof.state == super::types::ProofState::Unspent {
                all_proofs.push(super::proofs::proof_data_to_cdk_proof(proof)?);
            }
        }
    }
    Ok(all_proofs)
}
/// Get event IDs for a specific mint and unit's tokens
fn get_event_ids_for_mint(mint_url: &str, unit: &str) -> Vec<String> {
    let store = WALLET_TOKENS.read();
    let data = store.data();
    let tokens = data.read();
    tokens
        .iter()
        .filter(|t| mint_matches(&t.mint, mint_url) && t.unit == unit)
        .map(|t| t.event_id.clone())
        .collect()
}
/// Update local state after sending a nutzap
///
/// Uses the safer CDK pattern: local state first with pending_id, then publish.
/// This ensures proofs are never lost if publish fails (can be recovered on restart).
async fn update_local_state_after_nutzap_send(
    mint_url: &str,
    unit: &str,
    keep_proofs: &[cdk::nuts::Proof],
    event_ids_to_delete: &[String],
) -> Result<(), String> {
    if keep_proofs.is_empty() {
        if let Err(e) = super::signals::atomic_token_replace(vec![], event_ids_to_delete) {
            log::error!("Failed atomic token replace (delete): {}", e);
            if let Err(sync_err) = crate::stores::cashu_cdk_bridge::sync_wallet_state().await {
                log::warn!(
                    "Failed to sync wallet state after atomic_token_replace error: {}",
                    sync_err
                );
            }
            return Err(format!("Failed to update wallet tokens: {}", e));
        }
        return Ok(());
    }
    let pending_event_id = format!("pending_{}", uuid::Uuid::new_v4());
    let keep_proof_data: Vec<ProofData> = keep_proofs.iter().map(cdk_proof_to_proof_data).collect();
    let token = TokenData {
        event_id: pending_event_id.clone(),
        mint: mint_url.to_string(),
        unit: unit.to_string(),
        proofs: keep_proof_data.clone(),
        created_at: chrono::Utc::now().timestamp() as u64,
    };
    if let Err(e) = super::signals::atomic_token_replace(vec![token], event_ids_to_delete) {
        log::error!("Failed atomic token replace: {}", e);
        if let Err(sync_err) = crate::stores::cashu_cdk_bridge::sync_wallet_state().await {
            log::warn!(
                "Failed to sync wallet state after atomic_token_replace error: {}",
                sync_err
            );
        }
        return Err(format!("Failed to update wallet tokens: {}", e));
    }
    super::proofs::register_proofs_in_event_map(&pending_event_id, &keep_proof_data);
    let real_event_id =
        publish_change_token_event(mint_url, unit, keep_proofs, event_ids_to_delete).await?;
    if real_event_id != pending_event_id {
        super::events::update_token_event_id(&pending_event_id, &real_event_id);
        super::proofs::register_proofs_in_event_map(&real_event_id, &keep_proof_data);
    }
    Ok(())
}
/// Publish a token event for change proofs after nutzap send
async fn publish_change_token_event(
    mint_url: &str,
    unit: &str,
    proofs: &[cdk::nuts::Proof],
    event_ids_to_delete: &[String],
) -> Result<String, String> {
    let signer = crate::stores::signer::get_signer()
        .ok_or("No signer available")?
        .as_nostr_signer();
    let pubkey_str = auth_store::get_pubkey().ok_or("Not authenticated")?;
    let pubkey = PublicKey::parse(&pubkey_str).map_err(|e| format!("Invalid pubkey: {}", e))?;
    let proof_data: Vec<ProofData> = proofs.iter().map(cdk_proof_to_proof_data).collect();
    let extended_proofs: Vec<ExtendedCashuProof> = proof_data
        .iter()
        .map(|p| ExtendedCashuProof::from(p.clone()))
        .collect();
    let token_event_data = ExtendedTokenEvent {
        mint: mint_url.to_string(),
        unit: unit.to_string(),
        proofs: extended_proofs,
        del: event_ids_to_delete.to_vec(),
    };
    let json_content = serde_json::to_string(&token_event_data)
        .map_err(|e| format!("Failed to serialize: {}", e))?;
    let encrypted = signer
        .nip44_encrypt(&pubkey, &json_content)
        .await
        .map_err(|e| format!("Failed to encrypt: {}", e))?;
    let builder = nostr_sdk::EventBuilder::new(Kind::CashuWalletUnspentProof, encrypted.clone());
    match crate::stores::publish_queue::signing::sign_event_builder(builder).await {
        Ok(signed_event) => {
            let event_id = signed_event.id.to_hex();
            let mut metadata = std::collections::HashMap::new();
            metadata.insert("pending_token_id".to_string(), event_id.clone());
            metadata.insert("mint_url".to_string(), mint_url.to_string());
            crate::stores::publish_queue::enqueue(
                signed_event,
                crate::stores::publish_queue::types::QueueEventType::Cashu,
                None,
                metadata,
            ).await;
            Ok(event_id)
        }
        Err(e) => {
            let pending_id = format!("pending_{}", uuid::Uuid::new_v4());
            log::warn!("Failed to sign change token event, queuing unsigned: {}", e);
            super::events::queue_token_event_for_retry(
                nostr_sdk::EventBuilder::new(Kind::CashuWalletUnspentProof, encrypted),
                pending_id.clone(),
                mint_url.to_string(),
            )
            .await;
            Ok(pending_id)
        }
    }
}
/// Publish a token event for redeemed nutzap proofs
async fn publish_redeemed_token_event(
    mint_url: &str,
    unit: &str,
    proofs: &[cdk::nuts::Proof],
) -> Result<String, String> {
    publish_change_token_event(mint_url, unit, proofs, &[]).await
}
/// Fetch existing nutzaps from relays (for initial load)
pub async fn fetch_pending_nutzaps() -> Result<usize, String> {
    let _my_info = match MY_NUTZAP_INFO.read().clone() {
        Some(info) => info,
        None => return Ok(0),
    };
    let my_pubkey_str = auth_store::get_pubkey().ok_or("Not authenticated")?;
    let my_pubkey =
        PublicKey::parse(&my_pubkey_str).map_err(|e| format!("Invalid pubkey: {}", e))?;
    let client = nostr_client::NOSTR_CLIENT
        .read()
        .as_ref()
        .ok_or("Client not initialized")?
        .clone();
    let since = Timestamp::now() - Duration::from_secs(30 * 24 * 60 * 60);
    let filter = Filter::new()
        .kind(Kind::from(9321))
        .pubkey(my_pubkey)
        .since(since);
    let events = client
        .fetch_events(filter, Duration::from_secs(10))
        .await
        .map_err(|e| format!("Failed to fetch nutzaps: {}", e))?;
    let mut new_count = 0;
    for event in events {
        match process_nutzap_event(&event).await {
            Ok(true) => new_count += 1,
            Ok(false) => {}
            Err(e) => log::warn!("Failed to process nutzap {}: {}", event.id.to_hex(), e),
        }
    }
    log::info!("Fetched {} new pending nutzaps", new_count);
    Ok(new_count)
}
#[cfg(test)]
mod tests {
    use super::is_transient_error;
    /// Test transient (network/connectivity) errors - safe to retry
    /// CDK pattern: These map to temporary failures, not protocol violations
    #[test]
    fn test_transient_errors_network() {
        assert!(
            is_transient_error("network error"),
            "network should be transient"
        );
        assert!(
            is_transient_error("connection refused"),
            "connection should be transient",
        );
        assert!(
            is_transient_error("request timeout"),
            "timeout should be transient"
        );
        assert!(
            is_transient_error("relay unavailable"),
            "unavailable should be transient",
        );
        assert!(
            is_transient_error("temporary failure"),
            "temporary should be transient",
        );
    }
    #[test]
    fn test_transient_errors_protocol() {
        assert!(is_transient_error("http 503 service unavailable"));
        assert!(is_transient_error("websocket disconnected"));
        assert!(is_transient_error("fetch failed"));
    }
    /// Test permanent (validation/state) errors - do NOT retry
    /// CDK pattern: These map to error codes 10xxx (proofs) and 11xxx (state)
    #[test]
    fn test_permanent_errors_cdk() {
        assert!(
            !is_transient_error("TokenAlreadySpent"),
            "already spent is permanent"
        );
        assert!(
            !is_transient_error("token already spent"),
            "already spent variant"
        );
        assert!(
            !is_transient_error("InvalidProofs"),
            "invalid proofs is permanent"
        );
        assert!(
            !is_transient_error("KeysetNotFound"),
            "unknown keyset is permanent"
        );
    }
    #[test]
    fn test_permanent_errors_validation() {
        assert!(!is_transient_error("invalid signature"));
        assert!(!is_transient_error("unknown mint"));
        assert!(!is_transient_error("keyset expired"));
        assert!(!is_transient_error("malformed token data"));
    }
    /// Test unknown errors default to transient (safer to retry)
    /// CDK pattern: Unknown errors should be treated as potentially recoverable
    #[test]
    fn test_unknown_defaults_to_transient() {
        assert!(
            is_transient_error("some random error"),
            "unknown should default transient",
        );
        assert!(
            is_transient_error("unexpected failure"),
            "unexpected should retry"
        );
        assert!(
            is_transient_error("oops something went wrong"),
            "generic errors retry"
        );
    }
    /// Test precedence: permanent patterns checked before transient
    #[test]
    fn test_pattern_precedence() {
        assert!(!is_transient_error("network returned invalid response"));
        assert!(!is_transient_error("connection expired keyset"));
    }
}
