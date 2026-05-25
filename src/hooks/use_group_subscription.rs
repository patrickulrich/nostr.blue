use crate::stores::notification_dispatcher::DispatcherHandle;
use dioxus::prelude::*;
use nostr_sdk::prelude::*;
use std::sync::{Arc, Mutex};

type OnGroupEvent = Arc<Mutex<Box<dyn FnMut(&nostr::Event)>>>;

struct GroupSubState {
    sub_id: SubscriptionId,
    client: Arc<nostr_sdk::Client>,
    handle: Option<DispatcherHandle>,
}

#[allow(clippy::arc_with_non_send_sync)]
fn spawn_group_listener(
    rx: tokio::sync::mpsc::UnboundedReceiver<std::sync::Arc<nostr::Event>>,
    on_event: OnGroupEvent,
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

#[allow(clippy::arc_with_non_send_sync)]
fn spawn_group_fallback_listener(
    client: Arc<nostr_sdk::Client>,
    sub_id: SubscriptionId,
    on_event: OnGroupEvent,
) {
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

#[allow(dead_code)]
pub fn use_group_relay(relay_url: String) -> Signal<bool> {
    let mut connected = use_signal(|| false);
    use_effect(move || {
        let relay_url = relay_url.clone();
        spawn(async move {
            if let Some(client) = crate::stores::nostr_client::get_client() {
                let _ = client.add_relay(&relay_url).await;
                let _ = client.connect_relay(&relay_url).await;
                connected.set(true);
            }
        });
    });
    connected
}

#[allow(dead_code, clippy::arc_with_non_send_sync)]
pub fn use_group_subscription(
    relay_url: &str,
    group_id: &str,
    on_event: impl FnMut(&nostr::Event) + 'static,
) {
    let mut sub_state: Signal<Option<Arc<GroupSubState>>> = use_signal(|| None);
    let on_event: OnGroupEvent = Arc::new(Mutex::new(Box::new(on_event)));

    let relay_url = relay_url.to_string();
    let group_id = group_id.to_string();

    use_effect(move || {
        let on_event = on_event.clone();
        let relay_url = relay_url.clone();
        let group_id = group_id.clone();
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

            let client = match crate::stores::nostr_client::get_client() {
                Some(c) => c,
                None => return,
            };

            let _ = client.add_relay(&relay_url).await;
            let _ = client.connect_relay(&relay_url).await;

            let filter =
                crate::stores::social::group_store::group_subscription_filter(&group_id);
            let url = match RelayUrl::parse(&relay_url) {
                Ok(u) => u,
                Err(_) => return,
            };

            match client.subscribe_to(vec![url], filter, None).await {
                Ok(output) => {
                    let sub_id = output.val;

                    let handle = DispatcherHandle::create(sub_id.clone())
                        .map(|(handle, rx)| {
                            spawn_group_listener(rx, on_event.clone());
                            handle
                        });

                    if handle.is_none() {
                        spawn_group_fallback_listener(
                            client.clone(),
                            sub_id.clone(),
                            on_event.clone(),
                        );
                    }

                    sub_state.set(Some(Arc::new(GroupSubState {
                        sub_id: sub_id.clone(),
                        client: client.clone(),
                        handle,
                    })));
                }
                Err(e) => {
                    log::error!("use_group_subscription: {}", e);
                }
            }
        });
    });

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
