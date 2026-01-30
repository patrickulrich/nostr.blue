//! NIP-60 Event Handling
//!
//! Functions for creating, publishing, and fetching Nostr events for the Cashu wallet.
//! Handles token events (kind 7375), history events (kind 7376), quote events (kind 7374),
//! and deletion events (kind 5).

// Allow dead_code for planned features not yet wired to UI
#![allow(dead_code)]

use dioxus::prelude::*;
use nostr::nips::nip60::{SpendingHistory, TransactionDirection};
use nostr_sdk::{EventId, Filter, Kind, PublicKey, Timestamp};
use std::time::Duration;

use super::proofs::rebuild_proof_event_map;
use super::signals::{PENDING_NOSTR_EVENTS, SHARED_LOCALSTORE, SYNC_STATE, WALLET_TOKENS};
use super::types::{
    PendingEventType, PendingNostrEvent, ProofData, ProofState, SyncState, TokenData,
    TokenEventData, WalletTokensStoreStoreExt,
};
use super::utils::normalize_mint_url;
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
        last_retry_at: None,
        pending_token_id: None,
        mint_url: None,
    };

    // Save to in-memory queue
    PENDING_NOSTR_EVENTS.write().push(pending.clone());

    // Persist to IndexedDB for offline support
    if let Some(ref localstore) = *SHARED_LOCALSTORE.read() {
        if let Err(e) = localstore.add_pending_event(&pending).await {
            log::warn!("Failed to persist pending event to IndexedDB: {}", e);
        }
    }

    log::debug!(
        "Queued {} event: {}",
        match event_type {
            PendingEventType::TokenEvent => "token",
            PendingEventType::DeletionEvent => "deletion",
            PendingEventType::HistoryEvent => "history",
            PendingEventType::QuoteEvent => "quote",
            PendingEventType::NutzapEvent => "nutzap",
        },
        event_id
    );

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
///
/// Optional `pending_token_id` and `mint_url` link this pending event to a token
/// in WALLET_TOKENS, allowing the token's event_id to be updated when background
/// publish succeeds.
pub async fn queue_signed_event_for_retry(
    event: nostr_sdk::Event,
    event_type: PendingEventType,
    pending_token_id: Option<String>,
    mint_url: Option<String>,
) {
    let event_id = event.id.to_hex();
    let event_json = match serde_json::to_string(&event) {
        Ok(j) => j,
        Err(e) => {
            log::error!("Failed to serialize event for queueing: {}", e);
            return;
        }
    };

    let pending_event = PendingNostrEvent {
        id: uuid::Uuid::new_v4().to_string(),
        builder_json: event_json,
        event_type: event_type.clone(),
        created_at: chrono::Utc::now().timestamp() as u64,
        retry_count: 0,
        last_retry_at: None,
        pending_token_id,
        mint_url,
    };

    // Add to in-memory queue
    PENDING_NOSTR_EVENTS.write().push(pending_event.clone());

    // Persist to IndexedDB
    if let Some(ref localstore) = *SHARED_LOCALSTORE.read() {
        if let Err(e) = localstore.add_pending_event(&pending_event).await {
            log::warn!("Failed to persist pending event: {}", e);
        }
    }

    log::info!(
        "Queued signed event {} for retry: {}",
        event_id,
        pending_event.id
    );
}

/// Sign an EventBuilder using the current signer
/// Returns Ok(Event) or Err(error_message)
pub async fn sign_event_builder(
    builder: nostr_sdk::EventBuilder,
) -> Result<nostr_sdk::Event, String> {
    let signer =
        crate::stores::signer::get_signer().ok_or_else(|| "No signer available".to_string())?;

    match signer {
        crate::stores::signer::SignerType::Keys(keys) => builder
            .sign_with_keys(&keys)
            .map_err(|e| format!("Failed to sign event: {}", e)),
        #[cfg(target_family = "wasm")]
        crate::stores::signer::SignerType::BrowserExtension(browser_signer) => builder
            .sign(&*browser_signer)
            .await
            .map_err(|e| format!("Failed to sign event: {}", e)),
        crate::stores::signer::SignerType::NostrConnect(remote_signer) => builder
            .sign(&*remote_signer)
            .await
            .map_err(|e| format!("Failed to sign event: {}", e)),
    }
}

/// Queue an EventBuilder for retry when initial publication fails
///
/// Optional `pending_token_id` and `mint_url` link this pending event to a token
/// in WALLET_TOKENS, allowing the token's event_id to be updated when background
/// publish succeeds.
pub async fn queue_event_for_retry(
    builder: nostr_sdk::EventBuilder,
    event_type: PendingEventType,
    pending_token_id: Option<String>,
    mint_url: Option<String>,
) {
    let event = match sign_event_builder(builder).await {
        Ok(e) => e,
        Err(e) => {
            log::error!("Failed to sign event for queueing: {}", e);
            return;
        }
    };

    queue_signed_event_for_retry(event, event_type, pending_token_id, mint_url).await;
}

/// Queue a token event for retry with tracking info
///
/// The `pending_token_id` links this pending event to a token in WALLET_TOKENS,
/// allowing us to update the token's event_id when background publish succeeds.
///
/// This is a convenience wrapper around `queue_signed_event_for_retry` that handles
/// signing the builder first. Eliminates duplicate queue+persist logic.
pub async fn queue_token_event_for_retry(
    builder: nostr_sdk::EventBuilder,
    pending_token_id: String,
    mint_url: String,
) {
    // Use shared signing helper
    let event = match sign_event_builder(builder).await {
        Ok(e) => e,
        Err(e) => {
            log::error!("Cannot queue token event: {}", e);
            return;
        }
    };

    // Reuse existing function - single path for queue+persist (CDK adaptive backoff pattern)
    queue_signed_event_for_retry(
        event,
        PendingEventType::TokenEvent,
        Some(pending_token_id.clone()),
        Some(mint_url),
    )
    .await;

    log::info!(
        "Queued token event for retry, pending_id={}",
        pending_token_id
    );
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

    let pubkey_str = auth_store::get_pubkey().ok_or("Not authenticated")?;
    let pubkey = PublicKey::parse(&pubkey_str).map_err(|e| format!("Invalid pubkey: {}", e))?;

    // Encrypt quote ID with NIP-44
    let encrypted = signer
        .nip44_encrypt(&pubkey, quote_id)
        .await
        .map_err(|e| format!("Failed to encrypt quote ID: {}", e))?;

    // Calculate expiration timestamp
    let expiration_ts = Timestamp::now() + (expiration_days * 24 * 60 * 60);

    let builder = nostr_sdk::EventBuilder::new(Kind::CashuWalletQuote, encrypted).tags(vec![
        nostr_sdk::Tag::custom(nostr_sdk::TagKind::custom("mint"), [mint_url]),
        nostr_sdk::Tag::expiration(expiration_ts),
    ]);

    let client = nostr_client::NOSTR_CLIENT
        .read()
        .as_ref()
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
    let client = nostr_client::NOSTR_CLIENT
        .read()
        .as_ref()
        .ok_or("Nostr client not initialized")?
        .clone();

    let mut tags = vec![nostr_sdk::Tag::event(
        nostr_sdk::EventId::from_hex(event_id).map_err(|e| format!("Invalid event ID: {}", e))?,
    )];

    tags.push(nostr_sdk::Tag::custom(
        nostr_sdk::TagKind::custom("k"),
        ["7374"],
    ));

    let deletion_builder = nostr_sdk::EventBuilder::new(Kind::from(5), "Quote expired").tags(tags);

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

    let pubkey_str = auth_store::get_pubkey().ok_or("Not authenticated")?;

    let client = nostr_client::NOSTR_CLIENT
        .read()
        .as_ref()
        .ok_or("Client not initialized")?
        .clone();

    let pubkey = PublicKey::parse(&pubkey_str).map_err(|e| format!("Invalid pubkey: {}", e))?;

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
            .author(pubkey)
            .kind(Kind::from(5))
            .since(Timestamp::from(last_sync_ts))
    } else {
        Filter::new().author(pubkey).kind(Kind::from(5))
    };

    let mut deleted_event_ids = HashSet::new();

    // Note: If we had previous sync state, deleted events from prior syncs are handled
    // through the del field in token events (deleted_via_del_field) or kind-5 deletions

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

    // Build token filter with since if incremental
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
                log::info!(
                    "Found {} deleted token events via del field",
                    deleted_via_del_field.len()
                );
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
                                                event_id_hex
                                            )
                                        })?;

                                    total_balance = total_balance
                                        .checked_add(token_balance)
                                        .ok_or_else(|| {
                                            format!(
                                                "Balance overflow when adding token event {}",
                                                event_id_hex
                                            )
                                        })?;

                                    tokens.push(TokenData {
                                        event_id: event_id_hex,
                                        // Normalize mint URL for consistent balance lookups
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
                    Err(e) => {
                        log::error!("Failed to decrypt token event {}: {}", event.id, e);
                    }
                }
            }

            // For incremental sync, merge with existing tokens
            if is_incremental {
                let new_token_count = tokens.len();
                let existing_tokens = WALLET_TOKENS.read().data().read().clone();
                let existing_event_ids: HashSet<String> =
                    existing_tokens.iter().map(|t| t.event_id.clone()).collect();

                // Remove deleted events from existing tokens
                let mut merged_tokens: Vec<TokenData> = existing_tokens
                    .into_iter()
                    .filter(|t| !all_deleted_events.contains(&t.event_id))
                    .collect();

                // Add new tokens that we don't already have
                for token in tokens {
                    if !existing_event_ids.contains(&token.event_id) {
                        merged_tokens.push(token);
                    }
                }

                // Recalculate total balance
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

                // Note: Token and balance updates are not atomic but are calculated from the same
                // data (total_balance is computed from merged_tokens before either write).
                // Both writes happen synchronously on the same thread. If inconsistency occurs
                // (e.g., crash between writes), recalculate_balance() can restore consistency.
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

            // Update sync state with new timestamp
            let new_sync_ts = Timestamp::now().as_secs();
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

    let pubkey_str = auth_store::get_pubkey().ok_or("Not authenticated")?;
    let pubkey = PublicKey::parse(&pubkey_str).map_err(|e| format!("Invalid pubkey: {}", e))?;

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

    // Build unencrypted tags for redeemed events
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

    let client = nostr_client::NOSTR_CLIENT
        .read()
        .as_ref()
        .ok_or("Client not initialized")?
        .clone();

    client
        .send_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to publish history event: {}", e))?;

    Ok(())
}

// =============================================================================
// Pending Events Processing
// =============================================================================

/// Publish a single pending event and return the Nostr event ID
async fn publish_pending_event(event: &PendingNostrEvent) -> Result<String, String> {
    let client = nostr_client::NOSTR_CLIENT
        .read()
        .as_ref()
        .ok_or("Nostr client not initialized")?
        .clone();

    let evt: nostr_sdk::Event = serde_json::from_str(&event.builder_json)
        .map_err(|e| format!("Failed to deserialize event: {}", e))?;

    // Get the event_id before publishing (deterministic from signature)
    let event_id = evt.id.to_hex();

    let output = client
        .send_event(&evt)
        .await
        .map_err(|e| format!("Failed to publish event: {}", e))?;

    // Check for at least one successful relay
    if output.success.is_empty() {
        // nostr-sdk pattern: duplicates start with "duplicate:" prefix
        let all_duplicates = output
            .failed
            .values()
            .all(|err| err.to_lowercase().starts_with("duplicate:"));

        if all_duplicates && !output.failed.is_empty() {
            log::debug!(
                "Event {} already exists on all relays (duplicate)",
                event_id
            );
            return Ok(event_id);
        }

        return Err(format!("All {} relays failed", output.failed.len()));
    }

    log::debug!(
        "Published to {}/{} relays",
        output.success.len(),
        output.success.len() + output.failed.len()
    );

    Ok(event_id)
}

/// Process pending events with adaptive backoff retry logic
///
/// Uses nostr-sdk pattern: adaptive multiplier with jitter to prevent synchronized retries.
pub async fn process_pending_events() -> Result<usize, String> {
    // Early check for client initialization (pattern from Nostr SDK)
    // Avoids per-event failures and retry count increments when client isn't ready
    if nostr_client::NOSTR_CLIENT.read().as_ref().is_none() {
        log::debug!("Nostr client not yet initialized, skipping pending events processing");
        return Ok(0);
    }

    const MAX_RETRIES: u32 = 5;
    const BASE_RETRY_DELAY_SECS: u64 = 10;
    const MAX_RETRY_DELAY_SECS: u64 = 60;

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
        // Use last_retry_at if available, otherwise created_at (for first attempt)
        let last_attempt = event.last_retry_at.unwrap_or(event.created_at);
        let elapsed = now.saturating_sub(last_attempt);

        // Adaptive backoff: BASE_DELAY * (1 + retry_count/2), capped at MAX_DELAY
        // This gives delays of: 10s, 15s, 20s, 25s, 30s (capped)
        let multiplier = 1 + (event.retry_count / 2);
        let adaptive_delay = BASE_RETRY_DELAY_SECS * (multiplier as u64);
        let retry_delay = adaptive_delay.min(MAX_RETRY_DELAY_SECS);

        if elapsed < retry_delay {
            log::debug!(
                "Event {} not ready for retry yet ({}s < {}s)",
                event.id,
                elapsed,
                retry_delay
            );
            continue;
        }

        match publish_pending_event(&event).await {
            Ok(nostr_event_id) => {
                log::info!(
                    "Published pending event: {} -> nostr:{}",
                    event.id,
                    nostr_event_id
                );

                // Handle TokenEvent: update token's event_id in WALLET_TOKENS
                if event.event_type == PendingEventType::TokenEvent {
                    if let Some(ref pending_id) = event.pending_token_id {
                        update_token_event_id(pending_id, &nostr_event_id);
                    } else if let Some(ref mint_url) = event.mint_url {
                        // Fallback: try to find token by mint_url when pending_token_id is missing
                        // CDK pattern: avoid ambiguous matching - only update if exactly one candidate
                        log::warn!("TokenEvent missing pending_token_id, attempting proof-based fallback for {}", mint_url);
                        let normalized_mint = super::utils::normalize_mint_url(mint_url);

                        // Collect candidates and check for ambiguity
                        let matched_token = {
                            let store = WALLET_TOKENS.read();
                            let data = store.data();
                            let tokens = data.read();

                            // Collect candidates at this mint with pending event_ids
                            let candidates: Vec<_> = tokens
                                .iter()
                                .filter(|t| {
                                    t.event_id.starts_with("pending_")
                                        && super::utils::mint_matches(&t.mint, &normalized_mint)
                                })
                                .collect();

                            if candidates.len() == 1 {
                                // Unambiguous: only one pending token at this mint
                                Some(candidates[0].event_id.clone())
                            } else if candidates.len() > 1 {
                                // Ambiguous: log warning, skip update to avoid wrong token
                                log::warn!(
                                    "Multiple pending tokens ({}) at mint {}, cannot disambiguate without proof matching",
                                    candidates.len(),
                                    mint_url
                                );
                                None
                            } else {
                                None
                            }
                        };

                        if let Some(old_id) = matched_token {
                            log::info!(
                                "Found pending token by mint fallback: {} -> {}",
                                old_id,
                                nostr_event_id
                            );
                            update_token_event_id(&old_id, &nostr_event_id);
                        }
                    }
                }

                let _ = remove_pending_event(&event.id).await;
                processed_count += 1;
            }
            Err(e) => {
                log::warn!("Failed to publish event {}: {}", event.id, e);

                let mut updated_event = event.clone();
                updated_event.retry_count += 1;
                // Record when this retry attempt happened for proper backoff calculation
                updated_event.last_retry_at = Some(now);

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

    // Rebuild proof event map to stay in sync (matches fetch_tokens pattern at line 594)
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

    let client = nostr_client::NOSTR_CLIENT
        .read()
        .as_ref()
        .ok_or("Client not initialized")?
        .clone();

    // Find tokens with pending_* event IDs
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

        // Check if there's already a non-pending token with matching proof secrets
        // This prevents republishing if another process already created the real event
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
                    // Check if all pending proofs exist in a non-pending token
                    !proof_secrets.is_empty() && proof_secrets == t_secrets
                })
                .map(|t| t.event_id.clone())
        };

        if let Some(real_event_id) = existing_real_event {
            // Found existing real event - just update the pending reference
            log::info!(
                "Found existing real event {} for pending token {}",
                real_event_id,
                old_event_id
            );
            update_token_event_id(&old_event_id, &real_event_id);
            reconciled += 1;
            continue;
        }

        // Build and publish token event
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

        match client.send_event_builder(builder).await {
            Ok(output) => {
                // Check if at least one relay succeeded (nostr-sdk auto-saves locally before relay)
                if !output.success.is_empty() {
                    let real_event_id = output.id().to_hex();
                    log::info!(
                        "Reconciled pending token: {} -> {} (to {} relays)",
                        old_event_id,
                        real_event_id,
                        output.success.len()
                    );

                    // Update in WALLET_TOKENS
                    update_token_event_id(&old_event_id, &real_event_id);
                    reconciled += 1;
                } else {
                    // No relays accepted - keep pending ID for future retry
                    log::warn!(
                        "No relays accepted reconciliation event for {}, will retry later",
                        old_event_id
                    );
                }
            }
            Err(e) => {
                log::warn!(
                    "Failed to publish reconciliation event for {}: {}",
                    old_event_id,
                    e
                );
                // Will retry on next reconciliation pass
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

    let client = nostr_client::NOSTR_CLIENT
        .read()
        .as_ref()
        .ok_or("Client not initialized")?
        .clone();

    // 1. Generate pending_id BEFORE any network operation (CDK Saga Pattern)
    let pending_id = format!("pending_{}", uuid::Uuid::new_v4());

    // Convert to extended proofs format
    let extended_proofs: Vec<ExtendedCashuProof> = proofs
        .iter()
        .map(|p| ExtendedCashuProof::from(p.clone()))
        .collect();

    let token_event_data = super::types::ExtendedTokenEvent {
        mint: mint_url.to_string(),
        unit: unit.to_string(),
        proofs: extended_proofs,
        del: vec![], // No deletions for orphan recovery
    };

    let json_content = serde_json::to_string(&token_event_data)
        .map_err(|e| format!("Failed to serialize: {}", e))?;

    let encrypted = signer
        .nip44_encrypt(&pubkey, &json_content)
        .await
        .map_err(|e| format!("Failed to encrypt: {}", e))?;

    let builder = nostr_sdk::EventBuilder::new(Kind::CashuWalletUnspentProof, encrypted);

    // Sign the event
    let unsigned = builder.clone().build(pubkey);
    let signed_event = unsigned
        .sign(&signer)
        .await
        .map_err(|e| format!("Failed to sign: {}", e))?;

    // 2. Insert into WALLET_TOKENS right before publish (saga setup - persist before network)
    // This ensures we don't leave dangling entries if serialize/encrypt/sign fails
    // Normalize mint URL for consistent balance lookups (matches fetch_tokens pattern)
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
        // Continue anyway - better to try publish than fail entirely
    } else {
        // Register proofs in event map for fast lookup
        super::proofs::register_proofs_in_event_map(&pending_id, proofs);
    }

    // 3. Attempt publish
    match client.send_event(&signed_event).await {
        Ok(output) => {
            // Check if at least one relay succeeded
            if !output.success.is_empty() {
                let real_event_id = output.id().to_hex();
                // 4. On success: update pending_id to real event_id
                update_token_event_id(&pending_id, &real_event_id);
                log::info!(
                    "Published orphaned proofs token event: {} (to {} relays)",
                    real_event_id,
                    output.success.len()
                );
                Ok(real_event_id)
            } else {
                // No relays accepted - token already persisted with pending_id
                log::warn!(
                    "No relays accepted orphaned proofs event, queuing for retry: {}",
                    pending_id
                );
                queue_signed_event_for_retry(
                    signed_event,
                    PendingEventType::TokenEvent,
                    Some(pending_id.clone()),
                    Some(mint_url.to_string()),
                )
                .await;
                // Return the pending_id since proofs are now tracked under it
                Ok(pending_id)
            }
        }
        Err(e) => {
            // 5. On failure: proofs remain accessible via pending_id for retry
            log::warn!(
                "Failed to publish orphaned proofs, queuing for retry: {}",
                e
            );
            queue_signed_event_for_retry(
                signed_event,
                PendingEventType::TokenEvent,
                Some(pending_id.clone()),
                Some(mint_url.to_string()),
            )
            .await;
            // Return the pending_id since proofs are now tracked under it
            Ok(pending_id)
        }
    }
}

/// Guard to prevent multiple processor spawns
#[cfg(target_arch = "wasm32")]
static PROCESSOR_RUNNING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

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
    use dioxus::prelude::spawn;
    use gloo_timers::future::TimeoutFuture;
    use std::sync::atomic::Ordering;

    // Atomically check if already running and set to true if not
    // compare_exchange returns Ok(false) if we successfully set it from false to true
    if PROCESSOR_RUNNING
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        log::debug!("Pending events processor already running, skipping spawn");
        return;
    }

    spawn(async {
        // Counter for periodic recovery (every 6th iteration when idle = ~6 min)
        let mut recovery_counter = 0u32;

        loop {
            // Adaptive interval: 30s when pending events exist, 60s otherwise
            let pending_count = PENDING_NOSTR_EVENTS.read().len();
            let interval_ms = if pending_count > 0 {
                30_000 // 30 seconds when there's work
            } else {
                60_000 // 60 seconds when idle (faster pickup of newly queued events)
            };

            TimeoutFuture::new(interval_ms).await;

            // Process pending Nostr events
            if let Err(e) = process_pending_events().await {
                log::error!("Error processing pending events: {}", e);
            }

            // Re-read count AFTER sleep and processing for maintenance decision
            // (original pending_count is stale after 30-60s sleep)
            let fresh_pending_count = PENDING_NOSTR_EVENTS.read().len();

            // Every 6th iteration when idle (roughly every 6 min), run maintenance tasks
            recovery_counter += 1;
            if recovery_counter >= 6 && fresh_pending_count == 0 {
                recovery_counter = 0;

                // Clean expired pending secrets (proofs pending at mint for >1 hour)
                super::signals::cleanup_expired_pending_secrets().await;

                // Reconcile any stuck pending event IDs (tokens with pending_* that never got real IDs)
                match reconcile_pending_event_ids().await {
                    Ok(count) if count > 0 => {
                        log::info!("Reconciled {} pending event IDs", count);
                    }
                    Err(e) => {
                        log::debug!("Pending event ID reconciliation error: {}", e);
                    }
                    _ => {}
                }

                // Run proof recovery in a separate task to avoid blocking the event loop
                // Uses futures::select! for WASM-compatible timeout
                spawn(async {
                    use futures::FutureExt;

                    let recovery_future = super::proof_recovery::run_full_recovery().fuse();
                    let timeout_future = TimeoutFuture::new(180_000).fuse(); // 3 min timeout

                    futures::pin_mut!(recovery_future, timeout_future);

                    futures::select! {
                        result = recovery_future => {
                            if result.recovered_count > 0 || result.spent_count > 0 {
                                log::info!(
                                    "Periodic recovery: {} recovered, {} spent",
                                    result.recovered_count,
                                    result.spent_count
                                );
                            }
                            if !result.errors.is_empty() {
                                for err in result.errors {
                                    log::debug!("Periodic recovery error: {}", err);
                                }
                            }
                        }
                        _ = timeout_future => {
                            log::warn!("Periodic recovery timed out after 3 minutes");
                        }
                    }
                });
            }
        }
    });

    log::info!(
        "Started pending events background processor (adaptive interval + periodic recovery)"
    );
}

/// No-op on non-WASM targets (gloo_timers is WASM-only)
#[cfg(not(target_arch = "wasm32"))]
pub fn start_pending_events_processor() {
    log::debug!("Pending events processor only runs in WASM");
}
