use super::engagement_fetch::{self, EngagementData};
use super::local_search;
use super::query_parser::{self, ParsedSearchQuery, SearchType};
use super::search_relays::get_connected_search_relays;
use crate::stores::nostr_client::{ensure_relays_ready, NOSTR_CLIENT};
use crate::stores::profiles::PROFILE_CACHE;
use crate::utils::debounced_collector::DebouncedCollector;
use crate::utils::video_kinds::all_video_kinds;
use dioxus::prelude::ReadableExt;
use nostr_sdk::prelude::*;
use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;
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

/// Sentinel error: no NIP-50 search relays are reachable and the query
/// requires full-text search (hashtag `#t` queries do not — they fall back to
/// the general pool). The route renders this as an actionable banner.
pub const ERR_SEARCH_RELAYS_UNREACHABLE: &str = "__search_relays_unreachable__";

/// Resolve `from:name` author references against `PROFILE_CACHE`
/// (name/display_name, case-insensitive substring).
///
/// Returns (resolved pubkeys, unresolved names) — the latter feeds the
/// "unknown author" query chip.
pub fn resolve_author_names(names: &[String]) -> (Vec<PublicKey>, Vec<String>) {
    let mut resolved = Vec::new();
    let mut unresolved = Vec::new();
    let cache = PROFILE_CACHE.read();
    for name in names {
        let needle = name.to_lowercase();
        let mut hit: Option<PublicKey> = None;
        for (pubkey_str, profile) in cache.iter() {
            let name_match = profile
                .name
                .as_ref()
                .map(|n| n.to_lowercase().contains(&needle))
                .unwrap_or(false);
            let display_match = profile
                .display_name
                .as_ref()
                .map(|d| d.to_lowercase().contains(&needle))
                .unwrap_or(false);
            if name_match || display_match {
                if let Ok(pk) = PublicKey::from_hex(pubkey_str) {
                    hit = Some(pk);
                    break;
                }
            }
        }
        match hit {
            Some(pk) => resolved.push(pk),
            None => unresolved.push(name.clone()),
        }
    }
    (resolved, unresolved)
}

/// Kinds searched per search tab (used when the query has no `kind:` op).
pub fn tab_default_kinds(tab: &str) -> Vec<Kind> {
    match tab {
        "articles" => vec![Kind::LongFormTextNote],
        "photos" => vec![Kind::Custom(20), Kind::Custom(21), Kind::Custom(22)],
        "videos" => all_video_kinds(),
        _ => vec![Kind::TextNote],
    }
}

/// Streaming content search: emits progressive batches through `on_batch`
/// as relays respond (first local-database hits, then streamed relay hits),
/// and returns the complete enriched result list plus the detected
/// `SearchType` when the stream closes.
///
/// Pipeline:
/// 1. **Local-first**: instant matches from the SDK database via the
///    tokenized matcher (`local_search`).
/// 2. **Relay leg**: operator-aware filters (`build_search_filters`) streamed
///    from the NIP-50 search relays. Hashtag queries fall back to the general
///    READ pool when search relays are unreachable (`#t` filters work
///    everywhere); full-text queries surface [`ERR_SEARCH_RELAYS_UNREACHABLE`]
///    instead of silently scanning the general pool (which ignores NIP-50 and
///    returns nothing useful).
/// 3. `-exclude` terms are applied client-side to the final list.
pub async fn search_content_streaming(
    query: &str,
    limit: usize,
    tab: &str,
    contact_pubkeys: &[PublicKey],
    on_batch: impl FnMut(Vec<ContentSearchResult>) + 'static,
) -> std::result::Result<(Vec<ContentSearchResult>, SearchType), String> {
    // `FnMut` (signal writes take `&mut self`), shared with the debounced
    // flush closures behind an `Rc<RefCell<…>>`.
    let on_batch = Rc::new(RefCell::new(on_batch));
    let emit = |batch: Vec<ContentSearchResult>| {
        if !batch.is_empty() {
            (on_batch.borrow_mut())(batch);
        }
    };
    if query.is_empty() {
        return Ok((Vec::new(), SearchType::FullText(ParsedSearchQuery::default())));
    }

    let search_type = query_parser::detect_search_type(query);
    let tab_kinds = tab_default_kinds(tab);

    match &search_type {
        SearchType::FullText(parsed) => {
            // Resolve `from:name` references into real pubkeys.
            let mut parsed = parsed.clone();
            if !parsed.author_names.is_empty() {
                let (resolved, _) = resolve_author_names(&parsed.author_names);
                for pk in resolved {
                    if !parsed.authors.contains(&pk) {
                        parsed.authors.push(pk);
                    }
                }
            }

            let filters =
                query_parser::build_search_filters(&parsed, limit, Some(tab_kinds.clone()));
            if filters.is_empty() {
                return Ok((Vec::new(), search_type));
            }

            let client_opt = (*NOSTR_CLIENT.read()).clone();
            let client = match client_opt {
                Some(c) => c,
                None => return Err("Nostr client not initialized".to_string()),
            };
            ensure_relays_ready(&client).await;

            // Shared accumulation state (the collector flushes from spawned
            // tasks, so dedup + merge live behind Rc<RefCell>).
            let seen_ids = Rc::new(RefCell::new(HashSet::<EventId>::new()));
            let all_results = Rc::new(RefCell::new(Vec::<ContentSearchResult>::new()));

            // 1. Local-first instant results.
            let local = local_search::search_local_content(&parsed, &tab_kinds, limit, contact_pubkeys).await;
            {
                let mut all = all_results.borrow_mut();
                let mut seen = seen_ids.borrow_mut();
                for result in local {
                    seen.insert(result.event.id);
                    all.push(result);
                }
            }
            if !all_results.borrow().is_empty() {
                emit(all_results.borrow().clone());
            }

            // 2. Relay leg. Only queries carrying a `search` field need
            // NIP-50; pure structured filters (kind:/from:/since:/…) work on
            // any relay.
            let needs_nip50 = filters.iter().any(|f| f.search.is_some());
            let search_urls = get_connected_search_relays(&client).await;
            if search_urls.is_empty() && needs_nip50 {
                // Surface the failure — local results were already emitted,
                // so the route can render them alongside the banner.
                return Err(ERR_SEARCH_RELAYS_UNREACHABLE.to_string());
            }

            for filter in filters {
                let stream_result = if search_urls.is_empty() {
                    client.stream_events(filter, Duration::from_secs(5)).await
                } else {
                    client
                        .stream_events_from(search_urls.clone(), filter, Duration::from_secs(5))
                        .await
                };
                match stream_result {
                    Ok(mut stream) => {
                        use futures::StreamExt;
                        let collector = DebouncedCollector::<Event>::new(120);
                        let contacts = contact_pubkeys.to_vec();
                        let text = parsed.text.clone();
                        while let Some(event) = stream.next().await {
                            let seen = seen_ids.clone();
                            let all = all_results.clone();
                            let batch_cb = on_batch.clone();
                            let contacts = contacts.clone();
                            let text = text.clone();
                            collector.extend([event], move |batch: Vec<Event>| {
                                let converted = convert_events(&batch, &seen, &contacts, &text);
                                if !converted.is_empty() {
                                    all.borrow_mut().extend(converted.iter().cloned());
                                    (batch_cb.borrow_mut())(converted);
                                }
                            });
                        }
                        // Tail: events buffered after the last debounce window.
                        let tail = collector.drain();
                        let converted = convert_events(&tail, &seen_ids, contact_pubkeys, &parsed.text);
                        if !converted.is_empty() {
                            all_results.borrow_mut().extend(converted.iter().cloned());
                            emit(converted);
                        }
                    }
                    Err(e) => {
                        log::warn!("Search stream failed: {e}");
                    }
                }
            }

            // 3. Client-side `-exclude` terms on the final list.
            let mut final_results =
                apply_client_side_filters(all_results.borrow().clone(), &parsed);
            enrich_with_engagement(&mut final_results).await;
            log::debug!(
                "Content search for '{}' returned {} results",
                query,
                final_results.len()
            );
            Ok((final_results, search_type))
        }
        SearchType::Hashtag(tag) => {
            let client_opt = (*NOSTR_CLIENT.read()).clone();
            let client = match client_opt {
                Some(c) => c,
                None => return Err("Nostr client not initialized".to_string()),
            };
            ensure_relays_ready(&client).await;

            let seen_ids = Rc::new(RefCell::new(HashSet::<EventId>::new()));
            let all_results = Rc::new(RefCell::new(Vec::<ContentSearchResult>::new()));

            // Local-first hashtag matches (`#t` is a structured filter the
            // local database handles natively).
            let local_parsed = ParsedSearchQuery {
                raw: format!("#{tag}"),
                hashtags: vec![tag.clone()],
                ..Default::default()
            };
            let local = local_search::search_local_content(&local_parsed, &tab_kinds, limit, contact_pubkeys).await;
            {
                let mut all = all_results.borrow_mut();
                let mut seen = seen_ids.borrow_mut();
                for result in local {
                    seen.insert(result.event.id);
                    all.push(result);
                }
            }
            if !all_results.borrow().is_empty() {
                emit(all_results.borrow().clone());
            }

            // `#t` filters are supported by every relay — no NIP-50 needed,
            // so the general pool is a valid fallback here.
            let filter = Filter::new()
                .kinds(tab_kinds)
                .hashtag(tag.as_str())
                .limit(limit);
            let search_urls = get_connected_search_relays(&client).await;
            let stream_result = if search_urls.is_empty() {
                client.stream_events(filter, Duration::from_secs(5)).await
            } else {
                client
                    .stream_events_from(search_urls, filter, Duration::from_secs(5))
                    .await
            };
            match stream_result {
                Ok(mut stream) => {
                    use futures::StreamExt;
                    let collector = DebouncedCollector::<Event>::new(120);
                    let contacts = contact_pubkeys.to_vec();
                    let tag_text = tag.clone();
                    while let Some(event) = stream.next().await {
                        let seen = seen_ids.clone();
                        let all = all_results.clone();
                        let batch_cb = on_batch.clone();
                        let contacts = contacts.clone();
                        let tag_text = tag_text.clone();
                        collector.extend([event], move |batch: Vec<Event>| {
                            let converted =
                                convert_events(&batch, &seen, &contacts, &tag_text);
                            if !converted.is_empty() {
                                all.borrow_mut().extend(converted.iter().cloned());
                                (batch_cb.borrow_mut())(converted);
                            }
                        });
                    }
                    let tail = collector.drain();
                    let converted = convert_events(&tail, &seen_ids, contact_pubkeys, tag);
                    if !converted.is_empty() {
                        all_results.borrow_mut().extend(converted.iter().cloned());
                        emit(converted);
                    }
                }
                Err(e) => {
                    log::warn!("Hashtag search stream failed: {e}");
                }
            }

            let mut final_results = all_results.borrow().clone();
            final_results.sort_by_key(|b| std::cmp::Reverse(b.relevance));
            enrich_with_engagement(&mut final_results).await;
            Ok((final_results, search_type))
        }
        // NIP-19 / hex lookups are redirected to their viewers by the route
        // before reaching the search pipeline.
        _ => Ok((Vec::new(), search_type)),
    }
}

/// Convert deduped raw events into scored search results.
fn convert_events(
    events: &[Event],
    seen_ids: &Rc<RefCell<HashSet<EventId>>>,
    contact_pubkeys: &[PublicKey],
    text_query: &str,
) -> Vec<ContentSearchResult> {
    let mut converted = Vec::new();
    for event in events {
        if seen_ids.borrow_mut().insert(event.id) {
            let is_from_contact = contact_pubkeys.contains(&event.pubkey);
            let relevance = calculate_relevance(event, text_query, is_from_contact);
            converted.push(ContentSearchResult {
                event: event.clone(),
                is_from_contact,
                relevance,
                engagement: None,
            });
        }
    }
    converted
}

async fn enrich_with_engagement(results: &mut [ContentSearchResult]) {
    let event_ids: Vec<EventId> = results.iter().map(|r| r.event.id).collect();
    let engagement_map = engagement_fetch::fetch_engagement(&event_ids)
        .await
        .unwrap_or_default();
    for result in results.iter_mut() {
        if let Some(data) = engagement_map.get(&result.event.id) {
            result.engagement = Some(data.clone());
        }
    }
}

fn apply_client_side_filters(
    results: Vec<ContentSearchResult>,
    query: &ParsedSearchQuery,
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

/// Search raw text notes (no operator parsing). Used by the mention dialog
/// and AI tools, which need raw-string full-text behavior.
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

/// Search raw articles (no operator parsing). Used by the mention dialog.
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
