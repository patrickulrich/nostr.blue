use crate::services::aggregation::{
    fetch_interaction_counts_batch, fetch_local_db_counts, stream_interaction_counts,
    sync_interaction_counts, InteractionCounts, InteractionStreamHandle,
};
use crate::utils::FeedItem;
use dioxus::prelude::*;
use std::collections::HashMap;
use std::time::Duration;

pub async fn fetch_and_stream_interactions(
    feed_items: &[FeedItem],
    is_first_load: bool,
    mut interaction_counts: Signal<HashMap<String, InteractionCounts>>,
    request_id: Signal<u32>,
    current_id: u32,
    mut interactions_loaded: Signal<bool>,
    mut interaction_stream_handle: Signal<Option<InteractionStreamHandle>>,
) {
    let event_ids: Vec<_> = feed_items.iter().map(|item| item.event().id).collect();
    if event_ids.is_empty() {
        interactions_loaded.set(true);
        return;
    }

    let local_counts = fetch_local_db_counts(&event_ids).await;
    if !local_counts.is_empty() && *request_id.peek() == current_id {
        interaction_counts.set(local_counts);
    }

    let counts = if is_first_load {
        fetch_interaction_counts_batch(event_ids.clone(), Duration::from_secs(5)).await
    } else {
        sync_interaction_counts(event_ids.clone(), Duration::from_secs(5)).await
    };
    if *request_id.peek() != current_id {
        return;
    }
    if let Ok(counts) = counts {
        interaction_counts.set(counts);
        interactions_loaded.set(true);
        match stream_interaction_counts(event_ids.clone(), interaction_counts, Some(600)).await {
            Ok(handle) => {
                if *request_id.peek() != current_id {
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
