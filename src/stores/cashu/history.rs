//! Transaction history
//!
//! Functions for fetching and creating transaction history events (NIP-60 kind 7376).
//! Supports incremental sync to avoid re-fetching all events.

use std::collections::HashSet;
use std::time::Duration;

use dioxus::prelude::*;
use nostr_sdk::nips::nip60::TransactionDirection;
use nostr_sdk::{Filter, Kind, PublicKey, Timestamp};

use super::signals::{SHARED_LOCALSTORE, SYNC_STATE, WALLET_HISTORY};
use super::types::{HistoryItem, SyncState, WalletHistoryStoreStoreExt};
use crate::stores::{auth_store, nostr_client};

// =============================================================================
// Public API
// =============================================================================

/// Fetch transaction history from relays (NIP-60 kind 7376) with incremental sync
///
/// Uses the sync state to fetch only new events since the last sync.
/// On first run or after reset, fetches all events.
pub async fn fetch_history() -> Result<(), String> {
    use nostr_sdk::signer::NostrSigner;

    let pubkey_str = auth_store::get_pubkey().ok_or("Not authenticated")?;

    let client = nostr_client::NOSTR_CLIENT
        .read()
        .as_ref()
        .ok_or("Client not initialized")?
        .clone();

    let pubkey = PublicKey::parse(&pubkey_str).map_err(|e| format!("Invalid pubkey: {}", e))?;

    // Load sync state - prefer IndexedDB, fallback to memory
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

    let last_sync_ts = sync_state.as_ref().map(|s| s.last_history_sync).unwrap_or(0);

    // Force full sync if WALLET_HISTORY is empty (e.g., after page refresh)
    // Even if we have a sync timestamp, we need all events if local state is empty
    let history_is_empty = WALLET_HISTORY.read().data().read().is_empty();
    let is_incremental = last_sync_ts > 0 && !history_is_empty;

    if is_incremental {
        log::info!("Fetching transaction history (incremental since {})", last_sync_ts);
    } else {
        log::info!("Fetching transaction history (full sync)");
    }

    // Ensure relays are ready before fetching
    nostr_client::ensure_relays_ready(&client).await;

    // Build filter with since if incremental
    let filter = if is_incremental {
        Filter::new()
            .author(pubkey)
            .kind(Kind::from(7376))
            .since(Timestamp::from(last_sync_ts))
    } else {
        Filter::new().author(pubkey).kind(Kind::from(7376))
    };

    match client.fetch_events(filter, Duration::from_secs(10)).await {
        Ok(events) => {
            let signer = crate::stores::signer::get_signer()
                .ok_or("No signer available")?
                .as_nostr_signer();

            let mut history = Vec::new();

            for event in events {
                // Decrypt history event using signer
                match signer.nip44_decrypt(&event.pubkey, &event.content).await {
                    Ok(decrypted) => {
                        // Parse JSON array: [["direction", "in"], ["amount", "100"], ["e", "id", "", "created"], ...]
                        match serde_json::from_str::<Vec<Vec<String>>>(&decrypted) {
                            Ok(pairs) => {
                                let mut direction = TransactionDirection::In;
                                let mut amount: Option<u64> = None;
                                let mut created_tokens = Vec::new();
                                let mut destroyed_tokens = Vec::new();

                                for pair in pairs {
                                    if pair.is_empty() {
                                        continue;
                                    }
                                    match pair[0].as_str() {
                                        "direction" => {
                                            if pair.len() > 1 {
                                                direction = if pair[1] == "in" {
                                                    TransactionDirection::In
                                                } else {
                                                    TransactionDirection::Out
                                                };
                                            }
                                        }
                                        "amount" => {
                                            if pair.len() > 1 {
                                                match pair[1].parse::<u64>() {
                                                    Ok(parsed_amount) => {
                                                        amount = Some(parsed_amount);
                                                    }
                                                    Err(e) => {
                                                        log::error!(
                                                            "Failed to parse amount in history event {}: '{}' - {}",
                                                            event.id.to_hex(),
                                                            pair[1],
                                                            e
                                                        );
                                                    }
                                                }
                                            }
                                        }
                                        "e" => {
                                            // Event reference: ["e", "event_id", "", "marker"]
                                            if pair.len() > 3 {
                                                match pair[3].as_str() {
                                                    "created" => created_tokens.push(pair[1].clone()),
                                                    "destroyed" => destroyed_tokens.push(pair[1].clone()),
                                                    _ => {}
                                                }
                                            }
                                        }
                                        _ => {}
                                    }
                                }

                                // Extract redeemed events from unencrypted tags
                                let redeemed_events: Vec<String> = event
                                    .tags
                                    .iter()
                                    .filter_map(|tag| {
                                        let vec = tag.clone().to_vec();
                                        if vec.len() > 3 && vec[0] == "e" && vec[3] == "redeemed" {
                                            Some(vec[1].clone())
                                        } else {
                                            None
                                        }
                                    })
                                    .collect();

                                // Only add to history if amount was successfully parsed
                                if let Some(parsed_amount) = amount {
                                    history.push(HistoryItem {
                                        event_id: event.id.to_hex(),
                                        direction,
                                        amount: parsed_amount,
                                        unit: "sat".to_string(),
                                        created_at: event.created_at.as_secs(),
                                        created_tokens,
                                        destroyed_tokens,
                                        redeemed_events,
                                    });
                                } else {
                                    log::warn!(
                                        "Skipping history event {} due to missing or invalid amount",
                                        event.id.to_hex()
                                    );
                                }
                            }
                            Err(e) => {
                                log::error!("Failed to parse history event {}: {}", event.id, e);
                            }
                        }
                    }
                    Err(e) => {
                        log::error!("Failed to decrypt history event {}: {}", event.id, e);
                    }
                }
            }

            // For incremental sync, merge with existing history
            if is_incremental {
                let new_history_count = history.len();
                let existing_history = WALLET_HISTORY.read().data().read().clone();
                let existing_event_ids: HashSet<String> = existing_history
                    .iter()
                    .map(|h| h.event_id.clone())
                    .collect();

                // Start with existing history and add new items that don't exist
                let mut merged_history = existing_history;
                for item in history {
                    if !existing_event_ids.contains(&item.event_id) {
                        merged_history.push(item);
                    }
                }

                // Sort by created_at descending (newest first)
                merged_history.sort_by(|a, b| b.created_at.cmp(&a.created_at));

                log::info!(
                    "Incremental history sync: {} total items (fetched {} new events)",
                    merged_history.len(),
                    new_history_count
                );

                *WALLET_HISTORY.read().data().write() = merged_history;
            } else {
                // Sort by created_at descending (newest first)
                history.sort_by(|a, b| b.created_at.cmp(&a.created_at));

                log::info!("Full history sync: {} items", history.len());
                *WALLET_HISTORY.read().data().write() = history;
            }

            // Update sync state with new timestamp
            let new_sync_ts = Timestamp::now().as_secs();
            let new_sync_state = SyncState {
                last_token_sync: sync_state.as_ref().map(|s| s.last_token_sync).unwrap_or(0),
                last_history_sync: new_sync_ts,
                last_deletion_sync: sync_state.as_ref().map(|s| s.last_deletion_sync).unwrap_or(0),
                known_token_event_ids: sync_state
                    .as_ref()
                    .map(|s| s.known_token_event_ids.clone())
                    .unwrap_or_default(),
            };

            // Update memory signal
            *SYNC_STATE.write() = Some(new_sync_state.clone());

            // Persist to IndexedDB for cross-session persistence
            if let Some(ref localstore) = *SHARED_LOCALSTORE.read() {
                if let Err(e) = localstore.save_sync_state(&new_sync_state).await {
                    log::warn!("Failed to persist sync state to IndexedDB: {}", e);
                }
            }

            Ok(())
        }
        Err(e) => {
            log::error!("Failed to fetch history: {}", e);
            Err(format!("Failed to fetch history: {}", e))
        }
    }
}

// Note: create_history_event and create_history_event_full are implemented in events.rs
// They are re-exported from mod.rs
