mod persistence;
pub mod processor;
pub mod signing;
pub mod types;

use dioxus::prelude::*;
use dioxus_stores::Store;
use std::collections::{HashMap, HashSet};
use types::{PublishQueueStore, PublishQueueStoreStoreExt, QueueEventStatus, QueueEventType, QueuedEvent};

pub static PUBLISH_QUEUE: GlobalSignal<Store<PublishQueueStore>> =
    Signal::global(|| Store::new(PublishQueueStore::default()));

pub async fn enqueue(
    event: nostr_sdk::Event,
    event_type: QueueEventType,
    target_relays: Option<Vec<String>>,
    metadata: HashMap<String, String>,
) -> String {
    let event_id = event.id.to_hex();
    let pubkey = event.pubkey.to_hex();
    let event_json = serde_json::to_string(&event).unwrap_or_default();
    let id = uuid::Uuid::new_v4().to_string();
    let created_at = event.created_at.as_secs();
    let is_addressable = event.kind.is_addressable();
    let do_coalesce = event.kind.is_replaceable() || is_addressable;
    let d_tag = event.tags.identifier().map(|s| s.to_string());
    let queued = QueuedEvent {
        id: id.clone(),
        event_json,
        event_type,
        event_id: event_id.clone(),
        pubkey,
        status: QueueEventStatus::Pending,
        target_relays,
        created_at,
        retry_count: 0,
        max_retries: 5,
        last_retry_at: None,
        metadata,
    };

    let action = {
        let queue = PUBLISH_QUEUE.write();
        let mut events = queue.events();
        let mut events_guard = events.write();
        if events_guard.iter().any(|e| e.event_id == event_id) {
            log::debug!("Dedup: event {} already in queue", event_id);
            return events_guard
                .iter()
                .find(|e| e.event_id == event_id)
                .map(|e| e.id.clone())
                .unwrap_or(id);
        }
        if do_coalesce {
            let mut to_remove: Option<usize> = None;
            for (i, existing) in events_guard.iter().enumerate() {
                if existing.pubkey != queued.pubkey {
                    continue;
                }
                let existing_kind = existing.kind();
                if existing_kind != Some(event.kind) {
                    continue;
                }
                if is_addressable {
                    let existing_d = existing.d_tag();
                    if existing_d != d_tag {
                        continue;
                    }
                }
                if existing.created_at <= created_at {
                    to_remove = Some(i);
                }
            }
            if let Some(idx) = to_remove {
                let old_id = events_guard[idx].id.clone();
                events_guard.remove(idx);
                drop(events_guard);
                drop(queue);
                EnqueueAction::CoalesceRemoveAndAdd { old_id, queued: Box::new(queued) }
            } else {
                events_guard.push(queued);
                drop(events_guard);
                drop(queue);
                EnqueueAction::PersistNew
            }
        } else {
            events_guard.push(queued);
            drop(events_guard);
            drop(queue);
            EnqueueAction::PersistNew
        }
    };

    match action {
        EnqueueAction::CoalesceRemoveAndAdd { old_id, queued } => {
            let _ = persistence::remove_queued_event(&old_id).await;
            let queue = PUBLISH_QUEUE.write();
            queue.events().write().push(*queued);
            drop(queue);
        }
        EnqueueAction::PersistNew => {}
    }

    let qe_clone = {
        let binding = PUBLISH_QUEUE.read().events();
        let queue_events = binding.read();
        queue_events
            .iter()
            .find(|e| e.id == id)
            .cloned()
    };
    if let Some(qe_clone) = qe_clone {
        let _ = persistence::add_queued_event(&qe_clone).await;
    }
    log::debug!("Enqueued event {} (queue id: {})", event_id, id);
    spawn(async {
        if let Err(e) = processor::process_once_guarded().await {
            log::error!("Immediate publish queue processing failed: {}", e);
        }
    });
    id
}

enum EnqueueAction {
    CoalesceRemoveAndAdd { old_id: String, queued: Box<QueuedEvent> },
    PersistNew,
}

pub async fn retry(id: &str) {
    let updated = {
        let queue = PUBLISH_QUEUE.write();
        let mut events = queue.events();
        let mut events_guard = events.write();
        if let Some(event) = events_guard.iter_mut().find(|e| e.id == id) {
            event.retry_count = 0;
            event.status = QueueEventStatus::Pending;
            event.last_retry_at = None;
            Some(event.clone())
        } else {
            None
        }
    };
    if let Some(updated) = updated {
        let _ = persistence::update_queued_event(&updated).await;
    }
}

pub async fn retry_all_failed() {
    let ids: Vec<String> = {
        let queue = PUBLISH_QUEUE.read();
        queue
            .events()
            .read()
            .iter()
            .filter(|e| {
                matches!(
                    e.status,
                    QueueEventStatus::Failed { .. }
                        | QueueEventStatus::MaxRetriesExceeded { .. }
                )
            })
            .map(|e| e.id.clone())
            .collect()
    };
    for id in ids {
        retry(&id).await;
    }
}

pub async fn abort(id: &str) {
    let updated = {
        let queue = PUBLISH_QUEUE.write();
        let mut events = queue.events();
        let mut events_guard = events.write();
        if let Some(event) = events_guard.iter_mut().find(|e| e.id == id) {
            event.status = QueueEventStatus::Aborted;
            Some(event.clone())
        } else {
            None
        }
    };
    if let Some(updated) = updated {
        let _ = persistence::update_queued_event(&updated).await;
    }
}

pub async fn clear_completed() {
    let ids: Vec<String> = {
        let queue = PUBLISH_QUEUE.read();
        queue
            .events()
            .read()
            .iter()
            .filter(|e| {
                matches!(
                    e.status,
                    QueueEventStatus::Success | QueueEventStatus::Aborted
                )
            })
            .map(|e| e.id.clone())
            .collect()
    };
    for id in ids {
        processor::deque(&id).await;
    }
}

pub fn get_pending_count() -> usize {
    let queue = PUBLISH_QUEUE.read();
    queue
        .events()
        .read()
        .iter()
        .filter(|e| {
            matches!(
                e.status,
                QueueEventStatus::Pending
                    | QueueEventStatus::Publishing
                    | QueueEventStatus::Failed { .. }
                    | QueueEventStatus::MaxRetriesExceeded { .. }
            )
        })
        .count()
}

pub async fn load_from_storage() {
    match persistence::get_all_queued_events().await {
        Ok(loaded) => {
            let queue = PUBLISH_QUEUE.write();
            let mut current = queue.events().read().clone();

            let mut loaded = loaded;
            for event in &mut loaded {
                if matches!(event.status, QueueEventStatus::Publishing) {
                    log::warn!(
                        "Resetting stuck Publishing event {} to Pending",
                        event.event_id
                    );
                    event.status = QueueEventStatus::Pending;
                }
            }

            let existing_ids: HashSet<String> =
                current.iter().map(|e| e.event_id.clone()).collect();
            for event in loaded {
                if !existing_ids.contains(&event.event_id) {
                    current.push(event);
                }
            }

            *queue.events().write() = current;
            log::info!(
                "Loaded events from storage (merged, total: {})",
                queue.events().read().len()
            );
        }
        Err(e) => {
            log::warn!("Failed to load publish queue from storage: {}", e);
        }
    }
}

pub fn get_status(queue_id: &str) -> Option<QueueEventStatus> {
    let queue = PUBLISH_QUEUE.read();
    queue
        .events()
        .read()
        .iter()
        .find(|e| e.id == queue_id)
        .map(|e| e.status.clone())
}

pub async fn enqueue_and_await(
    event: nostr_sdk::Event,
    event_type: QueueEventType,
    target_relays: Option<Vec<String>>,
    metadata: HashMap<String, String>,
) -> StdResult<String, String> {
    let event_id = event.id.to_hex();
    let queue_id = enqueue(event, event_type, target_relays, metadata).await;

    if let Err(e) = processor::process_once_guarded().await {
        return Err(format!("Publish queue processing failed: {}", e));
    }

    let mut attempts = 0u32;
    loop {
        let status = get_status(&queue_id);
        match status {
            Some(QueueEventStatus::Success) => return Ok(event_id),
            Some(QueueEventStatus::Failed { error }) => return Err(error),
            Some(QueueEventStatus::MaxRetriesExceeded { error }) => return Err(error),
            Some(QueueEventStatus::Aborted) => return Err("Publish aborted".to_string()),
            _ => {}
        }
        attempts += 1;
        if attempts >= 100 {
            return Err("Timed out waiting for event to be published to relays".to_string());
        }
        crate::stores::nostr_client::platform_sleep_ms(200).await;
    }
}

pub fn start_processor() {
    processor::start_publish_queue_processor();
    processor::resume_gossip_propagation();
}

pub fn stop_processor() {
    processor::stop_processor();
}

type StdResult<T, E> = std::result::Result<T, E>;
