use crate::hooks::use_stale_guard::StaleGuard;
use crate::services::aggregation::{
    fetch_interaction_counts_batch, fetch_interaction_counts_batch_first_load,
    fetch_local_db_counts, stream_interaction_counts, sync_interaction_counts, InteractionCounts,
    InteractionStreamHandle, COUNTS_FIRST_LOAD_BACKFILL,
};
use crate::utils::FeedItem;
use dioxus::prelude::*;
use std::collections::HashMap;
use std::time::Duration;

pub async fn fetch_and_stream_interactions(
    feed_items: &[FeedItem],
    is_first_load: bool,
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

    let counts = if is_first_load {
        // Phase 2 stall mitigation: the first-load bulk backfill doubles
        // the boot event burst (every fetched event parses + verifies on
        // the main thread) exactly when the stream completes and the first
        // full-feed render flushes. Local-DB counts above already paint
        // instant numbers; the accurate backfill can wait until the feed
        // has rendered and the loop is idle. Refresh loads keep the
        // immediate negentropy-sync path.
        //
        // v2 (Option B flipped): with the backfill disabled there is
        // nothing left to stagger — skipping the 3s sleep lets the live
        // interaction stream start immediately after the local counts.
        if COUNTS_FIRST_LOAD_BACKFILL {
            crate::platform::timer::sleep_ms(3000).await;
            if stale.is_stale(token) {
                log::debug!("Interaction backfill skipped: stale during stagger");
                return;
            }
        }
        fetch_interaction_counts_batch_first_load(event_ids.clone(), Duration::from_secs(5))
            .await
    } else {
        sync_interaction_counts(event_ids.clone(), Duration::from_secs(5)).await
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

pub async fn fetch_paginated_interactions(
    unique_items: &[FeedItem],
    mut interaction_counts: Signal<HashMap<String, InteractionCounts>>,
) {
    let event_ids: Vec<_> = unique_items.iter().map(|item| item.event().id).collect();
    if let Ok(new_counts) = fetch_interaction_counts_batch(event_ids, Duration::from_secs(5)).await {
        interaction_counts.extend(new_counts);
        log::info!(
            "Fetched interaction counts for {} paginated items",
            unique_items.len()
        );
    }
}
