//! One-shot NIP-78 fetch with quorum-EOSE early-exit.
//!
//! Replaces the previous pattern of `client.fetch_events(filter, timeout)`
//! which waits for ALL relays to EOSE (or the timeout to fire). Since many
//! pool relays are dead and will never EOSE, this often burns the full
//! timeout on every load.
//!
//! Instead, this module manually subscribes, counts per-relay EOSE, and
//! early-exits when the quorum threshold is met (using the existing
//! [`eose_threshold`] function from `feeds/realtime.rs` — `max(3, 30% of
//! connected)`).

use std::time::Duration;

use nostr::message::relay::RelayMessage;
use nostr::Event;
use nostr_sdk::{Client, RelayPoolNotification, SubscriptionId};

/// One-shot fetch: subscribe, collect events + EOSE, early-exit on quorum,
/// return the newest event (by `created_at`).
///
/// The subscription is explicitly closed before returning.
pub async fn fetch_newest_with_quorum(
    client: &Client,
    filter: nostr::Filter,
    timeout: Duration,
) -> Result<Option<Event>, String> {
    // Subscribe (no auto-close — we manage the lifecycle manually).
    let sub_output = client
        .subscribe(filter, None)
        .await
        .map_err(|e| format!("subscribe: {e}"))?;
    let sub_id = sub_output.val;
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
    use nostr_relay_pool::RelayStatus;

    client
        .relays()
        .await
        .iter()
        .filter(|(_, r)| r.status() == RelayStatus::Connected && r.flags().has_read())
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
            recv_result = notifications.recv() => {
                match recv_result {
                    Ok(RelayPoolNotification::Event {
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
                    Ok(RelayPoolNotification::Message { message, .. }) => {
                        if let RelayMessage::EndOfStoredEvents(id) = &message {
                            if id.as_ref() == sub_id {
                                eose_count += 1;
                                if eose_count >= threshold {
                                    log::debug!(
                                        "fetch_newest_with_quorum: quorum reached \
                                         (eose_count={eose_count}/{relay_count}, threshold={threshold})"
                                    );
                                    break;
                                }
                            }
                        }
                    }
                    Ok(RelayPoolNotification::Shutdown) => break,
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                        log::warn!(
                            "fetch_newest_with_quorum: lagged, skipped {skipped} events, continuing"
                        );
                        continue;
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                    Ok(_) => {}
                }
            }
        }
    }
    Ok(best)
}
