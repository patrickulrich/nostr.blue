use crate::stores::nostr_client;
use crate::stores::notification_dispatcher::DispatcherHandle;
use dioxus::prelude::*;
use nostr_sdk::prelude::*;
use nostr_sdk::client::Client;
use nostr_sdk::relay::SubscribeAutoCloseOptions;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

type OnEvent = Arc<Mutex<Box<dyn FnMut(&nostr::event::Event)>>>;

async fn subscribe_with_close_opts(
    client: &Client,
    filter: &Filter,
    urls: Option<Vec<RelayUrl>>,
    close_opts: Option<SubscribeAutoCloseOptions>,
) -> Result<Output<SubscriptionId>, Error> {
    let mut req = match urls {
        Some(urls) => {
            let targets: Vec<_> = urls
                .into_iter()
                .map(|u| (u, vec![filter.clone()]))
                .collect();
            client.subscribe(targets)
        }
        None => client.subscribe(filter.clone()),
    };
    if let Some(opts) = close_opts {
        req = req.close_on(opts);
    }
    req.await
}

struct SubState {
    sub_id: SubscriptionId,
    client: Arc<Client>,
    handle: Option<DispatcherHandle>,
}

#[allow(clippy::type_complexity, clippy::arc_with_non_send_sync)]
fn spawn_dispatched_listener(
    rx: tokio::sync::mpsc::UnboundedReceiver<std::sync::Arc<nostr::event::Event>>,
    on_event: OnEvent,
) {
    crate::platform::spawn::spawn_catch_unwind("relay_dispatch", async move {
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
    on_event: impl FnMut(&nostr::event::Event) + 'static,
) {
    use_relay_subscription_opts(filter, None, on_event);
}

#[allow(dead_code, clippy::arc_with_non_send_sync)]
pub fn use_relay_subscription_opts(
    filter: Option<Filter>,
    close_opts: Option<SubscribeAutoCloseOptions>,
    on_event: impl FnMut(&nostr::event::Event) + 'static,
) {
    use_relay_subscription_to(filter, close_opts, Vec::<String>::new(), on_event);
}

#[allow(clippy::arc_with_non_send_sync)]
pub fn use_relay_subscription_to(
    filter: Option<Filter>,
    close_opts: Option<SubscribeAutoCloseOptions>,
    relay_urls: Vec<String>,
    on_event: impl FnMut(&nostr::event::Event) + 'static,
) {
    let mut sub_state: Signal<Option<Arc<SubState>>> = use_signal(|| None);
    let on_event: OnEvent = Arc::new(Mutex::new(Box::new(on_event)));
    let cancelled = use_hook(|| Arc::new(AtomicBool::new(false)));
    let cancelled_drop = cancelled.clone();

    use_effect(use_reactive!(|filter, relay_urls| {
        let on_event = on_event.clone();
        let cancelled = cancelled.clone();
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

            if cancelled.load(Ordering::Relaxed) {
                return;
            }

            let filter = match filter {
                Some(f) => f,
                None => return,
            };

            let client = match nostr_client::get_client() {
                Some(c) => c,
                None => return,
            };

            let urls: Vec<RelayUrl> = relay_urls
                .iter()
                .filter_map(|u| RelayUrl::parse(u).ok())
                .collect();

            let result = if urls.is_empty() {
                subscribe_with_close_opts(&client, &filter, None, close_opts).await
            } else {
                match subscribe_with_close_opts(
                    &client,
                    &filter,
                    Some(urls.clone()),
                    close_opts,
                )
                .await
                {
                    Ok(output) => Ok(output),
                    Err(e) => {
                        // Targeted subscriptions hard-fail the ENTIRE call if
                        // any URL is not a pool member (`RelayNotFound` — see
                        // nostr-relay-pool subscribe_targeted). One unknown
                        // room-relay URL (e.g. the host's outbox on an
                        // a host's own relay) would otherwise silently kill
                        // the whole REQ. Intersect with pool membership and
                        // retry; degrade to the global pool if nothing
                        // survives.
                        let members: std::collections::HashSet<String> = client
                            .relays()
                            .all()
                            .await
                            .keys()
                            .map(|u| u.as_str().to_string())
                            .collect();
                        let member_urls: Vec<RelayUrl> = urls
                            .iter()
                            .filter(|u| members.contains(u.as_str()))
                            .cloned()
                            .collect();
                        let dropped: Vec<&str> = urls
                            .iter()
                            .filter(|u| !members.contains(u.as_str()))
                            .map(|u| u.as_str())
                            .collect();
                        if !dropped.is_empty() {
                            log::warn!(
                                "use_relay_subscription: {} not in pool ({}), retrying with {}/{} member relays",
                                dropped.join(", "),
                                e,
                                member_urls.len(),
                                urls.len()
                            );
                        }
                        if member_urls.is_empty() {
                            subscribe_with_close_opts(&client, &filter, None, close_opts).await
                        } else {
                            subscribe_with_close_opts(
                                &client,
                                &filter,
                                Some(member_urls),
                                close_opts,
                            )
                            .await
                        }
                    }
                }
            };

            if cancelled.load(Ordering::Relaxed) {
                if let Ok(output) = result {
                    let _ = client.unsubscribe(&output.value).await;
                }
                return;
            }

            match result {
                Ok(output) => {
                    let sub_id = output.value;

                    let handle = DispatcherHandle::create(sub_id.clone())
                        .map(|(handle, rx)| {
                            spawn_dispatched_listener(rx, on_event.clone());
                            handle
                        });

                    if handle.is_none() {
                        spawn_fallback_listener(client.clone(), sub_id.clone(), on_event.clone());
                    }

                    if cancelled.load(Ordering::Relaxed) {
                        if let Some(handle) = handle {
                            spawn(async move {
                                handle.unregister().await;
                            });
                        } else {
                            let _ = client.unsubscribe(&sub_id).await;
                        }
                        return;
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
        cancelled_drop.store(true, Ordering::Relaxed);
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
    crate::platform::spawn::spawn_catch_unwind("relay_fallback", async move {
        let mut notifications = client.notifications();
        loop {
            match notifications.next().await {
                Some(nostr_sdk::client::ClientNotification::Event {
                    subscription_id,
                    event,
                    ..
                }) => {
                    if subscription_id == sub_id {
                        // TODO(0.45-migration): the old broadcast-receiver
                        // `try_recv()` burst-drain was removed —
                        // `notifications()` now returns a Stream with no
                        // try_recv. The callback fires once per event instead
                        // of per drained batch.
                        if let Ok(mut cb) = on_event.lock() {
                            cb(&event);
                        }
                    }
                }
                Some(nostr_sdk::client::ClientNotification::Shutdown) => break,
                // Stream ended (client shut down / dropped).
                None => break,
                Some(_) => {}
            }
        }
    });
}
