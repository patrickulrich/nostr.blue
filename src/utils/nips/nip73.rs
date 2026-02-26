//! NIP-73: External Content IDs
//!
//! Utilities for extracting and displaying external content references from Nostr events.
//! Leverages the rust-nostr SDK's built-in NIP-73 support.
use nostr_sdk::prelude::*;
pub use nostr::nips::nip73::ExternalContentId;
/// Extract external content references from an event's `i` tags
///
/// Returns a vector of (ExternalContentId, Option<Url>) tuples where the URL
/// is the optional hint provided in the tag.
pub fn extract_external_content(event: &Event) -> Vec<(ExternalContentId, Option<Url>)> {
    let mut results = Vec::new();
    for tag in event.tags.iter() {
        if let Some(TagStandard::ExternalContent { content, hint, .. }) = tag
            .as_standardized()
        {
            results.push((content.clone(), hint.clone()));
        }
    }
    results
}
/// Get a human-readable display name for an external content type
pub fn get_display_name(content: &ExternalContentId) -> String {
    match content {
        ExternalContentId::Url(url) => format!("Link: {}", shorten_url(url.as_str())),
        ExternalContentId::Hashtag(tag) => format!("#{}", tag),
        ExternalContentId::Book(isbn) => format!("ISBN: {}", isbn),
        ExternalContentId::Paper(doi) => format!("DOI: {}", doi),
        ExternalContentId::Movie(isan) => format!("ISAN: {}", isan),
        ExternalContentId::PodcastFeed(guid) => format!("Podcast: {}", truncate_id(guid)),
        ExternalContentId::PodcastEpisode(guid) => {
            format!("Episode: {}", truncate_id(guid))
        }
        ExternalContentId::PodcastPublisher(guid) => {
            format!("Publisher: {}", truncate_id(guid))
        }
        ExternalContentId::Geohash(hash) => format!("Location: {}", hash),
        ExternalContentId::BlockchainTransaction { chain, transaction_hash, .. } => {
            if chain == "bitcoin" {
                format!("TX: {}", truncate_id(transaction_hash))
            } else {
                format!("{} TX: {}", chain, truncate_id(transaction_hash))
            }
        }
        ExternalContentId::BlockchainAddress { chain, address, .. } => {
            if chain == "bitcoin" {
                format!("Address: {}", truncate_id(address))
            } else {
                format!("{} Address: {}", chain, truncate_id(address))
            }
        }
    }
}
/// Get an external explorer/reference URL for the content
///
/// Returns None for content types that don't have a standard external service.
pub fn get_explorer_url(content: &ExternalContentId) -> Option<String> {
    match content {
        ExternalContentId::Url(url) => Some(url.to_string()),
        ExternalContentId::Hashtag(tag) => {
            Some(format!("/topics/t/{}", tag))
        }
        ExternalContentId::Book(isbn) => {
            Some(format!("https://openlibrary.org/isbn/{}", isbn))
        }
        ExternalContentId::Paper(doi) => Some(format!("https://doi.org/{}", doi)),
        ExternalContentId::Movie(isan) => {
            Some(format!("https://web.isan.org/public/en/search?isan={}", isan))
        }
        ExternalContentId::PodcastFeed(guid) => {
            Some(format!("https://podcastindex.org/podcast/{}", guid))
        }
        ExternalContentId::PodcastEpisode(guid) => {
            Some(format!("https://podcastindex.org/search?q={}", guid))
        }
        ExternalContentId::PodcastPublisher(_) => None,
        ExternalContentId::Geohash(hash) => Some(format!("https://geohash.org/{}", hash)),
        ExternalContentId::BlockchainTransaction { chain, transaction_hash, .. } => {
            if chain == "bitcoin" {
                Some(format!("https://mempool.space/tx/{}", transaction_hash))
            } else if chain == "ethereum" {
                Some(format!("https://etherscan.io/tx/{}", transaction_hash))
            } else {
                None
            }
        }
        ExternalContentId::BlockchainAddress { chain, address, .. } => {
            if chain == "bitcoin" {
                Some(format!("https://mempool.space/address/{}", address))
            } else if chain == "ethereum" {
                Some(format!("https://etherscan.io/address/{}", address))
            } else {
                None
            }
        }
    }
}
/// Extract the raw identifier from an ExternalContentId
pub fn get_raw_identifier(content: &ExternalContentId) -> String {
    match content {
        ExternalContentId::Url(url) => url.to_string(),
        ExternalContentId::Hashtag(tag) => tag.clone(),
        ExternalContentId::Book(isbn) => isbn.clone(),
        ExternalContentId::Paper(doi) => doi.clone(),
        ExternalContentId::Movie(isan) => isan.clone(),
        ExternalContentId::PodcastFeed(guid) => guid.clone(),
        ExternalContentId::PodcastEpisode(guid) => guid.clone(),
        ExternalContentId::PodcastPublisher(guid) => guid.clone(),
        ExternalContentId::Geohash(hash) => hash.clone(),
        ExternalContentId::BlockchainTransaction { transaction_hash, .. } => {
            transaction_hash.clone()
        }
        ExternalContentId::BlockchainAddress { address, .. } => address.clone(),
    }
}
/// Truncate an ID for display (show first and last 4 characters)
fn truncate_id(id: &str) -> String {
    if id.len() > 12 {
        format!("{}...{}", &id[0..6], &id[id.len() - 4..])
    } else {
        id.to_string()
    }
}
/// Shorten a URL for display
fn shorten_url(url: &str) -> String {
    let without_protocol = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
        .unwrap_or(url);
    if without_protocol.len() > 40 {
        format!("{}...", &without_protocol[0..37])
    } else {
        without_protocol.to_string()
    }
}
