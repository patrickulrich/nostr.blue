use super::*;

use crate::stores::nostr_client::get_client;
use crate::stores::signer::SIGNER_INFO;
use crate::utils::bolt11::parse_bolt11_amount;
use dioxus::prelude::ReadableExt;
use futures::join;
use instant::{Duration, Instant};
use nostr_relay_pool::{SyncDirection, SyncOptions};
use nostr_sdk::{Event, EventId, Filter, Kind, TagStandard, Timestamp};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

#[derive(Clone, Eq, Hash, PartialEq)]
struct Nip45CacheKey {
    relay_url: String,
    kind: u16,
}

/// NIP-45 support status for a relay
#[derive(Clone)]
struct Nip45SupportStatus {
    /// Whether relay supports COUNT
    supported: bool,
    /// When this status was recorded
    checked_at: Instant,
}

impl Nip45SupportStatus {
    fn new(supported: bool) -> Self {
        Self {
            supported,
            checked_at: Instant::now(),
        }
    }

    /// Negative results expire after 10 minutes (relay may have been updated)
    /// Positive results don't expire (once confirmed, unlikely to change)
    fn is_valid(&self) -> bool {
        if self.supported {
            true
        } else {
            self.checked_at.elapsed() < Duration::from_secs(600)
        }
    }
}

/// Cache for tracking which relays support NIP-45 COUNT
///
/// - `Nip45SupportStatus { supported: true }`: Relay supports COUNT (permanent)
/// - `Nip45SupportStatus { supported: false }`: Relay failed COUNT (TTL: 10 minutes)
/// - Not present: Unknown, needs testing
static NIP45_SUPPORT: OnceLock<Mutex<HashMap<Nip45CacheKey, Nip45SupportStatus>>> = OnceLock::new();

/// Get or initialize the NIP-45 support cache
fn get_nip45_cache() -> &'static Mutex<HashMap<Nip45CacheKey, Nip45SupportStatus>> {
    NIP45_SUPPORT.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Attempt to get COUNT from relays that support NIP-45
///
/// This is a best-effort optimization - if no relays support COUNT or
/// all COUNT requests fail, returns None and caller should fall back
/// to full event fetch.
///
/// # Arguments
/// * `event_id` - The event to count interactions for
/// * `kind` - The interaction kind to count (Reaction, Repost, etc.)
/// * `timeout` - Short timeout for COUNT request (should be quick)
///
/// # Returns
/// * `Some(count)` - COUNT succeeded on at least one relay
/// * `None` - COUNT not supported or failed on all relays
#[allow(dead_code)]
async fn try_count_from_relays(event_id: &EventId, kind: Kind, timeout: Duration) -> Option<usize> {
    let client = get_client()?;
    let filter = Filter::new().kind(kind).event(*event_id);
    let relays = client.relays().await;
    for (url, relay) in relays.iter() {
        let url_str = url.to_string();
        let cache_key = Nip45CacheKey {
            relay_url: url_str.clone(),
            kind: kind.as_u16(),
        };
        let should_try = {
            let cache = get_nip45_cache().lock().unwrap_or_else(|poisoned| {
                log::warn!("NIP-45 cache mutex was poisoned, recovering");
                poisoned.into_inner()
            });
            match cache.get(&cache_key) {
                Some(status) if status.is_valid() => status.supported,
                Some(_) => true,
                None => true,
            }
        };
        if !should_try {
            continue;
        }
        let count_timeout = Duration::from_millis(timeout.as_millis().min(2000) as u64);
        match relay.count_events(filter.clone(), count_timeout).await {
            Ok(count) => {
                {
                    let mut cache = get_nip45_cache().lock().unwrap_or_else(|poisoned| {
                        log::warn!("NIP-45 cache mutex was poisoned, recovering");
                        poisoned.into_inner()
                    });
                    cache.insert(cache_key.clone(), Nip45SupportStatus::new(true));
                }
                log::debug!("COUNT from {}: {} events", url, count);
                return Some(count);
            }
            Err(e) => {
                {
                    let mut cache = get_nip45_cache().lock().unwrap_or_else(|poisoned| {
                        log::warn!("NIP-45 cache mutex was poisoned, recovering");
                        poisoned.into_inner()
                    });
                    cache.insert(cache_key.clone(), Nip45SupportStatus::new(false));
                }
                log::debug!("COUNT failed on {}: {}", url, e);
            }
        }
    }
    None
}

/// Get interaction counts using COUNT when available, with fallback to full fetch
///
/// This is the COUNT-first strategy with silent fallback:
/// 1. Try COUNT on supporting relays (fast, low bandwidth)
/// 2. If COUNT unavailable, fall back to full event fetch
///
/// Note: COUNT only returns totals, not user's own reaction state.
/// User reaction state is determined separately via full fetch or cache.
#[allow(dead_code)]
pub async fn get_counts_with_count_fallback(
    event_id: &EventId,
    timeout: Duration,
) -> InteractionCounts {
    let mut counts = InteractionCounts::default();
    let (reactions, reposts, replies, zaps) = join!(
        try_count_from_relays(event_id, Kind::Reaction, timeout),
        try_count_from_relays(event_id, Kind::Repost, timeout),
        async {
            let (text_notes, comments) = join!(
                try_count_from_relays(event_id, Kind::TextNote, timeout),
                try_count_from_relays(event_id, Kind::Comment, timeout)
            );
            match (text_notes, comments) {
                (Some(text_notes), Some(comments)) => Some(text_notes + comments),
                _ => None,
            }
        },
        try_count_from_relays(event_id, Kind::ZapReceipt, timeout),
    );
    let mut needs_fallback = false;
    if let Some(count) = reactions {
        counts.likes = count;
    } else {
        needs_fallback = true;
    }
    if let Some(count) = reposts {
        counts.reposts = count;
    } else {
        needs_fallback = true;
    }
    if let Some(count) = replies {
        counts.replies = count;
    } else {
        needs_fallback = true;
    }
    if let Some(count) = zaps {
        counts.zaps = count;
    }
    if needs_fallback {
        log::debug!(
            "COUNT incomplete for {}, using full fetch",
            event_id.to_hex()
        );
        if let Ok(batch_counts) = fetch_interaction_counts_batch(vec![*event_id], timeout).await {
            if let Some(fetched) = batch_counts.get(&event_id.to_hex()) {
                return fetched.clone();
            }
        }
    }
    counts
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
    let (cached_counts, cache_hits, uncached_ids) = {
        let mut cache = get_counts_cache().lock().unwrap_or_else(|poisoned| {
            log::warn!("Counts cache mutex was poisoned, recovering");
            poisoned.into_inner()
        });
        let cached_counts = cache.get_batch(&event_ids);
        let cache_hits = cached_counts.len();
        let uncached_ids: Vec<EventId> = event_ids
            .iter()
            .filter(|id| !cached_counts.contains_key(&id.to_hex()))
            .cloned()
            .collect();
        (cached_counts, cache_hits, uncached_ids)
    };
    log::info!(
        "Batch fetching interaction counts for {} events ({} cache hits, {} cache misses)",
        event_ids.len(),
        cache_hits,
        uncached_ids.len()
    );
    if uncached_ids.is_empty() {
        log::info!("All counts served from cache!");
        return Ok(cached_counts);
    }
    let client = get_client().ok_or("Client not initialized")?;
    const MAX_RELAY_LIMIT: usize = 5000;
    let requested_limit = uncached_ids.len() * 200;
    let capped_limit = requested_limit.min(MAX_RELAY_LIMIT);
    let filter = Filter::new()
        .kinds(vec![
            Kind::TextNote,
            Kind::Comment,
            Kind::Reaction,
            Kind::Repost,
            Kind::ZapReceipt,
        ])
        .events(uncached_ids.clone())
        .limit(capped_limit);
    let db_events: Vec<Event> = match client.database().query(filter.clone()).await {
        Ok(events) => {
            let count = events.len();
            if count > 0 {
                log::info!("Found {} interaction events in local database", count);
            }
            events.into_iter().collect()
        }
        Err(e) => {
            log::debug!("Database query for interactions failed: {}", e);
            Vec::new()
        }
    };
    let relay_events: Vec<Event> = match client.fetch_events(filter, timeout).await {
        Ok(events) => {
            log::info!("Fetched {} interaction events from relays", events.len());
            events.into_iter().collect()
        }
        Err(e) => {
            if !db_events.is_empty() {
                log::warn!(
                    "Relay fetch failed but using {} cached events: {}",
                    db_events.len(),
                    e
                );
                Vec::new()
            } else {
                return Err(format!("Failed to fetch interactions: {}", e));
            }
        }
    };
    let mut event_map: HashMap<EventId, Event> = HashMap::new();
    for event in db_events {
        event_map.insert(event.id, event);
    }
    for event in relay_events {
        event_map.insert(event.id, event);
    }
    let events: Vec<Event> = event_map.into_values().collect();
    log::info!(
        "Processing {} total interaction events (DB + relay, deduplicated)",
        events.len()
    );
    let mut freshly_fetched: HashMap<String, InteractionCounts> = HashMap::new();
    let requested_ids: std::collections::HashSet<String> =
        uncached_ids.iter().map(|id| id.to_hex()).collect();
    for event_id in &uncached_ids {
        freshly_fetched.insert(event_id.to_hex(), InteractionCounts::default());
    }
    let current_user_pk: Option<nostr_sdk::PublicKey> = SIGNER_INFO
        .read()
        .as_ref()
        .and_then(|info| nostr_sdk::PublicKey::from_hex(&info.public_key).ok());
    let mut user_reactions_seen: std::collections::HashSet<String> =
        std::collections::HashSet::new();
    for event in events {
        let referenced_event_id = match extract_referenced_event(&event, &requested_ids) {
            Some(id) => id,
            None => continue,
        };
        let event_key = referenced_event_id.to_hex();
        let counts = freshly_fetched.entry(event_key.clone()).or_default();
        let is_current_user = current_user_pk
            .map(|pk| event.pubkey == pk)
            .unwrap_or(false);
        match event.kind {
            kind if is_reply_kind(kind) => counts.replies += 1,
            Kind::Reaction => {
                let content = event.content.trim();
                if content != "-" {
                    counts.likes += 1;
                }
                if is_current_user && !user_reactions_seen.contains(&event_key) {
                    user_reactions_seen.insert(event_key.clone());
                    if content == "-" {
                        counts.user_liked = Some(false);
                        counts.user_reaction = None;
                        counts.user_reaction_url = None;
                    } else {
                        counts.user_liked = Some(true);
                        counts.user_reaction = Some(content.to_string());
                        if content.starts_with(':') && content.ends_with(':') && content.len() > 2 {
                            let shortcode = &content[1..content.len() - 1];
                            let emoji_url = event.tags.iter().find_map(|tag| {
                                let tag_slice = tag.as_slice();
                                if tag_slice.len() >= 3
                                    && tag_slice.first().map(|s| s.as_str()) == Some("emoji")
                                    && tag_slice.get(1).map(|s| s.as_str()) == Some(shortcode)
                                {
                                    tag_slice.get(2).map(|s| s.to_string())
                                } else {
                                    None
                                }
                            });
                            counts.user_reaction_url = emoji_url;
                        } else {
                            counts.user_reaction_url = None;
                        }
                    }
                }
            }
            Kind::Repost => {
                counts.reposts += 1;
                if is_current_user {
                    counts.user_reposted = Some(true);
                    counts.user_repost_id = Some(event.id.to_hex());
                }
            }
            Kind::ZapReceipt => {
                counts.zaps += 1;
                if let Some(amount) = extract_zap_amount(&event) {
                    counts.zap_amount_sats += amount;
                }
                if let Some(sender) = extract_zap_sender(&event) {
                    if current_user_pk
                        .map(|pk| sender == pk.to_hex())
                        .unwrap_or(false)
                    {
                        counts.user_zapped = Some(true);
                    }
                }
            }
            _ => {}
        }
    }
    {
        let mut cache = get_counts_cache().lock().unwrap_or_else(|poisoned| {
            log::warn!("Counts cache mutex was poisoned, recovering");
            poisoned.into_inner()
        });
        cache.insert_batch(freshly_fetched.clone());
    }
    let mut final_counts = cached_counts;
    final_counts.extend(freshly_fetched);
    log::info!(
        "Returning {} interaction counts ({} from cache, {} freshly fetched)",
        final_counts.len(),
        cache_hits,
        uncached_ids.len()
    );
    Ok(final_counts)
}

/// Sync interaction counts using negentropy set reconciliation
///
/// This is more efficient than full fetch for subsequent refreshes:
/// - Uses negentropy to determine which events are missing locally
/// - Only fetches new events that appeared since last sync
/// - Incrementally updates cached counts without refetching everything
///
/// # When to use
/// - First load: Use `fetch_interaction_counts_batch` (no local data to reconcile)
/// - Subsequent refreshes: Use `sync_interaction_counts` (incremental updates)
///
/// # Fallback
/// If sync fails, silently falls back to full fetch behavior.
pub async fn sync_interaction_counts(
    event_ids: Vec<EventId>,
    timeout: Duration,
) -> Result<HashMap<String, InteractionCounts>, String> {
    if event_ids.is_empty() {
        return Ok(HashMap::new());
    }
    let client = get_client().ok_or("Client not initialized")?;
    let filter = Filter::new()
        .kinds(vec![
            Kind::TextNote,
            Kind::Comment,
            Kind::Reaction,
            Kind::Repost,
            Kind::ZapReceipt,
        ])
        .events(event_ids.clone());
    let sync_opts = SyncOptions::default()
        .direction(SyncDirection::Down)
        .initial_timeout(timeout);
    let sync_result = client.sync(filter.clone(), &sync_opts).await;
    match sync_result {
        Ok(output) => {
            let reconciliation = output.val;
            let new_event_count = reconciliation.received.len();
            if new_event_count == 0 {
                log::info!("Negentropy sync: no new interaction events found");
                let mut cache = get_counts_cache().lock().unwrap_or_else(|poisoned| {
                    log::warn!("Counts cache mutex was poisoned, recovering");
                    poisoned.into_inner()
                });
                return Ok(cache.get_batch(&event_ids));
            }
            log::info!(
                "Negentropy sync: {} new interaction events to process",
                new_event_count
            );
            let mut new_events = Vec::new();
            for event_id in &reconciliation.received {
                if let Ok(Some(event)) = client.database().event_by_id(event_id).await {
                    new_events.push(event);
                }
            }
            let mut result = {
                let mut cache = get_counts_cache().lock().unwrap_or_else(|poisoned| {
                    log::warn!("Counts cache mutex was poisoned, recovering");
                    poisoned.into_inner()
                });
                cache.get_batch(&event_ids)
            };
            for event_id in &event_ids {
                let hex = event_id.to_hex();
                result.entry(hex).or_insert_with(InteractionCounts::default);
            }
            let current_user_pk: Option<nostr_sdk::PublicKey> = SIGNER_INFO
                .read()
                .as_ref()
                .and_then(|info| nostr_sdk::PublicKey::from_hex(&info.public_key).ok());
            let requested_ids: std::collections::HashSet<String> =
                event_ids.iter().map(|id| id.to_hex()).collect();
            let mut user_reactions_seen: std::collections::HashSet<String> =
                std::collections::HashSet::new();
            for event in new_events {
                let referenced_event_id = match extract_referenced_event(&event, &requested_ids) {
                    Some(id) => id,
                    None => continue,
                };
                let event_key = referenced_event_id.to_hex();
                let counts = result.entry(event_key.clone()).or_default();
                let is_current_user = current_user_pk
                    .map(|pk| event.pubkey == pk)
                    .unwrap_or(false);
                match event.kind {
                    kind if is_reply_kind(kind) => counts.replies += 1,
                    Kind::Reaction => {
                        let content = event.content.trim();
                        if content != "-" {
                            counts.likes += 1;
                        }
                        if is_current_user && !user_reactions_seen.contains(&event_key) {
                            user_reactions_seen.insert(event_key.clone());
                            if content == "-" {
                                counts.user_liked = Some(false);
                                counts.user_reaction = None;
                                counts.user_reaction_url = None;
                            } else {
                                counts.user_liked = Some(true);
                                counts.user_reaction = Some(content.to_string());
                                if content.starts_with(':')
                                    && content.ends_with(':')
                                    && content.len() > 2
                                {
                                    let shortcode = &content[1..content.len() - 1];
                                    let emoji_url = event.tags.iter().find_map(|tag| {
                                        let tag_slice = tag.as_slice();
                                        if tag_slice.len() >= 3
                                            && tag_slice.first().map(|s| s.as_str())
                                                == Some("emoji")
                                            && tag_slice.get(1).map(|s| s.as_str())
                                                == Some(shortcode)
                                        {
                                            tag_slice.get(2).map(|s| s.to_string())
                                        } else {
                                            None
                                        }
                                    });
                                    counts.user_reaction_url = emoji_url;
                                } else {
                                    counts.user_reaction_url = None;
                                }
                            }
                        }
                    }
                    Kind::Repost => {
                        counts.reposts += 1;
                        if is_current_user {
                            counts.user_reposted = Some(true);
                            counts.user_repost_id = Some(event.id.to_hex());
                        }
                    }
                    Kind::ZapReceipt => {
                        counts.zaps += 1;
                        if let Some(amount) = extract_zap_amount(&event) {
                            counts.zap_amount_sats += amount;
                        }
                        if let Some(sender) = extract_zap_sender(&event) {
                            if current_user_pk
                                .map(|pk| sender == pk.to_hex())
                                .unwrap_or(false)
                            {
                                counts.user_zapped = Some(true);
                            }
                        }
                    }
                    _ => {}
                }
            }
            {
                let mut cache = get_counts_cache().lock().unwrap_or_else(|poisoned| {
                    log::warn!("Counts cache mutex was poisoned, recovering");
                    poisoned.into_inner()
                });
                cache.insert_batch(result.clone());
            }
            log::info!(
                "Negentropy sync complete: updated {} interaction counts",
                result.len()
            );
            Ok(result)
        }
        Err(e) => {
            log::debug!("Negentropy sync failed, falling back to full fetch: {}", e);
            fetch_interaction_counts_batch(event_ids, timeout).await
        }
    }
}

/// Extract the event ID being referenced by an interaction event
/// Only returns the event ID if it matches one of the requested IDs
/// If requested_ids is empty, returns the first 'e' tag found (for trending/all events)
pub(super) fn extract_referenced_event(
    event: &Event,
    requested_ids: &std::collections::HashSet<String>,
) -> Option<EventId> {
    for tag in event.tags.iter() {
        if let Some(TagStandard::Event { event_id, .. }) = tag.as_standardized() {
            if requested_ids.is_empty() {
                return Some(*event_id);
            }
            if requested_ids.contains(&event_id.to_hex()) {
                return Some(*event_id);
            }
        }
    }
    None
}

/// Extract the zap sender's pubkey from a zap receipt event (kind 9735)
///
/// Checks uppercase `P` tag first (standard), then falls back to
/// parsing the `description` tag JSON for the `pubkey` field.
pub(super) fn extract_zap_sender(event: &Event) -> Option<String> {
    // Try uppercase P tag first (zap sender)
    if let Some(sender) = event.tags.iter().find_map(|tag| {
        let slice = tag.as_slice();
        if slice.len() >= 2 && slice.first()?.as_str() == "P" {
            Some(slice.get(1)?.as_str().to_string())
        } else {
            None
        }
    }) {
        return Some(sender);
    }
    // Fall back to description tag JSON pubkey
    event.tags.iter().find_map(|tag| {
        let slice = tag.as_slice();
        if slice.first()?.as_str() == "description" {
            let zap_request_json = slice.get(1)?.as_str();
            if let Ok(zap_request) = serde_json::from_str::<serde_json::Value>(zap_request_json) {
                return zap_request
                    .get("pubkey")
                    .and_then(|p| p.as_str())
                    .map(|s| s.to_string());
            }
        }
        None
    })
}

/// Extract zap amount in satoshis from a zap event (kind 9735)
pub(super) fn extract_zap_amount(event: &Event) -> Option<u64> {
    if let Some(bolt11_tag) = event.tags.iter().find(|tag| {
        tag.as_slice()
            .first()
            .map(|k| k.as_str() == "bolt11")
            .unwrap_or(false)
    }) {
        if let Some(bolt11) = bolt11_tag.as_slice().get(1) {
            if let Some(amount) = parse_bolt11_amount(bolt11.as_str()) {
                return Some(amount);
            }
        }
    }
    if let Some(description_tag) = event.tags.iter().find(|tag| {
        tag.as_slice()
            .first()
            .map(|k| k.as_str() == "description")
            .unwrap_or(false)
    }) {
        if let Some(desc) = description_tag.as_slice().get(1) {
            return parse_amount_from_description(desc.as_str());
        }
    }
    None
}

/// Parse amount from zap request description
pub(super) fn parse_amount_from_description(description: &str) -> Option<u64> {
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(description) {
        if let Some(tags) = json.get("tags").and_then(|t| t.as_array()) {
            for tag in tags {
                if let Some(tag_vals) = tag.as_array() {
                    if tag_vals.first().and_then(|v| v.as_str()) == Some("amount") {
                        if let Some(amount_str) = tag_vals.get(1).and_then(|v| v.as_str()) {
                            if let Ok(millisats) = amount_str.parse::<u64>() {
                                return Some(millisats / 1000);
                            }
                        }
                    }
                }
            }
        }
        if let Some(amount) = json.get("amount") {
            if let Some(amount_str) = amount.as_str() {
                if let Ok(millisats) = amount_str.parse::<u64>() {
                    return Some(millisats / 1000);
                }
            } else if let Some(amount_num) = amount.as_u64() {
                return Some(amount_num / 1000);
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
            Kind::Comment,
            Kind::Reaction,
            Kind::Repost,
            Kind::ZapReceipt,
        ])
        .since(since)
        .limit(limit);
    let events = client
        .fetch_events(filter, Duration::from_secs(10))
        .await
        .map_err(|e| format!("Failed to fetch trending interactions: {}", e))?;
    let mut counts_map: HashMap<String, InteractionCounts> = HashMap::new();
    let empty_filter = std::collections::HashSet::new();
    for event in events {
        let referenced_event_id = match extract_referenced_event(&event, &empty_filter) {
            Some(id) => id,
            None => continue,
        };
        let event_key = referenced_event_id.to_hex();
        let counts = counts_map.entry(event_key).or_default();
        match event.kind {
            kind if is_reply_kind(kind) => counts.replies += 1,
            Kind::Reaction => {
                if event.content.trim() != "-" {
                    counts.likes += 1;
                }
            }
            Kind::Repost => counts.reposts += 1,
            Kind::ZapReceipt => {
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
