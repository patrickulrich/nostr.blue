//! One-shot NIP-78 fetch with quorum-EOSE early-exit.
//!
//! Replaces the previous pattern of `client.fetch_events(filter, timeout)`
//! which waits for ALL relays to EOSE (or the timeout to fire). Since many
//! pool relays are dead and will never EOSE, this often burns the full
//! timeout on every load.
//!
//! Instead, this module manually subscribes, counts per-relay EOSE, and
//! early-exits when the quorum threshold is met (via the unified EOSE policy
//! module `stores::nostr_client::eose` — realtime-liveness shape).

use std::time::Duration;

use futures::StreamExt;
use nostr::message::relay::RelayMessage;
use nostr::event::Event;
use nostr_sdk::client::ClientNotification;
use nostr_sdk::client::{Client};
use nostr_sdk::prelude::{SubscriptionId};


/// One-shot fetch: subscribe, collect events + EOSE, early-exit on quorum,
/// return the newest event (by `created_at`).
///
/// The subscription is explicitly closed before returning.
pub async fn fetch_newest_with_quorum(
    client: &Client,
    filter: nostr::filter::Filter,
    timeout: Duration,
) -> Result<Option<Event>, String> {
    // Subscribe (no auto-close — we manage the lifecycle manually).
    let sub_output = client
        .subscribe(filter)
        .await
        .map_err(|e| format!("subscribe: {e}"))?;
    let sub_id = sub_output.value;
    let result = collect_with_quorum(client, &sub_id, timeout).await;
    // Always unsubscribe, even on error.
    let _ = client.unsubscribe(&sub_id).await;
    result
}

/// Count relays that can actually answer this REQ: `client.subscribe()`
/// dispatches to READ-flagged relays only (`__read_relay_urls` in the SDK
/// pool), so the quorum denominator must be Connected + READ-flagged.
/// Counting all pool members (disconnected, write-only, DISCOVERY-only)
/// inflates the threshold and defeats the early-exit.
async fn connected_read_relay_count(client: &Client) -> usize {
    use nostr_sdk::relay::RelayStatus;

    client
        .relays()
        .await
        .iter()
        .filter(|(_, r)| r.status() == RelayStatus::Connected && r.capabilities().has_read())
        .count()
}

/// Collect events + EOSE for a subscription until quorum or timeout.
async fn collect_with_quorum(
    client: &Client,
    sub_id: &SubscriptionId,
    timeout: Duration,
) -> Result<Option<Event>, String> {
    let mut notifications = client.notifications();
    let mut best: Option<Event> = None;
    let mut eose_count: usize = 0;

    // Determine the target relay count from the pool.
    let relay_count = connected_read_relay_count(client).await;

    // Compute the quorum threshold: max(3, 30% of connected READ relays).
    let threshold = crate::feeds::realtime::eose_threshold(relay_count, relay_count);

    let deadline = crate::platform::timer::sleep(timeout);
    tokio::pin!(deadline);

    loop {
        tokio::select! {
            _ = &mut deadline => {
                log::debug!(
                    "fetch_newest_with_quorum: timeout (eose_count={eose_count}/{relay_count}, threshold={threshold})"
                );
                break;
            }
            recv_result = notifications.next() => {
                match recv_result {
                    Some(ClientNotification::Event {
                        subscription_id,
                        event,
                        ..
                    }) if subscription_id == *sub_id => {
                        let event = *event;
                        match &best {
                            None => best = Some(event),
                            Some(current) => {
                                if event.created_at > current.created_at {
                                    best = Some(event);
                                }
                            }
                        }
                    }
                    Some(ClientNotification::Message { message, .. }) => {
                        match message.as_ref() {
                            // The SDK only emits `ClientNotification::Event`
                            // the FIRST time an event is seen; events already
                            // persisted (a prefs blob from a previous load)
                            // surface ONLY as raw `Message(Event)` — consume
                            // them here or cached blobs are silently missed.
                            RelayMessage::Event { subscription_id, event }
                                if subscription_id.as_ref() == sub_id =>
                            {
                                match &best {
                                    None => best = Some((**event).clone()),
                                    Some(current) => {
                                        if event.created_at > current.created_at {
                                            best = Some((**event).clone());
                                        }
                                    }
                                }
                            }
                            RelayMessage::EndOfStoredEvents(id) if id.as_ref() == sub_id => {
                                eose_count += 1;
                                if eose_count >= threshold {
                                    log::debug!(
                                        "fetch_newest_with_quorum: quorum reached \
                                         (eose_count={eose_count}/{relay_count}, threshold={threshold})"
                                    );
                                    break;
                                }
                            }
                            _ => {}
                        }
                    }
                    Some(ClientNotification::Shutdown) => break,
                    // Stream ended (client shut down / dropped).
                    None => break,
                    Some(_) => {}
                }
            }
        }
    }
    Ok(best)
}
