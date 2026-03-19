use super::*;

use crate::stores::nostr_client::get_client;
use crate::stores::signer::SIGNER_INFO;
use dioxus::prelude::{ReadableExt, Signal, WritableExt};
use instant::{Duration, Instant};
use nostr_sdk::{
    Event, EventId, Filter, Kind, RelayPoolNotification, SubscriptionId, TagStandard, Timestamp,
};
use std::collections::{HashMap, HashSet};

/// Subscription handle for cleanup
#[derive(Clone, Debug)]
pub struct InteractionStreamHandle {
    pub subscription_id: SubscriptionId,
    /// Task handle for cancellation (Dioxus pattern)
    task: Option<dioxus::dioxus_core::Task>,
}

impl Drop for InteractionStreamHandle {
    fn drop(&mut self) {
        if let Some(task) = self.task.as_ref() {
            task.cancel();
        }
    }
}

impl InteractionStreamHandle {
    /// Cancel the background notification handler and unsubscribe
    /// nostr-sdk pattern: graceful shutdown via signal, then cleanup
    pub async fn unsubscribe(mut self) {
        if let Some(task) = self.task.take() {
            task.cancel();
            log::debug!(
                "Cancelled interaction stream task for {:?}",
                self.subscription_id
            );
        }
        if let Some(client) = crate::stores::nostr_client::get_client() {
            crate::stores::subscription_manager::unsubscribe(&client, &self.subscription_id).await;
        }
    }
}

/// Increment cached counts from streaming update
///
/// Updates the L2 cache with a new interaction event.
/// Returns the updated counts if the event is in cache, None otherwise.
///
/// # Arguments
/// * `event_id` - The event ID being interacted with (hex string)
/// * `kind` - The interaction kind (TextNote, Reaction, Repost, ZapReceipt)
/// * `content` - The event content (used for reactions to detect "-" unlikes)
/// * `is_current_user` - Whether this interaction is from the current user
/// * `zap_amount` - Optional zap amount in satoshis
/// * `repost_event_id` - Optional repost event ID for undo functionality
pub fn increment_cached_counts(
    event_id: &str,
    kind: Kind,
    content: Option<&str>,
    is_current_user: bool,
    zap_amount: Option<u64>,
    repost_event_id: Option<&str>,
) -> Option<InteractionCounts> {
    let mut cache = get_counts_cache().lock().unwrap_or_else(|poisoned| {
        log::warn!("Counts cache mutex was poisoned, recovering");
        poisoned.into_inner()
    });
    let cache_ttl = cache.ttl;
    if let Some(cached) = cache.cache.get_mut(event_id) {
        if cached.cached_at.elapsed() > cache_ttl {
            cache.cache.pop(event_id);
            return None;
        }
        cached.cached_at = Instant::now();
        match kind {
            kind if is_reply_kind(kind) => cached.counts.replies += 1,
            Kind::Reaction => {
                let content = content.unwrap_or("+");
                if content != "-" {
                    cached.counts.likes += 1;
                }
                if is_current_user {
                    if content == "-" {
                        cached.counts.user_liked = Some(false);
                        cached.counts.user_reaction = None;
                        cached.counts.user_reaction_url = None;
                    } else {
                        cached.counts.user_liked = Some(true);
                        cached.counts.user_reaction = Some(content.to_string());
                    }
                }
            }
            Kind::Repost => {
                cached.counts.reposts += 1;
                if is_current_user {
                    cached.counts.user_reposted = Some(true);
                    if let Some(id) = repost_event_id {
                        cached.counts.user_repost_id = Some(id.to_string());
                    }
                }
            }
            Kind::ZapReceipt => {
                cached.counts.zaps += 1;
                if let Some(amount) = zap_amount {
                    cached.counts.zap_amount_sats += amount;
                }
                if is_current_user {
                    cached.counts.user_zapped = Some(true);
                }
            }
            _ => {}
        }
        Some(cached.counts.clone())
    } else {
        None
    }
}

/// Extract the event ID being interacted with from an interaction event
///
/// Only returns an event ID if it's in the set of tracked IDs.
fn extract_referenced_event_for_streaming(
    event: &Event,
    tracked_ids: &HashSet<String>,
) -> Option<String> {
    for tag in event.tags.iter() {
        if let Some(TagStandard::Event { event_id, .. }) = tag.as_standardized() {
            let id_hex = event_id.to_hex();
            if tracked_ids.contains(&id_hex) {
                return Some(id_hex);
            }
        }
    }
    None
}

/// Start streaming interactions for a set of events
///
/// Opens a subscription for interaction events (replies, reactions, reposts, zaps)
/// that reference the given event IDs. Uses the handle_notifications() pattern
/// from nostr-sdk for event processing.
///
/// # Arguments
/// * `event_ids` - Vector of event IDs to track interactions for
/// * `interaction_counts` - Signal to update with new counts (Dioxus reactive state)
/// * `post_eose_timeout_secs` - Optional timeout in seconds after EOSE before closing subscription (default: 600)
///
/// # Returns
/// * `Ok(InteractionStreamHandle)` - Handle containing subscription ID for cleanup
/// * `Err(String)` - Error message if subscription fails
///
/// # Deduplication
/// - nostr-sdk automatically deduplicates events (RelayPoolNotification::Event only fires once per event)
/// - Uses `since: Timestamp::now()` to only receive new events
/// - Only updates existing cache entries (no-op if event not in cache)
///
/// # Important: Event Gap Risk
/// Because the filter uses `since: Timestamp::now()`, events that occur between the last fetch
/// and when the subscription starts will be missed. Callers should start streaming before or
/// concurrently with their first fetch to avoid gaps. The nostr-sdk deduplication ensures
/// overlapping fetches don't cause double-counting.
pub async fn stream_interaction_counts(
    event_ids: Vec<EventId>,
    interaction_counts: Signal<HashMap<String, InteractionCounts>>,
    post_eose_timeout_secs: Option<u64>,
) -> Result<InteractionStreamHandle, String> {
    use nostr_relay_pool::relay::ReqExitPolicy;
    use nostr_relay_pool::{RelayStatus as PoolRelayStatus, SubscribeAutoCloseOptions};
    if event_ids.is_empty() {
        return Err("No event IDs to stream".to_string());
    }
    let client = get_client().ok_or("Client not initialized")?;
    let tracked_ids: HashSet<String> = event_ids.iter().map(|id| id.to_hex()).collect();
    let filter = Filter::new()
        .kinds(vec![
            Kind::TextNote,
            Kind::Comment,
            Kind::Reaction,
            Kind::Repost,
            Kind::ZapReceipt,
        ])
        .events(event_ids)
        .since(Timestamp::now());
    let mut attempts = 0;
    const MAX_ATTEMPTS: u32 = 5;
    let connected_urls = loop {
        let relays = client.relays().await;
        let urls: Vec<nostr_sdk::RelayUrl> = relays
            .iter()
            .filter(|(_, r)| r.status() == PoolRelayStatus::Connected)
            .filter_map(|(url, _)| nostr_sdk::RelayUrl::parse(url.as_str()).ok())
            .collect();
        if !urls.is_empty() || attempts >= MAX_ATTEMPTS {
            break urls;
        }
        attempts += 1;
        log::debug!(
            "Waiting for relay connections (attempt {}/{})",
            attempts,
            MAX_ATTEMPTS
        );
        crate::platform::timer::sleep_ms(500).await;
    };
    if connected_urls.is_empty() {
        return Err("No connected relays for interaction streaming after retries".to_string());
    }
    log::info!(
        "Fast interaction streaming: subscribing to {} connected relays (bypassing gossip)",
        connected_urls.len()
    );
    let timeout = Duration::from_secs(post_eose_timeout_secs.unwrap_or(600));
    let auto_close = SubscribeAutoCloseOptions::default()
        .exit_policy(ReqExitPolicy::WaitDurationAfterEOSE(timeout));
    let subscription_id = client
        .subscribe_to(connected_urls, filter, Some(auto_close))
        .await
        .map(|output| output.val)
        .map_err(|e| format!("Failed to subscribe: {}", e))?;
    log::info!(
        "Started interaction stream subscription {:?} for {} events",
        subscription_id,
        tracked_ids.len()
    );
    let current_user_pk: Option<nostr_sdk::PublicKey> = SIGNER_INFO
        .read()
        .as_ref()
        .and_then(|info| nostr_sdk::PublicKey::from_hex(&info.public_key).ok());
    let tracked_ids = std::sync::Arc::new(tracked_ids);
    let sub_id = std::sync::Arc::new(subscription_id.clone());
    let task = dioxus::prelude::spawn(async move {
        let client = match get_client() {
            Some(c) => c,
            None => {
                log::error!("Client not available for interaction stream handler");
                return;
            }
        };
        if let Err(e) = client
            .handle_notifications(|notification| {
                let tracked_ids = std::sync::Arc::clone(&tracked_ids);
                let sub_id = std::sync::Arc::clone(&sub_id);
                let mut interaction_counts = interaction_counts;
                async move {
                    if let RelayPoolNotification::Event {
                        subscription_id: event_sub_id,
                        event,
                        ..
                    } = notification
                    {
                        if event_sub_id != *sub_id {
                            return Ok(false);
                        }
                        let referenced_id =
                            match extract_referenced_event_for_streaming(&event, &tracked_ids) {
                                Some(id) => id,
                                None => return Ok(false),
                            };
                        let is_current_user = if event.kind == Kind::ZapReceipt {
                            super::counting::extract_zap_sender(&event)
                                .map(|sender| {
                                    current_user_pk
                                        .map(|pk| sender == pk.to_hex())
                                        .unwrap_or(false)
                                })
                                .unwrap_or(false)
                        } else {
                            current_user_pk
                                .map(|pk| event.pubkey == pk)
                                .unwrap_or(false)
                        };
                        let zap_amount = if event.kind == Kind::ZapReceipt {
                            super::counting::extract_zap_amount(&event)
                        } else {
                            None
                        };
                        let content = if event.kind == Kind::Reaction {
                            Some(event.content.trim())
                        } else {
                            None
                        };
                        let repost_id = if event.kind == Kind::Repost && is_current_user {
                            Some(event.id.to_hex())
                        } else {
                            None
                        };
                        if let Some(updated_counts) = increment_cached_counts(
                            &referenced_id,
                            event.kind,
                            content,
                            is_current_user,
                            zap_amount,
                            repost_id.as_deref(),
                        ) {
                            interaction_counts
                                .write()
                                .insert(referenced_id.clone(), updated_counts);
                            log::debug!(
                                "Streamed interaction update for {}: kind={}",
                                &referenced_id[..8.min(referenced_id.len())],
                                event.kind.as_u16()
                            );
                        }
                    }
                    Ok(false)
                }
            })
            .await
        {
            log::debug!("Interaction stream handler ended: {}", e);
        }
    });
    Ok(InteractionStreamHandle {
        subscription_id,
        task: Some(task),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn increment_cached_counts_treats_comment_as_reply() {
        let event_id = "event-id";
        {
            let mut cache = get_counts_cache().lock().unwrap_or_else(|poisoned| {
                log::warn!("Counts cache mutex was poisoned, recovering");
                poisoned.into_inner()
            });
            cache.insert(event_id.to_string(), InteractionCounts::default());
        }

        let updated = increment_cached_counts(event_id, Kind::Comment, None, false, None, None)
            .expect("counts should be updated");

        assert_eq!(updated.replies, 1);
    }
}
