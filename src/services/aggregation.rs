/// Event interaction aggregation service
///
/// Provides batch fetching of interaction counts (replies, likes, reposts, zaps)
/// for multiple events in a single query. This dramatically reduces database
/// queries compared to fetching counts per-event.
///
/// # Performance Impact
/// - Before: N queries (one per event in feed)
/// - After: 1 query (batched for all events)
/// - Example: 100 notes → 99% reduction in queries (100 → 1)

use nostr_sdk::{Event, EventId, Filter, Kind, Timestamp, TagKind, SingleLetterTag, Alphabet};
use crate::stores::nostr_client::get_client;
use std::collections::HashMap;
use std::time::Duration;

/// Aggregated interaction counts for a single event
#[derive(Clone, Debug, Default, PartialEq)]
pub struct InteractionCounts {
    pub replies: usize,
    pub likes: usize,
    pub reposts: usize,
    pub zaps: usize,
    pub zap_amount_sats: u64,
}

/// Batch fetch interaction counts for multiple events
///
/// # Arguments
/// * `event_ids` - Vector of event IDs to fetch interactions for
/// * `timeout` - Query timeout duration
///
/// # Returns
/// HashMap mapping event_id (hex) to its interaction counts
///
/// # Example
/// ```
/// let event_ids = feed_events.iter().map(|e| e.id).collect();
/// let counts = fetch_interaction_counts_batch(event_ids, Duration::from_secs(5)).await?;
///
/// // Pass to NoteCard
/// NoteCard { event, counts: counts.get(&event.id.to_hex()) }
/// ```
pub async fn fetch_interaction_counts_batch(
    event_ids: Vec<EventId>,
    timeout: Duration,
) -> Result<HashMap<String, InteractionCounts>, String> {
    if event_ids.is_empty() {
        return Ok(HashMap::new());
    }

    let client = get_client().ok_or("Client not initialized")?;

    log::info!("Batch fetching interaction counts for {} events", event_ids.len());

    // Create filter for all interaction types across all events
    // This is a single query that replaces N individual queries
    let filter = Filter::new()
        .kinds(vec![
            Kind::TextNote,   // kind 1 - replies
            Kind::Reaction,   // kind 7 - likes
            Kind::Repost,     // kind 6 - reposts
            Kind::from(9735), // kind 9735 - zaps
        ])
        .events(event_ids.clone())
        .limit(event_ids.len() * 100); // Reasonable limit per event

    // Fetch all interactions in one query
    let events = client
        .fetch_events(filter, timeout)
        .await
        .map_err(|e| format!("Failed to fetch interactions: {}", e))?;

    log::info!("Fetched {} total interaction events", events.len());

    // Aggregate counts by event_id
    let mut counts_map: HashMap<String, InteractionCounts> = HashMap::new();

    // Initialize all event IDs with zero counts
    for event_id in &event_ids {
        counts_map.insert(event_id.to_hex(), InteractionCounts::default());
    }

    // Count interactions
    for event in events {
        // Get the event this interaction is referencing
        let referenced_event_id = match extract_referenced_event(&event) {
            Some(id) => id,
            None => continue,
        };

        let event_key = referenced_event_id.to_hex();

        // Get or create counts entry
        let counts = counts_map.entry(event_key).or_default();

        // Increment appropriate counter
        match event.kind {
            Kind::TextNote => counts.replies += 1,
            Kind::Reaction => counts.likes += 1,
            Kind::Repost => counts.reposts += 1,
            Kind::Custom(9735) => {
                counts.zaps += 1;
                // Extract zap amount from bolt11 tag
                if let Some(amount) = extract_zap_amount(&event) {
                    counts.zap_amount_sats += amount;
                }
            }
            _ => {}
        }
    }

    Ok(counts_map)
}

/// Extract the event ID being referenced by an interaction event
fn extract_referenced_event(event: &Event) -> Option<EventId> {
    // Check for 'e' tags (most interactions use this)
    for tag in event.tags.iter() {
        if tag.kind() == TagKind::SingleLetter(SingleLetterTag::lowercase(Alphabet::E)) {
            if let Some(content) = tag.content() {
                // Content is tab-separated: event_id\trelay_url\tmarker
                let parts: Vec<&str> = content.split('\t').collect();
                if !parts.is_empty() {
                    if let Ok(event_id) = EventId::from_hex(parts[0]) {
                        return Some(event_id);
                    }
                }
            }
        }
    }
    None
}

/// Extract zap amount in satoshis from a zap event (kind 9735)
fn extract_zap_amount(event: &Event) -> Option<u64> {
    // Look for 'bolt11' tag first
    if let Some(bolt11_tag) = event.tags.iter().find(|tag| {
        let vec = (*tag).clone().to_vec();
        vec.get(0).map(|k| k.as_str() == "bolt11").unwrap_or(false)
    }) {
        let tag_vec = bolt11_tag.clone().to_vec();
        if let Some(bolt11) = tag_vec.get(1) {
            // Parse bolt11 invoice to extract amount
            // For now, try to extract from description tag as fallback
            // Full bolt11 parsing would require additional dependency
            if let Some(amount) = parse_bolt11_amount(bolt11.as_str()) {
                return Some(amount);
            }
        }
    }

    // Fallback: check description tag for amount
    if let Some(description_tag) = event.tags.iter().find(|tag| {
        let vec = (*tag).clone().to_vec();
        vec.get(0).map(|k| k.as_str() == "description").unwrap_or(false)
    }) {
        let tag_vec = description_tag.clone().to_vec();
        if let Some(desc) = tag_vec.get(1) {
            // Description contains the zap request which has amount
            return parse_amount_from_description(desc.as_str());
        }
    }

    None
}

/// Parse amount from bolt11 invoice string
/// This is a simplified parser - a full implementation would use a bolt11 crate
fn parse_bolt11_amount(_bolt11: &str) -> Option<u64> {
    // TODO: Implement proper bolt11 parsing
    // For now, return None and rely on description parsing
    None
}

/// Parse amount from zap request description
fn parse_amount_from_description(description: &str) -> Option<u64> {
    // Try to parse the description as JSON to extract amount
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(description) {
        if let Some(amount) = json.get("amount") {
            if let Some(amount_str) = amount.as_str() {
                // Amount in description is in millisats
                if let Ok(millisats) = amount_str.parse::<u64>() {
                    return Some(millisats / 1000); // Convert to sats
                }
            } else if let Some(amount_num) = amount.as_u64() {
                return Some(amount_num / 1000); // Convert to sats
            }
        }
    }
    None
}

/// Fetch interaction counts for a time range (useful for trending/popular feeds)
///
/// This fetches all interactions in a given time period and groups by event.
/// Useful for "trending" or "popular" feeds that want to rank by recent engagement.
#[allow(dead_code)]
pub async fn fetch_trending_interactions(
    since: Timestamp,
    limit: usize,
) -> Result<HashMap<String, InteractionCounts>, String> {
    let client = get_client().ok_or("Client not initialized")?;

    log::info!("Fetching trending interactions since {}", since);

    let filter = Filter::new()
        .kinds(vec![
            Kind::TextNote,
            Kind::Reaction,
            Kind::Repost,
            Kind::from(9735),
        ])
        .since(since)
        .limit(limit);

    let events = client
        .fetch_events(filter, Duration::from_secs(10))
        .await
        .map_err(|e| format!("Failed to fetch trending interactions: {}", e))?;

    let mut counts_map: HashMap<String, InteractionCounts> = HashMap::new();

    for event in events {
        let referenced_event_id = match extract_referenced_event(&event) {
            Some(id) => id,
            None => continue,
        };

        let event_key = referenced_event_id.to_hex();
        let counts = counts_map.entry(event_key).or_default();

        match event.kind {
            Kind::TextNote => counts.replies += 1,
            Kind::Reaction => counts.likes += 1,
            Kind::Repost => counts.reposts += 1,
            Kind::Custom(9735) => {
                counts.zaps += 1;
                if let Some(amount) = extract_zap_amount(&event) {
                    counts.zap_amount_sats += amount;
                }
            }
            _ => {}
        }
    }

    Ok(counts_map)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_interaction_counts_default() {
        let counts = InteractionCounts::default();
        assert_eq!(counts.replies, 0);
        assert_eq!(counts.likes, 0);
        assert_eq!(counts.reposts, 0);
        assert_eq!(counts.zaps, 0);
        assert_eq!(counts.zap_amount_sats, 0);
    }

    #[test]
    fn test_parse_zap_description() {
        let desc = r#"{"amount":"5000","content":"Great post!"}"#;
        let amount = parse_amount_from_description(desc);
        assert_eq!(amount, Some(5)); // 5000 millisats = 5 sats
    }
}
