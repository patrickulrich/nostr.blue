use crate::services::aggregation::{
    fetch_interaction_counts_batch, fetch_local_db_counts, InteractionCounts,
};
use dioxus::prelude::*;
use lru::LruCache;
use std::collections::HashSet;
use std::num::NonZeroUsize;
use std::time::Duration;

static INTERACTION_QUEUE: GlobalSignal<HashSet<String>> = Signal::global(HashSet::new);
static INTERACTION_GLOBAL_CACHE: GlobalSignal<LruCache<String, InteractionCounts>> =
    Signal::global(|| LruCache::new(NonZeroUsize::new(10_000).unwrap()));

pub fn get_global_interaction(event_id: &str) -> Option<InteractionCounts> {
    INTERACTION_GLOBAL_CACHE.read().peek(event_id).cloned()
}

pub fn enqueue_interaction_fetch(event_id: &str) {
    let mut queue = INTERACTION_QUEUE.write();
    if !queue.contains(event_id) {
        queue.insert(event_id.to_string());
    }
}

pub fn remove_from_interaction_queue(event_id: &str) {
    INTERACTION_QUEUE.write().remove(event_id);
}

pub async fn process_interaction_queue() {
    let ids: Vec<String> = {
        let mut queue = INTERACTION_QUEUE.write();
        let ids: Vec<String> = queue.drain().collect();
        ids
    };
    if ids.is_empty() {
        return;
    }
    let parsed: Vec<nostr_sdk::EventId> = ids
        .iter()
        .filter_map(|id| nostr_sdk::EventId::from_hex(id).ok())
        .collect();
    if parsed.is_empty() {
        return;
    }

    let local_counts = fetch_local_db_counts(&parsed).await;
    if !local_counts.is_empty() {
        let mut cache = INTERACTION_GLOBAL_CACHE.write();
        for (id, counts) in local_counts {
            cache.put(id, counts);
        }
    }

    if let Ok(counts) = fetch_interaction_counts_batch(parsed, Duration::from_secs(5)).await {
        let mut cache = INTERACTION_GLOBAL_CACHE.write();
        for (id, counts) in counts {
            cache.put(id, counts);
        }
    }
}

#[component]
pub fn GlobalInteractionProcessor() -> Element {
    use_future(move || async move {
        loop {
            crate::platform::timer::sleep_ms(300).await;
            process_interaction_queue().await;
        }
    });

    rsx! {}
}

#[component]
pub fn UseGlobalInteraction(event_id: String) -> Element {
    let event_id_drop = event_id.clone();
    use_hook(move || {
        enqueue_interaction_fetch(&event_id);
    });
    use_drop(move || {
        remove_from_interaction_queue(&event_id_drop);
    });

    rsx! {}
}
