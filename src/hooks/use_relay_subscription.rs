use crate::stores::nostr_client;
use dioxus::prelude::*;
use nostr_sdk::prelude::*;
use nostr_sdk::{Client, RelayPoolNotification, SubscribeAutoCloseOptions, SubscriptionId};
use std::sync::{Arc, Mutex};

type OnEvent = Arc<Mutex<Box<dyn FnMut(&nostr::Event)>>>;

struct SubState {
    sub_id: SubscriptionId,
    client: Arc<Client>,
}

#[allow(clippy::type_complexity, clippy::arc_with_non_send_sync)]
fn spawn_listener(client: Arc<Client>, sub_id: SubscriptionId, on_event: OnEvent) {
    spawn(async move {
        let mut notifications = client.notifications();
        while let Ok(notification) = notifications.recv().await {
            if let RelayPoolNotification::Event {
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
                let _ = old.client.unsubscribe(&old.sub_id).await;
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
                    sub_state.set(Some(Arc::new(SubState {
                        sub_id: sub_id.clone(),
                        client: client.clone(),
                    })));
                    spawn_listener(client, sub_id, on_event);
                }
                Err(e) => {
                    log::error!("use_relay_subscription: {}", e);
                }
            }
        });
    }));

    use_drop(move || {
        if let Some(old) = sub_state() {
            spawn(async move {
                let _ = old.client.unsubscribe(&old.sub_id).await;
            });
        }
    });
}
