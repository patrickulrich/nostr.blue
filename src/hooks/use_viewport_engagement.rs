use crate::hooks::use_global_interaction::LARGE_INTERACTION_BATCH_SKIP;
use dioxus::prelude::*;
use nostr_sdk::prelude::EventId;
use std::collections::HashSet;

pub static ENGAGED_IDS: GlobalSignal<HashSet<String>> = Signal::global(HashSet::new);

/// Enqueue visible event anchors for the idle-deferred coalesced count
/// flush (3c-4) instead of bulk-fetching them directly.
///
/// The old `fetch_counts_for_visible` called
/// `fetch_interaction_counts_batch` with the shared 200/page × 300-total
/// burst bounds the moment cards scrolled into view — overlapping the
/// pagination flush (three concurrent `count_fetch` spans observed; a
/// 28-anchor viewport batch pulled 292 events and stalled the loop
/// 3.9s mid-scroll, and boot viewport batches held spans open 9.8s +
/// 5.1s beside the first-load stream). Enqueuing folds the visible set
/// into the SAME trailing-debounce flush: every scroll tick defers the
/// burst further, one coalesced, pagination-capped fetch fires when
/// scrolling pauses, and local-DB counts plus the live interaction
/// stream keep the cards painted in the meantime.
///
/// Ids are marked engaged regardless of the queue state below so the
/// viewport observer doesn't re-fire for the same visible set every
/// tick; a skipped/failed anchor is not retried here (same accepted
/// tradeoff as before — the global processor + live stream cover it).
pub async fn enqueue_counts_for_visible(
    event_ids: Vec<String>,
    mut pending: Signal<HashSet<EventId>>,
    mut flush_nonce: Signal<u64>,
) {
    if event_ids.is_empty() {
        return;
    }
    let parsed: Vec<EventId> = event_ids
        .iter()
        .filter_map(|id| EventId::parse(id).ok())
        .collect();
    if parsed.is_empty() {
        return;
    }
    {
        let mut engaged = ENGAGED_IDS.write();
        for id in &event_ids {
            engaged.insert(id.clone());
        }
    }
    // v2 boot-stall plan: home-feed cards are covered by the engagement
    // live interaction stream — queueing a large visible set only defers a
    // boot parse+verify burst (see LARGE_INTERACTION_BATCH_SKIP).
    if parsed.len() >= LARGE_INTERACTION_BATCH_SKIP {
        log::info!(
            "Viewport engagement: {} visible events, skipping relay backfill (live stream covers)",
            parsed.len()
        );
        return;
    }
    let added = {
        let mut queue = pending.write();
        let before = queue.len();
        queue.extend(parsed);
        queue.len() > before
    };
    if added {
        // Re-arm the trailing debounce: the flush effect's newest spawn
        // owns the (now larger) coalesced round.
        *flush_nonce.write() += 1;
    }
}

pub fn clear_engaged() {
    ENGAGED_IDS.write().clear();
}
