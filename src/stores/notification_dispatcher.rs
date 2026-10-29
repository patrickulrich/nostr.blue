use nostr_sdk::prelude::*;
use nostr_sdk::client::{Client};
use nostr_sdk::prelude::{SubscriptionId};
use nostr::message::relay::RelayMessage;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

type EventSender = tokio::sync::mpsc::UnboundedSender<Arc<nostr::event::Event>>;
type SubscriberList = Vec<(u64, EventSender)>;

/// Native-listener batching machinery (unused on wasm where the listener
/// runs on the Dioxus runtime without the ndb caches).
#[cfg(not(target_arch = "wasm32"))]
mod native_batching {
    use std::time::Duration;

    /// Flush the native listener's cache batch once it holds this many events.
    pub const BATCH_SIZE: usize = 50;
    /// …or once this much time has passed since the last flush, whichever
    /// comes first (keeps latency bounded during slow trickles).
    pub const BATCH_WINDOW: Duration = Duration::from_millis(100);

    /// Outcome of one notification-stream receive, for the select! loop.
    pub enum ControlFlow {
        Event(nostr::message::SubscriptionId, Box<nostr::event::Event>),
        Shutdown,
        Closed,
        Continue,
    }

    /// Flush the pending ndb-cache batch: one lock acquisition per cache, one
    /// queue extend — instead of one per event.
    pub fn flush_batch(batch: &mut Vec<nostr::event::Event>) {
        if batch.is_empty() {
            return;
        }
        #[cfg(feature = "native")]
        {
            crate::stores::ndb::cache_events_batch(batch);
            crate::stores::ndb::unknown_ids::queue_events_batch(std::mem::take(batch));
        }
        #[cfg(not(feature = "native"))]
        batch.clear();
    }
}

#[cfg(not(target_arch = "wasm32"))]
use native_batching::flush_batch;
#[cfg(not(target_arch = "wasm32"))]
use native_batching::ControlFlow;
#[cfg(not(target_arch = "wasm32"))]
use native_batching::{BATCH_SIZE, BATCH_WINDOW};
#[cfg(not(target_arch = "wasm32"))]
use std::time::Duration;

#[allow(clippy::type_complexity)]
struct DispatcherInner {
    next_id: u64,
    subscribers: HashMap<SubscriptionId, SubscriberList>,
}

/// Bounded per-(subscription, event) dedup for dispatcher fan-out.
///
/// Events surface through BOTH `ClientNotification::Event` (first-seen only)
/// and raw `Message(Event)` relay responses — the latter being the ONLY shape
/// for events already persisted to the local DB (replayed REQ results after a
/// reconnect, refresh fetches). Consuming both sources means the same event
/// can reach the dispatcher more than once (once per relay for the raw
/// shape); this set collapses those duplicates so subscribers keep seeing
/// each event exactly once per subscription. Bounded FIFO eviction keeps the
/// memory flat during backfill floods.
///
/// Shared with `subscription_manager`'s pre-init fallback listener so the
/// boot-window path applies the same dedup as the dispatcher.
pub(crate) struct RecentEvents {
    seen: std::collections::HashSet<(SubscriptionId, nostr::event::EventId)>,
    order: std::collections::VecDeque<(SubscriptionId, nostr::event::EventId)>,
}

impl RecentEvents {
    const CAP: usize = 8192;

    pub(crate) fn new() -> Self {
        Self {
            seen: std::collections::HashSet::with_capacity(Self::CAP),
            order: std::collections::VecDeque::with_capacity(Self::CAP),
        }
    }

    /// Returns `true` when the key is new (forward), `false` on duplicate.
    pub(crate) fn insert(&mut self, key: (SubscriptionId, nostr::event::EventId)) -> bool {
        if self.seen.insert(key.clone()) {
            self.order.push_back(key);
            if self.order.len() > Self::CAP {
                if let Some(evicted) = self.order.pop_front() {
                    self.seen.remove(&evicted);
                }
            }
            true
        } else {
            false
        }
    }
}

static DISPATCHER: std::sync::OnceLock<NotificationDispatcher> = std::sync::OnceLock::new();

pub struct NotificationDispatcher {
    inner: Arc<Mutex<DispatcherInner>>,
    client: Arc<Client>,
}

impl NotificationDispatcher {
    pub fn init(client: Arc<Client>) {
        let dispatcher = NotificationDispatcher {
            inner: Arc::new(Mutex::new(DispatcherInner {
                next_id: 0,
                subscribers: HashMap::new(),
            })),
            client,
        };
        let _ = DISPATCHER.set(dispatcher);
    }

    pub fn instance() -> Option<&'static NotificationDispatcher> {
        DISPATCHER.get()
    }

    pub fn subscribe(
        &self,
        sub_id: SubscriptionId,
    ) -> (u64, tokio::sync::mpsc::UnboundedReceiver<std::sync::Arc<nostr::event::Event>>) {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let id = inner.next_id;
        inner.next_id += 1;
        inner.subscribers.entry(sub_id).or_default().push((id, tx));
        (id, rx)
    }

    pub fn unsubscribe(&self, sub_id: &SubscriptionId, callback_id: u64) {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(senders) = inner.subscribers.get_mut(sub_id) {
            senders.retain(|(id, _)| *id != callback_id);
            if senders.is_empty() {
                inner.subscribers.remove(sub_id);
            }
        }
    }

    pub fn start_listener(&self) {
        let inner = self.inner.clone();

        #[cfg(not(target_arch = "wasm32"))]
        {
            std::thread::spawn(move || {
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .expect("notification dispatcher runtime");

                rt.block_on(async move {
                    // Outer re-read loop: the client is rebuilt on login/logout.
                    // When the old client shuts down its notification stream
                    // ends; re-read `get_client()` and continue with the new
                    // client instead of exiting.
                    loop {
                        let Some(client) =
                            crate::stores::nostr_client::get_client()
                        else {
                            crate::stores::nostr_client::platform_sleep_ms(1000).await;
                            continue;
                        };
                        let mut notifications = client.notifications();
                        // Backfill floods (reconnects, negentropy syncs) deliver
                        // hundreds of events per second. Per-event work here
                        // (INFO logging + mutex lock/unlock per cache insert)
                        // saturates the tao stdout pipe and starves main-thread
                        // lock readers, freezing UI interactions while scrolls
                        // (pure chromium) keep working. Buffer and flush in
                        // batches instead.
                        let mut batch: Vec<nostr::event::Event> = Vec::with_capacity(BATCH_SIZE);
                        let mut batch_deadline =
                            tokio::time::Instant::now() + BATCH_WINDOW;
                        let mut events_seen: u64 = 0;
                        let mut log_window_start = tokio::time::Instant::now();
                        let mut recent = RecentEvents::new();
                        loop {
                            let recv = async {
                                match notifications.next().await {
                                    Some(ClientNotification::Event {
                                        subscription_id,
                                        event,
                                        ..
                                    }) => ControlFlow::Event(subscription_id, event),
                                    Some(ClientNotification::Message { relay_url, message }) => {
                                        // F1: refusal skip-list. A relay
                                        // answering CLOSED with an
                                        // error/auth/block refusal (e.g.
                                        // ditto.pub "error: too many
                                        // subscriptions") is stamped so
                                        // fetch paths skip it for the TTL
                                        // instead of re-issuing REQs that
                                        // are guaranteed to be refused.
                                        if let RelayMessage::Closed { message, .. } =
                                            message.as_ref()
                                        {
                                            if crate::stores::relay::refusal::is_refusal_message(
                                                message,
                                            ) {
                                                crate::stores::relay::refusal::mark_refused(
                                                    &relay_url,
                                                    message,
                                                );
                                            }
                                        }
                                        // Events already persisted to the local DB
                                        // (replayed REQ results after a reconnect,
                                        // refresh fetches) surface ONLY as raw
                                        // `Message(Event)` — the SDK emits
                                        // `ClientNotification::Event` solely on
                                        // first-seen. Route them through the same
                                        // fan-out or subscribers miss them.
                                        if let RelayMessage::Event { subscription_id, event } =
                                            message.as_ref()
                                        {
                                            ControlFlow::Event(
                                                (**subscription_id).clone(),
                                                Box::new((**event).clone()),
                                            )
                                        } else {
                                            ControlFlow::Continue
                                        }
                                    }
                                    Some(ClientNotification::Shutdown) => ControlFlow::Shutdown,
                                    // Stream ended (client shut down / dropped).
                                    None => ControlFlow::Closed,
                                }
                            };
                            tokio::select! {
                                flow = recv => {
                                    match flow {
                                        ControlFlow::Event(subscription_id, event) => {
                                            if !recent.insert((subscription_id.clone(), event.id)) {
                                                continue;
                                            }
                                            events_seen += 1;
                                            #[cfg(feature = "native")]
                                            batch.push((*event).clone());
                                            let inner =
                                                inner.lock().unwrap_or_else(|e| e.into_inner());
                                            if let Some(senders) =
                                                inner.subscribers.get(&subscription_id)
                                            {
                                                let event = Arc::new((*event).clone());
                                                for (_, tx) in senders {
                                                    let _ = tx.send(event.clone());
                                                }
                                            }
                                            if batch.len() >= BATCH_SIZE {
                                                flush_batch(&mut batch);
                                            }
                                        }
                                        ControlFlow::Shutdown => {
                                            flush_batch(&mut batch);
                                            log::info!("notification_dispatcher: relay pool shutdown, re-reading client");
                                            break;
                                        }
                                        ControlFlow::Closed => {
                                            flush_batch(&mut batch);
                                            log::info!("notification_dispatcher: notification stream ended, re-reading client");
                                            break;
                                        }
                                        ControlFlow::Continue => {}
                                    }
                                }
                                _ = tokio::time::sleep_until(batch_deadline) => {
                                    flush_batch(&mut batch);
                                    batch_deadline = tokio::time::Instant::now() + BATCH_WINDOW;
                                }
                            }
                            // One aggregate log line per second instead of one
                            // per event — the per-event version flooded the
                            // stdout pipe during backfills and stalled every
                            // thread that logged.
                            //
                            // Phase 1: rates above the burst threshold log at
                            // WARN — every relay event parses + verifies on
                            // the main thread (wasm), so sustained >50/s is
                            // the CPU-load proxy the stall detector
                            // correlates against.
                            if events_seen > 0
                                && log_window_start.elapsed() >= Duration::from_secs(1)
                            {
                                if events_seen > 50 {
                                    log::warn!(
                                        "notification_dispatcher: EVENT BURST {events_seen}/s (verify+parse load)"
                                    );
                                } else {
                                    log::info!(
                                        "notification_dispatcher: {events_seen} events this second"
                                    );
                                }
                                events_seen = 0;
                                log_window_start = tokio::time::Instant::now();
                            }
                        }
                        // Brief backoff so a shut-down-but-still-present client
                        // (empty stream) can't hot-spin this loop.
                        crate::stores::nostr_client::platform_sleep_ms(1000).await;
                    }
                });
            });
        }

        #[cfg(target_arch = "wasm32")]
        {
        crate::platform::spawn::spawn_local_catch_unwind("dispatcher", async move {
            // Outer re-read loop: the client is rebuilt on login/logout. When
            // the old client shuts down its notification stream ends; re-read
            // `get_client()` and continue with the new client.
            loop {
                let Some(client) = crate::stores::nostr_client::get_client() else {
                    crate::stores::nostr_client::platform_sleep_ms(1000).await;
                    continue;
                };
                let mut notifications = client.notifications();
                let mut recent = RecentEvents::new();
                loop {
                    match notifications.next().await {
                        Some(ClientNotification::Event {
                            subscription_id,
                            event,
                            ..
                        }) => {
                            if !recent.insert((subscription_id.clone(), event.id)) {
                                continue;
                            }
                            let inner = inner.lock().unwrap_or_else(|e| e.into_inner());
                            if let Some(senders) = inner.subscribers.get(&subscription_id) {
                                let event = Arc::new(*event);
                                for (_, tx) in senders {
                                    let _ = tx.send(event.clone());
                                }
                            }
                        }
                        // Events already persisted to the local DB (replayed
                        // REQ results after a reconnect, refresh fetches)
                        // surface ONLY as raw `Message(Event)` — the SDK emits
                        // `ClientNotification::Event` solely on first-seen.
                        // Route them through the same fan-out (deduped) or
                        // subscribers miss them.
                        Some(ClientNotification::Message { relay_url, message }) => {
                            // F1: refusal skip-list (see the arm above).
                            if let RelayMessage::Closed { message, .. } = message.as_ref() {
                                if crate::stores::relay::refusal::is_refusal_message(message) {
                                    crate::stores::relay::refusal::mark_refused(
                                        &relay_url,
                                        message,
                                    );
                                }
                            }
                            if let RelayMessage::Event { subscription_id, event } =
                                message.as_ref()
                            {
                                let key = ((**subscription_id).clone(), event.id);
                                if !recent.insert(key) {
                                    continue;
                                }
                                let inner = inner.lock().unwrap_or_else(|e| e.into_inner());
                                if let Some(senders) = inner.subscribers.get(&**subscription_id) {
                                    let event = Arc::new((**event).clone());
                                    for (_, tx) in senders {
                                        let _ = tx.send(event.clone());
                                    }
                                }
                            }
                        }
                        Some(ClientNotification::Shutdown) => {
                            log::info!("notification_dispatcher: relay pool shutdown, re-reading client");
                            break;
                        }
                        // Stream ended (client shut down / dropped).
                        None => {
                            log::info!("notification_dispatcher: notification stream ended, re-reading client");
                            break;
                        }
                    }
                }
                // Brief backoff so a shut-down-but-still-present client
                // (empty stream) can't hot-spin this loop.
                crate::stores::nostr_client::platform_sleep_ms(1000).await;
            }
        });
        }
    }
}

pub struct DispatcherHandle {
    sub_id: SubscriptionId,
    callback_id: u64,
    client: Arc<Client>,
}

impl DispatcherHandle {
    pub fn create(
        sub_id: SubscriptionId,
    ) -> Option<(Self, tokio::sync::mpsc::UnboundedReceiver<std::sync::Arc<nostr::event::Event>>)> {
        let dispatcher = NotificationDispatcher::instance()?;
        let (callback_id, rx) = dispatcher.subscribe(sub_id.clone());
        Some((
            DispatcherHandle {
                sub_id,
                callback_id,
                client: dispatcher.client.clone(),
            },
            rx,
        ))
    }

    pub async fn unregister(self) {
        if let Some(dispatcher) = NotificationDispatcher::instance() {
            dispatcher.unsubscribe(&self.sub_id, self.callback_id);
        }
        let _ = self.client.unsubscribe(&self.sub_id).await;
    }
}
