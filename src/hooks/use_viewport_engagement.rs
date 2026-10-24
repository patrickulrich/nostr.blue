use crate::hooks::use_global_interaction::LARGE_INTERACTION_BATCH_SKIP;
use crate::services::aggregation::{fetch_interaction_counts_batch, InteractionCounts};
use dioxus::prelude::*;
use std::collections::{HashSet, HashMap};
use std::time::Duration;

pub static ENGAGED_IDS: GlobalSignal<HashSet<String>> = Signal::global(HashSet::new);

pub async fn fetch_counts_for_visible(
    event_ids: Vec<String>,
    mut interaction_counts: Signal<HashMap<String, InteractionCounts>>,
) {
    if event_ids.is_empty() {
        return;
    }
    let parsed: Vec<nostr::event::EventId> = event_ids
        .iter()
        .filter_map(|id| nostr::event::EventId::parse(id).ok())
        .collect();
    if parsed.is_empty() {
        return;
    }
    // Mark engaged regardless of the fetch path below so the viewport
    // observer doesn't re-fire for the same visible set every tick.
    {
        let mut engaged = ENGAGED_IDS.write();
        for id in &event_ids {
            engaged.insert(id.clone());
        }
    }
    // v2 boot-stall plan: home-feed cards are covered by the engagement
    // live interaction stream — bulk-fetching a large visible set only adds
    // a boot parse+verify burst (see LARGE_INTERACTION_BATCH_SKIP).
    if parsed.len() >= LARGE_INTERACTION_BATCH_SKIP {
        log::info!(
            "Viewport engagement: {} visible events, skipping relay backfill (live stream covers)",
            parsed.len()
        );
        return;
    }
    if let Ok(new_counts) = fetch_interaction_counts_batch(parsed, Duration::from_secs(5)).await {
        interaction_counts.extend(new_counts);
    }
}

pub fn clear_engaged() {
    ENGAGED_IDS.write().clear();
}
