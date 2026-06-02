use nostr_sdk::prelude::*;
use nostr_sdk::{Client, RelayPoolNotification, SubscriptionId};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

type EventSender = tokio::sync::mpsc::UnboundedSender<Arc<nostr::Event>>;
type SubscriberList = Vec<(u64, EventSender)>;

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
                    while let Ok(RelayPoolNotification::Event {
                        subscription_id,
                        event,
                        ..
                    }) = notifications.recv().await
                    {
                        #[cfg(feature = "native")]
                        {
                            crate::stores::ndb::unknown_ids::queue_event((*event).clone());
                            crate::stores::ndb::cache_event(&event);
                            log::info!("notification_dispatcher: cached event {:?} in bridge cache", event.id.to_hex());
                        }
                        let inner = inner.lock().unwrap_or_else(|e| e.into_inner());
                        if let Some(senders) = inner.subscribers.get(&subscription_id) {
                            let event = Arc::new(*event.clone());
                            for (_, tx) in senders {
                                let _ = tx.send(event.clone());
                            }
                        }
                    }
                    log::info!("notification_dispatcher: notification stream closed, exiting native listener");
                });
            });
        }

        #[cfg(target_arch = "wasm32")]
        {
            wasm_bindgen_futures::spawn_local(async move {
                let mut notifications = client.notifications();
                while let Ok(RelayPoolNotification::Event {
                    subscription_id,
                    event,
                    ..
                }) = notifications.recv().await
                {
                    let inner = inner.lock().unwrap_or_else(|e| e.into_inner());
                    if let Some(senders) = inner.subscribers.get(&subscription_id) {
                        let event = Arc::new(*event.clone());
                        for (_, tx) in senders {
                            let _ = tx.send(event.clone());
                        }
                    }
                }
                log::info!("notification_dispatcher: notification stream closed, exiting wasm listener");
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
