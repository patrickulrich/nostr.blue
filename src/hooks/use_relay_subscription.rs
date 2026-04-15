use crate::stores::nostr_client;
use crate::stores::notification_dispatcher::DispatcherHandle;
use dioxus::prelude::*;
use nostr_sdk::prelude::*;
use nostr_sdk::{Client, SubscribeAutoCloseOptions, SubscriptionId};
use std::sync::{Arc, Mutex};

type OnEvent = Arc<Mutex<Box<dyn FnMut(&nostr::Event)>>>;

struct SubState {
    sub_id: SubscriptionId,
    client: Arc<Client>,
    handle: Option<DispatcherHandle>,
}

#[allow(clippy::type_complexity, clippy::arc_with_non_send_sync)]
fn spawn_dispatched_listener(
    rx: tokio::sync::mpsc::UnboundedReceiver<std::sync::Arc<nostr::Event>>,
    on_event: OnEvent,
) {
    spawn(async move {
        let mut rx = rx;
        while let Some(event) = rx.recv().await {
            if let Ok(mut cb) = on_event.lock() {
                cb(&event);
            }
        }
    });
}

#[allow(dead_code)]
pub fn use_relay_subscription(
    filter: Option<Filter>,
    on_event: impl FnMut(&nostr::Event) + 'static,
) {
    use_relay_subscription_opts(filter, None, on_event);
}

#[allow(dead_code, clippy::arc_with_non_send_sync)]
pub fn use_relay_subscription_opts(
    filter: Option<Filter>,
    close_opts: Option<SubscribeAutoCloseOptions>,
    on_event: impl FnMut(&nostr::Event) + 'static,
) {
    let mut sub_state: Signal<Option<Arc<SubState>>> = use_signal(|| None);
    let on_event: OnEvent = Arc::new(Mutex::new(Box::new(on_event)));

    use_effect(use_reactive!(|filter| {
        let on_event = on_event.clone();
        spawn(async move {
            if let Some(old) = sub_state() {
                if let Ok(s) = Arc::try_unwrap(old) {
                    if let Some(handle) = s.handle {
                        handle.unregister().await;
                    } else {
                        let _ = s.client.unsubscribe(&s.sub_id).await;
                    }
                } else if let Some(old) = sub_state() {
                    let _ = old.client.unsubscribe(&old.sub_id).await;
                }
            }
            sub_state.set(None);

            let filter = match filter {
                Some(f) => f,
                None => return,
            };

            let client = match nostr_client::get_client() {
                Some(c) => c,
                None => return,
            };

            match client.subscribe(filter, close_opts).await {
                Ok(output) => {
                    let sub_id = output.val;

                    let handle = DispatcherHandle::create(sub_id.clone())
                        .map(|(handle, rx)| {
                            spawn_dispatched_listener(rx, on_event.clone());
                            handle
                        });

                    if handle.is_none() {
                        spawn_fallback_listener(client.clone(), sub_id.clone(), on_event.clone());
                    }

                    sub_state.set(Some(Arc::new(SubState {
                        sub_id: sub_id.clone(),
                        client: client.clone(),
                        handle,
                    })));
                }
                Err(e) => {
                    log::error!("use_relay_subscription: {}", e);
                }
            }
        });
    }));

    use_drop(move || {
        if let Some(old) = sub_state() {
            if let Ok(s) = Arc::try_unwrap(old) {
                if let Some(handle) = s.handle {
                    spawn(async move {
                        handle.unregister().await;
                    });
                } else {
                    spawn(async move {
                        let _ = s.client.unsubscribe(&s.sub_id).await;
                    });
                }
            } else if let Some(old) = sub_state() {
                spawn(async move {
                    let _ = old.client.unsubscribe(&old.sub_id).await;
                });
            }
        }
    });
}

#[allow(clippy::type_complexity, clippy::arc_with_non_send_sync)]
fn spawn_fallback_listener(client: Arc<Client>, sub_id: SubscriptionId, on_event: OnEvent) {
    spawn(async move {
        let mut notifications = client.notifications();
        while let Ok(notification) = notifications.recv().await {
            if let nostr_sdk::RelayPoolNotification::Event {
                subscription_id,
                event,
                ..
            } = notification
            {
                if subscription_id == sub_id {
                    if let Ok(mut cb) = on_event.lock() {
                        cb(&event);
                    }
                }
            }
        }
    });
}
