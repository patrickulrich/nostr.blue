use super::*;

use crate::stores::nostr_client::get_client;
use crate::stores::signer::SIGNER_INFO;
use crate::utils::format::safe_slice;
use dioxus::prelude::{ReadableExt, Signal, WritableExt};
use futures::StreamExt;
use instant::{Duration, Instant};
use nostr::prelude::SingleLetterTag;
use nostr::prelude::{
    Event, EventId, Filter, Kind, SubscriptionId, Timestamp,
};
use nostr_sdk::client::ClientNotification;
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
/// * `kind` - The interaction kind (TextNote, Reaction, Repost, GenericRepost, ZapReceipt)
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
    is_quote: bool,
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
        if is_quote {
            // NIP-18 q-tag quote: a reshare, never a reply (Amethyst boost
            // parity). No user-state: undoing a quote means deleting the
            // note.
            cached.counts.reposts += 1;
            return Some(cached.counts.clone());
        }
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
            Kind::Repost | Kind::GenericRepost => {
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

/// Extract the event IDs being referenced by an interaction event from the
/// set of tracked IDs — marker-aware, multi-id, including NIP-18 `q`-tag
/// quotes (flagged `quote: true`, counted as reshares). See
/// `attribute_referenced_events` for the attribution semantics.
fn extract_referenced_events_for_streaming(
    event: &Event,
    tracked_ids: &HashSet<String>,
) -> Vec<(String, bool)> {
    super::counting::attribute_referenced_events(event, tracked_ids)
        .into_iter()
        .map(|r| (r.id.to_hex(), r.quote))
        .collect()
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
/// - nostr-sdk automatically deduplicates events (ClientNotification::Event only fires once per event)
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
    mut interaction_counts: Signal<HashMap<String, InteractionCounts>>,
    post_eose_timeout_secs: Option<u64>,
) -> Result<InteractionStreamHandle, String> {
    use nostr_sdk::relay::{RelayStatus as PoolRelayStatus, ReqExitPolicy, SubscribeAutoCloseOptions};
    if event_ids.is_empty() {
        return Err("No event IDs to stream".to_string());
    }
    let client = get_client().ok_or("Client not initialized")?;
    let tracked_ids: HashSet<String> = event_ids.iter().map(|id| id.to_hex()).collect();
    let since = Timestamp::now();
    let anchor_hexes: Vec<String> = event_ids.iter().map(|id| id.to_hex()).collect();
    let interaction_filter = Filter::new()
        .kinds(vec![
            Kind::TextNote,
            Kind::Comment,
            Kind::Reaction,
            Kind::Repost,
            Kind::GenericRepost,
            Kind::ZapReceipt,
            Kind::Custom(1010),
        ])
        .events(event_ids)
        .since(since);
    // NIP-18 quote arm (Amethyst parity): `q`-tag quotes are reshares the
    // `#e` dimensions can never see.
    let quote_filter = Filter::new()
        .kinds(vec![Kind::TextNote, Kind::Comment])
        .custom_tags(
            SingleLetterTag::from_char('q').expect("valid single-letter tag"),
            anchor_hexes,
        )
        .since(since);
    let mut attempts = 0;
    const MAX_ATTEMPTS: u32 = 5;
    let connected_urls = loop {
        let relays = client.relays().all().await;
        let urls: Vec<nostr::types::url::RelayUrl> = relays
            .iter()
            .filter(|(_, r)| r.status() == PoolRelayStatus::Connected)
            .filter_map(|(url, _)| nostr::types::url::RelayUrl::parse(url.as_str()).ok())
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
    let targets: Vec<_> = connected_urls
        .into_iter()
        .map(|u| (u, vec![interaction_filter.clone(), quote_filter.clone()]))
        .collect();
    // Pool-membership fallback: a relay disconnecting between the
    // connected-snapshot above and the subscribe would fail the WHOLE
    // targeted REQ (`RelayNotFound`), dropping the stream to cache-only
    // ("Failed to start interaction stream"). subscribe_target intersects
    // with current membership and retries, degrading to pool-auto only if
    // nothing survives.
    let subscription_id = crate::stores::subscription_manager::subscribe_target(
        &client,
        crate::stores::subscription_manager::SubTarget::Targeted(targets),
        Some(auto_close),
    )
    .await
    .map(|output| output.value)
    .map_err(|e| format!("Failed to subscribe: {}", e))?;
    log::info!(
        "Started interaction stream subscription {:?} for {} events",
        subscription_id,
        tracked_ids.len()
    );
    let current_user_pk: Option<nostr::key::PublicKey> = SIGNER_INFO
        .read()
        .as_ref()
        .and_then(|info| nostr::key::PublicKey::from_hex(&info.public_key).ok());
    let tracked_ids = std::sync::Arc::new(tracked_ids);
    let sub_id = std::sync::Arc::new(subscription_id.clone());
    let task = crate::platform::spawn::spawn_catch_unwind("interaction_stream", async move {
        let client = match get_client() {
            Some(c) => c,
            None => {
                log::error!("Client not available for interaction stream handler");
                return;
            }
        };
        // Manual notification loop instead of `client.handle_notifications(...)`.
        // The SDK's `handle_notifications` uses `while let Ok(...)` which silently
        // dies on broadcast `Lagged` (a transient slow-consumer error). A manual
        // loop keeps the interaction stream alive; in 0.45 the NotificationStream
        // handles lag internally and ends when the client shuts down.
        let mut notifications = client.notifications();
        loop {
            match notifications.next().await {
                Some(ClientNotification::Event {
                    subscription_id: event_sub_id,
                    event,
                    ..
                }) => {
                    if event_sub_id != *sub_id {
                        continue;
                    }
                    let referenced_ids =
                        extract_referenced_events_for_streaming(&event, &tracked_ids);
                    if referenced_ids.is_empty() {
                        continue;
                    }
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
                    let repost_id = if matches!(event.kind, Kind::Repost | Kind::GenericRepost)
                        && is_current_user
                    {
                        Some(event.id.to_hex())
                    } else {
                        None
                    };
                    for (referenced_id, is_quote) in referenced_ids {
                        if let Some(updated_counts) = increment_cached_counts(
                            &referenced_id,
                            event.kind,
                            content,
                            is_current_user,
                            zap_amount,
                            repost_id.as_deref(),
                            is_quote,
                        ) {
                            // Merge, never overwrite: the L2 entry can lag
                            // the signal map (batch attribution happened
                            // before later merges), and a replace would
                            // lower painted counters — the count-flicker
                            // regression.
                            let mut map = interaction_counts.write();
                            match map.get_mut(&referenced_id) {
                                Some(existing) => existing.merge_max(&updated_counts),
                                None => {
                                    map.insert(referenced_id.clone(), updated_counts);
                                }
                            }
                            log::debug!(
                                "Streamed interaction update for {}: kind={} quote={}",
                                safe_slice(&referenced_id, 8),
                                event.kind.as_u16(),
                                is_quote
                            );
                        }
                        if event.kind.as_u16() == 1010 {
                            if let Ok(original_id) = nostr::event::EventId::from_hex(&referenced_id)
                            {
                                crate::stores::edit_cache::process_edit_event(
                                    &original_id,
                                    &event,
                                    None,
                                );
                            }
                        }
                    }
                }
                Some(ClientNotification::Shutdown) => {
                    log::debug!("Interaction stream: pool shutdown");
                    break;
                }
                // Stream ended (client shut down / dropped).
                None => {
                    log::debug!("Interaction stream: stream ended");
                    break;
                }
                Some(_) => {}
            }
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
        let event_id = format!(
            "event-id-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system time should be after unix epoch")
                .as_nanos()
        );
        {
            let mut cache = get_counts_cache().lock().unwrap_or_else(|poisoned| {
                log::warn!("Counts cache mutex was poisoned, recovering");
                poisoned.into_inner()
            });
            cache.invalidate(&event_id);
            cache.insert(event_id.clone(), InteractionCounts::default());
        }

        let updated = increment_cached_counts(&event_id, Kind::Comment, None, false, None, None, false)
            .expect("counts should be updated");

        assert_eq!(updated.replies, 1);

        let mut cache = get_counts_cache().lock().unwrap_or_else(|poisoned| {
            log::warn!("Counts cache mutex was poisoned, recovering");
            poisoned.into_inner()
        });
        cache.invalidate(&event_id);
    }

    /// A streamed NIP-18 `q`-tag quote increments the reshare counter —
    /// never the reply counter — even though the carrying event is a
    /// kind-1 text note (Amethyst boost parity).
    #[test]
    fn increment_cached_counts_treats_quote_as_repost() {
        let event_id = format!(
            "event-id-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system time should be after unix epoch")
                .as_nanos()
        );
        {
            let mut cache = get_counts_cache().lock().unwrap_or_else(|poisoned| {
                log::warn!("Counts cache mutex was poisoned, recovering");
                poisoned.into_inner()
            });
            cache.invalidate(&event_id);
            cache.insert(event_id.clone(), InteractionCounts::default());
        }

        let updated = increment_cached_counts(&event_id, Kind::TextNote, None, false, None, None, true)
            .expect("counts should be updated");

        assert_eq!(updated.reposts, 1);
        assert_eq!(updated.replies, 0);
        assert_eq!(updated.user_reposted, None, "quotes carry no undo state");

        let mut cache = get_counts_cache().lock().unwrap_or_else(|poisoned| {
            log::warn!("Counts cache mutex was poisoned, recovering");
            poisoned.into_inner()
        });
        cache.invalidate(&event_id);
    }
}
