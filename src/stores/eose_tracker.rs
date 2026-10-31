use nostr_sdk::prelude::*;
use nostr_sdk::client::{Client};
use nostr_sdk::prelude::{RelayMessage};

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

const SINCE_BUFFER_SECS: u64 = 120;

struct EoseTrackerInner {
    eose_map: HashMap<String, u64>,
}

pub struct EoseTracker {
    inner: Arc<Mutex<EoseTrackerInner>>,
}

static EOSE_TRACKER: OnceLock<EoseTracker> = OnceLock::new();

impl EoseTracker {
    pub fn init(_client: Arc<Client>) {
        let tracker = EoseTracker {
            inner: Arc::new(Mutex::new(EoseTrackerInner {
                eose_map: HashMap::new(),
            })),
        };
        let _ = EOSE_TRACKER.set(tracker);

        let inner = EOSE_TRACKER.get().unwrap().inner.clone();

        #[cfg(not(target_arch = "wasm32"))]
        {
            std::thread::spawn(move || {
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .expect("eose tracker runtime");

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
                        loop {
                            match notifications.next().await {
                                Some(ClientNotification::Message {
                                    relay_url,
                                    message,
                                }) if matches!(message.as_ref(), RelayMessage::EndOfStoredEvents(_)) => {
                                    let now = Timestamp::now().as_secs();
                                    let mut guard = inner.lock().unwrap_or_else(|e| e.into_inner());
                                    guard
                                        .eose_map
                                        .entry(relay_url.to_string())
                                        .and_modify(|ts| {
                                            if now > *ts {
                                                *ts = now;
                                            }
                                        })
                                        .or_insert(now);
                                }
                                Some(ClientNotification::Event { relay_url, .. }) => {
                                    let now = Timestamp::now().as_secs();
                                    let mut guard = inner.lock().unwrap_or_else(|e| e.into_inner());
                                    if let Some(ts) = guard.eose_map.get_mut(relay_url.as_str()) {
                                        if now > *ts {
                                            *ts = now;
                                        }
                                    }
                                }
                                Some(ClientNotification::Shutdown) => break,
                                // Stream ended (client shut down / dropped).
                                None => {
                                    log::info!("eose_tracker: notification stream ended, re-reading client");
                                    break;
                                }
                                Some(_) => {}
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
        crate::platform::spawn::spawn_local_catch_unwind("eose_tracker", async move {
            // Outer re-read loop: the client is rebuilt on login/logout. When
            // the old client shuts down its notification stream ends; re-read
            // `get_client()` and continue with the new client.
            loop {
                let Some(client) = crate::stores::nostr_client::get_client() else {
                    crate::stores::nostr_client::platform_sleep_ms(1000).await;
                    continue;
                };
                let mut notifications = client.notifications();
                loop {
                    match notifications.next().await {
                        Some(ClientNotification::Message {
                            relay_url,
                            message,
                        }) => {
                            if !matches!(
                                message.as_ref(),
                                RelayMessage::EndOfStoredEvents(_)
                            ) {
                                continue;
                            }
                            let now = Timestamp::now().as_secs();
                            let mut guard = inner.lock().unwrap_or_else(|e| e.into_inner());
                            guard
                                .eose_map
                                .entry(relay_url.to_string())
                                .and_modify(|ts| {
                                    if now > *ts {
                                        *ts = now;
                                    }
                                })
                                .or_insert(now);
                        }
                        Some(ClientNotification::Event { relay_url, .. }) => {
                            let now = Timestamp::now().as_secs();
                            let mut guard = inner.lock().unwrap_or_else(|e| e.into_inner());
                            if let Some(ts) = guard.eose_map.get_mut(relay_url.as_str()) {
                                if now > *ts {
                                    *ts = now;
                                }
                            }
                        }
                        Some(ClientNotification::Shutdown) => break,
                        // Stream ended (client shut down / dropped).
                        None => {
                            log::info!("eose_tracker: notification stream ended, re-reading client");
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

    pub fn get_min_since() -> Option<u64> {
        EOSE_TRACKER.get().and_then(|t| {
            let map = t.inner.lock().unwrap_or_else(|e| e.into_inner());
            map.eose_map
                .values()
                .min()
                .map(|ts| ts.saturating_sub(SINCE_BUFFER_SECS))
        })
    }

    #[allow(dead_code)]
    pub fn get_since(relay_url: &str) -> Option<u64> {
        EOSE_TRACKER.get().and_then(|t| {
            t.inner
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .eose_map
                .get(relay_url)
                .map(|ts| ts.saturating_sub(SINCE_BUFFER_SECS))
        })
    }
}
