use super::engagement_fetch::{self, EngagementData};
use super::query_parser::{self, SearchType};
use super::search_relays::get_connected_search_relays;
use crate::stores::nostr_client::{ensure_relays_ready, NOSTR_CLIENT};
use crate::utils::video_kinds::all_video_kinds;
use dioxus::prelude::ReadableExt;
use nostr_sdk::prelude::*;
use std::time::Duration;

#[derive(Clone, Debug)]
pub struct ContentSearchResult {
    pub event: Event,
    pub is_from_contact: bool,
    pub relevance: u32,
    pub engagement: Option<EngagementData>,
}

impl PartialEq for ContentSearchResult {
    fn eq(&self, other: &Self) -> bool {
        self.event.id == other.event.id
    }
}
impl Eq for ContentSearchResult {}

#[allow(dead_code)]
pub async fn search_content(
    query: &str,
    limit: usize,
    contact_pubkeys: &[PublicKey],
) -> std::result::Result<(Vec<ContentSearchResult>, SearchType), String> {
    if query.is_empty() {
        return Ok((Vec::new(), SearchType::FullText(Default::default())));
    }

    let search_type = query_parser::detect_search_type(query);

    match &search_type {
        SearchType::FullText(parsed) => {
            let filters = query_parser::build_search_filters(parsed, limit);
            if filters.is_empty() {
                return Ok((Vec::new(), search_type));
            }

            let client_opt = (*NOSTR_CLIENT.read()).clone();
            let client = match client_opt {
                Some(c) => c,
                None => return Err("Nostr client not initialized".to_string()),
            };
            ensure_relays_ready(&client).await;

            let search_urls = get_connected_search_relays(&client).await;
            let mut all_events: Vec<Event> = Vec::new();
            let mut seen_ids = std::collections::HashSet::new();

            for filter in filters {
                let fetch_result = if search_urls.is_empty() {
                    client
                        .fetch_events(filter, Duration::from_secs(5))
                        .await
                } else {
                    client
                        .fetch_events_from(search_urls.clone(), filter, Duration::from_secs(5))
                        .await
                };
                if let Ok(events) = fetch_result {
                    for event in events {
                        if seen_ids.insert(event.id) {
                            all_events.push(event);
                        }
                    }
                }
            }

            let text_query = &parsed.text;
            let mut results: Vec<ContentSearchResult> = all_events
                .into_iter()
                .map(|event| {
                    let is_from_contact = contact_pubkeys.contains(&event.pubkey);
                    let relevance = calculate_relevance(&event, text_query, is_from_contact);
                    ContentSearchResult {
                        event,
                        is_from_contact,
                        relevance,
                        engagement: None,
                    }
                })
                .collect();

            results = apply_client_side_filters(results, parsed);

            let event_ids: Vec<EventId> = results.iter().map(|r| r.event.id).collect();
            let engagement_map = engagement_fetch::fetch_engagement(&event_ids).await.unwrap_or_default();
            for result in &mut results {
                if let Some(data) = engagement_map.get(&result.event.id) {
                    result.engagement = Some(data.clone());
                }
            }

            log::debug!("Content search for '{}' returned {} results", query, results.len());
            Ok((results, search_type))
        }
        SearchType::Hashtag(tag) => {
            let client_opt = (*NOSTR_CLIENT.read()).clone();
            let client = match client_opt {
                Some(c) => c,
                None => return Err("Nostr client not initialized".to_string()),
            };
            ensure_relays_ready(&client).await;

            let filter = Filter::new()
                .kind(Kind::TextNote)
                .hashtag(tag.as_str())
                .limit(limit);
            let search_urls = get_connected_search_relays(&client).await;
            let fetch_result = if search_urls.is_empty() {
                client.fetch_events(filter, Duration::from_secs(5)).await
            } else {
                client
                    .fetch_events_from(search_urls, filter, Duration::from_secs(5))
                    .await
            };

            let results = process_events(fetch_result, tag, contact_pubkeys);
            Ok((results, search_type))
        }
        _ => Ok((Vec::new(), search_type)),
    }
}

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
    let mut results = process_events(fetch_result, query, contact_pubkeys);

    let event_ids: Vec<EventId> = results.iter().map(|r| r.event.id).collect();
    let engagement_map = engagement_fetch::fetch_engagement(&event_ids).await.unwrap_or_default();
    for result in &mut results {
        if let Some(data) = engagement_map.get(&result.event.id) {
            result.engagement = Some(data.clone());
        }
    }

    Ok(results)
}

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
        .kind(Kind::LongFormTextNote)
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
    Ok(process_events(fetch_result, query, contact_pubkeys))
}

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
    Ok(process_events(fetch_result, query, contact_pubkeys))
}

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
    Ok(process_events(fetch_result, query, contact_pubkeys))
}

fn process_events<E: std::fmt::Display>(
    fetch_result: std::result::Result<Events, E>,
    query: &str,
    contact_pubkeys: &[PublicKey],
) -> Vec<ContentSearchResult> {
    match fetch_result {
        Ok(events) => {
            let mut results: Vec<ContentSearchResult> = events
                .into_iter()
                .map(|event| {
                    let is_from_contact = contact_pubkeys.contains(&event.pubkey);
                    let relevance = calculate_relevance(&event, query, is_from_contact);
                    ContentSearchResult {
                        event,
                        is_from_contact,
                        relevance,
                        engagement: None,
                    }
                })
                .collect();
            results.sort_by_key(|b| std::cmp::Reverse(b.relevance));
            results
        }
        Err(e) => {
            log::error!("Search failed: {}", e);
            Vec::new()
        }
    }
}

#[allow(dead_code)]
fn apply_client_side_filters(
    results: Vec<ContentSearchResult>,
    query: &query_parser::ParsedSearchQuery,
) -> Vec<ContentSearchResult> {
    if query.exclude_terms.is_empty() {
        return results;
    }
    results
        .into_iter()
        .filter(|result| {
            let content_lower = result.event.content.to_lowercase();
            query
                .exclude_terms
                .iter()
                .all(|term| !content_lower.contains(term))
        })
        .collect()
}

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
