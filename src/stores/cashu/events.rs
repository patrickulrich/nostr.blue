//! NIP-60 Event Handling
//!
//! Functions for creating, publishing, and fetching Nostr events for the Cashu wallet.
//! Handles token events (kind 7375), history events (kind 7376), quote events (kind 7374),
//! and deletion events (kind 5).

// Allow dead_code for planned features not yet wired to UI
#![allow(dead_code)]

use dioxus::prelude::*;
use nostr_sdk::{Kind, Filter, PublicKey, EventId, Timestamp};
use nostr::nips::nip60::{SpendingHistory, TransactionDirection};
use std::time::Duration;

use super::signals::{
    PENDING_NOSTR_EVENTS, WALLET_TOKENS, WALLET_BALANCE, SHARED_LOCALSTORE, SYNC_STATE,
};
use super::types::{TokenData, ProofData, ProofState, TokenEventData, WalletTokensStoreStoreExt, PendingEventType, PendingNostrEvent, SyncState};
use super::proofs::rebuild_proof_event_map;
use crate::stores::{auth_store, nostr_client};

// =============================================================================
// Event Queue Management
// =============================================================================

/// Queue a Nostr event for publication (with offline support)
///
/// Events are saved to IndexedDB for persistence across app restarts.
/// A background task will publish queued events when possible.
pub async fn queue_nostr_event(
    event_json: String,
    event_type: PendingEventType,
) -> Result<String, String> {
    use uuid::Uuid;

    let event_id = Uuid::new_v4().to_string();
    let created_at = chrono::Utc::now().timestamp() as u64;

    let pending = PendingNostrEvent {
        id: event_id.clone(),
        builder_json: event_json,
        event_type: event_type.clone(),
        created_at,
        retry_count: 0,
    };

    // Save to in-memory queue
    PENDING_NOSTR_EVENTS.write().push(pending.clone());

    // Persist to IndexedDB for offline support
    if let Some(ref localstore) = *SHARED_LOCALSTORE.read() {
        if let Err(e) = localstore.add_pending_event(&pending).await {
            log::warn!("Failed to persist pending event to IndexedDB: {}", e);
        }
    }

    log::debug!("Queued {} event: {}",
        match event_type {
            PendingEventType::TokenEvent => "token",
            PendingEventType::DeletionEvent => "deletion",
            PendingEventType::HistoryEvent => "history",
            PendingEventType::QuoteEvent => "quote",
        },
        event_id);

    Ok(event_id)
}

/// Remove a pending event from the queue and IndexedDB
pub async fn remove_pending_event(event_id: &str) -> Result<(), String> {
    // Remove from in-memory queue
    PENDING_NOSTR_EVENTS.write().retain(|e| e.id != event_id);

    // Remove from IndexedDB
    if let Some(ref localstore) = *SHARED_LOCALSTORE.read() {
        if let Err(e) = localstore.remove_pending_event(event_id).await {
            log::warn!("Failed to remove pending event from IndexedDB: {}", e);
        }
    }

    log::debug!("Removed pending event from queue: {}", event_id);
    Ok(())
}

/// Queue an already-signed Event for retry when publication fails
pub async fn queue_signed_event_for_retry(event: nostr_sdk::Event, event_type: PendingEventType) {
    match serde_json::to_string(&event) {
        Ok(event_json) => {
            match queue_nostr_event(event_json, event_type).await {
                Ok(queue_id) => {
                    log::info!("Queued signed event {} for retry: {}", event.id.to_hex(), queue_id);
                }
                Err(queue_err) => {
                    log::error!("Failed to queue event for retry: {}", queue_err);
                }
            }
        }
        Err(json_err) => {
            log::error!("Failed to serialize event for queueing: {}", json_err);
        }
    }
}

/// Queue an EventBuilder for retry when initial publication fails
pub async fn queue_event_for_retry(builder: nostr_sdk::EventBuilder, event_type: PendingEventType) {
    let signer = match crate::stores::signer::get_signer() {
        Some(s) => s,
        None => {
            log::error!("Cannot queue failed event: no signer available");
            return;
        }
    };

    let event_type_clone = event_type.clone();
    let sign_and_queue = |event: nostr_sdk::Event| async move {
        match serde_json::to_string(&event) {
            Ok(event_json) => {
                match queue_nostr_event(event_json, event_type_clone).await {
                    Ok(queue_id) => {
                        log::info!("Queued failed event for retry: {}", queue_id);
                    }
                    Err(queue_err) => {
                        log::error!("Failed to queue event for retry: {}", queue_err);
                    }
                }
            }
            Err(json_err) => {
                log::error!("Failed to serialize event for queueing: {}", json_err);
            }
        }
    };

    match signer {
        crate::stores::signer::SignerType::Keys(keys) => {
            match builder.sign_with_keys(&keys) {
                Ok(event) => sign_and_queue(event).await,
                Err(sign_err) => {
                    log::error!("Failed to sign event for queueing: {}", sign_err);
                }
            }
        }
        #[cfg(target_family = "wasm")]
        crate::stores::signer::SignerType::BrowserExtension(browser_signer) => {
            match builder.sign(&*browser_signer).await {
                Ok(event) => sign_and_queue(event).await,
                Err(sign_err) => {
                    log::error!("Failed to sign event for queueing: {}", sign_err);
                }
            }
        }
        crate::stores::signer::SignerType::NostrConnect(remote_signer) => {
            match builder.sign(&*remote_signer).await {
                Ok(event) => sign_and_queue(event).await,
                Err(sign_err) => {
                    log::error!("Failed to sign event for queueing: {}", sign_err);
                }
            }
        }
    }
}

/// Get count of pending events waiting to be published
pub fn get_pending_event_count() -> usize {
    PENDING_NOSTR_EVENTS.read().len()
}

// =============================================================================
// Quote Events (Kind 7374)
// =============================================================================

/// Publish a quote event to relays (NIP-60 kind 7374)
pub async fn publish_quote_event(
    quote_id: &str,
    mint_url: &str,
    expiration_days: u64,
) -> Result<String, String> {
    use nostr_sdk::signer::NostrSigner;

    let signer = crate::stores::signer::get_signer()
        .ok_or("No signer available")?
        .as_nostr_signer();

    let pubkey_str = auth_store::get_pubkey()
        .ok_or("Not authenticated")?;
    let pubkey = PublicKey::parse(&pubkey_str)
        .map_err(|e| format!("Invalid pubkey: {}", e))?;

    // Encrypt quote ID with NIP-44
    let encrypted = signer.nip44_encrypt(&pubkey, quote_id).await
        .map_err(|e| format!("Failed to encrypt quote ID: {}", e))?;

    // Calculate expiration timestamp
    let expiration_ts = Timestamp::now() + (expiration_days * 24 * 60 * 60);

    let builder = nostr_sdk::EventBuilder::new(Kind::CashuWalletQuote, encrypted)
        .tags(vec![
            nostr_sdk::Tag::custom(nostr_sdk::TagKind::custom("mint"), [mint_url]),
            nostr_sdk::Tag::expiration(expiration_ts),
        ]);

    let client = nostr_client::NOSTR_CLIENT.read().as_ref()
        .ok_or("Nostr client not initialized")?
        .clone();

    match client.send_event_builder(builder).await {
        Ok(output) => {
            let event_id = output.id().to_hex();
            log::info!("Published quote event for quote {}: {}", quote_id, event_id);
            Ok(event_id)
        }
        Err(e) => {
            log::warn!("Failed to publish quote event: {}", e);
            Err(format!("Failed to publish quote event: {}", e))
        }
    }
}

/// Delete a quote event from relays
pub async fn delete_quote_event(event_id: &str) -> Result<(), String> {
    let client = nostr_client::NOSTR_CLIENT.read().as_ref()
        .ok_or("Nostr client not initialized")?
        .clone();

    let mut tags = vec![nostr_sdk::Tag::event(
        nostr_sdk::EventId::from_hex(event_id)
            .map_err(|e| format!("Invalid event ID: {}", e))?
    )];

    tags.push(nostr_sdk::Tag::custom(
        nostr_sdk::TagKind::custom("k"),
        ["7374"]
    ));

    let deletion_builder = nostr_sdk::EventBuilder::new(
        Kind::from(5),
        "Quote expired"
    ).tags(tags);

    match client.send_event_builder(deletion_builder).await {
        Ok(_) => {
            log::info!("Published deletion for quote event: {}", event_id);
            Ok(())
        }
        Err(e) => {
            log::warn!("Failed to delete quote event: {}", e);
            Ok(()) // Non-critical
        }
    }
}

// =============================================================================
// Token Events (Kind 7375)
// =============================================================================

/// Fetch token events (kind 7375) with incremental sync support
///
/// Uses the `since` filter when sync state exists to avoid fetching all events.
/// On first run or after reset, fetches all events to build initial state.
pub async fn fetch_tokens() -> Result<(), String> {
    use nostr_sdk::signer::NostrSigner;
    use std::collections::HashSet;

    let pubkey_str = auth_store::get_pubkey()
        .ok_or("Not authenticated")?;

    let client = nostr_client::NOSTR_CLIENT.read().as_ref()
        .ok_or("Client not initialized")?.clone();

    let pubkey = PublicKey::parse(&pubkey_str)
        .map_err(|e| format!("Invalid pubkey: {}", e))?;

    // Load existing sync state - prefer IndexedDB, fallback to memory
    let sync_state = if let Some(ref localstore) = *SHARED_LOCALSTORE.read() {
        match localstore.load_sync_state().await {
            Ok(Some(state)) => {
                // Also update memory signal
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

    // Build deletion filter with since if incremental
    let deletion_filter = if is_incremental {
        Filter::new()
            .author(pubkey.clone())
            .kind(Kind::from(5))
            .since(Timestamp::from(last_sync_ts))
    } else {
        Filter::new()
            .author(pubkey.clone())
            .kind(Kind::from(5))
    };

    let mut deleted_event_ids = HashSet::new();

    // Note: If we had previous sync state, deleted events from prior syncs are handled
    // through the del field in token events (deleted_via_del_field) or kind-5 deletions

    if let Ok(deletion_events) = client.fetch_events(deletion_filter, Duration::from_secs(10)).await {
        for del_event in deletion_events {
            for tag in del_event.tags.iter() {
                if let Some(nostr::TagStandard::Event { event_id, .. }) = tag.as_standardized() {
                    deleted_event_ids.insert(event_id.to_hex());
                }
            }
        }
        if !deleted_event_ids.is_empty() {
            log::info!("Found {} deleted token events via kind-5", deleted_event_ids.len());
        }
    }

    // Build token filter with since if incremental
    let filter = if is_incremental {
        Filter::new()
            .author(pubkey)
            .kind(Kind::from(7375))
            .since(Timestamp::from(last_sync_ts))
    } else {
        Filter::new()
            .author(pubkey)
            .kind(Kind::from(7375))
    };

    match client.fetch_events(filter, Duration::from_secs(10)).await {
        Ok(events) => {
            let signer = crate::stores::signer::get_signer()
                .ok_or("No signer available")?
                .as_nostr_signer();

            let events: Vec<_> = events.into_iter().collect();

            // First pass: collect deleted event IDs from del fields
            let mut deleted_via_del_field = HashSet::new();

            for event in &events {
                if deleted_event_ids.contains(&event.id.to_hex()) {
                    continue;
                }

                if let Ok(decrypted) = signer.nip44_decrypt(&event.pubkey, &event.content).await {
                    if let Ok(token_event) = serde_json::from_str::<TokenEventData>(&decrypted) {
                        for del_event_id in &token_event.del {
                            deleted_via_del_field.insert(del_event_id.clone());
                        }
                    }
                }
            }

            if !deleted_via_del_field.is_empty() {
                log::info!("Found {} deleted token events via del field", deleted_via_del_field.len());
            }

            let all_deleted_events: HashSet<String> = deleted_event_ids
                .union(&deleted_via_del_field)
                .cloned()
                .collect();

            // Second pass: collect tokens
            let mut tokens = Vec::new();
            let mut total_balance = 0u64;

            for event in &events {
                let event_id_hex = event.id.to_hex();

                if all_deleted_events.contains(&event_id_hex) {
                    log::debug!("Skipping deleted token event: {}", event_id_hex);
                    continue;
                }

                match signer.nip44_decrypt(&event.pubkey, &event.content).await {
                    Ok(decrypted) => {
                        match serde_json::from_str::<TokenEventData>(&decrypted) {
                            Ok(token_event) => {
                                let proofs: Vec<ProofData> = token_event.proofs.iter()
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
                                    })
                                    .collect();

                                if !proofs.is_empty() {
                                    let token_balance: u64 = proofs.iter()
                                        .map(|p| p.amount)
                                        .try_fold(0u64, |acc, amount| acc.checked_add(amount))
                                        .ok_or_else(|| format!(
                                            "Proof amount overflow in token event {}",
                                            event_id_hex
                                        ))?;

                                    total_balance = total_balance.checked_add(token_balance)
                                        .ok_or_else(|| format!(
                                            "Balance overflow when adding token event {}",
                                            event_id_hex
                                        ))?;

                                    tokens.push(TokenData {
                                        event_id: event_id_hex,
                                        mint: token_event.mint.clone(),
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
                    Err(e) => {
                        log::error!("Failed to decrypt token event {}: {}", event.id, e);
                    }
                }
            }

            // For incremental sync, merge with existing tokens
            if is_incremental {
                let new_token_count = tokens.len();
                let existing_tokens = WALLET_TOKENS.read().data().read().clone();
                let existing_event_ids: HashSet<String> = existing_tokens.iter()
                    .map(|t| t.event_id.clone())
                    .collect();

                // Remove deleted events from existing tokens
                let mut merged_tokens: Vec<TokenData> = existing_tokens.into_iter()
                    .filter(|t| !all_deleted_events.contains(&t.event_id))
                    .collect();

                // Add new tokens that we don't already have
                for token in tokens {
                    if !existing_event_ids.contains(&token.event_id) {
                        merged_tokens.push(token);
                    }
                }

                // Recalculate total balance
                total_balance = merged_tokens.iter()
                    .flat_map(|t| &t.proofs)
                    .map(|p| p.amount)
                    .try_fold(0u64, |acc, amount| acc.checked_add(amount))
                    .ok_or("Balance calculation overflow in merge")?;

                log::info!("Incremental sync: {} total tokens, {} sats (fetched {} new events)",
                    merged_tokens.len(), total_balance, new_token_count);

                *WALLET_TOKENS.read().data().write() = merged_tokens;
            } else {
                log::info!("Full sync: {} token events with {} sats", tokens.len(), total_balance);
                *WALLET_TOKENS.read().data().write() = tokens;
            }

            *WALLET_BALANCE.write() = total_balance;

            // Update sync state with new timestamp
            let new_sync_ts = Timestamp::now().as_secs();
            let known_ids: HashSet<String> = WALLET_TOKENS.read().data().read()
                .iter()
                .map(|t| t.event_id.clone())
                .collect();

            let new_sync_state = SyncState {
                last_token_sync: new_sync_ts,
                last_history_sync: sync_state.as_ref().map(|s| s.last_history_sync).unwrap_or(0),
                last_deletion_sync: sync_state.as_ref().map(|s| s.last_deletion_sync).unwrap_or(0),
                known_token_event_ids: known_ids,
            };

            // Update memory signal
            *SYNC_STATE.write() = Some(new_sync_state.clone());

            // Persist to IndexedDB for cross-session persistence
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

// =============================================================================
// History Events (Kind 7376)
// =============================================================================

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

    let signer = crate::stores::signer::get_signer()
        .ok_or("No signer available")?
        .as_nostr_signer();

    let pubkey_str = auth_store::get_pubkey()
        .ok_or("Not authenticated")?;
    let pubkey = PublicKey::parse(&pubkey_str)
        .map_err(|e| format!("Invalid pubkey: {}", e))?;

    let direction_enum = match direction {
        "in" => TransactionDirection::In,
        "out" => TransactionDirection::Out,
        _ => return Err("Invalid direction".to_string()),
    };

    let mut spending_history = SpendingHistory::new(direction_enum, amount);

    // Add created event IDs
    for token_id in created_tokens {
        if let Ok(event_id) = EventId::from_hex(&token_id) {
            spending_history = spending_history.add_created(event_id);
        } else {
            log::warn!("Skipping invalid created token ID: {}", token_id);
        }
    }

    // Add destroyed event IDs
    for token_id in destroyed_tokens {
        if let Ok(event_id) = EventId::from_hex(&token_id) {
            spending_history = spending_history.add_destroyed(event_id);
        } else {
            log::warn!("Skipping invalid destroyed token ID: {}", token_id);
        }
    }

    // Add redeemed event IDs
    for token_id in redeemed_tokens {
        if let Ok(event_id) = EventId::from_hex(&token_id) {
            spending_history = spending_history.add_redeemed(event_id);
        } else {
            log::warn!("Skipping invalid redeemed token ID: {}", token_id);
        }
    }

    // Build encrypted content
    let mut content_data: Vec<Vec<String>> = vec![
        vec!["direction".to_string(), spending_history.direction.to_string()],
        vec!["amount".to_string(), spending_history.amount.to_string()],
        vec!["unit".to_string(), "sat".to_string()],
    ];

    for event_id in &spending_history.created {
        content_data.push(vec![
            "e".to_string(),
            event_id.to_hex(),
            String::new(),
            "created".to_string()
        ]);
    }

    for event_id in &spending_history.destroyed {
        content_data.push(vec![
            "e".to_string(),
            event_id.to_hex(),
            String::new(),
            "destroyed".to_string()
        ]);
    }

    let json_content = serde_json::to_string(&content_data)
        .map_err(|e| format!("Failed to serialize history event: {}", e))?;

    let encrypted = signer.nip44_encrypt(&pubkey, &json_content).await
        .map_err(|e| format!("Failed to encrypt history event: {}", e))?;

    // Build unencrypted tags for redeemed events
    let mut tags = Vec::new();
    for event_id in &spending_history.redeemed {
        tags.push(nostr_sdk::Tag::parse([
            "e".to_string(),
            event_id.to_hex(),
            String::new(),
            "redeemed".to_string()
        ]).map_err(|e| format!("Failed to create redeemed tag: {}", e))?);
    }

    let builder = nostr_sdk::EventBuilder::new(
        Kind::CashuWalletSpendingHistory,
        encrypted
    ).tags(tags);

    let client = nostr_client::NOSTR_CLIENT.read().as_ref()
        .ok_or("Client not initialized")?.clone();

    client.send_event_builder(builder).await
        .map_err(|e| format!("Failed to publish history event: {}", e))?;

    Ok(())
}

// =============================================================================
// Pending Events Processing
// =============================================================================

/// Publish a single pending event
async fn publish_pending_event(event: &PendingNostrEvent) -> Result<(), String> {
    let client = nostr_client::NOSTR_CLIENT.read().as_ref()
        .ok_or("Nostr client not initialized")?
        .clone();

    let evt: nostr_sdk::Event = serde_json::from_str(&event.builder_json)
        .map_err(|e| format!("Failed to deserialize event: {}", e))?;

    client.send_event(&evt).await
        .map_err(|e| format!("Failed to publish event: {}", e))?;

    Ok(())
}

/// Process pending events with exponential backoff retry logic
pub async fn process_pending_events() -> Result<usize, String> {
    const MAX_RETRIES: u32 = 5;
    const BASE_RETRY_DELAY_SECS: u64 = 60;

    let pending_events = PENDING_NOSTR_EVENTS.read().clone();
    let mut processed_count = 0;

    log::info!("Processing {} pending events", pending_events.len());

    for event in pending_events {
        if event.retry_count >= MAX_RETRIES {
            log::warn!("Event {} exceeded max retries, removing", event.id);
            let _ = remove_pending_event(&event.id).await;
            continue;
        }

        let now = chrono::Utc::now().timestamp() as u64;
        let elapsed = now.saturating_sub(event.created_at);
        let retry_delay = BASE_RETRY_DELAY_SECS * (2_u64.pow(event.retry_count));

        if elapsed < retry_delay {
            log::debug!("Event {} not ready for retry yet", event.id);
            continue;
        }

        match publish_pending_event(&event).await {
            Ok(_) => {
                log::info!("Published pending event: {}", event.id);
                let _ = remove_pending_event(&event.id).await;
                processed_count += 1;
            }
            Err(e) => {
                log::warn!("Failed to publish event {}: {}", event.id, e);

                let mut updated_event = event.clone();
                updated_event.retry_count += 1;

                // Update in memory
                let mut events = PENDING_NOSTR_EVENTS.write();
                if let Some(pos) = events.iter().position(|e| e.id == event.id) {
                    events[pos] = updated_event.clone();
                }
                drop(events);

                // Update in IndexedDB
                if let Some(ref localstore) = *SHARED_LOCALSTORE.read() {
                    let _ = localstore.update_pending_event(&updated_event).await;
                }
            }
        }
    }

    if processed_count > 0 {
        log::info!("Processed {} pending events", processed_count);
    }

    Ok(processed_count)
}

/// Start background task to process pending events periodically
pub fn start_pending_events_processor() {
    use dioxus::prelude::spawn;

    spawn(async {
        loop {
            #[cfg(target_arch = "wasm32")]
            {
                use gloo_timers::future::TimeoutFuture;
                TimeoutFuture::new(5 * 60 * 1000).await;
            }

            if let Err(e) = process_pending_events().await {
                log::error!("Error processing pending events: {}", e);
            }
        }
    });

    log::info!("Started pending events background processor (5 minute interval)");
}
