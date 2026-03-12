use super::search_relays::get_connected_search_relays;
use crate::stores::nostr_client::{ensure_relays_ready, NOSTR_CLIENT};
use crate::utils::video_kinds::all_video_kinds;
use dioxus::prelude::ReadableExt;
use nostr_sdk::prelude::*;
use std::time::Duration;
/// Result type for content search
#[derive(Clone, Debug)]
pub struct ContentSearchResult {
    pub event: Event,
    pub is_from_contact: bool,
    pub relevance: u32,
}
/// Equality is intentionally based solely on event.id for deduplication purposes.
/// Two results with the same event ID are considered equal even if is_from_contact
/// or relevance differ.
impl PartialEq for ContentSearchResult {
    fn eq(&self, other: &Self) -> bool {
        self.event.id == other.event.id
    }
}
impl Eq for ContentSearchResult {}
/// Search for text notes (Kind 1) using NIP-50
pub async fn search_text_notes(
    query: &str,
    limit: usize,
    contact_pubkeys: &[PublicKey],
) -> std::result::Result<Vec<ContentSearchResult>, String> {
    if query.is_empty() {
        return Ok(Vec::new());
    }
    let client_opt = (*NOSTR_CLIENT.read()).clone();
    let client = match client_opt {
        Some(c) => c,
        None => return Err("Nostr client not initialized".to_string()),
    };
    log::debug!("Searching for text notes matching: {}", query);
    ensure_relays_ready(&client).await;
    let filter = Filter::new()
        .kind(Kind::TextNote)
        .search(query)
        .limit(limit);
    let search_urls = get_connected_search_relays(&client).await;
    let fetch_result = if search_urls.is_empty() {
        client.fetch_events(filter, Duration::from_secs(5)).await
    } else {
        client
            .fetch_events_from(search_urls, filter, Duration::from_secs(5))
            .await
    };
    match fetch_result {
        Ok(events) => {
            log::debug!("Found {} text notes from relays", events.len());
            let mut results: Vec<ContentSearchResult> = events
                .into_iter()
                .map(|event| {
                    let is_from_contact = contact_pubkeys.contains(&event.pubkey);
                    let relevance = calculate_relevance(&event, query, is_from_contact);
                    ContentSearchResult {
                        event,
                        is_from_contact,
                        relevance,
                    }
                })
                .collect();
            results.sort_by(|a, b| b.relevance.cmp(&a.relevance));
            log::debug!(
                "Text note search for '{}' returned {} results",
                query,
                results.len()
            );
            Ok(results)
        }
        Err(e) => {
            log::error!("Failed to search text notes: {}", e);
            Err(format!("Failed to search text notes: {}", e))
        }
    }
}
/// Search for long-form articles (Kind 30023) using NIP-50
pub async fn search_articles(
    query: &str,
    limit: usize,
    contact_pubkeys: &[PublicKey],
) -> std::result::Result<Vec<ContentSearchResult>, String> {
    if query.is_empty() {
        return Ok(Vec::new());
    }
    let client_opt = (*NOSTR_CLIENT.read()).clone();
    let client = match client_opt {
        Some(c) => c,
        None => return Err("Nostr client not initialized".to_string()),
    };
    log::debug!("Searching for articles matching: {}", query);
    ensure_relays_ready(&client).await;
    let filter = Filter::new()
        .kind(Kind::from(30023))
        .search(query)
        .limit(limit);
    let search_urls = get_connected_search_relays(&client).await;
    let fetch_result = if search_urls.is_empty() {
        client.fetch_events(filter, Duration::from_secs(5)).await
    } else {
        client
            .fetch_events_from(search_urls, filter, Duration::from_secs(5))
            .await
    };
    match fetch_result {
        Ok(events) => {
            log::debug!("Found {} articles from relays", events.len());
            let mut results: Vec<ContentSearchResult> = events
                .into_iter()
                .map(|event| {
                    let is_from_contact = contact_pubkeys.contains(&event.pubkey);
                    let relevance = calculate_relevance(&event, query, is_from_contact);
                    ContentSearchResult {
                        event,
                        is_from_contact,
                        relevance,
                    }
                })
                .collect();
            results.sort_by(|a, b| b.relevance.cmp(&a.relevance));
            log::debug!(
                "Article search for '{}' returned {} results",
                query,
                results.len()
            );
            Ok(results)
        }
        Err(e) => {
            log::error!("Failed to search articles: {}", e);
            Err(format!("Failed to search articles: {}", e))
        }
    }
}
/// Search for photos (Kind 20 - NIP-68) using NIP-50
pub async fn search_photos(
    query: &str,
    limit: usize,
    contact_pubkeys: &[PublicKey],
) -> std::result::Result<Vec<ContentSearchResult>, String> {
    if query.is_empty() {
        return Ok(Vec::new());
    }
    let client_opt = (*NOSTR_CLIENT.read()).clone();
    let client = match client_opt {
        Some(c) => c,
        None => return Err("Nostr client not initialized".to_string()),
    };
    log::debug!("Searching for photos matching: {}", query);
    ensure_relays_ready(&client).await;
    let filter = Filter::new()
        .kind(Kind::Custom(20))
        .search(query)
        .limit(limit);
    let search_urls = get_connected_search_relays(&client).await;
    let fetch_result = if search_urls.is_empty() {
        client.fetch_events(filter, Duration::from_secs(5)).await
    } else {
        client
            .fetch_events_from(search_urls, filter, Duration::from_secs(5))
            .await
    };
    match fetch_result {
        Ok(events) => {
            log::debug!("Found {} photo events from relays", events.len());
            let mut results: Vec<ContentSearchResult> = events
                .into_iter()
                .map(|event| {
                    let is_from_contact = contact_pubkeys.contains(&event.pubkey);
                    let relevance = calculate_relevance(&event, query, is_from_contact);
                    ContentSearchResult {
                        event,
                        is_from_contact,
                        relevance,
                    }
                })
                .collect();
            results.sort_by(|a, b| b.relevance.cmp(&a.relevance));
            log::debug!(
                "Photo search for '{}' returned {} results",
                query,
                results.len()
            );
            Ok(results)
        }
        Err(e) => {
            log::error!("Failed to search photos: {}", e);
            Err(format!("Failed to search photos: {}", e))
        }
    }
}
/// Search for videos (Kind 21 & 22 - NIP-71) using NIP-50
pub async fn search_videos(
    query: &str,
    limit: usize,
    contact_pubkeys: &[PublicKey],
) -> std::result::Result<Vec<ContentSearchResult>, String> {
    if query.is_empty() {
        return Ok(Vec::new());
    }
    let client_opt = (*NOSTR_CLIENT.read()).clone();
    let client = match client_opt {
        Some(c) => c,
        None => return Err("Nostr client not initialized".to_string()),
    };
    log::debug!("Searching for videos matching: {}", query);
    ensure_relays_ready(&client).await;
    let filter = Filter::new()
        .kinds(all_video_kinds())
        .search(query)
        .limit(limit);
    let search_urls = get_connected_search_relays(&client).await;
    let fetch_result = if search_urls.is_empty() {
        client.fetch_events(filter, Duration::from_secs(5)).await
    } else {
        client
            .fetch_events_from(search_urls, filter, Duration::from_secs(5))
            .await
    };
    match fetch_result {
        Ok(events) => {
            log::debug!("Found {} video events from relays", events.len());
            let mut results: Vec<ContentSearchResult> = events
                .into_iter()
                .map(|event| {
                    let is_from_contact = contact_pubkeys.contains(&event.pubkey);
                    let relevance = calculate_relevance(&event, query, is_from_contact);
                    ContentSearchResult {
                        event,
                        is_from_contact,
                        relevance,
                    }
                })
                .collect();
            results.sort_by(|a, b| b.relevance.cmp(&a.relevance));
            log::debug!(
                "Video search for '{}' returned {} results",
                query,
                results.len()
            );
            Ok(results)
        }
        Err(e) => {
            log::error!("Failed to search videos: {}", e);
            Err(format!("Failed to search videos: {}", e))
        }
    }
}
/// Get user's contact list public keys
pub async fn get_contact_pubkeys() -> Vec<PublicKey> {
    let client_opt = (*NOSTR_CLIENT.read()).clone();
    let client = match client_opt {
        Some(c) => c,
        None => {
            log::warn!("Nostr client not initialized");
            return Vec::new();
        }
    };
    match client
        .get_contact_list_public_keys(Duration::from_secs(5))
        .await
    {
        Ok(pubkeys) => {
            log::debug!("Found {} contacts", pubkeys.len());
            pubkeys
        }
        Err(e) => {
            log::warn!("Failed to fetch contact list: {}", e);
            Vec::new()
        }
    }
}
/// Calculate relevance score for a content search result
fn calculate_relevance(event: &Event, query: &str, is_from_contact: bool) -> u32 {
    let query_lower = query.to_lowercase();
    let content_lower = event.content.to_lowercase();
    let mut relevance = 0u32;
    if is_from_contact {
        relevance += 10000;
    }
    if content_lower.contains(&query_lower) {
        relevance += 500;
    }
    if content_lower.starts_with(&query_lower) {
        relevance += 300;
    }
    let now = Timestamp::now();
    let age_seconds = now.as_secs().saturating_sub(event.created_at.as_secs());
    let age_days = age_seconds / 86400;
    if age_days < 1 {
        relevance += 200;
    } else if age_days < 7 {
        relevance += 100;
    } else if age_days < 30 {
        relevance += 50;
    }
    relevance
}
