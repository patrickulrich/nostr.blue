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
    let parsed: Vec<nostr_sdk::EventId> = event_ids
        .iter()
        .filter_map(|id| nostr_sdk::EventId::parse(id).ok())
        .collect();
    if parsed.is_empty() {
        return;
    }
    if let Ok(new_counts) = fetch_interaction_counts_batch(parsed, Duration::from_secs(5)).await {
        let mut engaged = ENGAGED_IDS.write();
        for id in &event_ids {
            engaged.insert(id.clone());
        }
        interaction_counts.extend(new_counts);
    }
}

pub fn clear_engaged() {
    ENGAGED_IDS.write().clear();
}
