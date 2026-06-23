//! Citation Store
//! Handles NKBIP-03 Citations - Kinds 30, 31, 32, 33
//!
//! Citation types:
//! - Kind 30: Internal Nostr reference
//! - Kind 31: External web reference
//! - Kind 32: Hardcopy (book/journal) reference
//! - Kind 33: Prompt (AI) reference
#![allow(dead_code)]
use crate::utils::format::safe_slice;
use dioxus::prelude::*;
use lru::LruCache;
use nostr::Event as NostrEvent;
use nostr_sdk::prelude::*;
use std::num::NonZeroUsize;
use std::time::Duration;
type StdResult<T, E> = std::result::Result<T, E>;
use crate::utils::nkbip03::{
    parse_citation, Citation, CitationType, KIND_EXTERNAL_WEB, KIND_HARDCOPY, KIND_INTERNAL_REF,
    KIND_PROMPT,
};
use crate::utils::nkbip06::generate_mime_tags;
use std::collections::HashMap;

fn ensure_relay_acceptance<T: std::fmt::Debug>(
    output: &nostr_relay_pool::Output<T>,
    action: &str,
) -> std::result::Result<(), String> {
    if output.success.is_empty() {
        let suffix = if action.is_empty() {
            String::new()
        } else {
            format!(" ({action})")
        };
        Err(format!(
            "Failed to publish citation: no relays accepted the event{}",
            suffix
        ))
    } else {
        Ok(())
    }
}
/// Cache sizes
const CITATION_CACHE_SIZE: usize = 200;
/// Cached citation with event data
#[derive(Clone, Debug)]
pub struct CachedCitation {
    pub event: NostrEvent,
    pub citation: Citation,
    pub naddr: Option<String>,
    pub a_tag: Option<String>,
}
impl PartialEq for CachedCitation {
    fn eq(&self, other: &Self) -> bool {
        self.event.id == other.event.id
    }
}

impl CachedCitation {
    /// Get the best identifier for this citation
    /// Returns naddr if available, otherwise nevent (bech32), falling back to hex
    pub fn identifier(&self) -> String {
        self.naddr.clone().unwrap_or_else(|| {
            self.event
                .id
                .to_bech32()
                .unwrap_or_else(|_| self.event.id.to_hex())
        })
    }
}

/// Citation list for display (grouped by type)
#[derive(Clone, Debug, Default)]
pub struct CitationGroup {
    pub internal: Vec<CachedCitation>,
    pub external: Vec<CachedCitation>,
    pub hardcopy: Vec<CachedCitation>,
    pub prompt: Vec<CachedCitation>,
}
impl CitationGroup {
    pub fn total_count(&self) -> usize {
        self.internal.len() + self.external.len() + self.hardcopy.len() + self.prompt.len()
    }
    pub fn all(&self) -> Vec<CachedCitation> {
        let mut all = Vec::new();
        all.extend(self.internal.clone());
        all.extend(self.external.clone());
        all.extend(self.hardcopy.clone());
        all.extend(self.prompt.clone());
        all
    }
}
/// Citation cache (keyed by event ID)
pub static CITATIONS_CACHE: GlobalSignal<LruCache<String, CachedCitation>> =
    GlobalSignal::new(|| LruCache::new(NonZeroUsize::new(CITATION_CACHE_SIZE).unwrap()));
/// User's citations (grouped by type)
pub static USER_CITATIONS: GlobalSignal<CitationGroup> = GlobalSignal::new(CitationGroup::default);
/// Loading state
pub static LOADING_CITATIONS: GlobalSignal<bool> = GlobalSignal::new(|| false);
/// Store initialization flag
pub static CITATION_STORE_INITIALIZED: GlobalSignal<bool> = GlobalSignal::new(|| false);
/// Get a citation from cache by event ID
pub fn get_cached_citation(event_id: &str) -> Option<CachedCitation> {
    CITATIONS_CACHE.read().peek(event_id).cloned()
}
/// Cache a citation
pub fn cache_citation(citation: CachedCitation) {
    CITATIONS_CACHE
        .write()
        .put(citation.event.id.to_hex(), citation);
}
/// Cache multiple citations
pub fn cache_citations(citations: &[CachedCitation]) {
    let mut cache = CITATIONS_CACHE.write();
    for citation in citations {
        cache.put(citation.event.id.to_hex(), citation.clone());
    }
}
/// Get all cached citations
pub fn get_all_cached_citations() -> Vec<CachedCitation> {
    let cache = CITATIONS_CACHE.read();
    cache.iter().map(|(_, c)| c.clone()).collect()
}
/// Clear cache
pub fn clear_cache() {
    CITATIONS_CACHE.write().clear();
    *USER_CITATIONS.write() = CitationGroup::default();
    *CITATION_STORE_INITIALIZED.write() = false;
}
/// Parse a citation event into a CachedCitation
pub fn parse_citation_event(event: &NostrEvent) -> Option<CachedCitation> {
    let event_kind = event.kind.as_u16();
    if ![
        KIND_INTERNAL_REF,
        KIND_EXTERNAL_WEB,
        KIND_HARDCOPY,
        KIND_PROMPT,
    ]
    .contains(&event_kind)
    {
        return None;
    }
    let citation = parse_citation(event).ok()?;
    let kind = citation.kind();
    let (naddr, a_tag) = if let Some(d_tag) = event.tags.identifier() {
        let naddr = Coordinate::new(Kind::Custom(kind), event.pubkey)
            .identifier(d_tag)
            .to_bech32()
            .ok();
        let a_tag = Some(format!("{}:{}:{}", kind, event.pubkey.to_hex(), d_tag));
        (naddr, a_tag)
    } else {
        (None, None)
    };
    Some(CachedCitation {
        event: event.clone(),
        citation,
        naddr,
        a_tag,
    })
}
/// Group citations by type
pub fn group_citations(citations: Vec<CachedCitation>) -> CitationGroup {
    let mut group = CitationGroup::default();
    for citation in citations {
        match citation.citation {
            Citation::Internal(_) => group.internal.push(citation),
            Citation::ExternalWeb(_) => group.external.push(citation),
            Citation::Hardcopy(_) => group.hardcopy.push(citation),
            Citation::Prompt(_) => group.prompt.push(citation),
        }
    }
    group
}
/// Build filter for all citation types by author
pub fn citations_filter(pubkey: PublicKey, limit: usize) -> Vec<Filter> {
    vec![
        Filter::new()
            .kind(Kind::Custom(KIND_INTERNAL_REF))
            .author(pubkey)
            .limit(limit),
        Filter::new()
            .kind(Kind::Custom(KIND_EXTERNAL_WEB))
            .author(pubkey)
            .limit(limit),
        Filter::new()
            .kind(Kind::Custom(KIND_HARDCOPY))
            .author(pubkey)
            .limit(limit),
        Filter::new()
            .kind(Kind::Custom(KIND_PROMPT))
            .author(pubkey)
            .limit(limit),
    ]
}
/// Build filter for internal citations
pub fn internal_citations_filter(pubkey: PublicKey, limit: usize) -> Filter {
    Filter::new()
        .kind(Kind::Custom(KIND_INTERNAL_REF))
        .author(pubkey)
        .limit(limit)
}
/// Build filter for external web citations
pub fn external_citations_filter(pubkey: PublicKey, limit: usize) -> Filter {
    Filter::new()
        .kind(Kind::Custom(KIND_EXTERNAL_WEB))
        .author(pubkey)
        .limit(limit)
}
/// Build filter for hardcopy citations
pub fn hardcopy_citations_filter(pubkey: PublicKey, limit: usize) -> Filter {
    Filter::new()
        .kind(Kind::Custom(KIND_HARDCOPY))
        .author(pubkey)
        .limit(limit)
}
/// Build filter for prompt/AI citations
pub fn prompt_citations_filter(pubkey: PublicKey, limit: usize) -> Filter {
    Filter::new()
        .kind(Kind::Custom(KIND_PROMPT))
        .author(pubkey)
        .limit(limit)
}
/// Build filter for a specific citation by coordinate
pub fn citation_by_coord_filter(pubkey: PublicKey, kind: u16, identifier: &str) -> Filter {
    Filter::new()
        .kind(Kind::Custom(kind))
        .author(pubkey)
        .identifier(identifier)
}
/// Build filter for citations referencing an event
pub fn citations_referencing_filter(event_id: &str, limit: usize) -> Filter {
    Filter::new()
        .kind(Kind::Custom(KIND_INTERNAL_REF))
        .custom_tag(SingleLetterTag::lowercase(Alphabet::C), event_id)
        .limit(limit)
}
/// Fetch all citations by author
pub async fn load_citations_by_author(
    pubkey_hex: &str,
    limit: usize,
) -> StdResult<CitationGroup, String> {
    let pubkey = PublicKey::from_hex(pubkey_hex).map_err(|e| format!("Invalid pubkey: {}", e))?;
    *LOADING_CITATIONS.write() = true;
    let filters = citations_filter(pubkey, limit);
    let mut all_citations = Vec::new();
    for filter in filters {
        let result =
            crate::stores::nostr_client::fetch_events_aggregated(filter, Duration::from_secs(10))
                .await;
        if let Ok(events) = result {
            for event in events {
                if let Some(citation) = parse_citation_event(&event) {
                    all_citations.push(citation);
                }
            }
        }
    }
    *LOADING_CITATIONS.write() = false;
    cache_citations(&all_citations);
    let group = group_citations(all_citations);
    log::info!("Fetched {} total citations", group.total_count());
    Ok(group)
}

pub async fn fetch_citations_by_author(
    pubkey_hex: &str,
    limit: usize,
) -> StdResult<CitationGroup, String> {
    let group = load_citations_by_author(pubkey_hex, limit).await?;
    *USER_CITATIONS.write() = group.clone();
    *CITATION_STORE_INITIALIZED.write() = true;
    Ok(group)
}
/// Fetch citations of a specific type
pub async fn fetch_citations_by_type(
    pubkey_hex: &str,
    citation_type: CitationType,
    limit: usize,
) -> StdResult<Vec<CachedCitation>, String> {
    let pubkey = PublicKey::from_hex(pubkey_hex).map_err(|e| format!("Invalid pubkey: {}", e))?;
    let filter = match citation_type {
        CitationType::Internal => internal_citations_filter(pubkey, limit),
        CitationType::ExternalWeb => external_citations_filter(pubkey, limit),
        CitationType::Hardcopy => hardcopy_citations_filter(pubkey, limit),
        CitationType::Prompt => prompt_citations_filter(pubkey, limit),
    };
    *LOADING_CITATIONS.write() = true;
    let result =
        crate::stores::nostr_client::fetch_events_aggregated(filter, Duration::from_secs(10)).await;
    *LOADING_CITATIONS.write() = false;
    match result {
        Ok(events) => {
            let citations: Vec<CachedCitation> =
                events.iter().filter_map(parse_citation_event).collect();
            cache_citations(&citations);
            Ok(citations)
        }
        Err(e) => Err(e),
    }
}
/// Fetch a specific citation by naddr
pub async fn fetch_citation_by_naddr(naddr: &str) -> StdResult<Option<CachedCitation>, String> {
    let coord = Coordinate::from_bech32(naddr).map_err(|e| format!("Invalid naddr: {}", e))?;
    let identifier = coord.identifier;
    if identifier.is_empty() {
        return Err("No identifier in naddr".to_string());
    }
    let filter = citation_by_coord_filter(coord.public_key, coord.kind.as_u16(), &identifier);
    let result =
        crate::stores::nostr_client::fetch_events_aggregated(filter, Duration::from_secs(10)).await;
    match result {
        Ok(events) => {
            if let Some(event) = events.first() {
                if let Some(citation) = parse_citation_event(event) {
                    cache_citation(citation.clone());
                    return Ok(Some(citation));
                }
            }
            Ok(None)
        }
        Err(e) => Err(e),
    }
}
/// Fetch multiple citations by their NIP-19 identifiers (nevent, naddr, note)
/// Returns a map of identifier -> CachedCitation for resolved citations
pub async fn fetch_citations_by_identifiers(
    identifiers: &[String],
) -> StdResult<HashMap<String, CachedCitation>, String> {
    let mut result_map = HashMap::new();
    if identifiers.is_empty() {
        return Ok(result_map);
    }
    let mut to_fetch: Vec<(String, CitationLookup)> = Vec::new();
    for identifier in identifiers {
        let cached = {
            let cache = CITATIONS_CACHE.read();
            cache
                .iter()
                .find(|(_, c)| {
                    c.naddr.as_ref() == Some(identifier)
                        || c.event.id.to_hex() == *identifier
                        || c.event.id.to_bech32().ok().as_ref() == Some(identifier)
                })
                .map(|(_, c)| c.clone())
        };
        if let Some(citation) = cached {
            result_map.insert(identifier.clone(), citation);
            continue;
        }
        if let Some(lookup) = parse_citation_identifier(identifier) {
            to_fetch.push((identifier.clone(), lookup));
        } else {
            log::warn!("Could not parse citation identifier: {}", identifier);
        }
    }
    for (identifier, lookup) in to_fetch {
        let result = match lookup {
            CitationLookup::EventId(event_id) => fetch_citation_by_event_id(&event_id).await,
            CitationLookup::Coordinate {
                pubkey,
                kind,
                d_tag,
            } => {
                let filter = citation_by_coord_filter(pubkey, kind, &d_tag);
                let events = crate::stores::nostr_client::fetch_events_aggregated(
                    filter,
                    Duration::from_secs(10),
                )
                .await;
                match events {
                    Ok(evts) => {
                        if let Some(event) = evts.first() {
                            Ok(parse_citation_event(event))
                        } else {
                            Ok(None)
                        }
                    }
                    Err(e) => Err(e),
                }
            }
        };
        if let Ok(Some(citation)) = result {
            cache_citation(citation.clone());
            result_map.insert(identifier, citation);
        }
    }
    log::info!(
        "Resolved {}/{} citations",
        result_map.len(),
        identifiers.len()
    );
    Ok(result_map)
}
/// Information needed to look up a citation
enum CitationLookup {
    EventId(EventId),
    Coordinate {
        pubkey: PublicKey,
        kind: u16,
        d_tag: String,
    },
}
/// Parse a NIP-19 identifier into lookup information
fn parse_citation_identifier(identifier: &str) -> Option<CitationLookup> {
    if identifier.starts_with("nevent") {
        if let Ok(nip19) = Nip19Event::from_bech32(identifier) {
            return Some(CitationLookup::EventId(nip19.event_id));
        }
    }
    if identifier.starts_with("note") {
        if let Ok(Nip19::EventId(event_id)) = Nip19::from_bech32(identifier) {
            return Some(CitationLookup::EventId(event_id));
        }
    }
    if identifier.starts_with("naddr") {
        if let Ok(coord) = Coordinate::from_bech32(identifier) {
            return Some(CitationLookup::Coordinate {
                pubkey: coord.public_key,
                kind: coord.kind.as_u16(),
                d_tag: coord.identifier,
            });
        }
    }
    if identifier.len() == 64 && identifier.chars().all(|c| c.is_ascii_hexdigit()) {
        if let Ok(event_id) = EventId::from_hex(identifier) {
            return Some(CitationLookup::EventId(event_id));
        }
    }
    None
}
/// Fetch a citation by event ID
async fn fetch_citation_by_event_id(
    event_id: &EventId,
) -> StdResult<Option<CachedCitation>, String> {
    if let Some(citation) = get_cached_citation(&event_id.to_hex()) {
        return Ok(Some(citation));
    }
    let filter = Filter::new().id(*event_id);
    let result =
        crate::stores::nostr_client::fetch_events_aggregated(filter, Duration::from_secs(10)).await;
    match result {
        Ok(events) => {
            if let Some(event) = events.first() {
                if let Some(citation) = parse_citation_event(event) {
                    cache_citation(citation.clone());
                    return Ok(Some(citation));
                }
            }
            Ok(None)
        }
        Err(e) => Err(e),
    }
}
/// Search citations by title or author name
pub async fn search_citations(
    query: &str,
    pubkey_hex: &str,
    limit: usize,
) -> StdResult<Vec<CachedCitation>, String> {
    let query_lower = query.to_lowercase();
    let group = fetch_citations_by_author(pubkey_hex, 200).await?;
    let matching: Vec<CachedCitation> = group
        .all()
        .into_iter()
        .filter(|c| match &c.citation {
            Citation::Internal(i) => {
                i.base.title.to_lowercase().contains(&query_lower)
                    || i.base.author.to_lowercase().contains(&query_lower)
            }
            Citation::ExternalWeb(e) => {
                e.base.title.to_lowercase().contains(&query_lower)
                    || e.base.author.to_lowercase().contains(&query_lower)
                    || e.url.to_lowercase().contains(&query_lower)
            }
            Citation::Hardcopy(h) => {
                h.base.title.to_lowercase().contains(&query_lower)
                    || h.base.author.to_lowercase().contains(&query_lower)
            }
            Citation::Prompt(p) => {
                p.llm.to_lowercase().contains(&query_lower)
                    || p.base
                        .summary
                        .as_ref()
                        .map(|s| s.to_lowercase().contains(&query_lower))
                        .unwrap_or(false)
            }
        })
        .take(limit)
        .collect();
    Ok(matching)
}
/// Publish an internal Nostr citation (Kind 30)
/// If existing_d_tag is provided (editing), it will be preserved to replace the existing event.
pub async fn publish_internal_citation(
    cited_address: &str,
    cited_text: &str,
    title: Option<&str>,
    author: Option<&str>,
    existing_d_tag: Option<&str>,
) -> StdResult<String, String> {
    let _client = crate::stores::nostr_client::get_client().ok_or("Client not initialized")?;
    if !*crate::stores::nostr_client::HAS_SIGNER.read() {
        return Err("No signer attached".to_string());
    }
    let d_tag = existing_d_tag.map(|s| s.to_string()).unwrap_or_else(|| {
        title
            .map(crate::utils::nip54::normalize_wiki_dtag)
            .unwrap_or_else(|| format!("citation-{}", safe_slice(cited_address, 8)))
    });
    let mut tags: Vec<Tag> = vec![
        Tag::identifier(&d_tag),
        Tag::custom(TagKind::Custom("c".into()), vec![cited_address.to_string()]),
        Tag::custom(
            TagKind::Custom("accessed_on".into()),
            vec![Timestamp::now().as_secs().to_string()],
        ),
    ];
    if let Some(t) = title {
        tags.push(Tag::custom(
            TagKind::Custom("title".into()),
            vec![t.to_string()],
        ));
    }
    if let Some(a) = author {
        tags.push(Tag::custom(
            TagKind::Custom("author".into()),
            vec![a.to_string()],
        ));
    }
    for mime_tag in generate_mime_tags(KIND_INTERNAL_REF) {
        tags.push(mime_tag);
    }
    let builder = EventBuilder::new(Kind::Custom(KIND_INTERNAL_REF), cited_text).tags(tags);
    let event = crate::stores::publish_queue::signing::sign_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to sign: {}", e))?;
    let event_id = event.id.to_hex();
    crate::stores::publish_queue::enqueue(
        event,
        crate::stores::publish_queue::types::QueueEventType::Other("citation".to_string()),
        None,
        std::collections::HashMap::new(),
    ).await;
    Ok(event_id)
}
/// Publish an external web citation (Kind 31)
/// If existing_d_tag is provided (editing), it will be preserved to replace the existing event.
pub async fn publish_external_citation(
    url: &str,
    cited_text: &str,
    title: Option<&str>,
    author: Option<&str>,
    existing_d_tag: Option<&str>,
) -> StdResult<String, String> {
    let _client = crate::stores::nostr_client::get_client().ok_or("Client not initialized")?;
    if !*crate::stores::nostr_client::HAS_SIGNER.read() {
        return Err("No signer attached".to_string());
    }
    let d_tag = existing_d_tag.map(|s| s.to_string()).unwrap_or_else(|| {
        title
            .map(crate::utils::nip54::normalize_wiki_dtag)
            .unwrap_or_else(|| {
                format!("web-{}", safe_slice(url, 20).replace([':', '/'], "-"))
            })
    });
    let mut tags: Vec<Tag> = vec![
        Tag::identifier(&d_tag),
        Tag::custom(TagKind::Custom("u".into()), vec![url.to_string()]),
        Tag::custom(
            TagKind::Custom("accessed_on".into()),
            vec![Timestamp::now().as_secs().to_string()],
        ),
    ];
    if let Some(t) = title {
        tags.push(Tag::custom(
            TagKind::Custom("title".into()),
            vec![t.to_string()],
        ));
    }
    if let Some(a) = author {
        tags.push(Tag::custom(
            TagKind::Custom("author".into()),
            vec![a.to_string()],
        ));
    }
    for mime_tag in generate_mime_tags(KIND_EXTERNAL_WEB) {
        tags.push(mime_tag);
    }
    let builder = EventBuilder::new(Kind::Custom(KIND_EXTERNAL_WEB), cited_text).tags(tags);
    let event = crate::stores::publish_queue::signing::sign_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to sign: {}", e))?;
    let event_id = event.id.to_hex();
    crate::stores::publish_queue::enqueue(
        event,
        crate::stores::publish_queue::types::QueueEventType::Other("citation".to_string()),
        None,
        std::collections::HashMap::new(),
    ).await;
    Ok(event_id)
}
/// Publish a hardcopy citation (Kind 32)
/// If existing_d_tag is provided (editing), it will be preserved to replace the existing event.
pub async fn publish_hardcopy_citation(
    title: &str,
    author: &str,
    cited_text: &str,
    page_range: Option<&str>,
    publisher: Option<&str>,
    doi: Option<&str>,
    existing_d_tag: Option<&str>,
) -> StdResult<String, String> {
    let _client = crate::stores::nostr_client::get_client().ok_or("Client not initialized")?;
    if !*crate::stores::nostr_client::HAS_SIGNER.read() {
        return Err("No signer attached".to_string());
    }
    let d_tag = existing_d_tag
        .map(|s| s.to_string())
        .unwrap_or_else(|| crate::utils::nip54::normalize_wiki_dtag(title));
    let mut tags: Vec<Tag> = vec![
        Tag::identifier(&d_tag),
        Tag::custom(TagKind::Custom("title".into()), vec![title.to_string()]),
        Tag::custom(TagKind::Custom("author".into()), vec![author.to_string()]),
        Tag::custom(
            TagKind::Custom("accessed_on".into()),
            vec![Timestamp::now().as_secs().to_string()],
        ),
    ];
    if let Some(p) = page_range {
        tags.push(Tag::custom(
            TagKind::Custom("page_range".into()),
            vec![p.to_string()],
        ));
    }
    if let Some(pub_) = publisher {
        tags.push(Tag::custom(
            TagKind::Custom("published_by".into()),
            vec![pub_.to_string()],
        ));
    }
    if let Some(d) = doi {
        tags.push(Tag::custom(
            TagKind::Custom("doi".into()),
            vec![d.to_string()],
        ));
    }
    for mime_tag in generate_mime_tags(KIND_HARDCOPY) {
        tags.push(mime_tag);
    }
    let builder = EventBuilder::new(Kind::Custom(KIND_HARDCOPY), cited_text).tags(tags);
    let event = crate::stores::publish_queue::signing::sign_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to sign: {}", e))?;
    let event_id = event.id.to_hex();
    crate::stores::publish_queue::enqueue(
        event,
        crate::stores::publish_queue::types::QueueEventType::Other("citation".to_string()),
        None,
        std::collections::HashMap::new(),
    ).await;
    Ok(event_id)
}
/// Publish a prompt/AI citation (Kind 33)
/// If existing_d_tag is provided (editing), it will be preserved to replace the existing event.
pub async fn publish_prompt_citation(
    llm: &str,
    cited_text: &str,
    conversation_summary: Option<&str>,
    url: Option<&str>,
    title: Option<&str>,
    author: Option<&str>,
    existing_d_tag: Option<&str>,
) -> StdResult<String, String> {
    let _client = crate::stores::nostr_client::get_client().ok_or("Client not initialized")?;
    if !*crate::stores::nostr_client::HAS_SIGNER.read() {
        return Err("No signer attached".to_string());
    }
    let d_tag = existing_d_tag.map(|s| s.to_string()).unwrap_or_else(|| {
        format!(
            "prompt-{}-{}",
            llm.to_lowercase().replace(' ', "-"),
            Timestamp::now().as_secs(),
        )
    });
    let mut tags: Vec<Tag> = vec![
        Tag::identifier(&d_tag),
        Tag::custom(TagKind::Custom("llm".into()), vec![llm.to_string()]),
        Tag::custom(
            TagKind::Custom("accessed_on".into()),
            vec![Timestamp::now().as_secs().to_string()],
        ),
    ];
    if let Some(s) = conversation_summary {
        tags.push(Tag::custom(
            TagKind::Custom("summary".into()),
            vec![s.to_string()],
        ));
    }
    if let Some(u) = url {
        tags.push(Tag::custom(
            TagKind::Custom("u".into()),
            vec![u.to_string()],
        ));
    }
    if let Some(t) = title {
        tags.push(Tag::custom(
            TagKind::Custom("title".into()),
            vec![t.to_string()],
        ));
    }
    if let Some(a) = author {
        tags.push(Tag::custom(
            TagKind::Custom("author".into()),
            vec![a.to_string()],
        ));
    }
    for mime_tag in generate_mime_tags(KIND_PROMPT) {
        tags.push(mime_tag);
    }
    let builder = EventBuilder::new(Kind::Custom(KIND_PROMPT), cited_text).tags(tags);
    let event = crate::stores::publish_queue::signing::sign_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to sign: {}", e))?;
    let event_id = event.id.to_hex();
    crate::stores::publish_queue::enqueue(
        event,
        crate::stores::publish_queue::types::QueueEventType::Other("citation".to_string()),
        None,
        std::collections::HashMap::new(),
    ).await;
    Ok(event_id)
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::nkbip03::{
        CitationBase, ExternalWebCitation, HardcopyCitation, InternalCitation, PromptCitation,
    };
    use nostr_sdk::{EventBuilder, Keys};
    fn test_keys() -> Keys {
        Keys::generate()
    }
    fn create_test_event(kind: u16) -> NostrEvent {
        let keys = test_keys();
        EventBuilder::new(Kind::Custom(kind), "test content")
            .sign_with_keys(&keys)
            .unwrap()
    }
    fn create_citation_base() -> CitationBase {
        CitationBase {
            event_id: "abc123".to_string(),
            pubkey: "0".repeat(64),
            title: "Test Citation".to_string(),
            author: "Test Author".to_string(),
            content: "Test content".to_string(),
            accessed_on: None,
            summary: None,
            created_at: 1234567890,
        }
    }
    fn create_internal_citation() -> CachedCitation {
        CachedCitation {
            event: create_test_event(KIND_INTERNAL_REF),
            citation: Citation::Internal(InternalCitation {
                base: create_citation_base(),
                coordinate: "30:pubkey:id".to_string(),
                published_on: None,
                location: None,
                geohash: None,
            }),
            naddr: None,
            a_tag: None,
        }
    }
    fn create_external_citation() -> CachedCitation {
        CachedCitation {
            event: create_test_event(KIND_EXTERNAL_WEB),
            citation: Citation::ExternalWeb(ExternalWebCitation {
                base: create_citation_base(),
                url: "https://example.com".to_string(),
                published_on: None,
                published_by: None,
                version: None,
                open_timestamp: None,
                location: None,
                geohash: None,
            }),
            naddr: None,
            a_tag: None,
        }
    }
    fn create_hardcopy_citation() -> CachedCitation {
        CachedCitation {
            event: create_test_event(KIND_HARDCOPY),
            citation: Citation::Hardcopy(HardcopyCitation {
                base: create_citation_base(),
                page_range: Some("1-10".to_string()),
                chapter_title: None,
                editor: None,
                published_on: None,
                published_by: Some("Test Publisher".to_string()),
                published_in: None,
                doi: None,
                version: None,
                location: None,
                geohash: None,
            }),
            naddr: None,
            a_tag: None,
        }
    }
    fn create_prompt_citation() -> CachedCitation {
        CachedCitation {
            event: create_test_event(KIND_PROMPT),
            citation: Citation::Prompt(PromptCitation {
                base: create_citation_base(),
                llm: "GPT-4".to_string(),
                version: Some("2024-01".to_string()),
                url: None,
            }),
            naddr: None,
            a_tag: None,
        }
    }
    #[test]
    fn test_citation_group_total_count_empty() {
        let group = CitationGroup::default();
        assert_eq!(group.total_count(), 0);
    }
    #[test]
    fn test_citation_group_total_count_with_data() {
        let mut group = CitationGroup::default();
        group.internal.push(create_internal_citation());
        group.external.push(create_external_citation());
        group.external.push(create_external_citation());
        group.hardcopy.push(create_hardcopy_citation());
        assert_eq!(group.total_count(), 4);
    }
    #[test]
    fn test_citation_group_all() {
        let mut group = CitationGroup::default();
        group.internal.push(create_internal_citation());
        group.external.push(create_external_citation());
        let all = group.all();
        assert_eq!(all.len(), 2);
    }
    #[test]
    fn test_group_citations_empty() {
        let citations = vec![];
        let group = group_citations(citations);
        assert_eq!(group.total_count(), 0);
    }
    #[test]
    fn test_group_citations_by_type() {
        let citations = vec![
            create_internal_citation(),
            create_external_citation(),
            create_external_citation(),
            create_hardcopy_citation(),
            create_prompt_citation(),
        ];
        let group = group_citations(citations);
        assert_eq!(group.internal.len(), 1);
        assert_eq!(group.external.len(), 2);
        assert_eq!(group.hardcopy.len(), 1);
        assert_eq!(group.prompt.len(), 1);
        assert_eq!(group.total_count(), 5);
    }
}
