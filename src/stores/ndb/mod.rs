#[cfg(feature = "native")]
#[allow(dead_code, clippy::type_complexity)]
pub mod commands;
#[cfg(feature = "native")]
#[allow(dead_code)]
pub mod queries;
#[cfg(feature = "native")]
#[allow(dead_code)]
pub mod subscriptions;
#[cfg(feature = "native")]
#[allow(dead_code)]
pub mod unknown_ids;
#[cfg(feature = "native")]
#[allow(dead_code, clippy::type_complexity)]
pub mod worker;

#[cfg(feature = "native")]
#[allow(unused_imports)]
pub use worker::{start_ndb_worker, stop_ndb_worker, take_event_receiver};

use nostr_ndb::NdbDatabase;
use std::sync::OnceLock;

#[cfg(feature = "native")]
use dioxus::core::spawn_forever;

#[cfg(feature = "native")]
use dioxus::prelude::*;

#[cfg(feature = "native")]
pub static NDB_LIVE_EVENTS: GlobalSignal<Vec<nostr::Event>> = Signal::global(Vec::new);

#[cfg(feature = "native")]
static RAW_NDB: OnceLock<NdbDatabase> = OnceLock::new();

#[cfg(feature = "native")]
pub fn get_ndb() -> Option<&'static NdbDatabase> {
    RAW_NDB.get()
}

#[cfg(feature = "native")]
pub fn set_ndb(db: NdbDatabase) -> Result<(), NdbDatabase> {
    RAW_NDB.set(db)
}

#[cfg(feature = "native")]
pub fn start_ndb_event_processor() {
    use crate::stores::ndb::commands::NdbEvent;

    let mut receiver = match take_event_receiver() {
        Some(rx) => rx,
        None => {
            log::warn!("NDB event receiver already taken");
            return;
        }
    };

    spawn_forever(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            while let Ok(event) = receiver.try_recv() {
                match event {
                    NdbEvent::SubscriptionUpdated { key, new_notes } => {
                        let mut new_events = Vec::new();
                        for note_data in new_notes {
                            match crate::stores::ndb::queries::note_data_to_event(&note_data) {
                                Ok(e) => {
                                    log::debug!(
                                        "NDB live event for subscription '{}': kind={}",
                                        key,
                                        note_data.kind
                                    );
                                    new_events.push(e);
                                }
                                Err(e) => {
                                    log::warn!("Failed to convert NDB note to event: {}", e);
                                }
                            }
                        }
                        if !new_events.is_empty() {
                            let mut live = NDB_LIVE_EVENTS.write();
                            live.extend(new_events);
                            if live.len() > 200 {
                                let excess = live.len() - 200;
                                live.drain(..excess);
                            }
                        }
                    }
                }
            }
        }
    });
}

#[cfg(feature = "native")]
#[allow(dead_code)]
pub fn drain_ndb_live_events() -> Vec<nostr::Event> {
    let mut live = NDB_LIVE_EVENTS.write();
    std::mem::take(&mut *live)
}
