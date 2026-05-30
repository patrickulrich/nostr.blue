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
        let mut buffer = Vec::new();
        while let Some(event) = rx.recv().await {
            buffer.push(event);
            while let Ok(event) = rx.try_recv() {
                buffer.push(event);
            }
            if let Ok(mut cb) = on_event.lock() {
                for event in &buffer {
                    cb(event);
                }
            }
            buffer.clear();
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
    use_relay_subscription_to(filter, close_opts, Vec::<String>::new(), on_event);
}

#[allow(clippy::arc_with_non_send_sync)]
pub fn use_relay_subscription_to(
    filter: Option<Filter>,
    close_opts: Option<SubscribeAutoCloseOptions>,
    relay_urls: Vec<String>,
    on_event: impl FnMut(&nostr::Event) + 'static,
) {
    let mut sub_state: Signal<Option<Arc<SubState>>> = use_signal(|| None);
    let on_event: OnEvent = Arc::new(Mutex::new(Box::new(on_event)));

    use_effect(use_reactive!(|filter, relay_urls| {
        let on_event = on_event.clone();
        spawn(async move {
            let old_sub = sub_state.peek().clone();
            sub_state.set(None);

            if let Some(old) = old_sub {
                match Arc::try_unwrap(old) {
                    Ok(s) => {
                        if let Some(handle) = s.handle {
                            handle.unregister().await;
                        } else {
                            let _ = s.client.unsubscribe(&s.sub_id).await;
                        }
                    }
                    Err(arc) => {
                        let _ = arc.client.unsubscribe(&arc.sub_id).await;
                    }
                }
            }

            let filter = match filter {
                Some(f) => f,
                None => return,
            };

            let client = match nostr_client::get_client() {
                Some(c) => c,
                None => return,
            };

            let urls: Vec<nostr::Url> = relay_urls
                .iter()
                .filter_map(|u| nostr::Url::parse(u).ok())
                .collect();

            let result = if urls.is_empty() {
                client.subscribe(filter, close_opts).await
            } else {
                client.subscribe_to(urls, filter, close_opts).await
            };

            match result {
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
        let mut buffer = Vec::new();
        while let Ok(notification) = notifications.recv().await {
            if let nostr_sdk::RelayPoolNotification::Event {
                subscription_id,
                event,
                ..
            } = notification
            {
                if subscription_id == sub_id {
                    buffer.push(event);
                    while let Ok(notification) = notifications.try_recv() {
                        if let nostr_sdk::RelayPoolNotification::Event {
                            subscription_id: sid,
                            event,
                            ..
                        } = notification
                        {
                            if sid == sub_id {
                                buffer.push(event);
                            }
                        }
                    }
                    if let Ok(mut cb) = on_event.lock() {
                        for event in &buffer {
                            cb(event);
                        }
                    }
                    buffer.clear();
                }
            }
        }
    });
}
