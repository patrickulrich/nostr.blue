//! Publication Store
//! Handles NKBIP-01 Publications - Kind 30040 (index) + Kind 30041 (content sections)
//!
//! Publications consist of:
//! - Kind 30040: Index event with empty content, title/d tags, ordered a-tags referencing children
//! - Kind 30041: Content sections with AsciiDoc markup, title/d tags

#![allow(dead_code)]

use dioxus::prelude::*;
use lru::LruCache;
use nostr_sdk::prelude::*;
use nostr::Event as NostrEvent;
use std::collections::HashMap;
use std::num::NonZeroUsize;
use std::time::Duration;

type StdResult<T, E> = std::result::Result<T, E>;

use crate::utils::nip54::normalize_wiki_dtag;
use crate::utils::nkbip06::{
    generate_mime_tags, extract_mime_from_event, NostrMimeType,
    build_mime_filter_tag, build_mime_filter_tag_for_kind,
    generate_m_tag, generate_nostr_m_tag,
    CATEGORY_METADATA, CATEGORY_ARTICLE, CATEGORY_MEDIA, USE_INDEX, USE_PUBLICATION_CONTENT,
};
use crate::utils::nkbip08::extract_book_wikilinks;

// ============================================================================
// Constants
// ============================================================================

/// Kind 30040 - Publication Index
pub const KIND_INDEX: u16 = 30040;

/// Kind 30041 - Publication Content (Section/Chapter)
pub const KIND_CONTENT: u16 = 30041;

/// Cache sizes
const PUBLICATION_CACHE_SIZE: usize = 100;
const SECTION_CACHE_SIZE: usize = 500;

// ============================================================================
// Publication Types
// ============================================================================

/// Publication display type
#[derive(Clone, Debug, PartialEq, Default)]
pub enum PublicationType {
    #[default]
    Book,
    Illustrated,
    Magazine,
    Documentation,
    Academic,
    Blog,
}

impl PublicationType {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "illustrated" => Self::Illustrated,
            "magazine" => Self::Magazine,
            "documentation" | "docs" => Self::Documentation,
            "academic" | "paper" => Self::Academic,
            "blog" => Self::Blog,
            _ => Self::Book,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Book => "book",
            Self::Illustrated => "illustrated",
            Self::Magazine => "magazine",
            Self::Documentation => "documentation",
            Self::Academic => "academic",
            Self::Blog => "blog",
        }
    }
}

/// Auto-update behavior for publication sections
#[derive(Clone, Debug, PartialEq, Default)]
pub enum AutoUpdate {
    #[default]
    No,
    Yes,
    Ask,
}

impl AutoUpdate {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "yes" => Self::Yes,
            "ask" => Self::Ask,
            _ => Self::No,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Yes => "yes",
            Self::Ask => "ask",
            Self::No => "no",
        }
    }
}

// ============================================================================
// Data Structures
// ============================================================================

/// Publication Index metadata (from Kind 30040)
#[derive(Clone, Debug, PartialEq)]
pub struct PublicationIndex {
    pub event: NostrEvent,
    pub naddr: String,
    pub a_tag: String,
    pub d_tag: String,
    pub title: String,
    pub author: Option<String>,
    pub summary: Option<String>,
    pub cover_image: Option<String>,
    pub published_on: Option<u64>,
    pub published_by: Option<String>,
    pub pub_type: PublicationType,
    pub auto_update: AutoUpdate,
    pub version: Option<String>,
    pub source_url: Option<String>,
    pub isbn: Option<String>,
    pub topics: Vec<String>,
    /// Ordered list of section addresses
    pub section_addresses: Vec<SectionReference>,
    /// Original author pubkey (for derivative works)
    pub original_author: Option<String>,
    /// Original event address (for derivative works)
    pub original_event: Option<String>,
    /// NKBIP-06 MIME type information
    pub mime_type: Option<NostrMimeType>,
}

/// Reference to a publication section
#[derive(Clone, Debug, PartialEq)]
pub struct SectionReference {
    pub address: String,       // kind:pubkey:d-tag
    pub relay_hint: Option<String>,
    pub event_id: Option<String>,
}

/// Publication Section/Chapter (from Kind 30041)
#[derive(Clone, Debug, PartialEq)]
pub struct PublicationSection {
    pub event: NostrEvent,
    pub naddr: String,
    pub a_tag: String,
    pub d_tag: String,
    pub title: String,
    pub content: String,
    pub wikilinks: Vec<String>,
    /// NKBIP-06 MIME type information
    pub mime_type: Option<NostrMimeType>,
}

/// Tree node for navigating publication structure
#[derive(Clone, Debug, PartialEq)]
pub struct PublicationNode {
    pub address: String,
    pub title: String,
    pub parent: Option<String>,
    pub children: Vec<String>,
    pub is_leaf: bool,
    pub resolved: bool,
    pub depth: usize,
}

/// Complete publication tree with resolved sections
#[derive(Clone, Debug)]
pub struct PublicationTree {
    pub root: PublicationIndex,
    pub nodes: HashMap<String, PublicationNode>,
    pub sections: HashMap<String, PublicationSection>,
    pub bookmark: Option<String>, // Current reading position
}

// Manual PartialEq for component props - compare by root event ID
impl PartialEq for PublicationTree {
    fn eq(&self, other: &Self) -> bool {
        self.root.event.id == other.root.event.id
            && self.bookmark == other.bookmark
    }
}

impl PublicationTree {
    pub fn new(root: PublicationIndex) -> Self {
        let mut nodes = HashMap::new();

        // Create root node
        nodes.insert(root.a_tag.clone(), PublicationNode {
            address: root.a_tag.clone(),
            title: root.title.clone(),
            parent: None,
            children: root.section_addresses.iter().map(|s| s.address.clone()).collect(),
            is_leaf: root.section_addresses.is_empty(),
            resolved: true,
            depth: 0,
        });

        // Create placeholder nodes for sections
        for (idx, section_ref) in root.section_addresses.iter().enumerate() {
            nodes.insert(section_ref.address.clone(), PublicationNode {
                address: section_ref.address.clone(),
                title: format!("Section {}", idx + 1),
                parent: Some(root.a_tag.clone()),
                children: vec![],
                is_leaf: true,
                resolved: false,
                depth: 1,
            });
        }

        Self {
            root,
            nodes,
            sections: HashMap::new(),
            bookmark: None,
        }
    }

    /// Get ordered list of section addresses for TOC
    pub fn get_toc(&self) -> Vec<String> {
        self.root.section_addresses.iter().map(|s| s.address.clone()).collect()
    }

    /// Update a section node with resolved data
    pub fn resolve_section(&mut self, section: PublicationSection) {
        if let Some(node) = self.nodes.get_mut(&section.a_tag) {
            node.title = section.title.clone();
            node.resolved = true;
        }
        self.sections.insert(section.a_tag.clone(), section);
    }
}

// ============================================================================
// Global Signals
// ============================================================================

/// Publication index cache (keyed by a_tag)
pub static PUBLICATIONS_CACHE: GlobalSignal<LruCache<String, PublicationIndex>> =
    GlobalSignal::new(|| LruCache::new(NonZeroUsize::new(PUBLICATION_CACHE_SIZE).unwrap()));

/// Section cache (keyed by a_tag)
pub static SECTIONS_CACHE: GlobalSignal<LruCache<String, PublicationSection>> =
    GlobalSignal::new(|| LruCache::new(NonZeroUsize::new(SECTION_CACHE_SIZE).unwrap()));

/// Active publication tree (for reader view)
pub static ACTIVE_PUBLICATION: GlobalSignal<Option<PublicationTree>> =
    GlobalSignal::new(|| None);

/// Loading states
pub static LOADING_PUBLICATIONS: GlobalSignal<bool> = GlobalSignal::new(|| false);
pub static LOADING_SECTIONS: GlobalSignal<bool> = GlobalSignal::new(|| false);

/// Store initialization flag
pub static PUBLICATION_STORE_INITIALIZED: GlobalSignal<bool> = GlobalSignal::new(|| false);

// ============================================================================
// Cache Functions
// ============================================================================

/// Get a publication index from cache
pub fn get_cached_publication(a_tag: &str) -> Option<PublicationIndex> {
    PUBLICATIONS_CACHE.read().peek(a_tag).cloned()
}

/// Get a publication by naddr
pub fn get_cached_publication_by_naddr(naddr: &str) -> Option<PublicationIndex> {
    let cache = PUBLICATIONS_CACHE.read();
    cache.iter().find(|(_, p)| p.naddr == naddr).map(|(_, p)| p.clone())
}

/// Cache a publication index
pub fn cache_publication(publication: PublicationIndex) {
    PUBLICATIONS_CACHE.write().put(publication.a_tag.clone(), publication);
}

/// Cache multiple publications
pub fn cache_publications(publications: &[PublicationIndex]) {
    let mut cache = PUBLICATIONS_CACHE.write();
    for pub_ in publications {
        cache.put(pub_.a_tag.clone(), pub_.clone());
    }
}

/// Get a section from cache
pub fn get_cached_section(a_tag: &str) -> Option<PublicationSection> {
    SECTIONS_CACHE.read().peek(a_tag).cloned()
}

/// Cache a section
pub fn cache_section(section: PublicationSection) {
    SECTIONS_CACHE.write().put(section.a_tag.clone(), section);
}

/// Cache multiple sections
pub fn cache_sections(sections: &[PublicationSection]) {
    let mut cache = SECTIONS_CACHE.write();
    for section in sections {
        cache.put(section.a_tag.clone(), section.clone());
    }
}

/// Get all cached publications
pub fn get_all_cached_publications() -> Vec<PublicationIndex> {
    let cache = PUBLICATIONS_CACHE.read();
    cache.iter().map(|(_, p)| p.clone()).collect()
}

/// Clear all caches
pub fn clear_cache() {
    PUBLICATIONS_CACHE.write().clear();
    SECTIONS_CACHE.write().clear();
    *ACTIVE_PUBLICATION.write() = None;
    *PUBLICATION_STORE_INITIALIZED.write() = false;
}

// ============================================================================
// Parsing Functions
// ============================================================================

/// Parse a Kind 30040 event into a PublicationIndex
pub fn parse_publication_index(event: &NostrEvent) -> Option<PublicationIndex> {
    if event.kind.as_u16() != KIND_INDEX {
        return None;
    }

    // Get d-tag (required)
    let d_tag = event.tags.identifier()?;

    // Get title (required)
    let title = event.tags.iter()
        .find_map(|tag| {
            let slice = tag.as_slice();
            if slice.first().map(|s| s.as_str()) == Some("title") {
                slice.get(1).map(|s| s.to_string())
            } else {
                None
            }
        })?;

    // Build a_tag and naddr
    let a_tag = format!("{}:{}:{}", KIND_INDEX, event.pubkey.to_hex(), d_tag);
    let naddr = Coordinate::new(Kind::Custom(KIND_INDEX), event.pubkey)
        .identifier(d_tag)
        .to_bech32()
        .ok()?;

    // Parse section references from a-tags
    let section_addresses: Vec<SectionReference> = event.tags.iter()
        .filter_map(|tag| {
            let slice = tag.as_slice();
            if slice.first().map(|s| s.as_str()) == Some("a") {
                let address = slice.get(1)?.to_string();
                let relay_hint = slice.get(2).map(|s| s.to_string());
                let event_id = slice.get(3).map(|s| s.to_string());
                Some(SectionReference { address, relay_hint, event_id })
            } else {
                None
            }
        })
        .collect();

    // Extract optional fields
    let mut author = None;
    let mut summary = None;
    let mut cover_image = None;
    let mut published_on = None;
    let mut published_by = None;
    let mut pub_type = PublicationType::default();
    let mut auto_update = AutoUpdate::default();
    let mut version = None;
    let mut source_url = None;
    let mut isbn = None;
    let mut topics = Vec::new();
    let mut original_author = None;
    let mut original_event = None;

    for tag in event.tags.iter() {
        let slice = tag.as_slice();
        match slice.first().map(|s| s.as_str()) {
            Some("author") => author = slice.get(1).map(|s| s.to_string()),
            Some("summary") => summary = slice.get(1).map(|s| s.to_string()),
            Some("image") => cover_image = slice.get(1).map(|s| s.to_string()),
            Some("published_on") => {
                published_on = slice.get(1).and_then(|s| s.parse().ok());
            }
            Some("published_by") => published_by = slice.get(1).map(|s| s.to_string()),
            Some("type") => {
                if let Some(t) = slice.get(1) {
                    pub_type = PublicationType::from_str(t);
                }
            }
            Some("auto-update") => {
                if let Some(v) = slice.get(1) {
                    auto_update = AutoUpdate::from_str(v);
                }
            }
            Some("version") => version = slice.get(1).map(|s| s.to_string()),
            Some("source") => source_url = slice.get(1).map(|s| s.to_string()),
            Some("i") => isbn = slice.get(1).map(|s| s.to_string()),
            Some("t") => {
                if let Some(t) = slice.get(1) {
                    topics.push(t.to_string());
                }
            }
            Some("p") => original_author = slice.get(1).map(|s| s.to_string()),
            Some("E") => original_event = slice.get(1).map(|s| s.to_string()),
            _ => {}
        }
    }

    // Extract NKBIP-06 MIME type information
    let mime_type = extract_mime_from_event(event);

    Some(PublicationIndex {
        event: event.clone(),
        naddr,
        a_tag,
        d_tag: d_tag.to_string(),
        title,
        author,
        summary,
        cover_image,
        published_on,
        published_by,
        pub_type,
        auto_update,
        version,
        source_url,
        isbn,
        topics,
        section_addresses,
        original_author,
        original_event,
        mime_type,
    })
}

/// Parse a Kind 30041 event into a PublicationSection
pub fn parse_publication_section(event: &NostrEvent) -> Option<PublicationSection> {
    if event.kind.as_u16() != KIND_CONTENT {
        return None;
    }

    // Get d-tag (required)
    let d_tag = event.tags.identifier()?;

    // Get title (required)
    let title = event.tags.iter()
        .find_map(|tag| {
            let slice = tag.as_slice();
            if slice.first().map(|s| s.as_str()) == Some("title") {
                slice.get(1).map(|s| s.to_string())
            } else {
                None
            }
        })?;

    // Build a_tag and naddr
    let a_tag = format!("{}:{}:{}", KIND_CONTENT, event.pubkey.to_hex(), d_tag);
    let naddr = Coordinate::new(Kind::Custom(KIND_CONTENT), event.pubkey)
        .identifier(d_tag)
        .to_bech32()
        .ok()?;

    // Extract wikilinks from tags
    let wikilinks: Vec<String> = event.tags.iter()
        .filter_map(|tag| {
            let slice = tag.as_slice();
            if slice.first().map(|s| s.as_str()) == Some("wikilink") {
                slice.get(1).map(|s| s.to_string())
            } else {
                None
            }
        })
        .collect();

    // Extract NKBIP-06 MIME type information
    let mime_type = extract_mime_from_event(event);

    Some(PublicationSection {
        event: event.clone(),
        naddr,
        a_tag,
        d_tag: d_tag.to_string(),
        title,
        content: event.content.clone(),
        wikilinks,
        mime_type,
    })
}

// ============================================================================
// Filter Builders
// ============================================================================

/// Build filter for publication indexes
pub fn publications_filter(limit: usize) -> Filter {
    Filter::new()
        .kind(Kind::Custom(KIND_INDEX))
        .limit(limit)
}

/// Build filter for publication indexes with pagination
pub fn publications_filter_paginated(limit: usize, until: Option<u64>) -> Filter {
    let mut filter = Filter::new()
        .kind(Kind::Custom(KIND_INDEX))
        .limit(limit);

    if let Some(ts) = until {
        filter = filter.until(Timestamp::from(ts));
    }

    filter
}

/// Build filter for publications by author
pub fn publications_by_author_filter(pubkey: PublicKey, limit: usize) -> Filter {
    Filter::new()
        .kind(Kind::Custom(KIND_INDEX))
        .author(pubkey)
        .limit(limit)
}

/// Build filter for a specific publication by coordinate
pub fn publication_by_coord_filter(pubkey: PublicKey, identifier: &str) -> Filter {
    Filter::new()
        .kind(Kind::Custom(KIND_INDEX))
        .author(pubkey)
        .identifier(identifier)
}

/// Build filter for publication sections
pub fn sections_filter(limit: usize) -> Filter {
    Filter::new()
        .kind(Kind::Custom(KIND_CONTENT))
        .limit(limit)
}

/// Build filter for sections by author
pub fn sections_by_author_filter(pubkey: PublicKey, limit: usize) -> Filter {
    Filter::new()
        .kind(Kind::Custom(KIND_CONTENT))
        .author(pubkey)
        .limit(limit)
}

/// Build filter for a specific section by coordinate
pub fn section_by_coord_filter(pubkey: PublicKey, identifier: &str) -> Filter {
    Filter::new()
        .kind(Kind::Custom(KIND_CONTENT))
        .author(pubkey)
        .identifier(identifier)
}

/// Build filter for multiple sections by their coordinates
pub fn sections_by_addresses_filter(addresses: &[SectionReference]) -> Vec<Filter> {
    addresses.iter().filter_map(|addr| {
        // Parse address: "kind:pubkey:d-tag"
        let parts: Vec<&str> = addr.address.split(':').collect();
        if parts.len() < 3 {
            return None;
        }

        let kind = parts[0].parse::<u16>().ok()?;
        let pubkey = PublicKey::from_hex(parts[1]).ok()?;
        let d_tag = parts[2..].join(":");

        Some(Filter::new()
            .kind(Kind::Custom(kind))
            .author(pubkey)
            .identifier(d_tag))
    }).collect()
}

/// Build filter for publication reactions
pub fn publication_reactions_filter(a_tag: &str, limit: usize) -> Filter {
    Filter::new()
        .kind(Kind::Reaction)
        .custom_tag(SingleLetterTag::lowercase(Alphabet::A), a_tag)
        .limit(limit)
}

/// Build filter for publication comments (kind 1111)
pub fn publication_comments_filter(a_tag: &str, limit: usize) -> Filter {
    Filter::new()
        .kind(Kind::Custom(1111))
        .custom_tag(SingleLetterTag::lowercase(Alphabet::A), a_tag)
        .limit(limit)
}

/// Build filter for publications by MIME category (NKBIP-06)
///
/// Filters publications by their M tag (Nostr MIME type).
/// Example: Filter for all publication indexes with category "meta-data"
pub fn publications_by_mime_category_filter(category: &str, use_case: &str, limit: usize) -> Filter {
    let m_tag_value = build_mime_filter_tag(category, use_case, true);
    Filter::new()
        .kind(Kind::Custom(KIND_INDEX))
        .custom_tag(SingleLetterTag::uppercase(Alphabet::M), &m_tag_value)
        .limit(limit)
}

/// Build filter for publication indexes by MIME type
pub fn publications_by_mime_filter(limit: usize) -> Filter {
    let m_tag_value = build_mime_filter_tag(CATEGORY_METADATA, USE_INDEX, true);
    Filter::new()
        .kind(Kind::Custom(KIND_INDEX))
        .custom_tag(SingleLetterTag::uppercase(Alphabet::M), &m_tag_value)
        .limit(limit)
}

/// Build filter for publication sections by MIME type
pub fn sections_by_mime_filter(limit: usize) -> Filter {
    let m_tag_value = build_mime_filter_tag(CATEGORY_ARTICLE, USE_PUBLICATION_CONTENT, true);
    Filter::new()
        .kind(Kind::Custom(KIND_CONTENT))
        .custom_tag(SingleLetterTag::uppercase(Alphabet::M), &m_tag_value)
        .limit(limit)
}

// ============================================================================
// Fetch Functions
// ============================================================================

/// Fetch publications with pagination
pub async fn fetch_publications(limit: usize, until: Option<u64>) -> StdResult<Vec<PublicationIndex>, String> {
    *LOADING_PUBLICATIONS.write() = true;

    let filter = publications_filter_paginated(limit, until);
    let result = crate::stores::nostr_client::fetch_events_aggregated(
        filter,
        Duration::from_secs(15),
    ).await;

    *LOADING_PUBLICATIONS.write() = false;

    match result {
        Ok(events) => {
            let publications: Vec<PublicationIndex> = events
                .iter()
                .filter_map(|e| parse_publication_index(e))
                .collect();

            cache_publications(&publications);
            *PUBLICATION_STORE_INITIALIZED.write() = true;

            log::info!("Fetched {} publications", publications.len());
            Ok(publications)
        }
        Err(e) => {
            log::error!("Failed to fetch publications: {}", e);
            Err(e)
        }
    }
}

/// Fetch a single publication by naddr
pub async fn fetch_publication_by_naddr(naddr: &str) -> StdResult<Option<PublicationIndex>, String> {
    // Check cache first
    if let Some(pub_) = get_cached_publication_by_naddr(naddr) {
        return Ok(Some(pub_));
    }

    // Parse naddr
    let coord = Coordinate::from_bech32(naddr)
        .map_err(|e| format!("Invalid naddr: {}", e))?;

    let identifier = coord.identifier;
    if identifier.is_empty() {
        return Err("No identifier in naddr".to_string());
    }

    let filter = publication_by_coord_filter(coord.public_key, &identifier);

    let result = crate::stores::nostr_client::fetch_events_aggregated(
        filter,
        Duration::from_secs(10),
    ).await;

    match result {
        Ok(events) => {
            if let Some(event) = events.first() {
                if let Some(pub_) = parse_publication_index(event) {
                    cache_publication(pub_.clone());
                    return Ok(Some(pub_));
                }
            }
            Ok(None)
        }
        Err(e) => {
            log::error!("Failed to fetch publication: {}", e);
            Err(e)
        }
    }
}

/// Fetch all sections for a publication
pub async fn fetch_publication_sections(publication: &PublicationIndex) -> StdResult<Vec<PublicationSection>, String> {
    if publication.section_addresses.is_empty() {
        return Ok(vec![]);
    }

    *LOADING_SECTIONS.write() = true;

    let filters = sections_by_addresses_filter(&publication.section_addresses);

    let mut all_sections = Vec::new();

    for filter in filters {
        let result = crate::stores::nostr_client::fetch_events_aggregated(
            filter,
            Duration::from_secs(10),
        ).await;

        if let Ok(events) = result {
            for event in events {
                if let Some(section) = parse_publication_section(&event) {
                    all_sections.push(section);
                }
            }
        }
    }

    *LOADING_SECTIONS.write() = false;

    cache_sections(&all_sections);
    log::info!("Fetched {} sections", all_sections.len());

    Ok(all_sections)
}

/// Fetch and build complete publication tree
pub async fn fetch_publication_tree(naddr: &str) -> StdResult<PublicationTree, String> {
    let publication = fetch_publication_by_naddr(naddr).await?
        .ok_or("Publication not found")?;

    let mut tree = PublicationTree::new(publication.clone());

    // Fetch all sections
    let sections = fetch_publication_sections(&publication).await?;

    // Resolve sections in tree
    for section in sections {
        tree.resolve_section(section);
    }

    // Store as active publication
    *ACTIVE_PUBLICATION.write() = Some(tree.clone());

    Ok(tree)
}

/// Fetch publications by author
pub async fn fetch_publications_by_author(pubkey_hex: &str, limit: usize) -> StdResult<Vec<PublicationIndex>, String> {
    let pubkey = PublicKey::from_hex(pubkey_hex)
        .map_err(|e| format!("Invalid pubkey: {}", e))?;

    let filter = publications_by_author_filter(pubkey, limit);

    let result = crate::stores::nostr_client::fetch_events_aggregated(
        filter,
        Duration::from_secs(10),
    ).await;

    match result {
        Ok(events) => {
            let publications: Vec<PublicationIndex> = events
                .iter()
                .filter_map(|e| parse_publication_index(e))
                .collect();

            cache_publications(&publications);
            Ok(publications)
        }
        Err(e) => Err(e),
    }
}

/// Search publications using a specific filter (for NKBIP-08 book reference queries)
pub async fn search_publications_with_filter(filter: Filter, limit: usize) -> StdResult<Vec<PublicationIndex>, String> {
    *LOADING_PUBLICATIONS.write() = true;

    let result = crate::stores::nostr_client::fetch_events_aggregated(
        filter.limit(limit),
        Duration::from_secs(15),
    ).await;

    *LOADING_PUBLICATIONS.write() = false;

    match result {
        Ok(events) => {
            let publications: Vec<PublicationIndex> = events
                .iter()
                .filter_map(|e| parse_publication_index(e))
                .collect();

            cache_publications(&publications);
            log::info!("Search found {} publications", publications.len());
            Ok(publications)
        }
        Err(e) => {
            log::error!("Failed to search publications: {}", e);
            Err(e)
        }
    }
}

/// Search publications by title or summary
pub async fn search_publications(query: &str, limit: usize) -> StdResult<Vec<PublicationIndex>, String> {
    let query_lower = query.to_lowercase();

    // Fetch a batch to search through
    let publications = fetch_publications(200, None).await?;

    let matching: Vec<PublicationIndex> = publications
        .into_iter()
        .filter(|p| {
            if p.title.to_lowercase().contains(&query_lower) {
                return true;
            }
            if let Some(ref summary) = p.summary {
                if summary.to_lowercase().contains(&query_lower) {
                    return true;
                }
            }
            if let Some(ref author) = p.author {
                if author.to_lowercase().contains(&query_lower) {
                    return true;
                }
            }
            for topic in &p.topics {
                if topic.to_lowercase().contains(&query_lower) {
                    return true;
                }
            }
            false
        })
        .take(limit)
        .collect();

    Ok(matching)
}

/// Get the MIME filter tag value for querying publications by content type
///
/// Uses NKBIP-06 M tag format for content-type based filtering.
/// Returns the M tag value (e.g., "meta-data/index/replaceable") that can be
/// used in filter queries.
pub fn get_publication_mime_filter_value(kind: u16) -> String {
    build_mime_filter_tag_for_kind(kind)
}

/// Get individual MIME tags for a publication kind
///
/// Returns (m_tag, M_tag) for adding to events individually
pub fn get_publication_mime_tags(kind: u16) -> (Tag, Tag) {
    (generate_m_tag(kind), generate_nostr_m_tag(kind))
}

/// Check if a publication contains media content
///
/// Uses CATEGORY_MEDIA for content classification
pub fn is_media_publication(mime_type: &Option<NostrMimeType>) -> bool {
    mime_type.as_ref()
        .map(|m| m.category == CATEGORY_MEDIA)
        .unwrap_or(false)
}

// ============================================================================
// Publishing Functions
// ============================================================================

/// Publish a new publication index
pub async fn publish_publication_index(
    title: &str,
    summary: Option<&str>,
    cover_image: Option<&str>,
    pub_type: PublicationType,
    topics: &[String],
    section_addresses: &[SectionReference],
) -> StdResult<String, String> {
    let client = crate::stores::nostr_client::get_client()
        .ok_or("Client not initialized")?;

    if !*crate::stores::nostr_client::HAS_SIGNER.read() {
        return Err("No signer attached".to_string());
    }

    // Generate d-tag from title
    let d_tag = normalize_wiki_dtag(title);

    // Build tags
    let mut tags: Vec<Tag> = vec![
        Tag::identifier(&d_tag),
        Tag::title(title),
        Tag::custom(TagKind::Custom("type".into()), vec![pub_type.as_str().to_string()]),
        Tag::custom(TagKind::Custom("auto-update".into()), vec!["no".to_string()]),
    ];

    // Add MIME tags
    for mime_tag in generate_mime_tags(KIND_INDEX) {
        tags.push(mime_tag);
    }

    if let Some(s) = summary {
        tags.push(Tag::custom(TagKind::Custom("summary".into()), vec![s.to_string()]));
    }

    if let Some(img) = cover_image {
        if let Ok(url) = Url::parse(img) {
            tags.push(Tag::image(url, None));
        }
    }

    for topic in topics {
        tags.push(Tag::hashtag(topic));
    }

    // Add ordered section references
    for section_ref in section_addresses {
        let mut tag_values = vec![section_ref.address.clone()];
        if let Some(ref relay) = section_ref.relay_hint {
            tag_values.push(relay.clone());
        }
        if let Some(ref event_id) = section_ref.event_id {
            if section_ref.relay_hint.is_none() {
                tag_values.push(String::new()); // Empty relay hint placeholder
            }
            tag_values.push(event_id.clone());
        }
        tags.push(Tag::custom(TagKind::Custom("a".into()), tag_values));
    }

    // Build and publish (content must be empty for index)
    let builder = EventBuilder::new(Kind::Custom(KIND_INDEX), "")
        .tags(tags);

    let output = client.send_event_builder(builder).await
        .map_err(|e| format!("Failed to publish publication index: {}", e))?;

    log::info!("Publication index published: {}", output.id().to_hex());
    Ok(output.id().to_hex())
}

/// Publish a publication section
pub async fn publish_publication_section(
    title: &str,
    content: &str,
    identifier: Option<&str>,
) -> StdResult<String, String> {
    let client = crate::stores::nostr_client::get_client()
        .ok_or("Client not initialized")?;

    if !*crate::stores::nostr_client::HAS_SIGNER.read() {
        return Err("No signer attached".to_string());
    }

    // Generate d-tag from title or use provided identifier
    let d_tag = identifier
        .map(|s| s.to_string())
        .unwrap_or_else(|| normalize_wiki_dtag(title));

    // Build tags
    let mut tags: Vec<Tag> = vec![
        Tag::identifier(&d_tag),
        Tag::title(title),
    ];

    // Add MIME tags
    for mime_tag in generate_mime_tags(KIND_CONTENT) {
        tags.push(mime_tag);
    }

    // Extract and add wikilinks from content
    let wikilinks = crate::utils::nip54::extract_wikilinks(content);
    for link in &wikilinks {
        tags.push(Tag::custom(
            TagKind::Custom("wikilink".into()),
            vec![link.target.clone()],
        ));
    }

    // Extract and add book reference tags for NKBIP-08 discoverability
    let book_refs = extract_book_wikilinks(content);
    for book_ref in &book_refs {
        // Add indexable tags (T, C, c, s, v) from the book reference
        let book_tags = book_ref.to_tags();
        tags.extend(book_tags);
    }

    // Build and publish
    let builder = EventBuilder::new(Kind::Custom(KIND_CONTENT), content)
        .tags(tags);

    let output = client.send_event_builder(builder).await
        .map_err(|e| format!("Failed to publish section: {}", e))?;

    log::info!("Publication section published: {}", output.id().to_hex());
    Ok(output.id().to_hex())
}

/// Update publication index with new section order
pub async fn update_publication_sections(
    publication: &PublicationIndex,
    new_section_addresses: &[SectionReference],
) -> StdResult<String, String> {
    // Re-publish with same d-tag (replaces old event)
    publish_publication_index(
        &publication.title,
        publication.summary.as_deref(),
        publication.cover_image.as_deref(),
        publication.pub_type.clone(),
        &publication.topics,
        new_section_addresses,
    ).await
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_publication_type_from_str() {
        assert_eq!(PublicationType::from_str("book"), PublicationType::Book);
        assert_eq!(PublicationType::from_str("illustrated"), PublicationType::Illustrated);
        assert_eq!(PublicationType::from_str("documentation"), PublicationType::Documentation);
        assert_eq!(PublicationType::from_str("docs"), PublicationType::Documentation);
        assert_eq!(PublicationType::from_str("unknown"), PublicationType::Book);
    }

    #[test]
    fn test_auto_update_from_str() {
        assert_eq!(AutoUpdate::from_str("yes"), AutoUpdate::Yes);
        assert_eq!(AutoUpdate::from_str("ask"), AutoUpdate::Ask);
        assert_eq!(AutoUpdate::from_str("no"), AutoUpdate::No);
        assert_eq!(AutoUpdate::from_str("unknown"), AutoUpdate::No);
    }

    #[test]
    fn test_section_reference() {
        let addr = "30041:abc123:chapter-1";
        let section_ref = SectionReference {
            address: addr.to_string(),
            relay_hint: Some("wss://relay.example.com".to_string()),
            event_id: Some("event123".to_string()),
        };
        assert_eq!(section_ref.address, addr);
        assert_eq!(section_ref.relay_hint, Some("wss://relay.example.com".to_string()));
    }
}
