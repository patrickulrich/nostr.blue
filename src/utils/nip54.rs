//! NIP-54 Wiki Utilities
//!
//! Handles parsing and creation of wiki events:
//! - Kind 30818: Wiki Article (addressable)
//! - Kind 818: Merge Request (regular)
//! - Kind 30819: Wiki Redirect (addressable)
//!
//! NIP-54 defines a standard for collaborative wiki articles on Nostr
//! using AsciiDoc content with [[wikilinks]] for internal linking.

use nostr_sdk::prelude::*;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::sync::LazyLock;

// ============================================================================
// Event Kind Constants
// ============================================================================

/// Wiki article event kind (addressable)
pub const KIND_WIKI_ARTICLE: u16 = 30818;

/// Merge request event kind (regular)
pub const KIND_MERGE_REQUEST: u16 = 818;

/// Wiki redirect event kind (addressable)
pub const KIND_WIKI_REDIRECT: u16 = 30819;

// ============================================================================
// Wikilink Types
// ============================================================================

/// A parsed wikilink from content
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WikiLink {
    /// The raw matched text including brackets
    pub raw: String,
    /// The target page identifier (normalized d-tag)
    pub target: String,
    /// The display text (if different from target)
    pub display: Option<String>,
}

impl WikiLink {
    /// Get the display text, falling back to target if not specified
    pub fn display_text(&self) -> &str {
        self.display.as_deref().unwrap_or(&self.target)
    }
}

// ============================================================================
// D-tag Normalization (NIP-54 spec)
// ============================================================================

/// Normalize a string to a valid NIP-54 wiki d-tag
///
/// Per NIP-54:
/// - Any non-letter character MUST be converted to a `-`
/// - All letters MUST be converted to lowercase
///
/// Examples:
/// - "Hello World!" -> "hello-world"
/// - "Bitcoin & Lightning" -> "bitcoin-lightning"
/// - "NIP-54" -> "nip--" (numbers become dashes)
/// - "Café" -> "caf-" (accented letters preserved, é -> e in lowercase)
pub fn normalize_wiki_dtag(input: &str) -> String {
    input
        .chars()
        .map(|c| {
            if c.is_alphabetic() {
                // Convert to lowercase, handling Unicode properly
                c.to_lowercase().next().unwrap_or('-')
            } else {
                '-'
            }
        })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

/// Check if a string is already a valid normalized wiki d-tag
pub fn is_normalized_dtag(input: &str) -> bool {
    if input.is_empty() {
        return false;
    }

    // Must not start or end with dash
    if input.starts_with('-') || input.ends_with('-') {
        return false;
    }

    // Must be all lowercase letters and single dashes (no consecutive dashes)
    let mut prev_dash = false;
    for c in input.chars() {
        if c == '-' {
            if prev_dash {
                return false; // consecutive dashes
            }
            prev_dash = true;
        } else if c.is_alphabetic() && c.is_lowercase() {
            prev_dash = false;
        } else {
            return false; // invalid character
        }
    }

    true
}

// ============================================================================
// Wikilink Parsing
// ============================================================================

// Lazy-initialized regex for wikilinks
// Matches: [[Target Page]] or [[target page|display text]]
static WIKILINK_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\[\[([^\]|]+)(?:\|([^\]]+))?\]\]").expect("Invalid wikilink regex")
});

/// Extract all wikilinks from content
///
/// Supports two formats:
/// 1. `[[Target Page]]` - links to "target-page", displays "Target Page"
/// 2. `[[target page|see this]]` - links to "target-page", displays "see this"
pub fn extract_wikilinks(content: &str) -> Vec<WikiLink> {
    WIKILINK_REGEX
        .captures_iter(content)
        .map(|cap| {
            let raw = cap
                .get(0)
                .map(|m| m.as_str().to_string())
                .unwrap_or_default();
            let target_raw = cap.get(1).map(|m| m.as_str()).unwrap_or("");
            let display = cap.get(2).map(|m| m.as_str().to_string());

            WikiLink {
                raw,
                target: normalize_wiki_dtag(target_raw),
                display,
            }
        })
        .collect()
}

/// Convert wikilinks to nostr tags for publishing
///
/// Creates tags in the format: ["w", "<normalized-target>"]
/// This allows querying for backlinks using the w tag
pub fn wikilinks_to_tags(links: &[WikiLink]) -> Vec<Tag> {
    let mut seen = std::collections::HashSet::new();
    links
        .iter()
        .filter_map(|link| {
            if seen.insert(&link.target) {
                Some(Tag::custom(
                    TagKind::Custom("w".into()),
                    vec![link.target.clone()],
                ))
            } else {
                None // Skip duplicates
            }
        })
        .collect()
}

// ============================================================================
// Wikilink Rendering
// ============================================================================

/// Render wikilinks in content to clickable HTML links
///
/// Converts `[[Target Page]]` to `<a href="/wiki/target-page">Target Page</a>`
pub fn render_wikilinks_to_html(content: &str) -> String {
    WIKILINK_REGEX
        .replace_all(content, |caps: &regex::Captures| {
            let target_raw = caps.get(1).map(|m| m.as_str()).unwrap_or("");
            let display = caps.get(2).map(|m| m.as_str()).unwrap_or(target_raw);
            let target = normalize_wiki_dtag(target_raw);

            format!(
                r#"<a href="/wiki/{}" class="wikilink" data-target="{}">{}</a>"#,
                target,
                target,
                html_escape(display)
            )
        })
        .to_string()
}

// ============================================================================
// Wiki Event Structures
// ============================================================================

/// A wiki article parsed from a Kind 30818 event
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WikiArticle {
    /// Event ID (hex)
    pub event_id: String,
    /// Author pubkey (hex)
    pub pubkey: String,
    /// Normalized identifier (d-tag)
    pub identifier: String,
    /// Display title (from title tag, or d-tag)
    pub title: String,
    /// Article summary (optional)
    pub summary: Option<String>,
    /// AsciiDoc content
    pub content: String,
    /// Forward wikilinks in this article
    pub forward_links: Vec<String>,
    /// Fork source coordinate (if forked)
    pub fork_source: Option<String>,
    /// Defer to another article (if set)
    pub defer_to: Option<String>,
    /// NIP-19 naddr
    pub naddr: String,
    /// Coordinate string: "30818:pubkey:identifier"
    pub coordinate: String,
    /// Created timestamp
    pub created_at: u64,
}

/// A wiki redirect parsed from a Kind 30819 event
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WikiRedirect {
    /// Event ID (hex)
    pub event_id: String,
    /// Author pubkey (hex)
    pub pubkey: String,
    /// Source identifier (d-tag)
    pub from: String,
    /// Target identifier (from content or a-tag)
    pub to: String,
    /// Created timestamp
    pub created_at: u64,
}

/// A merge request parsed from a Kind 818 event
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WikiMergeRequest {
    /// Event ID (hex)
    pub event_id: String,
    /// Requester pubkey (hex)
    pub pubkey: String,
    /// Target article coordinate (a-tag)
    pub destination: String,
    /// Destination author pubkey
    pub destination_author: String,
    /// Source article event ID (e-tag with "source" marker)
    pub source_event_id: String,
    /// Base version event ID (optional, for diff)
    pub base_version: Option<String>,
    /// Request message/explanation
    pub content: String,
    /// Created timestamp
    pub created_at: u64,
}

// ============================================================================
// Parsing Functions
// ============================================================================

/// Parse a wiki article from a Kind 30818 event
pub fn parse_wiki_article(event: &Event) -> Result<WikiArticle, String> {
    if event.kind.as_u16() != KIND_WIKI_ARTICLE {
        return Err(format!(
            "Expected kind {}, got {}",
            KIND_WIKI_ARTICLE,
            event.kind.as_u16()
        ));
    }

    let pubkey = event.pubkey.to_hex();

    // Extract required d-tag (identifier)
    let identifier = get_tag_value(event, "d").ok_or("Missing required 'd' tag (identifier)")?;

    // Validate identifier is normalized
    if !is_normalized_dtag(&identifier) {
        // Accept but log warning - some implementations may not normalize
        log::warn!("Wiki article d-tag is not normalized: {}", identifier);
    }

    // Build coordinate and naddr
    let coordinate = format!("{}:{}:{}", KIND_WIKI_ARTICLE, pubkey, identifier);
    let naddr = build_wiki_naddr(&pubkey, &identifier).unwrap_or_default();

    // Get title (prefer title tag, fall back to identifier)
    let title = get_tag_value(event, "title").unwrap_or_else(|| identifier.clone());

    // Optional summary
    let summary = get_tag_value(event, "summary");

    // Extract forward wikilinks from content
    let forward_links = extract_wikilinks(&event.content)
        .into_iter()
        .map(|w| w.target)
        .collect();

    // Check for fork/defer markers
    let fork_source =
        get_tag_with_marker(event, "a", "fork").or_else(|| get_tag_with_marker(event, "e", "fork"));
    let defer_to = get_tag_with_marker(event, "a", "defer")
        .or_else(|| get_tag_with_marker(event, "e", "defer"));

    Ok(WikiArticle {
        event_id: event.id.to_hex(),
        pubkey,
        identifier,
        title,
        summary,
        content: event.content.clone(),
        forward_links,
        fork_source,
        defer_to,
        naddr,
        coordinate,
        created_at: event.created_at.as_secs(),
    })
}

/// Parse a wiki redirect from a Kind 30819 event
pub fn parse_wiki_redirect(event: &Event) -> Result<WikiRedirect, String> {
    if event.kind.as_u16() != KIND_WIKI_REDIRECT {
        return Err(format!(
            "Expected kind {}, got {}",
            KIND_WIKI_REDIRECT,
            event.kind.as_u16()
        ));
    }

    let pubkey = event.pubkey.to_hex();

    // Extract source d-tag
    let from = get_tag_value(event, "d").ok_or("Missing required 'd' tag")?;

    // Extract target - from content or a-tag
    let to = if !event.content.trim().is_empty() {
        normalize_wiki_dtag(&event.content)
    } else {
        get_tag_value(event, "a")
            .and_then(|coord| coord.split(':').next_back().map(|s| s.to_string()))
            .ok_or("Missing redirect target")?
    };

    Ok(WikiRedirect {
        event_id: event.id.to_hex(),
        pubkey,
        from,
        to,
        created_at: event.created_at.as_secs(),
    })
}

/// Parse a merge request from a Kind 818 event
pub fn parse_merge_request(event: &Event) -> Result<WikiMergeRequest, String> {
    if event.kind.as_u16() != KIND_MERGE_REQUEST {
        return Err(format!(
            "Expected kind {}, got {}",
            KIND_MERGE_REQUEST,
            event.kind.as_u16()
        ));
    }

    let pubkey = event.pubkey.to_hex();

    // Extract destination (a-tag)
    let destination = get_tag_value(event, "a").ok_or("Missing required 'a' tag (destination)")?;

    // Extract destination author (p-tag)
    let destination_author =
        get_tag_value(event, "p").ok_or("Missing required 'p' tag (destination author)")?;

    // Extract source event ID (e-tag with "source" marker)
    let source_event_id =
        get_tag_with_marker(event, "e", "source").ok_or("Missing source event ID")?;

    // Optional base version
    let base_version = get_first_e_tag_without_marker(event);

    Ok(WikiMergeRequest {
        event_id: event.id.to_hex(),
        pubkey,
        destination,
        destination_author,
        source_event_id,
        base_version,
        content: event.content.clone(),
        created_at: event.created_at.as_secs(),
    })
}

// ============================================================================
// NIP-19 Functions
// ============================================================================

/// Build a NIP-19 naddr for a wiki article
pub fn build_wiki_naddr(pubkey: &str, identifier: &str) -> Option<String> {
    let pk = PublicKey::from_hex(pubkey).ok()?;
    let coordinate = Coordinate::new(Kind::Custom(KIND_WIKI_ARTICLE), pk).identifier(identifier);
    let nip19 = Nip19Coordinate::new(coordinate, vec![]);
    nip19.to_bech32().ok()
}

// ============================================================================
// Filter Builders
// ============================================================================

/// Build a filter for wiki articles
pub fn wiki_articles_filter(limit: usize) -> Filter {
    Filter::new()
        .kind(Kind::Custom(KIND_WIKI_ARTICLE))
        .limit(limit)
}

/// Build a filter for backlinks (articles linking to a target)
pub fn wiki_backlinks_filter(identifier: &str, limit: usize) -> Filter {
    let normalized = normalize_wiki_dtag(identifier);
    Filter::new()
        .kind(Kind::Custom(KIND_WIKI_ARTICLE))
        .custom_tag(SingleLetterTag::lowercase(Alphabet::W), normalized)
        .limit(limit)
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Simple HTML escaping for display text
fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

/// Get first value of a tag by name
fn get_tag_value(event: &Event, tag_name: &str) -> Option<String> {
    event
        .tags
        .iter()
        .find(|t| t.as_slice().first().map(|s| s.as_str()) == Some(tag_name))
        .and_then(|t| t.as_slice().get(1).map(|s| s.to_string()))
}

/// Get tag value with a specific marker (e.g., ["a", "...", "relay", "fork"])
fn get_tag_with_marker(event: &Event, tag_name: &str, marker: &str) -> Option<String> {
    event
        .tags
        .iter()
        .find(|t| {
            let slice = t.as_slice();
            slice.first().map(|s| s.as_str()) == Some(tag_name)
                && slice.iter().any(|s| s.as_str() == marker)
        })
        .and_then(|t| t.as_slice().get(1).map(|s| s.to_string()))
}

/// Get first e-tag that doesn't have "source" marker
fn get_first_e_tag_without_marker(event: &Event) -> Option<String> {
    event
        .tags
        .iter()
        .find(|t| {
            let slice = t.as_slice();
            slice.first().map(|s| s.as_str()) == Some("e")
                && !slice.iter().any(|s| s.as_str() == "source")
        })
        .and_then(|t| t.as_slice().get(1).map(|s| s.to_string()))
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_wiki_dtag() {
        assert_eq!(normalize_wiki_dtag("Hello World!"), "hello-world");
        assert_eq!(
            normalize_wiki_dtag("Bitcoin & Lightning"),
            "bitcoin-lightning"
        );
        assert_eq!(normalize_wiki_dtag("NIP-54"), "nip");
        assert_eq!(normalize_wiki_dtag("  Spaces  "), "spaces");
        assert_eq!(normalize_wiki_dtag("CamelCase"), "camelcase");
        assert_eq!(normalize_wiki_dtag("multiple---dashes"), "multiple-dashes");
        assert_eq!(normalize_wiki_dtag("123numbers456"), "numbers");
    }

    #[test]
    fn test_is_normalized_dtag() {
        assert!(is_normalized_dtag("hello-world"));
        assert!(is_normalized_dtag("bitcoin"));
        assert!(!is_normalized_dtag("-leading"));
        assert!(!is_normalized_dtag("trailing-"));
        assert!(!is_normalized_dtag("has--double"));
        assert!(!is_normalized_dtag("HasUppercase"));
        assert!(!is_normalized_dtag("has123numbers"));
        assert!(!is_normalized_dtag(""));
    }

    #[test]
    fn test_extract_wikilinks() {
        let content = "Check out [[Bitcoin]] and [[Nostr Protocol|Nostr]] for more info.";
        let links = extract_wikilinks(content);

        assert_eq!(links.len(), 2);
        assert_eq!(links[0].target, "bitcoin");
        assert_eq!(links[0].display, None);
        assert_eq!(links[1].target, "nostr-protocol");
        assert_eq!(links[1].display, Some("Nostr".to_string()));
    }

    #[test]
    fn test_render_wikilinks_to_html() {
        let content = "See [[Bitcoin]] for details.";
        let rendered = render_wikilinks_to_html(content);
        assert!(rendered.contains(r#"href="/wiki/bitcoin""#));
        assert!(rendered.contains(">Bitcoin</a>"));
    }

    #[test]
    fn test_wikilink_display_text() {
        let link = WikiLink {
            raw: "[[test|Display]]".to_string(),
            target: "test".to_string(),
            display: Some("Display".to_string()),
        };
        assert_eq!(link.display_text(), "Display");

        let link_no_display = WikiLink {
            raw: "[[test]]".to_string(),
            target: "test".to_string(),
            display: None,
        };
        assert_eq!(link_no_display.display_text(), "test");
    }

    #[test]
    fn test_build_wiki_naddr() {
        let pubkey = "79be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798";
        let identifier = "bitcoin";

        let naddr = build_wiki_naddr(pubkey, identifier);
        assert!(naddr.is_some());
        assert!(naddr.unwrap().starts_with("naddr1"));
    }
}
