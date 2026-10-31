use crate::hooks::use_stale_guard::StaleGuard;
use crate::services::aggregation::{
    fetch_interaction_counts_batch_first_load, fetch_local_db_counts, stream_interaction_counts,
    InteractionCounts, InteractionStreamHandle, COUNTS_FIRST_LOAD_BACKFILL,
};
use crate::utils::FeedItem;
use dioxus::prelude::*;
use std::collections::HashMap;
use std::time::Duration;

/// Cheap interactions path: paint local-DB counts instantly, then start
/// the live interaction stream (and, only when
/// [`COUNTS_FIRST_LOAD_BACKFILL`] is re-enabled, the bounded first-load
/// backfill before it).
///
/// This used to branch on `is_first_load` into a full negentropy counts
/// sync (`sync_interaction_counts`) for "refresh" loads — but that branch
/// was dead code until the settle pass existed: the epoch reset always
/// resets `interactions_loaded`, so every epoch spawn computed
/// `is_first_load=true`. When the settle pass became the first caller to
/// reach the sync it caused a ~12s main-thread ingest stall (every
/// reconciled event parses + verifies inline on the wasm UI thread —
/// `verify_and_cache` in the SDK relay handler). The settle pass now
/// enqueues its rendered delta into the idle-deferred count queue
/// (`pending_count_flush` in the home feed) instead of re-running this
/// pipeline.
pub async fn fetch_and_stream_interactions(
    feed_items: &[FeedItem],
    mut interaction_counts: Signal<HashMap<String, InteractionCounts>>,
    stale: StaleGuard,
    token: u64,
    mut interactions_loaded: Signal<bool>,
    mut interaction_stream_handle: Signal<Option<InteractionStreamHandle>>,
) {
    let event_ids: Vec<_> = feed_items.iter().map(|item| item.event().id).collect();
    if event_ids.is_empty() {
        interactions_loaded.set(true);
        return;
    }

    let local_counts = fetch_local_db_counts(&event_ids).await;
    if !local_counts.is_empty() && !stale.is_stale(token) {
        interaction_counts.set(local_counts);
    }

    // Phase 2 stall mitigation: the first-load bulk backfill doubles the
    // boot event burst (every fetched event parses + verifies on the main
    // thread) exactly when the stream completes and the first full-feed
    // render flushes. Local-DB counts above already paint instant
    // numbers; the accurate backfill can wait until the feed has rendered
    // and the loop is idle.
    //
    // v2 (Option B flipped): with the backfill disabled there is nothing
    // left to stagger — skipping the 3s sleep lets the live interaction
    // stream start immediately after the local counts.
    let counts = if COUNTS_FIRST_LOAD_BACKFILL {
        crate::platform::timer::sleep_ms(3000).await;
        if stale.is_stale(token) {
            log::debug!("Interaction backfill skipped: stale during stagger");
            return;
        }
        fetch_interaction_counts_batch_first_load(event_ids.clone(), Duration::from_secs(5)).await
    } else {
        Ok(HashMap::new())
    };
    if stale.is_stale(token) {
        return;
    }
    if let Ok(counts) = counts {
        // Empty result = backfill disabled (Option B) or nothing fetched:
        // keep the already-painted local-DB counts rather than wiping them.
        if !counts.is_empty() {
            interaction_counts.set(counts);
        }
        interactions_loaded.set(true);
        match stream_interaction_counts(event_ids.clone(), interaction_counts, Some(600)).await {
            Ok(handle) => {
                if stale.is_stale(token) {
                    log::debug!("Discarding stale interaction stream handle");
                    handle.unsubscribe().await;
                    return;
                }
                interaction_stream_handle.set(Some(handle));
            }
            Err(e) => {
                log::error!(
                    "Failed to start interaction stream: {} (event_count={}, cached_count={})",
                    e,
                    event_ids.len(),
                    interaction_counts.peek().len()
                );
            }
        }
    }
}
