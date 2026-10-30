use crate::services::aggregation::{
    fetch_interaction_counts_batch_micro, fetch_local_db_counts, InteractionCounts,
};
use dioxus::prelude::*;
use lru::LruCache;
use std::collections::HashSet;
use std::num::NonZeroUsize;
use std::time::Duration;

static INTERACTION_QUEUE: GlobalSignal<HashSet<String>> = Signal::global(HashSet::new);
static INTERACTION_GLOBAL_CACHE: GlobalSignal<LruCache<String, InteractionCounts>> =
    Signal::global(|| LruCache::new(NonZeroUsize::new(10_000).unwrap()));

/// v2 boot-stall plan: above this many mounted events, the processor skips
/// the RELAY backfill entirely. Large sets are feed surfaces that already
/// have their own count paths (home engagement + live interaction stream,
/// community viewer, route-level fetches); every event still gets instant
/// local-DB counts below. Small batches (threads, embeds) keep full relay
/// accuracy. This is the Amethyst model: local counting + live streams, no
/// bulk backfill for large sets.
pub const LARGE_INTERACTION_BATCH_SKIP: usize = 30;

pub fn get_global_interaction(event_id: &str) -> Option<InteractionCounts> {
    INTERACTION_GLOBAL_CACHE.read().peek(event_id).cloned()
}

pub fn enqueue_interaction_fetch(event_id: &str) {
    // v2 boot-stall plan: don't re-enqueue ids the processor already
    // resolved — feed-state resets remount every card and previously
    // re-triggered overlapping boot backfills (33→61→71 event batches
    // observed in the 98630372 smoke). `peek()` (not `read()`) so the
    // render-scope `use_hook` caller doesn't subscribe to the cache signal.
    if INTERACTION_GLOBAL_CACHE.peek().peek(event_id).is_some() {
        return;
    }
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
    let parsed: Vec<nostr::event::EventId> = ids
        .iter()
        .filter_map(|id| nostr::event::EventId::from_hex(id).ok())
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

    // v2 boot-stall plan: large mounted sets are feed surfaces with their
    // own count paths (engagement live stream, route-level fetches) —
    // bulk-fetching interactions for all of them dumps a parse+verify burst
    // onto the main thread during boot (133+95+75 events observed in the
    // 98630372 smoke). Local counts above still paint instantly; the home
    // feed's live stream converges the rest.
    if parsed.len() >= LARGE_INTERACTION_BATCH_SKIP {
        log::info!(
            "Interaction processor: {} mounted events, skipping relay backfill (local counts kept)",
            parsed.len()
        );
        return;
    }

    // Micro path (boot-bounded): the shared entry's 200x2x300 caps are the
    // safety net for accuracy surfaces (note viewer, polls, AI tools); this
    // processor fires repeatedly during boot as cards mount, so it uses the
    // tighter pagination bounds (100x2x150). Truncated counts converge via
    // the live interaction stream.
    if let Ok(counts) = fetch_interaction_counts_batch_micro(parsed, Duration::from_secs(5)).await
    {
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
