use nostr_sdk::prelude::*;
use nostr_sdk::{Client, RelayMessage, RelayPoolNotification};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use tokio::sync::broadcast::error::RecvError;

const SINCE_BUFFER_SECS: u64 = 120;

struct EoseTrackerInner {
    eose_map: HashMap<String, u64>,
}

pub struct EoseTracker {
    inner: Arc<Mutex<EoseTrackerInner>>,
}

static EOSE_TRACKER: OnceLock<EoseTracker> = OnceLock::new();

impl EoseTracker {
    pub fn init(client: Arc<Client>) {
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
                    let mut notifications = client.notifications();
                    loop {
                        match notifications.recv().await {
                            Ok(RelayPoolNotification::Message {
                                relay_url,
                                message: RelayMessage::EndOfStoredEvents(_),
                            }) => {
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
                            Ok(RelayPoolNotification::Event { relay_url, .. }) => {
                                let now = Timestamp::now().as_secs();
                                let mut guard = inner.lock().unwrap_or_else(|e| e.into_inner());
                                if let Some(ts) = guard.eose_map.get_mut(relay_url.as_str()) {
                                    if now > *ts {
                                        *ts = now;
                                    }
                                }
                            }
                            Ok(RelayPoolNotification::Shutdown) => break,
                            // Transient: keep going, channel is still alive.
                            Err(RecvError::Lagged(skipped)) => {
                                log::warn!("eose_tracker: lagged, skipped {} events, continuing", skipped);
                                continue;
                            }
                            Err(RecvError::Closed) => {
                                log::info!("eose_tracker: notification channel closed, exiting native listener");
                                break;
                            }
                            _ => {}
                        }
                    }
                });
            });
        }

        #[cfg(target_arch = "wasm32")]
        {
        crate::platform::spawn::spawn_local_catch_unwind("eose_tracker", async move {
            let mut notifications = client.notifications();
            loop {
                match notifications.recv().await {
                    Ok(RelayPoolNotification::Message {
                        relay_url,
                        message: RelayMessage::EndOfStoredEvents(_),
                    }) => {
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
                    Ok(RelayPoolNotification::Event { relay_url, .. }) => {
                        let now = Timestamp::now().as_secs();
                        let mut guard = inner.lock().unwrap_or_else(|e| e.into_inner());
                        if let Some(ts) = guard.eose_map.get_mut(relay_url.as_str()) {
                            if now > *ts {
                                *ts = now;
                            }
                        }
                    }
                    Ok(RelayPoolNotification::Shutdown) => break,
                    // Transient: keep going, channel is still alive.
                    Err(RecvError::Lagged(skipped)) => {
                        log::warn!("eose_tracker: lagged, skipped {} events, continuing", skipped);
                        continue;
                    }
                    Err(RecvError::Closed) => {
                        log::info!("eose_tracker: notification channel closed, exiting wasm listener");
                        break;
                    }
                    _ => {}
                }
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
