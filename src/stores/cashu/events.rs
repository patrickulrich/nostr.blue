//! NIP-60 Event Handling
//!
//! Functions for creating, publishing, and fetching Nostr events for the Cashu wallet.
//! Handles token events (kind 7375), history events (kind 7376), quote events (kind 7374),
//! and deletion events (kind 5).
#![allow(dead_code)]
use super::proofs::rebuild_proof_event_map;
#[allow(deprecated)]
use super::signals::PENDING_NOSTR_EVENTS;
use super::signals::{SHARED_LOCALSTORE, SYNC_STATE, WALLET_TOKENS};
use super::types::{
    PendingEventType, PendingNostrEvent, ProofData, ProofState, SyncState, TokenData,
    TokenEventData, WalletTokensStoreStoreExt,
};
use super::utils::normalize_mint_url;
use crate::stores::{auth_store, nostr_client};
use dioxus::prelude::*;
use nostr::nips::nip60::{SpendingHistory, TransactionDirection};
use nostr_sdk::{EventId, Filter, Kind, PublicKey, Timestamp};
use std::time::Duration;
#[deprecated(note = "Use publish_queue::enqueue instead. This uses the old Cashu-specific queue.")]
pub async fn queue_nostr_event(
    event_json: String,
    event_type: PendingEventType,
) -> Result<String, String> {
    let event: nostr_sdk::Event = serde_json::from_str(&event_json)
        .map_err(|e| format!("Failed to deserialize event: {}", e))?;
    let mut metadata = std::collections::HashMap::new();
    metadata.insert("event_type".to_string(), format!("{:?}", event_type));
    Ok(crate::stores::publish_queue::enqueue(
        event,
        crate::stores::publish_queue::types::QueueEventType::Cashu,
        None,
        metadata,
    ).await)
}
#[deprecated(note = "Use publish_queue::processor::deque instead. This uses the old Cashu-specific queue.")]
pub async fn remove_pending_event(event_id: &str) -> Result<(), String> {
    #[allow(deprecated)]
    PENDING_NOSTR_EVENTS.write().retain(|e| e.id != event_id);
    if let Some(ref localstore) = *SHARED_LOCALSTORE.read() {
        if let Err(e) = localstore.remove_pending_event(event_id).await {
            log::warn!("Failed to remove pending event from IndexedDB: {}", e);
        }
    }
    log::debug!("Removed pending event from queue: {}", event_id);
    Ok(())
}
pub async fn queue_signed_event_for_retry(
    event: nostr_sdk::Event,
    _event_type: PendingEventType,
    pending_token_id: Option<String>,
    mint_url: Option<String>,
) {
    let mut metadata = std::collections::HashMap::new();
    if let Some(tid) = pending_token_id {
        metadata.insert("pending_token_id".to_string(), tid);
    }
    if let Some(mu) = mint_url {
        metadata.insert("mint_url".to_string(), mu);
    }
    crate::stores::publish_queue::enqueue(
        event,
        crate::stores::publish_queue::types::QueueEventType::Cashu,
        None,
        metadata,
    ).await;
}

pub async fn queue_signed_event_for_retry_result(
    event: nostr_sdk::Event,
    _event_type: PendingEventType,
    pending_token_id: Option<String>,
    mint_url: Option<String>,
) -> Result<(), String> {
    let mut metadata = std::collections::HashMap::new();
    if let Some(tid) = pending_token_id {
        metadata.insert("pending_token_id".to_string(), tid);
    }
    if let Some(mu) = mint_url {
        metadata.insert("mint_url".to_string(), mu);
    }
    crate::stores::publish_queue::enqueue(
        event,
        crate::stores::publish_queue::types::QueueEventType::Cashu,
        None,
        metadata,
    ).await;
    Ok(())
}
pub async fn sign_event_builder(
    builder: nostr_sdk::EventBuilder,
) -> Result<nostr_sdk::Event, String> {
    crate::stores::publish_queue::signing::sign_event_builder(builder).await
}

pub async fn sign_event_builder_with_signer(
    builder: nostr_sdk::EventBuilder,
    signer: crate::stores::signer::SignerType,
) -> Result<nostr_sdk::Event, String> {
    crate::stores::publish_queue::signing::sign_event_builder_with_signer(builder, signer).await
}

pub async fn publish_signed_event(
    _client: &nostr_sdk::Client,
    event: &nostr_sdk::Event,
) -> Result<(), String> {
    crate::stores::publish_queue::enqueue(
        event.clone(),
        crate::stores::publish_queue::types::QueueEventType::Cashu,
        None,
        std::collections::HashMap::new(),
    ).await;
    Ok(())
}
pub async fn queue_token_event_for_retry_with_history(
    builder: nostr_sdk::EventBuilder,
    pending_token_id: String,
    mint_url: String,
    _history_amount: u64,
    _history_type: String,
) -> Result<(), String> {
    let event = crate::stores::publish_queue::signing::sign_event_builder(builder).await?;
    let mut metadata = std::collections::HashMap::new();
    metadata.insert("pending_token_id".to_string(), pending_token_id);
    metadata.insert("mint_url".to_string(), mint_url);
    crate::stores::publish_queue::enqueue(
        event,
        crate::stores::publish_queue::types::QueueEventType::Cashu,
        None,
        metadata,
    ).await;
    Ok(())
}

pub async fn queue_event_for_retry(
    builder: nostr_sdk::EventBuilder,
    _event_type: PendingEventType,
    pending_token_id: Option<String>,
    mint_url: Option<String>,
) -> Result<(), String> {
    let event = crate::stores::publish_queue::signing::sign_event_builder(builder).await?;
    let mut metadata = std::collections::HashMap::new();
    if let Some(tid) = pending_token_id {
        metadata.insert("pending_token_id".to_string(), tid);
    }
    if let Some(mu) = mint_url {
        metadata.insert("mint_url".to_string(), mu);
    }
    crate::stores::publish_queue::enqueue(
        event,
        crate::stores::publish_queue::types::QueueEventType::Cashu,
        None,
        metadata,
    ).await;
    Ok(())
}
pub async fn queue_token_event_for_retry(
    builder: nostr_sdk::EventBuilder,
    pending_token_id: String,
    mint_url: String,
) {
    let event = match crate::stores::publish_queue::signing::sign_event_builder(builder).await {
        Ok(e) => e,
        Err(e) => {
            log::error!("Cannot queue token event: {}", e);
            return;
        }
    };
    let mut metadata = std::collections::HashMap::new();
    metadata.insert("pending_token_id".to_string(), pending_token_id.clone());
    metadata.insert("mint_url".to_string(), mint_url);
    crate::stores::publish_queue::enqueue(
        event,
        crate::stores::publish_queue::types::QueueEventType::Cashu,
        None,
        metadata,
    ).await;
    log::info!(
        "Queued token event for retry, pending_id={}",
        pending_token_id
    );
}
#[deprecated(note = "Use publish_queue::get_pending_count instead")]
pub fn get_pending_event_count() -> usize {
    #[allow(deprecated)]
    PENDING_NOSTR_EVENTS.read().len()
}
pub async fn publish_quote_event(
    quote_id: &str,
    mint_url: &str,
    expiration_days: u64,
) -> Result<String, String> {
    let signer_type = crate::stores::signer::get_signer().ok_or("No signer available")?;
    let signer = signer_type.as_nostr_signer();
    let pubkey_str = auth_store::get_pubkey().ok_or("Not authenticated")?;
    let pubkey = PublicKey::parse(&pubkey_str).map_err(|e| format!("Invalid pubkey: {}", e))?;
    let encrypted = signer
        .nip44_encrypt(&pubkey, quote_id)
        .await
        .map_err(|e| format!("Failed to encrypt quote ID: {}", e))?;
    let expiration_ts = Timestamp::now() + (expiration_days * 24 * 60 * 60);
    let builder = nostr_sdk::EventBuilder::new(Kind::CashuWalletQuote, encrypted).tags(vec![
        nostr_sdk::Tag::custom(nostr_sdk::TagKind::custom("mint"), [mint_url]),
        nostr_sdk::Tag::expiration(expiration_ts),
    ]);
    let event = crate::stores::publish_queue::signing::sign_event_builder_with_signer(builder, signer_type).await?;
    let event_id = event.id.to_hex();
    crate::stores::publish_queue::enqueue(
        event,
        crate::stores::publish_queue::types::QueueEventType::Cashu,
        None,
        std::collections::HashMap::new(),
    ).await;
    log::info!("Queued quote event for quote {}: {}", quote_id, event_id);
    Ok(event_id)
}
pub async fn delete_quote_event(event_id: &str) -> Result<(), String> {
    let mut tags = vec![nostr_sdk::Tag::event(
        nostr_sdk::EventId::from_hex(event_id).map_err(|e| format!("Invalid event ID: {}", e))?,
    )];
    tags.push(nostr_sdk::Tag::custom(
        nostr_sdk::TagKind::custom("k"),
        ["7374"],
    ));
    let deletion_builder = nostr_sdk::EventBuilder::new(Kind::from(5), "Quote expired").tags(tags);
    let event = crate::stores::publish_queue::signing::sign_event_builder(deletion_builder).await?;
    crate::stores::publish_queue::enqueue(
        event,
        crate::stores::publish_queue::types::QueueEventType::Cashu,
        None,
        std::collections::HashMap::new(),
    ).await;
    log::info!("Queued deletion for quote event: {}", event_id);
    Ok(())
}
/// Fetch token events (kind 7375) with incremental sync support
///
/// Uses the `since` filter when sync state exists to avoid fetching all events.
/// On first run or after reset, fetches all events to build initial state.
pub async fn fetch_tokens() -> Result<(), String> {
    use std::collections::HashSet;
    let pubkey_str = auth_store::get_pubkey().ok_or("Not authenticated")?;
    let client = nostr_client::NOSTR_CLIENT
        .read()
        .as_ref()
        .ok_or("Client not initialized")?
        .clone();
    let pubkey = PublicKey::parse(&pubkey_str).map_err(|e| format!("Invalid pubkey: {}", e))?;
    let sync_state = if let Some(ref localstore) = *SHARED_LOCALSTORE.read() {
        match localstore.load_sync_state().await {
            Ok(Some(state)) => {
                *SYNC_STATE.write() = Some(state.clone());
                Some(state)
            }
            Ok(None) => SYNC_STATE.read().clone(),
            Err(e) => {
                log::warn!("Failed to load sync state from IndexedDB: {}", e);
                SYNC_STATE.read().clone()
            }
        }
    } else {
        SYNC_STATE.read().clone()
    };
    let last_sync_ts = sync_state.as_ref().map(|s| s.last_token_sync).unwrap_or(0);
    let is_incremental = last_sync_ts > 0;
    if is_incremental {
        log::info!("Fetching token events (incremental since {})", last_sync_ts);
    } else {
        log::info!("Fetching token events (full sync)");
    }
    nostr_client::ensure_relays_ready(&client).await;
    let deletion_filter = if is_incremental {
        Filter::new()
            .author(pubkey)
            .kind(Kind::from(5))
            .since(Timestamp::from(last_sync_ts))
    } else {
        Filter::new().author(pubkey).kind(Kind::from(5))
    };
    let mut deleted_event_ids = HashSet::new();
    if let Ok(deletion_events) = client
        .fetch_events(deletion_filter, Duration::from_secs(10))
        .await
    {
        for del_event in deletion_events {
            for tag in del_event.tags.iter() {
                if let Some(nostr::TagStandard::Event { event_id, .. }) = tag.as_standardized() {
                    deleted_event_ids.insert(event_id.to_hex());
                }
            }
        }
        if !deleted_event_ids.is_empty() {
            log::info!(
                "Found {} deleted token events via kind-5",
                deleted_event_ids.len()
            );
        }
    }
    let filter = if is_incremental {
        Filter::new()
            .author(pubkey)
            .kind(Kind::from(7375))
            .since(Timestamp::from(last_sync_ts))
    } else {
        Filter::new().author(pubkey).kind(Kind::from(7375))
    };
    match client.fetch_events(filter, Duration::from_secs(10)).await {
        Ok(events) => {
            let signer = crate::stores::signer::get_signer()
                .ok_or("No signer available")?
                .as_nostr_signer();
            let events: Vec<_> = events.into_iter().collect();
            let mut decrypted_map: std::collections::HashMap<nostr_sdk::EventId, String> =
                std::collections::HashMap::new();
            let mut deleted_via_del_field = HashSet::new();
            for event in &events {
                if deleted_event_ids.contains(&event.id.to_hex()) {
                    continue;
                }
                match super::internal::nip44_decrypt_cached(
                    signer.clone(),
                    event.id,
                    event.pubkey,
                    event.content.clone(),
                )
                .await
                {
                    Ok(decrypted) => {
                        if let Ok(token_event) = serde_json::from_str::<TokenEventData>(&decrypted) {
                            for del_event_id in &token_event.del {
                                deleted_via_del_field.insert(del_event_id.clone());
                            }
                        }
                        decrypted_map.insert(event.id, decrypted);
                    }
                    Err(e) => {
                        log::error!("Failed to decrypt token event {}: {}", event.id, e);
                    }
                }
            }
            if !deleted_via_del_field.is_empty() {
                log::info!(
                    "Found {} deleted token events via del field",
                    deleted_via_del_field.len()
                );
            }
            let all_deleted_events: HashSet<String> = deleted_event_ids
                .union(&deleted_via_del_field)
                .cloned()
                .collect();
            let mut tokens = Vec::new();
            let mut total_balance = 0u64;
            for event in &events {
                let event_id_hex = event.id.to_hex();
                if all_deleted_events.contains(&event_id_hex) {
                    log::debug!("Skipping deleted token event: {}", event_id_hex);
                    continue;
                }
                let decrypted = match decrypted_map.get(&event.id) {
                    Some(d) => d,
                    None => continue,
                };
                match serde_json::from_str::<TokenEventData>(decrypted) {
                    Ok(token_event) => {
                        let proofs: Vec<ProofData> = token_event
                            .proofs
                            .iter()
                            .map(|p| ProofData {
                                id: if p.id.is_empty() {
                                    format!("{}_{}", p.secret, p.amount)
                                } else {
                                    p.id.clone()
                                },
                                amount: p.amount,
                                secret: p.secret.clone(),
                                c: p.c.clone(),
                                witness: p.witness.clone(),
                                dleq: p.dleq.clone(),
                                state: ProofState::Unspent,
                                transaction_id: None,
                                state_set_at: None,
                            })
                            .collect();
                        if !proofs.is_empty() {
                            let token_balance: u64 = proofs
                                .iter()
                                .map(|p| p.amount)
                                .try_fold(0u64, |acc, amount| acc.checked_add(amount))
                                .ok_or_else(|| {
                                    format!(
                                        "Proof amount overflow in token event {}",
                                        event_id_hex,
                                    )
                                })?;
                            total_balance =
                                total_balance.checked_add(token_balance).ok_or_else(|| {
                                    format!(
                                        "Balance overflow when adding token event {}",
                                        event_id_hex,
                                    )
                                })?;
                            tokens.push(TokenData {
                                event_id: event_id_hex,
                                mint: normalize_mint_url(&token_event.mint),
                                unit: token_event.unit.clone(),
                                proofs,
                                created_at: event.created_at.as_secs(),
                            });
                        }
                    }
                    Err(e) => {
                        log::error!("Failed to parse token event {}: {}", event.id, e);
                    }
                }
            }
            if is_incremental {
                let new_token_count = tokens.len();
                let existing_tokens = WALLET_TOKENS.read().data().read().clone();
                let existing_event_ids: HashSet<String> =
                    existing_tokens.iter().map(|t| t.event_id.clone()).collect();
                let mut merged_tokens: Vec<TokenData> = existing_tokens
                    .into_iter()
                    .filter(|t| !all_deleted_events.contains(&t.event_id))
                    .collect();
                for token in tokens {
                    if !existing_event_ids.contains(&token.event_id) {
                        merged_tokens.push(token);
                    }
                }
                total_balance = merged_tokens
                    .iter()
                    .flat_map(|t| &t.proofs)
                    .map(|p| p.amount)
                    .try_fold(0u64, |acc, amount| acc.checked_add(amount))
                    .ok_or("Balance calculation overflow in merge")?;
                log::info!(
                    "Incremental sync: {} total tokens, {} sats (fetched {} new events)",
                    merged_tokens.len(),
                    total_balance,
                    new_token_count
                );
                *WALLET_TOKENS.read().data().write() = merged_tokens;
            } else {
                log::info!(
                    "Full sync: {} token events with {} sats",
                    tokens.len(),
                    total_balance
                );
                *WALLET_TOKENS.read().data().write() = tokens;
            }
            super::signals::update_wallet_balances();
            let new_sync_ts = Timestamp::now().as_secs().saturating_sub(300);
            let known_ids: HashSet<String> = WALLET_TOKENS
                .read()
                .data()
                .read()
                .iter()
                .map(|t| t.event_id.clone())
                .collect();
            let new_sync_state = SyncState {
                last_token_sync: new_sync_ts,
                last_history_sync: sync_state
                    .as_ref()
                    .map(|s| s.last_history_sync)
                    .unwrap_or(0),
                last_deletion_sync: sync_state
                    .as_ref()
                    .map(|s| s.last_deletion_sync)
                    .unwrap_or(0),
                known_token_event_ids: known_ids,
            };
            *SYNC_STATE.write() = Some(new_sync_state.clone());
            if let Some(ref localstore) = *SHARED_LOCALSTORE.read() {
                if let Err(e) = localstore.save_sync_state(&new_sync_state).await {
                    log::warn!("Failed to persist sync state to IndexedDB: {}", e);
                }
            }
            rebuild_proof_event_map();
            Ok(())
        }
        Err(e) => {
            log::error!("Failed to fetch token events: {}", e);
            Err(format!("Failed to fetch token events: {}", e))
        }
    }
}
/// Create a history event (kind 7376)
pub async fn create_history_event(
    direction: &str,
    amount: u64,
    created_tokens: Vec<String>,
    destroyed_tokens: Vec<String>,
) -> Result<(), String> {
    create_history_event_full(direction, amount, created_tokens, destroyed_tokens, vec![]).await
}
/// Create a history event with full control over all fields
pub async fn create_history_event_full(
    direction: &str,
    amount: u64,
    created_tokens: Vec<String>,
    destroyed_tokens: Vec<String>,
    redeemed_tokens: Vec<String>,
) -> Result<(), String> {
    use nostr_sdk::signer::NostrSigner;
    let signer_type = crate::stores::signer::get_signer().ok_or("No signer available")?;
    let signer = signer_type.as_nostr_signer();
    let pubkey_str = auth_store::get_pubkey().ok_or("Not authenticated")?;
    let pubkey = PublicKey::parse(&pubkey_str).map_err(|e| format!("Invalid pubkey: {}", e))?;
    let direction_enum = match direction {
        "in" => TransactionDirection::In,
        "out" => TransactionDirection::Out,
        _ => return Err("Invalid direction".to_string()),
    };
    let mut spending_history = SpendingHistory::new(direction_enum, amount);
    for token_id in created_tokens {
        if let Ok(event_id) = EventId::from_hex(&token_id) {
            spending_history = spending_history.add_created(event_id);
        } else {
            log::warn!("Skipping invalid created token ID: {}", token_id);
        }
    }
    for token_id in destroyed_tokens {
        if let Ok(event_id) = EventId::from_hex(&token_id) {
            spending_history = spending_history.add_destroyed(event_id);
        } else {
            log::warn!("Skipping invalid destroyed token ID: {}", token_id);
        }
    }
    for token_id in redeemed_tokens {
        if let Ok(event_id) = EventId::from_hex(&token_id) {
            spending_history = spending_history.add_redeemed(event_id);
        } else {
            log::warn!("Skipping invalid redeemed token ID: {}", token_id);
        }
    }
    let mut content_data: Vec<Vec<String>> = vec![
        vec![
            "direction".to_string(),
            spending_history.direction.to_string(),
        ],
        vec!["amount".to_string(), spending_history.amount.to_string()],
        vec!["unit".to_string(), "sat".to_string()],
    ];
    for event_id in &spending_history.created {
        content_data.push(vec![
            "e".to_string(),
            event_id.to_hex(),
            String::new(),
            "created".to_string(),
        ]);
    }
    for event_id in &spending_history.destroyed {
        content_data.push(vec![
            "e".to_string(),
            event_id.to_hex(),
            String::new(),
            "destroyed".to_string(),
        ]);
    }
    let json_content = serde_json::to_string(&content_data)
        .map_err(|e| format!("Failed to serialize history event: {}", e))?;
    let encrypted = signer
        .nip44_encrypt(&pubkey, &json_content)
        .await
        .map_err(|e| format!("Failed to encrypt history event: {}", e))?;
    let mut tags = Vec::new();
    for event_id in &spending_history.redeemed {
        tags.push(
            nostr_sdk::Tag::parse([
                "e".to_string(),
                event_id.to_hex(),
                String::new(),
                "redeemed".to_string(),
            ])
            .map_err(|e| format!("Failed to create redeemed tag: {}", e))?,
        );
    }
    let builder =
        nostr_sdk::EventBuilder::new(Kind::CashuWalletSpendingHistory, encrypted).tags(tags);
    let event = crate::stores::publish_queue::signing::sign_event_builder_with_signer(builder, signer_type).await?;
    crate::stores::publish_queue::enqueue(
        event,
        crate::stores::publish_queue::types::QueueEventType::Cashu,
        None,
        std::collections::HashMap::new(),
    ).await;
    Ok(())
}
async fn publish_pending_event(_event: &PendingNostrEvent) -> Result<String, String> {
    Err("Deprecated: universal publish queue handles publishing".to_string())
}

pub async fn process_pending_events() -> Result<usize, String> {
    log::debug!("process_pending_events is a no-op; universal publish queue handles this");
    Ok(0)
}
/// Update a token's event_id after successful background publish
///
/// Called when a pending TokenEvent is successfully published to Nostr.
/// Updates the token in WALLET_TOKENS from the pending_id to the real Nostr event_id.
pub(crate) fn update_token_event_id(pending_id: &str, real_event_id: &str) {
    let pending_id_owned = pending_id.to_string();
    let real_event_id_owned = real_event_id.to_string();
    if let Err(e) = super::signals::atomic_token_update(|tokens| {
        if let Some(token) = tokens.iter_mut().find(|t| t.event_id == pending_id_owned) {
            log::info!(
                "Updating token event_id: {} -> {}",
                pending_id_owned,
                real_event_id_owned
            );
            token.event_id = real_event_id_owned.clone();
            Ok(())
        } else {
            Err(format!(
                "Token with pending_id {} not found",
                pending_id_owned
            ))
        }
    }) {
        log::warn!("Failed to update token event_id: {}", e);
        return;
    }
    rebuild_proof_event_map();
}
/// Reconcile tokens with pending event IDs
///
/// Finds tokens in WALLET_TOKENS that have `pending_*` event IDs and attempts
/// to publish them to Nostr. This handles cases where background retry failed
/// permanently but proofs are valid in CDK.
///
/// CDK Pattern: Similar to CDK's `check_all_pending_proofs()` which reconciles
/// pending proof states. We apply the same pattern for NIP-60 event IDs.
///
/// Called periodically by the background processor.
pub async fn reconcile_pending_event_ids() -> Result<usize, String> {
    use super::types::ExtendedCashuProof;
    use nostr_sdk::signer::NostrSigner;
    let signer = crate::stores::signer::get_signer()
        .ok_or("No signer available")?
        .as_nostr_signer();
    let pubkey_str = auth_store::get_pubkey().ok_or("Not authenticated")?;
    let pubkey = PublicKey::parse(&pubkey_str).map_err(|e| format!("Invalid pubkey: {}", e))?;
    let pending_tokens: Vec<TokenData> = {
        let store = WALLET_TOKENS.read();
        let data = store.data();
        let tokens = data.read();
        tokens
            .iter()
            .filter(|t| t.event_id.starts_with("pending_"))
            .cloned()
            .collect()
    };
    if pending_tokens.is_empty() {
        return Ok(0);
    }
    log::info!(
        "Found {} tokens with pending event IDs to reconcile",
        pending_tokens.len()
    );
    let mut reconciled = 0;
    for token in pending_tokens {
        let old_event_id = token.event_id.clone();
        let proof_secrets: std::collections::HashSet<&str> =
            token.proofs.iter().map(|p| p.secret.as_str()).collect();
        let existing_real_event = {
            let store = WALLET_TOKENS.read();
            let data = store.data();
            let tokens = data.read();
            tokens
                .iter()
                .filter(|t| !t.event_id.starts_with("pending_"))
                .find(|t| {
                    let t_secrets: std::collections::HashSet<&str> =
                        t.proofs.iter().map(|p| p.secret.as_str()).collect();
                    !proof_secrets.is_empty() && proof_secrets == t_secrets
                })
                .map(|t| t.event_id.clone())
        };
        if let Some(real_event_id) = existing_real_event {
            log::info!(
                "Found existing real event {} for pending token {}",
                real_event_id,
                old_event_id
            );
            update_token_event_id(&old_event_id, &real_event_id);
            reconciled += 1;
            continue;
        }
        let extended_proofs: Vec<ExtendedCashuProof> = token
            .proofs
            .iter()
            .map(|p| ExtendedCashuProof::from(p.clone()))
            .collect();
        let token_event_data = super::types::ExtendedTokenEvent {
            mint: token.mint.clone(),
            unit: token.unit.clone(),
            proofs: extended_proofs,
            del: vec![],
        };
        let json_content = match serde_json::to_string(&token_event_data) {
            Ok(j) => j,
            Err(e) => {
                log::warn!("Failed to serialize token {}: {}", old_event_id, e);
                continue;
            }
        };
        let encrypted = match signer.nip44_encrypt(&pubkey, &json_content).await {
            Ok(e) => e,
            Err(e) => {
                log::warn!("Failed to encrypt token {}: {}", old_event_id, e);
                continue;
            }
        };
        let builder = nostr_sdk::EventBuilder::new(Kind::CashuWalletUnspentProof, encrypted);
        match crate::stores::publish_queue::signing::sign_event_builder(builder).await {
            Ok(signed_event) => {
                let real_event_id = signed_event.id.to_hex();
                crate::stores::publish_queue::enqueue(
                    signed_event,
                    crate::stores::publish_queue::types::QueueEventType::Cashu,
                    None,
                    std::collections::HashMap::new(),
                ).await;
                log::info!(
                    "Reconciled pending token: {} -> {} (queued for publish)",
                    old_event_id,
                    real_event_id
                );
                update_token_event_id(&old_event_id, &real_event_id);
                reconciled += 1;
            }
            Err(e) => {
                log::warn!(
                    "Failed to sign reconciliation event for {}: {}",
                    old_event_id,
                    e
                );
            }
        }
    }
    if reconciled > 0 {
        log::info!("Reconciled {} pending event IDs", reconciled);
    }
    Ok(reconciled)
}
/// Publish a token event for orphaned proofs discovered in CDK
///
/// This follows the same pattern as publish_send_events but without deletion events.
/// Used by `sync_orphaned_cdk_proofs_to_nostr()` to publish proofs that exist in CDK
/// but are not in WALLET_TOKENS (e.g., from crashed send/melt operations).
///
/// CRASH SAFETY (CDK Saga Pattern): Persists state BEFORE external operations.
/// 1. Generate pending_id BEFORE any network operation
/// 2. Insert TokenData into WALLET_TOKENS first
/// 3. Attempt publish
/// 4. On success: update pending_id to real event_id
/// 5. On failure: proofs remain accessible via pending_id for retry
///
/// # Arguments
/// * `mint_url` - The mint URL for these proofs
/// * `proofs` - The proof data to publish
/// * `unit` - The unit for these proofs (CDK pattern: unit passed explicitly, not deduced from proofs)
pub async fn publish_orphaned_proofs_event(
    mint_url: &str,
    proofs: &[ProofData],
    unit: &str,
) -> Result<String, String> {
    use super::types::ExtendedCashuProof;
    use nostr_sdk::signer::NostrSigner;
    if proofs.is_empty() {
        return Err("No proofs to publish".to_string());
    }
    let signer = crate::stores::signer::get_signer()
        .ok_or("No signer available")?
        .as_nostr_signer();
    let pubkey_str = auth_store::get_pubkey().ok_or("Not authenticated")?;
    let pubkey = PublicKey::parse(&pubkey_str).map_err(|e| format!("Invalid pubkey: {}", e))?;
    let pending_id = format!("pending_{}", uuid::Uuid::new_v4());
    let extended_proofs: Vec<ExtendedCashuProof> = proofs
        .iter()
        .map(|p| ExtendedCashuProof::from(p.clone()))
        .collect();
    let token_event_data = super::types::ExtendedTokenEvent {
        mint: mint_url.to_string(),
        unit: unit.to_string(),
        proofs: extended_proofs,
        del: vec![],
    };
    let json_content = serde_json::to_string(&token_event_data)
        .map_err(|e| format!("Failed to serialize: {}", e))?;
    let encrypted = signer
        .nip44_encrypt(&pubkey, &json_content)
        .await
        .map_err(|e| format!("Failed to encrypt: {}", e))?;
    let builder = nostr_sdk::EventBuilder::new(Kind::CashuWalletUnspentProof, encrypted);
    let signed_event = crate::stores::publish_queue::signing::sign_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to sign: {}", e))?;
    let normalized_mint = normalize_mint_url(mint_url);
    let token_data = TokenData {
        event_id: pending_id.clone(),
        mint: normalized_mint.clone(),
        unit: unit.to_string(),
        proofs: proofs.to_vec(),
        created_at: chrono::Utc::now().timestamp() as u64,
    };
    if let Err(e) = super::signals::atomic_token_update(|tokens| {
        tokens.push(token_data);
        Ok(())
    }) {
        log::warn!("Failed to pre-persist orphaned proofs token: {}", e);
    } else {
        super::proofs::register_proofs_in_event_map(&pending_id, proofs);
    }
    let mut metadata = std::collections::HashMap::new();
    metadata.insert("pending_token_id".to_string(), pending_id.clone());
    metadata.insert("mint_url".to_string(), mint_url.to_string());
    crate::stores::publish_queue::enqueue(
        signed_event,
        crate::stores::publish_queue::types::QueueEventType::Cashu,
        None,
        metadata,
    ).await;
    log::info!("Queued orphaned proofs token event: {}", pending_id);
    Ok(pending_id)
}
/// Start background task to process pending events periodically
///
/// Uses AtomicBool guard to ensure only one processor runs at a time.
/// Calling this function multiple times is safe - subsequent calls are no-ops.
///
/// Uses adaptive interval: 30s when there are pending events, 60s when idle.
/// Maintenance tasks (recovery, cleanup) run every 6th idle iteration (~6 min cadence).
/// Also runs periodic proof recovery and pending secrets cleanup.
#[cfg(target_arch = "wasm32")]
pub fn start_pending_events_processor() {
    crate::stores::publish_queue::start_processor();
    log::info!("Redirected to universal publish queue processor");
}

#[cfg(not(target_arch = "wasm32"))]
pub fn start_pending_events_processor() {
    crate::stores::publish_queue::start_processor();
    log::info!("Redirected to universal publish queue processor (native)");
}
