use nostr_sdk::prelude::*;
use nostr_sdk::{Client, RelayPoolNotification, SubscriptionId};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::sync::broadcast::error::RecvError;

type EventSender = tokio::sync::mpsc::UnboundedSender<Arc<nostr::Event>>;
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
        Event(nostr_sdk::SubscriptionId, Box<nostr::Event>),
        Shutdown,
        Closed,
        Continue,
    }

    /// Flush the pending ndb-cache batch: one lock acquisition per cache, one
    /// queue extend — instead of one per event.
    pub fn flush_batch(batch: &mut Vec<nostr::Event>) {
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
    ) -> (u64, tokio::sync::mpsc::UnboundedReceiver<std::sync::Arc<nostr::Event>>) {
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
        let client = self.client.clone();

        #[cfg(not(target_arch = "wasm32"))]
        {
            std::thread::spawn(move || {
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .expect("notification dispatcher runtime");

                rt.block_on(async move {
                    let mut notifications = client.notifications();
                    // Backfill floods (reconnects, negentropy syncs) deliver
                    // hundreds of events per second. Per-event work here
                    // (INFO logging + mutex lock/unlock per cache insert)
                    // saturates the tao stdout pipe and starves main-thread
                    // lock readers, freezing UI interactions while scrolls
                    // (pure chromium) keep working. Buffer and flush in
                    // batches instead.
                    let mut batch: Vec<nostr::Event> = Vec::with_capacity(BATCH_SIZE);
                    let mut batch_deadline =
                        tokio::time::Instant::now() + BATCH_WINDOW;
                    let mut events_seen: u64 = 0;
                    let mut log_window_start = tokio::time::Instant::now();
                    loop {
                        let recv = async {
                            match notifications.recv().await {
                                Ok(RelayPoolNotification::Event {
                                    subscription_id,
                                    event,
                                    ..
                                }) => ControlFlow::Event(subscription_id, event),
                                Ok(RelayPoolNotification::Shutdown) => ControlFlow::Shutdown,
                                // Transient: consumer was too slow and missed N messages.
                                // Channel is still alive — must NOT exit or features silently die.
                                Err(RecvError::Lagged(skipped)) => {
                                    log::warn!(
                                        "notification_dispatcher: lagged, skipped {} events, continuing",
                                        skipped
                                    );
                                    ControlFlow::Continue
                                }
                                // Channel closed (pool dropped). Genuine termination.
                                Err(RecvError::Closed) => ControlFlow::Closed,
                                Ok(_) => ControlFlow::Continue,
                            }
                        };
                        tokio::select! {
                            flow = recv => {
                                match flow {
                                    ControlFlow::Event(subscription_id, event) => {
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
                                        log::info!("notification_dispatcher: relay pool shutdown, exiting native listener");
                                        break;
                                    }
                                    ControlFlow::Closed => {
                                        flush_batch(&mut batch);
                                        log::info!("notification_dispatcher: notification channel closed, exiting native listener");
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
                        if events_seen > 0
                            && log_window_start.elapsed() >= Duration::from_secs(1)
                        {
                            log::info!(
                                "notification_dispatcher: {events_seen} events this second"
                            );
                            events_seen = 0;
                            log_window_start = tokio::time::Instant::now();
                        }
                    }
                });
            });
        }

        #[cfg(target_arch = "wasm32")]
        {
        crate::platform::spawn::spawn_local_catch_unwind("dispatcher", async move {
            let mut notifications = client.notifications();
            loop {
                match notifications.recv().await {
                    Ok(RelayPoolNotification::Event {
                        subscription_id,
                        event,
                        ..
                    }) => {
                        let inner = inner.lock().unwrap_or_else(|e| e.into_inner());
                        if let Some(senders) = inner.subscribers.get(&subscription_id) {
                            let event = Arc::new(*event.clone());
                            for (_, tx) in senders {
                                let _ = tx.send(event.clone());
                            }
                        }
                    }
                    Ok(RelayPoolNotification::Shutdown) => {
                        log::info!("notification_dispatcher: relay pool shutdown, exiting wasm listener");
                        break;
                    }
                    // Transient: consumer was too slow and missed N messages.
                    // Channel is still alive — must NOT exit or features silently die.
                    Err(RecvError::Lagged(skipped)) => {
                        log::warn!(
                            "notification_dispatcher: lagged, skipped {} events, continuing",
                            skipped
                        );
                        continue;
                    }
                    // Channel closed (pool dropped). Genuine termination.
                    Err(RecvError::Closed) => {
                        log::info!("notification_dispatcher: notification channel closed, exiting wasm listener");
                        break;
                    }
                    Ok(_) => {}
                }
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
    ) -> Option<(Self, tokio::sync::mpsc::UnboundedReceiver<std::sync::Arc<nostr::Event>>)> {
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
