//! Subscription management helpers using SDK's auto-close options
//!
//! Provides consistent subscription patterns:
//! - `subscribe_realtime` - Long-lived subscriptions with inactivity timeout
//! - `subscribe_once` - One-time fetches that close after EOSE
//! - `subscribe_for_events` - Wait for specific number of events
//! - `spawn_subscription` - Dispatcher-backed subscription with a stale
//!   guard and a shutdown handle (replaces the hand-rolled
//!   `spawn + notifications() + loop` boilerplate)
//! - `FilterDiffTracker` - skip-when-unchanged diffing for re-targeting
use nostr_sdk::relay::{ReqExitPolicy, SubscribeAutoCloseOptions};
use nostr_sdk::prelude::{Client, Filter, SubscriptionId};
use nostr::types::url::RelayUrl;
use std::sync::Arc;
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
        .subscribe(filter)
        .close_on(auto_close)
        .await
        .map(|output| output.value)
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
        .subscribe(filter)
        .close_on(auto_close)
        .await
        .map(|output| output.value)
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
        .subscribe(filter)
        .close_on(auto_close)
        .await
        .map(|output| output.value)
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
        .subscribe(filter)
        .close_on(auto_close)
        .await
        .map(|output| output.value)
        .map_err(|e| format!("Failed to subscribe: {}", e))
}
/// Unsubscribe from a subscription
///
/// Helper to cleanly unsubscribe and log the result
pub async fn unsubscribe(client: &Client, subscription_id: &SubscriptionId) {
    let _ = client.unsubscribe(subscription_id).await;
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

// ---------------------------------------------------------------------------
// spawn_subscription: dispatcher-backed subscription primitive
// ---------------------------------------------------------------------------

/// Where to run the REQ.
#[derive(Clone, Debug)]
pub enum SubTarget {
    /// Pool-auto routing (READ relays, gossip-augmented).
    Auto(Filter),
    /// Explicit per-relay filters.
    #[allow(dead_code)] // consumed as remaining notification loops migrate (plan phase 3)
    Targeted(Vec<(RelayUrl, Vec<Filter>)>),
}

/// Handle to a subscription spawned with [`spawn_subscription`].
///
/// The listener stops invoking the callback once `is_stale` turns true, but
/// the relay REQ stays open until [`SpawnedSubscription::shutdown`] (or the
/// caller's own unsubscribe) runs — stale-gating alone must not leak REQs.
pub struct SpawnedSubscription {
    client: Arc<Client>,
    sub_id: SubscriptionId,
    handle: Option<crate::stores::notification_dispatcher::DispatcherHandle>,
}

impl SpawnedSubscription {
    /// Close the subscription everywhere (dispatcher unlisten + relay CLOSE).
    /// Idempotent-best-effort: safe to call more than once.
    pub async fn shutdown(mut self) {
        if let Some(handle) = self.handle.take() {
            handle.unregister().await;
        }
        let _ = self.client.unsubscribe(&self.sub_id).await;
        log::debug!("spawn_subscription: closed {:?}", self.sub_id);
    }

    #[allow(dead_code)] // consumed as remaining notification loops migrate (plan phase 3)
    pub fn sub_id(&self) -> &SubscriptionId {
        &self.sub_id
    }
}

impl Drop for SpawnedSubscription {
    /// Leak-safety: a dropped guard still unregisters its dispatcher entry
    /// and closes the REQ. Callers that want the subscription to outlive a
    /// scope must keep the guard alive (or hand lifetime to the dispatcher's
    /// own liveness) — see the topics feeds for the canonical pattern.
    fn drop(&mut self) {
        if std::thread::panicking() {
            return;
        }
        let client = self.client.clone();
        let sub_id = self.sub_id.clone();
        let handle = self.handle.take();
        crate::platform::spawn::spawn_catch_unwind("sub_drop", async move {
            if let Some(handle) = handle {
                handle.unregister().await;
            }
            let _ = client.unsubscribe(&sub_id).await;
        });
    }
}

/// Subscribe with automatic relay selection OR explicit targets, applying
/// the pool-membership fallback for targeted REQs.
///
/// A targeted subscribe hard-fails with `RelayNotFound` if ANY url is not a
/// current pool member — a relay that disconnects between the caller's
/// snapshot and the subscribe would otherwise kill the whole REQ. This
/// intersects the targets with current pool membership and retries, degrading
/// to pool-auto routing only when nothing survives. Shared by
/// [`spawn_subscription_opts`] and the aggregation interaction streamer.
pub async fn subscribe_target(
    client: &Client,
    target: SubTarget,
    close_opts: Option<SubscribeAutoCloseOptions>,
) -> Result<nostr_sdk::client::Output<SubscriptionId>, nostr_sdk::error::Error> {
    match target {
        SubTarget::Auto(filter) => {
            let mut req = client.subscribe(filter);
            if let Some(opts) = close_opts {
                req = req.close_on(opts);
            }
            req.await
        }
        SubTarget::Targeted(targets) => {
            match targeted_subscribe(client, targets.clone(), close_opts).await {
                Ok(output) => Ok(output),
                Err(e) => {
                    // Targeted subscriptions hard-fail the ENTIRE call if any
                    // URL is not a pool member (`RelayNotFound`). Intersect
                    // with pool membership and retry; degrade to the global
                    // pool if nothing survives (same contract as
                    // `use_relay_subscription_to`).
                    let members: std::collections::HashSet<String> = client
                        .relays()
                        .all()
                        .await
                        .keys()
                        .map(|u| u.as_str().to_string())
                        .collect();
                    let member_urls: Vec<RelayUrl> = targets
                        .iter()
                        .map(|(u, _)| u.clone())
                        .filter(|u| members.contains(u.as_str()))
                        .collect();
                    let dropped: Vec<&str> = targets
                        .iter()
                        .map(|(u, _)| u.as_str())
                        .filter(|u| !members.contains(*u))
                        .collect();
                    if !dropped.is_empty() {
                        log::warn!(
                            "spawn_subscription: {} not in pool ({}), retrying with {}/{} member relays",
                            dropped.join(", "),
                            e,
                            member_urls.len(),
                            targets.len()
                        );
                    }
                    if member_urls.is_empty() {
                        // Degrade to pool-auto with the UNION of the targets'
                        // filters: `SubTarget::Targeted` carries per-relay
                        // filter lists, and taking only the first relay's
                        // first filter would silently drop every other
                        // filter. Per-relay scoping is lost (every filter
                        // goes to every READ relay) — logged, since all
                        // current callers pass identical per-relay filters.
                        let mut union: Vec<Filter> = Vec::new();
                        for (_, fs) in &targets {
                            for f in fs {
                                if !union.contains(f) {
                                    union.push(f.clone());
                                }
                            }
                        }
                        if union.is_empty() {
                            return Err(nostr_sdk::error::Error::other("empty filters"));
                        }
                        log::warn!(
                            "spawn_subscription: no targeted URLs in pool; broadcasting {} merged filter(s) pool-wide (per-relay scoping lost)",
                            union.len()
                        );
                        let mut req = client.subscribe(union);
                        if let Some(opts) = close_opts {
                            req = req.close_on(opts);
                        }
                        req.await
                    } else {
                        let retry: Vec<(RelayUrl, Vec<Filter>)> = targets
                            .into_iter()
                            .filter(|(u, _)| members.contains(u.as_str()))
                            .collect();
                        targeted_subscribe(client, retry, close_opts).await
                    }
                }
            }
        }
    }
}

async fn targeted_subscribe(
    client: &Client,
    targets: Vec<(RelayUrl, Vec<Filter>)>,
    close_opts: Option<SubscribeAutoCloseOptions>,
) -> Result<nostr_sdk::client::Output<SubscriptionId>, nostr_sdk::error::Error> {
    let mut req = client.subscribe(targets);
    if let Some(opts) = close_opts {
        req = req.close_on(opts);
    }
    req.await
}

/// Callback + stale-check types: mirror the proven `use_relay_subscription`
/// shapes. Dioxus signals are thread-local (not `Send`), so callbacks that
/// write signals must go through `Arc<Mutex<Box<dyn FnMut>>>` — the same
/// contract the hook's fallback listener already uses on every platform.
pub type OnEvent = std::sync::Arc<std::sync::Mutex<Box<dyn FnMut(&nostr::event::Event)>>>;
pub type StaleCheck = std::sync::Arc<std::sync::Mutex<Box<dyn Fn() -> bool>>>;
/// Optional stream-end hook: invoked once when the listener terminates
/// (stale detected, dispatcher channel closed after a client rebuild, or
/// raw stream end). For callers whose reconnect logic must observe listener
/// death — e.g. the notifications store clears its sub-id so the next
/// effect re-arms the subscription.
pub type OnEnd = std::sync::Arc<std::sync::Mutex<Box<dyn FnMut()>>>;

/// Component-scoped [`spawn_subscription_opts`]: the listener is tied to the
/// current reactive scope (dies with the component) — the right default for
/// feeds and card subscriptions.
pub async fn spawn_subscription(
    client: Arc<Client>,
    target: SubTarget,
    close_opts: Option<SubscribeAutoCloseOptions>,
    is_stale: StaleCheck,
    on_event: OnEvent,
) -> Result<SpawnedSubscription, String> {
    spawn_subscription_opts(client, target, close_opts, false, is_stale, on_event, None).await
}

/// App-lifetime [`spawn_subscription_opts`]: the listener survives route
/// changes (`spawn_forever`) — for subscriptions owned by stores (e.g. the
/// NIP-78 prefs live sync), not components.
pub async fn spawn_subscription_forever(
    client: Arc<Client>,
    target: SubTarget,
    close_opts: Option<SubscribeAutoCloseOptions>,
    is_stale: StaleCheck,
    on_event: OnEvent,
) -> Result<SpawnedSubscription, String> {
    spawn_subscription_opts(client, target, close_opts, true, is_stale, on_event, None).await
}

/// [`spawn_subscription_forever`] with a stream-end hook — see [`OnEnd`].
pub async fn spawn_subscription_forever_with_end(
    client: Arc<Client>,
    target: SubTarget,
    close_opts: Option<SubscribeAutoCloseOptions>,
    is_stale: StaleCheck,
    on_event: OnEvent,
    on_end: OnEnd,
) -> Result<SpawnedSubscription, String> {
    spawn_subscription_opts(client, target, close_opts, true, is_stale, on_event, Some(on_end))
        .await
}

/// Subscribe and fan events to `on_event` through the notification
/// dispatcher (with a raw-notifications fallback), replacing the hand-rolled
/// `spawn + notifications() + loop + sub-id match + stale-check` boilerplate.
///
/// - `is_stale`: checked before every callback; once true the listener stops
///   (the REQ itself closes on [`SpawnedSubscription::shutdown`] — typically
///   from the effect re-run that already unsubscribes old ids).
/// - `forever`: app-lifetime scope vs component scope.
/// - `on_end`: optional one-shot stream-end hook (see [`OnEnd`]).
async fn spawn_subscription_opts(
    client: Arc<Client>,
    target: SubTarget,
    close_opts: Option<SubscribeAutoCloseOptions>,
    forever: bool,
    is_stale: StaleCheck,
    on_event: OnEvent,
    on_end: Option<OnEnd>,
) -> Result<SpawnedSubscription, String> {
    fn run_end_hook(on_end: &Option<OnEnd>) {
        if let Some(hook) = on_end {
            if let Ok(mut cb) = hook.lock() {
                cb();
            }
        }
    }
    fn run_stale_check(is_stale: &StaleCheck) -> bool {
        is_stale.lock().map(|f| f()).unwrap_or(true)
    }

    let output = subscribe_target(&client, target, close_opts)
        .await
        .map_err(|e| format!("spawn_subscription: subscribe failed: {e}"))?;
    let sub_id = output.value;

    if let Some((handle, mut rx)) =
        crate::stores::notification_dispatcher::DispatcherHandle::create(sub_id.clone())
    {
        let listener = async move {
            while let Some(event) = rx.recv().await {
                if run_stale_check(&is_stale) {
                    break;
                }
                if let Ok(mut cb) = on_event.lock() {
                    cb(&event);
                }
            }
            run_end_hook(&on_end);
        };
        if forever {
            crate::platform::spawn::spawn_forever_catch_unwind("sub_dispatch_forever", listener);
        } else {
            crate::platform::spawn::spawn_catch_unwind("sub_dispatch", listener);
        }
        Ok(SpawnedSubscription {
            client,
            sub_id,
            handle: Some(handle),
        })
    } else {
        // Dispatcher unavailable (pre-init): raw notifications fallback,
        // consuming BOTH event shapes (first-seen + already-DB-cached
        // Message(Event) — see notification_dispatcher for why) with the
        // same bounded dedup the dispatcher applies — without it the same
        // event fires `on_event` twice here (once per shape).
        let fallback_client = client.clone();
        let fallback_sub = sub_id.clone();
        let fallback = async move {
            use futures::StreamExt;
            let mut notifications = fallback_client.notifications();
            let mut recent = crate::stores::notification_dispatcher::RecentEvents::new();
            loop {
                match notifications.next().await {
                    Some(nostr_sdk::client::ClientNotification::Event {
                        subscription_id,
                        event,
                        ..
                    }) => {
                        if subscription_id == fallback_sub {
                            if run_stale_check(&is_stale) {
                                break;
                            }
                            if recent.insert((subscription_id.clone(), event.id)) {
                                if let Ok(mut cb) = on_event.lock() {
                                    cb(&event);
                                }
                            }
                        }
                    }
                    Some(nostr_sdk::client::ClientNotification::Message { message, .. }) => {
                        if let nostr::message::relay::RelayMessage::Event {
                            subscription_id,
                            event,
                        } = message.as_ref()
                        {
                            if **subscription_id == fallback_sub {
                                if run_stale_check(&is_stale) {
                                    break;
                                }
                                if recent.insert(((**subscription_id).clone(), event.id)) {
                                    if let Ok(mut cb) = on_event.lock() {
                                        cb(event);
                                    }
                                }
                            }
                        }
                    }
                    Some(nostr_sdk::client::ClientNotification::Shutdown) => break,
                    None => break,
                }
            }
            run_end_hook(&on_end);
        };
        if forever {
            crate::platform::spawn::spawn_forever_catch_unwind("sub_fallback_forever", fallback);
        } else {
            crate::platform::spawn::spawn_catch_unwind("sub_fallback", fallback);
        }
        Ok(SpawnedSubscription {
            client,
            sub_id,
            handle: None,
        })
    }
}

// ---------------------------------------------------------------------------
// Filter diffing (skip-when-unchanged)
// ---------------------------------------------------------------------------

/// Outcome of diffing a desired per-relay filter map against the last
/// applied one.
#[allow(dead_code)] // consumed as re-targeting lands in use_relay_subscription (plan phase 3)
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SubscriptionDiff {
    /// Desired == last applied: skip every relay round-trip. This is the
    /// common case for Dioxus effect re-runs that recompute the same map.
    Unchanged,
    /// The relay set or any relay's filters changed. The SDK (0.45) rejects
    /// duplicate REQ ids per relay and its `unsubscribe` closes on every
    /// relay, so applying this diff means unsubscribe + resubscribe — but
    /// only when something actually changed.
    Changed {
        added_or_changed: Vec<(RelayUrl, Vec<Filter>)>,
        removed: Vec<RelayUrl>,
    },
}

/// Skip-when-unchanged tracker for one subscription's per-relay filter map.
///
/// The per-relay granularity Amethyst gets from owning its transport is not
/// exposed by nostr-sdk 0.45 (duplicate sub ids per relay error; unsubscribe
/// is all-relays), so the diff here is used to SKIP the churn, not to send
/// per-relay deltas.
#[allow(dead_code)] // consumed as re-targeting lands in use_relay_subscription (plan phase 3)
#[derive(Debug, Default)]
pub struct FilterDiffTracker {
    last: Option<std::collections::HashMap<String, Vec<Filter>>>,
}

impl FilterDiffTracker {
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self { last: None }
    }

    /// Diff `desired` against the last applied map without recording it.
    #[allow(dead_code)]
    pub fn diff(
        &self,
        desired: &std::collections::HashMap<RelayUrl, Vec<Filter>>,
    ) -> SubscriptionDiff {
        let empty = std::collections::HashMap::new();
        let last = self.last.as_ref().unwrap_or(&empty);

        let mut added_or_changed = Vec::new();
        let mut removed = Vec::new();

        for (url, filters) in desired {
            match last.get(url.as_str()) {
                Some(prev) if prev == filters => {}
                _ => added_or_changed.push((url.clone(), filters.clone())),
            }
        }
        for url in last.keys() {
            if !desired.keys().any(|u| u.as_str() == url) {
                if let Ok(relay_url) = RelayUrl::parse(url) {
                    removed.push(relay_url);
                }
            }
        }

        if added_or_changed.is_empty() && removed.is_empty() {
            SubscriptionDiff::Unchanged
        } else {
            SubscriptionDiff::Changed {
                added_or_changed,
                removed,
            }
        }
    }

    /// Record `desired` as the newly applied map (call after subscribing).
    #[allow(dead_code)]
    pub fn applied(&mut self, desired: &std::collections::HashMap<RelayUrl, Vec<Filter>>) {
        self.last = Some(
            desired
                .iter()
                .map(|(u, f)| (u.as_str().to_string(), f.clone()))
                .collect(),
        );
    }

    /// Forget the last applied map (e.g. after an explicit unsubscribe).
    #[allow(dead_code)]
    pub fn clear(&mut self) {
        self.last = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn url(s: &str) -> RelayUrl {
        RelayUrl::parse(s).unwrap()
    }

    fn map(entries: &[(&str, Vec<Filter>)]) -> std::collections::HashMap<RelayUrl, Vec<Filter>> {
        entries
            .iter()
            .map(|(u, f)| (url(u), f.clone()))
            .collect()
    }

    #[test]
    fn diff_first_apply_is_changed() {
        let t = FilterDiffTracker::new();
        let desired = map(&[("wss://a.relay", vec![Filter::new().limit(5)])]);
        match t.diff(&desired) {
            SubscriptionDiff::Changed { added_or_changed, removed } => {
                assert_eq!(added_or_changed.len(), 1);
                assert!(removed.is_empty());
            }
            SubscriptionDiff::Unchanged => panic!("first apply must be Changed"),
        }
    }

    #[test]
    fn diff_identical_map_is_unchanged() {
        let mut t = FilterDiffTracker::new();
        let desired = map(&[
            ("wss://a.relay", vec![Filter::new().limit(5)]),
            ("wss://b.relay", vec![Filter::new().limit(10)]),
        ]);
        t.applied(&desired);
        // A fresh but equal map (different allocation, same content).
        let again = map(&[
            ("wss://a.relay", vec![Filter::new().limit(5)]),
            ("wss://b.relay", vec![Filter::new().limit(10)]),
        ]);
        assert_eq!(t.diff(&again), SubscriptionDiff::Unchanged);
    }

    #[test]
    fn diff_detects_filter_changes_and_removals() {
        let mut t = FilterDiffTracker::new();
        t.applied(&map(&[
            ("wss://a.relay", vec![Filter::new().limit(5)]),
            ("wss://b.relay", vec![Filter::new().limit(10)]),
        ]));
        match t.diff(&map(&[
            ("wss://a.relay", vec![Filter::new().limit(20)]), // changed
            ("wss://c.relay", vec![Filter::new().limit(1)]),  // added
        ])) {
            SubscriptionDiff::Changed { added_or_changed, removed } => {
                assert_eq!(added_or_changed.len(), 2, "changed + added");
                assert_eq!(removed, vec![url("wss://b.relay")]);
            }
            SubscriptionDiff::Unchanged => panic!("must be Changed"),
        }
    }

    #[test]
    fn clear_forces_next_diff_changed() {
        let mut t = FilterDiffTracker::new();
        let desired = map(&[("wss://a.relay", vec![Filter::new().limit(5)])]);
        t.applied(&desired);
        t.clear();
        assert!(matches!(t.diff(&desired), SubscriptionDiff::Changed { .. }));
    }
}
