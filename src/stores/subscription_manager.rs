//! Subscription management helpers using SDK's auto-close options
//!
//! Provides consistent subscription patterns:
//! - `subscribe_realtime` - Long-lived subscriptions with inactivity timeout
//! - `subscribe_once` - One-time fetches that close after EOSE
//! - `subscribe_for_events` - Wait for specific number of events
use nostr_relay_pool::relay::ReqExitPolicy;
use nostr_relay_pool::SubscribeAutoCloseOptions;
use nostr_sdk::{Client, Filter, SubscriptionId};
use std::time::Duration;
/// Subscribe for real-time updates with auto-close after inactivity
///
/// The subscription will close automatically after the specified duration
/// of inactivity following EOSE (End Of Stored Events). This prevents
/// resource leaks while allowing real-time event streaming.
///
/// # Arguments
/// * `client` - The Nostr client
/// * `filter` - The subscription filter
/// * `idle_timeout_secs` - Seconds of inactivity after EOSE before auto-close (default: 300)
///
/// # Returns
/// The subscription ID for tracking and manual cleanup if needed
pub async fn subscribe_realtime(
    client: &Client,
    filter: Filter,
    idle_timeout_secs: Option<u64>,
) -> Result<SubscriptionId, String> {
    let timeout = Duration::from_secs(idle_timeout_secs.unwrap_or(300));
    let auto_close = SubscribeAutoCloseOptions::default()
        .exit_policy(ReqExitPolicy::WaitDurationAfterEOSE(timeout));
    client
        .subscribe(filter, Some(auto_close))
        .await
        .map(|output| output.val)
        .map_err(|e| format!("Failed to subscribe: {}", e))
}
/// Subscribe for a one-time fetch that closes after EOSE
///
/// The subscription will close automatically once all relays have sent EOSE
/// (End Of Stored Events). Use this for initial data fetches where you don't
/// need ongoing updates.
///
/// # Arguments
/// * `client` - The Nostr client
/// * `filter` - The subscription filter
/// * `timeout` - Maximum time to wait for events
///
/// # Returns
/// The subscription ID (subscription will auto-close)
#[allow(dead_code)]
pub async fn subscribe_once(
    client: &Client,
    filter: Filter,
    timeout: Duration,
) -> Result<SubscriptionId, String> {
    let auto_close = SubscribeAutoCloseOptions::default()
        .exit_policy(ReqExitPolicy::ExitOnEOSE)
        .timeout(Some(timeout));
    client
        .subscribe(filter, Some(auto_close))
        .await
        .map(|output| output.val)
        .map_err(|e| format!("Failed to subscribe: {}", e))
}
/// Subscribe and wait for a specific number of events after EOSE
///
/// Useful when you expect a certain number of new events and want to
/// close the subscription once they arrive.
///
/// # Arguments
/// * `client` - The Nostr client
/// * `filter` - The subscription filter
/// * `event_count` - Number of events to wait for after EOSE (max 65535)
/// * `timeout` - Maximum time to wait
///
/// # Returns
/// The subscription ID
#[allow(dead_code)]
pub async fn subscribe_for_events(
    client: &Client,
    filter: Filter,
    event_count: u16,
    timeout: Duration,
) -> Result<SubscriptionId, String> {
    let auto_close = SubscribeAutoCloseOptions::default()
        .exit_policy(ReqExitPolicy::WaitForEventsAfterEOSE(event_count))
        .timeout(Some(timeout));
    client
        .subscribe(filter, Some(auto_close))
        .await
        .map(|output| output.val)
        .map_err(|e| format!("Failed to subscribe: {}", e))
}
/// Subscribe with full control over auto-close behavior
///
/// # Arguments
/// * `client` - The Nostr client
/// * `filter` - The subscription filter
/// * `exit_policy` - When to auto-close the subscription
/// * `timeout` - Optional maximum time for the subscription
/// * `idle_timeout` - Optional timeout for idle connections
///
/// # Returns
/// The subscription ID
#[allow(dead_code)]
pub async fn subscribe_with_options(
    client: &Client,
    filter: Filter,
    exit_policy: ReqExitPolicy,
    timeout: Option<Duration>,
    idle_timeout: Option<Duration>,
) -> Result<SubscriptionId, String> {
    let mut auto_close = SubscribeAutoCloseOptions::default().exit_policy(exit_policy);
    if let Some(t) = timeout {
        auto_close = auto_close.timeout(Some(t));
    }
    if let Some(t) = idle_timeout {
        auto_close = auto_close.idle_timeout(Some(t));
    }
    client
        .subscribe(filter, Some(auto_close))
        .await
        .map(|output| output.val)
        .map_err(|e| format!("Failed to subscribe: {}", e))
}
/// Unsubscribe from a subscription
///
/// Helper to cleanly unsubscribe and log the result
pub async fn unsubscribe(client: &Client, subscription_id: &SubscriptionId) {
    client.unsubscribe(subscription_id).await;
    log::debug!("Unsubscribed from {:?}", subscription_id);
}
/// Unsubscribe from multiple subscriptions
///
/// Helper to cleanly unsubscribe from a list of subscriptions
pub async fn unsubscribe_all(client: &Client, subscription_ids: &[SubscriptionId]) {
    for id in subscription_ids {
        unsubscribe(client, id).await;
    }
}
