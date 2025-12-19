//! Wiki Store
//! Handles NIP-54 Wiki Articles - Kind 30818
//!
//! Wiki articles feature:
//! - AsciiDoc content with wikilinks
//! - Normalized d-tag identifiers
//! - Backlinks and forward links
//! - Merge requests and redirects

#![allow(dead_code)]

use dioxus::prelude::*;
use lru::LruCache;
use nostr_sdk::prelude::*;
use nostr::Event as NostrEvent;
use std::collections::HashMap;
use std::num::NonZeroUsize;
use std::time::Duration;

type StdResult<T, E> = std::result::Result<T, E>;

use crate::utils::nip54::{
    normalize_wiki_dtag, extract_wikilinks, wikilinks_to_tags,
    WikiArticle, WikiRedirect, WikiMergeRequest,
    parse_wiki_article, parse_wiki_redirect, parse_merge_request,
    KIND_WIKI_ARTICLE, KIND_WIKI_REDIRECT, KIND_MERGE_REQUEST,
    wiki_articles_filter as build_wiki_filter,
    wiki_backlinks_filter as build_backlinks_filter,
};
use crate::utils::nkbip06::{
    generate_mime_tags, extract_mime_from_event, NostrMimeType,
    build_mime_filter_tag, CATEGORY_ARTICLE, USE_WIKI,
};

// ============================================================================
// Constants
// ============================================================================

/// Cache sizes
const WIKI_CACHE_SIZE: usize = 200;
const BACKLINKS_CACHE_SIZE: usize = 100;

// ============================================================================
// Data Structures
// ============================================================================

/// Cached wiki page with metadata
#[derive(Clone, Debug)]
pub struct CachedWikiPage {
    pub event: NostrEvent,
    pub article: WikiArticle,
    pub naddr: String,
    pub a_tag: String,
    pub forward_links: Vec<String>,
    pub backward_links: Vec<String>,
    /// NKBIP-06 MIME type information
    pub mime_type: Option<NostrMimeType>,
}

impl PartialEq for CachedWikiPage {
    fn eq(&self, other: &Self) -> bool {
        self.a_tag == other.a_tag && self.event.created_at == other.event.created_at
    }
}

/// Wiki page metadata for list displays
#[derive(Clone, Debug, PartialEq)]
pub struct WikiMetadata {
    pub identifier: String,
    pub title: String,
    pub summary: Option<String>,
    pub author_pubkey: String,
    pub created_at: u64,
    pub naddr: String,
}

/// Wiki engagement stats
#[derive(Clone, Debug, Default)]
pub struct WikiEngagement {
    pub edits: usize,
    pub backlinks: usize,
    pub views: usize, // If tracked
}

// ============================================================================
// Global Signals
// ============================================================================

/// Wiki page cache (keyed by identifier)
pub static WIKI_CACHE: GlobalSignal<LruCache<String, CachedWikiPage>> =
    GlobalSignal::new(|| LruCache::new(NonZeroUsize::new(WIKI_CACHE_SIZE).unwrap()));

/// Backlinks cache (keyed by identifier -> list of linking identifiers)
pub static BACKLINKS_CACHE: GlobalSignal<HashMap<String, Vec<String>>> =
    GlobalSignal::new(HashMap::new);

/// Redirects cache (source -> target identifier)
pub static REDIRECTS_CACHE: GlobalSignal<HashMap<String, String>> =
    GlobalSignal::new(HashMap::new);

/// Recent wiki pages for browse view
pub static RECENT_WIKI_PAGES: GlobalSignal<Vec<WikiMetadata>> =
    GlobalSignal::new(Vec::new);

/// Loading states
pub static LOADING_WIKI: GlobalSignal<bool> = GlobalSignal::new(|| false);
pub static LOADING_BACKLINKS: GlobalSignal<bool> = GlobalSignal::new(|| false);

/// Store initialization flag
pub static WIKI_STORE_INITIALIZED: GlobalSignal<bool> = GlobalSignal::new(|| false);

// ============================================================================
// Cache Functions
// ============================================================================

/// Get a wiki page from cache by identifier
pub fn get_cached_wiki_page(identifier: &str) -> Option<CachedWikiPage> {
    // Use identifier as-is since cache keys are the actual d-tags from events
    WIKI_CACHE.read().peek(identifier).cloned()
}

/// Get a wiki page by naddr
pub fn get_cached_wiki_page_by_naddr(naddr: &str) -> Option<CachedWikiPage> {
    let cache = WIKI_CACHE.read();
    cache.iter().find(|(_, p)| p.naddr == naddr).map(|(_, p)| p.clone())
}

/// Cache a wiki page
pub fn cache_wiki_page(page: CachedWikiPage) {
    let identifier = page.article.identifier.clone();
    WIKI_CACHE.write().put(identifier, page);
}

/// Cache multiple wiki pages
pub fn cache_wiki_pages(pages: &[CachedWikiPage]) {
    let mut cache = WIKI_CACHE.write();
    for page in pages {
        cache.put(page.article.identifier.clone(), page.clone());
    }
}

/// Get backlinks for an identifier
pub fn get_cached_backlinks(identifier: &str) -> Vec<String> {
    let normalized = normalize_wiki_dtag(identifier);
    BACKLINKS_CACHE.read().get(&normalized).cloned().unwrap_or_default()
}

/// Cache backlinks for an identifier
pub fn cache_backlinks(identifier: &str, backlinks: Vec<String>) {
    let normalized = normalize_wiki_dtag(identifier);
    BACKLINKS_CACHE.write().insert(normalized, backlinks);
}

/// Get redirect target for an identifier
pub fn get_redirect_target(identifier: &str) -> Option<String> {
    let normalized = normalize_wiki_dtag(identifier);
    REDIRECTS_CACHE.read().get(&normalized).cloned()
}

/// Cache a redirect
pub fn cache_redirect(source: &str, target: &str) {
    let source_normalized = normalize_wiki_dtag(source);
    let target_normalized = normalize_wiki_dtag(target);
    REDIRECTS_CACHE.write().insert(source_normalized, target_normalized);
}

/// Get all cached wiki pages
pub fn get_all_cached_wiki_pages() -> Vec<CachedWikiPage> {
    let cache = WIKI_CACHE.read();
    cache.iter().map(|(_, p)| p.clone()).collect()
}

/// Clear all caches
pub fn clear_cache() {
    WIKI_CACHE.write().clear();
    BACKLINKS_CACHE.write().clear();
    REDIRECTS_CACHE.write().clear();
    *RECENT_WIKI_PAGES.write() = Vec::new();
    *WIKI_STORE_INITIALIZED.write() = false;
}

// ============================================================================
// Parsing Functions
// ============================================================================

/// Parse an event into a CachedWikiPage
pub fn parse_wiki_page_event(event: &NostrEvent) -> Option<CachedWikiPage> {
    let article = parse_wiki_article(event).ok()?;

    let naddr = Coordinate::new(Kind::Custom(KIND_WIKI_ARTICLE), event.pubkey)
        .identifier(&article.identifier)
        .to_bech32()
        .ok()?;

    let a_tag = format!("{}:{}:{}", KIND_WIKI_ARTICLE, event.pubkey.to_hex(), article.identifier);

    // Extract forward links from content
    let forward_links: Vec<String> = extract_wikilinks(&event.content)
        .iter()
        .map(|link| normalize_wiki_dtag(&link.target))
        .collect();

    // Extract NKBIP-06 MIME type information
    let mime_type = extract_mime_from_event(event);

    Some(CachedWikiPage {
        event: event.clone(),
        article,
        naddr,
        a_tag,
        forward_links,
        backward_links: vec![], // Populated separately
        mime_type,
    })
}

/// Extract WikiMetadata from CachedWikiPage
pub fn extract_wiki_metadata(page: &CachedWikiPage) -> WikiMetadata {
    WikiMetadata {
        identifier: page.article.identifier.clone(),
        title: page.article.title.clone(),
        summary: page.article.summary.clone(),
        author_pubkey: page.event.pubkey.to_hex(),
        created_at: page.event.created_at.as_secs(),
        naddr: page.naddr.clone(),
    }
}

// ============================================================================
// Filter Builders
// ============================================================================

/// Build filter for wiki pages
pub fn wiki_pages_filter(limit: usize) -> Filter {
    build_wiki_filter(limit)
}

/// Build filter for wiki pages with pagination
pub fn wiki_pages_filter_paginated(limit: usize, until: Option<u64>) -> Filter {
    let mut filter = Filter::new()
        .kind(Kind::Custom(KIND_WIKI_ARTICLE))
        .limit(limit);

    if let Some(ts) = until {
        filter = filter.until(Timestamp::from(ts));
    }

    filter
}

/// Build filter for wiki pages by author
pub fn wiki_pages_by_author_filter(pubkey: PublicKey, limit: usize) -> Filter {
    Filter::new()
        .kind(Kind::Custom(KIND_WIKI_ARTICLE))
        .author(pubkey)
        .limit(limit)
}

/// Build filter for a specific wiki page
/// Note: We don't normalize here because the identifier should already be
/// the actual d-tag from the event. Normalizing would break lookups for
/// identifiers containing numbers (e.g., "nkbip-01" would become "nkbip").
pub fn wiki_page_by_identifier_filter(identifier: &str) -> Filter {
    Filter::new()
        .kind(Kind::Custom(KIND_WIKI_ARTICLE))
        .identifier(identifier)
}

/// Build filter for wiki page by author and identifier
pub fn wiki_page_by_coord_filter(pubkey: PublicKey, identifier: &str) -> Filter {
    Filter::new()
        .kind(Kind::Custom(KIND_WIKI_ARTICLE))
        .author(pubkey)
        .identifier(identifier)
}

/// Build filter for backlinks (pages linking to target)
pub fn wiki_backlinks_filter(identifier: &str, limit: usize) -> Filter {
    build_backlinks_filter(identifier, limit)
}

/// Build filter for wiki redirects
pub fn wiki_redirects_filter(limit: usize) -> Filter {
    Filter::new()
        .kind(Kind::Custom(KIND_WIKI_REDIRECT))
        .limit(limit)
}

/// Build filter for a specific redirect
pub fn wiki_redirect_filter(identifier: &str) -> Filter {
    let normalized = normalize_wiki_dtag(identifier);
    Filter::new()
        .kind(Kind::Custom(KIND_WIKI_REDIRECT))
        .identifier(&normalized)
}

/// Build filter for merge requests for a wiki page
pub fn wiki_merge_requests_filter(a_tag: &str, limit: usize) -> Filter {
    Filter::new()
        .kind(Kind::Custom(KIND_MERGE_REQUEST))
        .custom_tag(SingleLetterTag::lowercase(Alphabet::A), a_tag)
        .limit(limit)
}

/// Build filter for wiki page reactions
pub fn wiki_reactions_filter(a_tag: &str, limit: usize) -> Filter {
    Filter::new()
        .kind(Kind::Reaction)
        .custom_tag(SingleLetterTag::lowercase(Alphabet::A), a_tag)
        .limit(limit)
}

/// Build filter for wiki pages by MIME type (NKBIP-06)
///
/// Filters wiki articles by their M tag (Nostr MIME type).
/// Wiki articles have MIME type "article/wiki/replaceable"
pub fn wiki_pages_by_mime_filter(limit: usize) -> Filter {
    let m_tag_value = build_mime_filter_tag(CATEGORY_ARTICLE, USE_WIKI, true);
    Filter::new()
        .kind(Kind::Custom(KIND_WIKI_ARTICLE))
        .custom_tag(SingleLetterTag::uppercase(Alphabet::M), &m_tag_value)
        .limit(limit)
}

/// Build filter for wiki pages by custom MIME category
pub fn wiki_pages_by_mime_category_filter(category: &str, use_case: &str, limit: usize) -> Filter {
    let m_tag_value = build_mime_filter_tag(category, use_case, true);
    Filter::new()
        .kind(Kind::Custom(KIND_WIKI_ARTICLE))
        .custom_tag(SingleLetterTag::uppercase(Alphabet::M), &m_tag_value)
        .limit(limit)
}

// ============================================================================
// Fetch Functions
// ============================================================================

/// Fetch wiki pages with pagination
pub async fn fetch_wiki_pages(limit: usize, until: Option<u64>) -> StdResult<Vec<CachedWikiPage>, String> {
    *LOADING_WIKI.write() = true;

    let filter = wiki_pages_filter_paginated(limit, until);
    let result = crate::stores::nostr_client::fetch_events_aggregated(
        filter,
        Duration::from_secs(15),
    ).await;

    *LOADING_WIKI.write() = false;

    match result {
        Ok(events) => {
            let pages: Vec<CachedWikiPage> = events
                .iter()
                .filter_map(|e| parse_wiki_page_event(e))
                .collect();

            cache_wiki_pages(&pages);

            // Update recent pages
            let metadata: Vec<WikiMetadata> = pages.iter()
                .map(extract_wiki_metadata)
                .collect();
            *RECENT_WIKI_PAGES.write() = metadata;

            *WIKI_STORE_INITIALIZED.write() = true;

            log::info!("Fetched {} wiki pages", pages.len());
            Ok(pages)
        }
        Err(e) => {
            log::error!("Failed to fetch wiki pages: {}", e);
            Err(e)
        }
    }
}

/// Fetch a wiki page by identifier
pub fn fetch_wiki_page_by_identifier(identifier: &str) -> std::pin::Pin<Box<dyn std::future::Future<Output = StdResult<Option<CachedWikiPage>, String>> + '_>> {
    Box::pin(async move {
        // Check cache first
        if let Some(page) = get_cached_wiki_page(identifier) {
            return Ok(Some(page));
        }

        // Check for redirect
        if let Some(target) = get_redirect_target(identifier) {
            return fetch_wiki_page_by_identifier(&target).await;
        }

    let filter = wiki_page_by_identifier_filter(identifier);

    let result = crate::stores::nostr_client::fetch_events_aggregated(
        filter,
        Duration::from_secs(10),
    ).await;

    match result {
        Ok(events) => {
            // Get the most recent version
            if let Some(event) = events.iter().max_by_key(|e| e.created_at) {
                if let Some(page) = parse_wiki_page_event(event) {
                    cache_wiki_page(page.clone());
                    return Ok(Some(page));
                }
            }
            Ok(None)
        }
        Err(e) => {
            log::error!("Failed to fetch wiki page: {}", e);
            Err(e)
        }
    }
    })
}

/// Fetch a wiki page by naddr
pub async fn fetch_wiki_page_by_naddr(naddr: &str) -> StdResult<Option<CachedWikiPage>, String> {
    // Check cache first
    if let Some(page) = get_cached_wiki_page_by_naddr(naddr) {
        return Ok(Some(page));
    }

    // Parse naddr
    let coord = Coordinate::from_bech32(naddr)
        .map_err(|e| format!("Invalid naddr: {}", e))?;

    let identifier = coord.identifier;
    if identifier.is_empty() {
        return Err("No identifier in naddr".to_string());
    }

    let filter = wiki_page_by_coord_filter(coord.public_key, &identifier);

    let result = crate::stores::nostr_client::fetch_events_aggregated(
        filter,
        Duration::from_secs(10),
    ).await;

    match result {
        Ok(events) => {
            if let Some(event) = events.first() {
                if let Some(page) = parse_wiki_page_event(event) {
                    cache_wiki_page(page.clone());
                    return Ok(Some(page));
                }
            }
            Ok(None)
        }
        Err(e) => Err(e),
    }
}

/// Fetch backlinks for a wiki page
pub async fn fetch_wiki_backlinks(identifier: &str, limit: usize) -> StdResult<Vec<CachedWikiPage>, String> {
    *LOADING_BACKLINKS.write() = true;

    let filter = wiki_backlinks_filter(identifier, limit);

    let result = crate::stores::nostr_client::fetch_events_aggregated(
        filter,
        Duration::from_secs(10),
    ).await;

    *LOADING_BACKLINKS.write() = false;

    match result {
        Ok(events) => {
            let pages: Vec<CachedWikiPage> = events
                .iter()
                .filter_map(|e| parse_wiki_page_event(e))
                .collect();

            // Cache backlinks
            let backlink_ids: Vec<String> = pages.iter()
                .map(|p| p.article.identifier.clone())
                .collect();
            cache_backlinks(identifier, backlink_ids);

            cache_wiki_pages(&pages);

            log::info!("Fetched {} backlinks for {}", pages.len(), identifier);
            Ok(pages)
        }
        Err(e) => {
            log::error!("Failed to fetch backlinks: {}", e);
            Err(e)
        }
    }
}

/// Fetch wiki pages by author
pub async fn fetch_wiki_pages_by_author(pubkey_hex: &str, limit: usize) -> StdResult<Vec<CachedWikiPage>, String> {
    let pubkey = PublicKey::from_hex(pubkey_hex)
        .map_err(|e| format!("Invalid pubkey: {}", e))?;

    let filter = wiki_pages_by_author_filter(pubkey, limit);

    let result = crate::stores::nostr_client::fetch_events_aggregated(
        filter,
        Duration::from_secs(10),
    ).await;

    match result {
        Ok(events) => {
            let pages: Vec<CachedWikiPage> = events
                .iter()
                .filter_map(|e| parse_wiki_page_event(e))
                .collect();

            cache_wiki_pages(&pages);
            Ok(pages)
        }
        Err(e) => Err(e),
    }
}

/// Fetch redirects and cache them
pub async fn fetch_wiki_redirects(limit: usize) -> StdResult<Vec<WikiRedirect>, String> {
    let filter = wiki_redirects_filter(limit);

    let result = crate::stores::nostr_client::fetch_events_aggregated(
        filter,
        Duration::from_secs(10),
    ).await;

    match result {
        Ok(events) => {
            let redirects: Vec<WikiRedirect> = events
                .iter()
                .filter_map(|e| parse_wiki_redirect(e).ok())
                .collect();

            // Cache redirects
            for redirect in &redirects {
                cache_redirect(&redirect.from, &redirect.to);
            }

            Ok(redirects)
        }
        Err(e) => Err(e),
    }
}

/// Fetch merge requests for a wiki page
pub async fn fetch_wiki_merge_requests(a_tag: &str, limit: usize) -> StdResult<Vec<WikiMergeRequest>, String> {
    let filter = wiki_merge_requests_filter(a_tag, limit);

    let result = crate::stores::nostr_client::fetch_events_aggregated(
        filter,
        Duration::from_secs(10),
    ).await;

    match result {
        Ok(events) => {
            let merge_requests: Vec<WikiMergeRequest> = events
                .iter()
                .filter_map(|e| parse_merge_request(e).ok())
                .collect();

            log::info!("Fetched {} merge requests for {}", merge_requests.len(), a_tag);
            Ok(merge_requests)
        }
        Err(e) => {
            log::error!("Failed to fetch merge requests: {}", e);
            Err(e)
        }
    }
}

/// Search wiki pages by title, content, or identifier
pub async fn search_wiki_pages(query: &str, limit: usize) -> StdResult<Vec<CachedWikiPage>, String> {
    let query_lower = query.to_lowercase();
    let query_normalized = normalize_wiki_dtag(query);

    // Fetch a batch to search through
    let pages = fetch_wiki_pages(300, None).await?;

    let matching: Vec<CachedWikiPage> = pages
        .into_iter()
        .filter(|p| {
            // Match identifier
            if p.article.identifier.contains(&query_normalized) {
                return true;
            }
            // Match title
            if p.article.title.to_lowercase().contains(&query_lower) {
                return true;
            }
            // Match summary
            if let Some(ref summary) = p.article.summary {
                if summary.to_lowercase().contains(&query_lower) {
                    return true;
                }
            }
            // Match content
            if p.event.content.to_lowercase().contains(&query_lower) {
                return true;
            }
            false
        })
        .take(limit)
        .collect();

    log::info!("Search '{}' found {} wiki pages", query, matching.len());
    Ok(matching)
}

// ============================================================================
// Publishing Functions
// ============================================================================

/// Publish a new wiki page
pub async fn publish_wiki_page(
    title: &str,
    content: &str,
    identifier: Option<&str>,
    summary: Option<&str>,
) -> StdResult<String, String> {
    let client = crate::stores::nostr_client::get_client()
        .ok_or("Client not initialized")?;

    if !*crate::stores::nostr_client::HAS_SIGNER.read() {
        return Err("No signer attached".to_string());
    }

    // Generate or use provided identifier
    let d_tag = identifier
        .map(|s| normalize_wiki_dtag(s))
        .unwrap_or_else(|| normalize_wiki_dtag(title));

    // Build tags
    let mut tags: Vec<Tag> = vec![
        Tag::identifier(&d_tag),
    ];

    // Add MIME tags
    for mime_tag in generate_mime_tags(KIND_WIKI_ARTICLE) {
        tags.push(mime_tag);
    }

    // Add title if different from d-tag
    let title_from_dtag = d_tag.replace('-', " ");
    if title.to_lowercase() != title_from_dtag {
        tags.push(Tag::title(title));
    }

    // Add summary
    if let Some(s) = summary {
        tags.push(Tag::custom(TagKind::Custom("summary".into()), vec![s.to_string()]));
    }

    // Extract and add wikilinks as w tags for backlink discovery
    let wikilinks = extract_wikilinks(content);
    tags.extend(wikilinks_to_tags(&wikilinks));

    // Build and publish
    let builder = EventBuilder::new(Kind::Custom(KIND_WIKI_ARTICLE), content)
        .tags(tags);

    let output = client.send_event_builder(builder).await
        .map_err(|e| format!("Failed to publish wiki page: {}", e))?;

    log::info!("Wiki page published: {}", output.id().to_hex());
    Ok(output.id().to_hex())
}

/// Fork a wiki page (create new version)
pub async fn fork_wiki_page(
    original: &CachedWikiPage,
    new_content: &str,
    summary: Option<&str>,
) -> StdResult<String, String> {
    let client = crate::stores::nostr_client::get_client()
        .ok_or("Client not initialized")?;

    if !*crate::stores::nostr_client::HAS_SIGNER.read() {
        return Err("No signer attached".to_string());
    }

    // Keep same identifier
    let d_tag = &original.article.identifier;

    // Build tags
    let mut tags: Vec<Tag> = vec![
        Tag::identifier(d_tag),
    ];

    // Add MIME tags
    for mime_tag in generate_mime_tags(KIND_WIKI_ARTICLE) {
        tags.push(mime_tag);
    }

    // Preserve title
    if !original.article.title.is_empty() {
        tags.push(Tag::title(&original.article.title));
    }

    // Add summary
    if let Some(s) = summary {
        tags.push(Tag::custom(TagKind::Custom("summary".into()), vec![s.to_string()]));
    }

    // Add fork reference
    tags.push(Tag::custom(
        TagKind::Custom("a".into()),
        vec![original.a_tag.clone(), "".to_string(), "fork".to_string()],
    ));
    tags.push(Tag::public_key(original.event.pubkey));

    // Extract and add wikilinks as w tags for backlink discovery
    let wikilinks = extract_wikilinks(new_content);
    tags.extend(wikilinks_to_tags(&wikilinks));

    // Build and publish
    let builder = EventBuilder::new(Kind::Custom(KIND_WIKI_ARTICLE), new_content)
        .tags(tags);

    let output = client.send_event_builder(builder).await
        .map_err(|e| format!("Failed to fork wiki page: {}", e))?;

    log::info!("Wiki page forked: {}", output.id().to_hex());
    Ok(output.id().to_hex())
}

/// Publish a wiki redirect
pub async fn publish_wiki_redirect(
    source_identifier: &str,
    target_identifier: &str,
) -> StdResult<String, String> {
    let client = crate::stores::nostr_client::get_client()
        .ok_or("Client not initialized")?;

    if !*crate::stores::nostr_client::HAS_SIGNER.read() {
        return Err("No signer attached".to_string());
    }

    let source_normalized = normalize_wiki_dtag(source_identifier);
    let target_normalized = normalize_wiki_dtag(target_identifier);

    let tags: Vec<Tag> = vec![
        Tag::identifier(&source_normalized),
        Tag::custom(
            TagKind::Custom("w".into()),
            vec![target_normalized.clone()],
        ),
    ];

    let builder = EventBuilder::new(Kind::Custom(KIND_WIKI_REDIRECT), "")
        .tags(tags);

    let output = client.send_event_builder(builder).await
        .map_err(|e| format!("Failed to publish redirect: {}", e))?;

    // Cache the redirect
    cache_redirect(&source_normalized, &target_normalized);

    log::info!("Wiki redirect published: {} -> {}", source_normalized, target_normalized);
    Ok(output.id().to_hex())
}

/// Publish a merge request for a wiki page
pub async fn publish_merge_request(
    target_a_tag: &str,
    target_pubkey_hex: &str,
    source_event_id: &str,
    reason: Option<&str>,
) -> StdResult<String, String> {
    let client = crate::stores::nostr_client::get_client()
        .ok_or("Client not initialized")?;

    if !*crate::stores::nostr_client::HAS_SIGNER.read() {
        return Err("No signer attached".to_string());
    }

    let target_pubkey = PublicKey::from_hex(target_pubkey_hex)
        .map_err(|e| format!("Invalid target pubkey: {}", e))?;

    let source_id = EventId::from_hex(source_event_id)
        .map_err(|e| format!("Invalid source event ID: {}", e))?;

    let tags: Vec<Tag> = vec![
        Tag::custom(TagKind::Custom("a".into()), vec![target_a_tag.to_string()]),
        Tag::event(source_id),
        Tag::public_key(target_pubkey),
    ];

    let content = reason.unwrap_or("");

    let builder = EventBuilder::new(Kind::Custom(KIND_MERGE_REQUEST), content)
        .tags(tags);

    let output = client.send_event_builder(builder).await
        .map_err(|e| format!("Failed to publish merge request: {}", e))?;

    log::info!("Merge request published: {}", output.id().to_hex());
    Ok(output.id().to_hex())
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wiki_metadata_title_from_identifier() {
        // Create a mock CachedWikiPage would require more setup
        // Just test the title conversion logic
        let identifier = "hello-world";
        let title: String = identifier
            .replace('-', " ")
            .split_whitespace()
            .map(|word| {
                let mut chars = word.chars();
                match chars.next() {
                    None => String::new(),
                    Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
                }
            })
            .collect::<Vec<_>>()
            .join(" ");
        assert_eq!(title, "Hello World");
    }

    #[test]
    fn test_normalize_for_filter() {
        let input = "Hello World!";
        let normalized = normalize_wiki_dtag(input);
        assert_eq!(normalized, "hello-world");
    }
}
