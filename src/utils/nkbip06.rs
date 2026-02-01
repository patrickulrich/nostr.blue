//! NKBIP-06 Nostr MIME Type Utilities
//!
//! Handles generation and parsing of Nostr-specific MIME type tags:
//! - `m` tag: Standard MIME type (text/plain, application/json, etc.)
//! - `M` tag: Nostr-specific categorization (category/use-case/replaceability)
//!
//! The M tag format is: `category/use-case/replaceability`
//! - category: note, article, meta-data, media, etc.
//! - use-case: microblog, comment, publication-content, wiki, index, etc.
//! - replaceability: replaceable, nonreplaceable
use nostr_sdk::prelude::*;
use serde::{Deserialize, Serialize};
/// Category: Short text notes
pub const CATEGORY_NOTE: &str = "note";
/// Category: Long-form articles/content
pub const CATEGORY_ARTICLE: &str = "article";
/// Category: Meta-data/index events
pub const CATEGORY_METADATA: &str = "meta-data";
/// Category: Media content
pub const CATEGORY_MEDIA: &str = "media";
/// Category: Social interactions
pub const CATEGORY_SOCIAL: &str = "social";
/// Use case: Microblog post (kind 1)
pub const USE_MICROBLOG: &str = "microblog";
/// Use case: Comment (kind 1111)
pub const USE_COMMENT: &str = "comment";
/// Use case: Publication index (kind 30040)
pub const USE_INDEX: &str = "index";
/// Use case: Publication content/section (kind 30041)
pub const USE_PUBLICATION_CONTENT: &str = "publication-content";
/// Use case: Wiki article (kind 30818)
pub const USE_WIKI: &str = "wiki";
/// Use case: Long-form content (kind 30023)
pub const USE_LONGFORM: &str = "long-form";
/// Use case: Vector embedding (kind 1987)
pub const USE_EMBEDDING: &str = "embedding";
/// Use case: Citation reference
pub const USE_CITATION: &str = "citation";
/// Use case: Directory/drive
pub const USE_DIRECTORY: &str = "directory";
/// Use case: Badge
pub const USE_BADGE: &str = "badge";
/// Event is replaceable (addressable events 30000-39999, or 10000-19999)
pub const REPLACEABILITY_REPLACEABLE: &str = "replaceable";
/// Event is not replaceable (regular events)
pub const REPLACEABILITY_NONREPLACEABLE: &str = "nonreplaceable";
/// Nostr MIME type information
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NostrMimeType {
    /// Standard MIME type (m tag)
    pub standard: String,
    /// Category (note, article, meta-data, etc.)
    pub category: String,
    /// Use case (microblog, wiki, index, etc.)
    pub use_case: String,
    /// Replaceability (replaceable, nonreplaceable)
    pub replaceability: String,
}
impl NostrMimeType {
    /// Create a new NostrMimeType
    pub fn new(
        standard: &str,
        category: &str,
        use_case: &str,
        replaceability: &str,
    ) -> Self {
        Self {
            standard: standard.to_string(),
            category: category.to_string(),
            use_case: use_case.to_string(),
            replaceability: replaceability.to_string(),
        }
    }
    /// Get the M tag value (category/use-case/replaceability)
    pub fn m_tag_value(&self) -> String {
        format!("{}/{}/{}", self.category, self.use_case, self.replaceability)
    }
    /// Create tags for this MIME type
    pub fn to_tags(&self) -> Vec<Tag> {
        vec![
            Tag::custom(TagKind::Custom("m".into()), vec![self.standard.clone()]),
            Tag::custom(TagKind::Custom("M".into()), vec![self.m_tag_value()]),
        ]
    }
}
/// Get NostrMimeType for a given event kind
pub fn mime_type_for_kind(kind: u16) -> NostrMimeType {
    match kind {
        1 => {
            NostrMimeType::new(
                "text/plain",
                CATEGORY_NOTE,
                USE_MICROBLOG,
                REPLACEABILITY_NONREPLACEABLE,
            )
        }
        1111 => {
            NostrMimeType::new(
                "text/plain",
                CATEGORY_NOTE,
                USE_COMMENT,
                REPLACEABILITY_NONREPLACEABLE,
            )
        }
        1987 => {
            NostrMimeType::new(
                "application/json",
                CATEGORY_METADATA,
                USE_EMBEDDING,
                REPLACEABILITY_NONREPLACEABLE,
            )
        }
        30 => {
            NostrMimeType::new(
                "text/plain",
                CATEGORY_METADATA,
                USE_CITATION,
                REPLACEABILITY_REPLACEABLE,
            )
        }
        31 => {
            NostrMimeType::new(
                "text/plain",
                CATEGORY_METADATA,
                USE_CITATION,
                REPLACEABILITY_REPLACEABLE,
            )
        }
        32 => {
            NostrMimeType::new(
                "text/plain",
                CATEGORY_METADATA,
                USE_CITATION,
                REPLACEABILITY_REPLACEABLE,
            )
        }
        33 => {
            NostrMimeType::new(
                "text/plain",
                CATEGORY_METADATA,
                USE_CITATION,
                REPLACEABILITY_REPLACEABLE,
            )
        }
        30023 => {
            NostrMimeType::new(
                "text/plain",
                CATEGORY_ARTICLE,
                USE_LONGFORM,
                REPLACEABILITY_REPLACEABLE,
            )
        }
        30040 => {
            NostrMimeType::new(
                "application/json",
                CATEGORY_METADATA,
                USE_INDEX,
                REPLACEABILITY_REPLACEABLE,
            )
        }
        30041 => {
            NostrMimeType::new(
                "text/plain",
                CATEGORY_ARTICLE,
                USE_PUBLICATION_CONTENT,
                REPLACEABILITY_REPLACEABLE,
            )
        }
        30042 => {
            NostrMimeType::new(
                "application/json",
                CATEGORY_METADATA,
                USE_DIRECTORY,
                REPLACEABILITY_REPLACEABLE,
            )
        }
        30043 => {
            NostrMimeType::new(
                "application/json",
                CATEGORY_METADATA,
                USE_DIRECTORY,
                REPLACEABILITY_REPLACEABLE,
            )
        }
        30044 => {
            NostrMimeType::new(
                "application/json",
                CATEGORY_METADATA,
                USE_DIRECTORY,
                REPLACEABILITY_REPLACEABLE,
            )
        }
        30045 => {
            NostrMimeType::new(
                "application/json",
                CATEGORY_METADATA,
                USE_DIRECTORY,
                REPLACEABILITY_REPLACEABLE,
            )
        }
        30818 => {
            NostrMimeType::new(
                "text/plain",
                CATEGORY_ARTICLE,
                USE_WIKI,
                REPLACEABILITY_REPLACEABLE,
            )
        }
        30009 => {
            NostrMimeType::new(
                "application/json",
                CATEGORY_METADATA,
                USE_BADGE,
                REPLACEABILITY_REPLACEABLE,
            )
        }
        8 => {
            NostrMimeType::new(
                "application/json",
                CATEGORY_SOCIAL,
                USE_BADGE,
                REPLACEABILITY_NONREPLACEABLE,
            )
        }
        _ => {
            let replaceability = if is_replaceable_kind(kind) {
                REPLACEABILITY_REPLACEABLE
            } else {
                REPLACEABILITY_NONREPLACEABLE
            };
            let category = if (30000..40000).contains(&kind) {
                CATEGORY_ARTICLE
            } else if (10000..20000).contains(&kind) {
                CATEGORY_METADATA
            } else {
                CATEGORY_NOTE
            };
            NostrMimeType::new("text/plain", category, "unknown", replaceability)
        }
    }
}
/// Check if a kind is replaceable
pub fn is_replaceable_kind(kind: u16) -> bool {
    (10000..20000).contains(&kind) || (30000..40000).contains(&kind)
}
/// Generate m and M tags for an event kind
pub fn generate_mime_tags(kind: u16) -> Vec<Tag> {
    mime_type_for_kind(kind).to_tags()
}
/// Generate just the m tag (standard MIME type)
pub fn generate_m_tag(kind: u16) -> Tag {
    let mime = mime_type_for_kind(kind);
    Tag::custom(TagKind::Custom("m".into()), vec![mime.standard])
}
/// Generate just the M tag (Nostr MIME type)
pub fn generate_nostr_m_tag(kind: u16) -> Tag {
    let mime = mime_type_for_kind(kind);
    Tag::custom(TagKind::Custom("M".into()), vec![mime.m_tag_value()])
}
/// Build a MIME filter tag value for filtering events by category/use-case
///
/// # Arguments
/// * `category` - The content category (note, article, meta-data, etc.)
/// * `use_case` - The use case (microblog, wiki, index, etc.)
/// * `replaceable` - Whether the event is replaceable
///
/// # Example
/// ```
/// let tag_value = build_mime_filter_tag("article", "wiki", true);
/// // Returns "article/wiki/replaceable"
/// ```
pub fn build_mime_filter_tag(
    category: &str,
    use_case: &str,
    replaceable: bool,
) -> String {
    let replaceability = if replaceable {
        REPLACEABILITY_REPLACEABLE
    } else {
        REPLACEABILITY_NONREPLACEABLE
    };
    format!("{}/{}/{}", category, use_case, replaceability)
}
/// Build a MIME filter tag value for a specific event kind
pub fn build_mime_filter_tag_for_kind(kind: u16) -> String {
    mime_type_for_kind(kind).m_tag_value()
}
/// Parse a Nostr MIME type from an M tag value
pub fn parse_nostr_mime(m_value: &str) -> Option<NostrMimeType> {
    let parts: Vec<&str> = m_value.split('/').collect();
    if parts.len() != 3 {
        return None;
    }
    Some(NostrMimeType {
        standard: String::new(),
        category: parts[0].to_string(),
        use_case: parts[1].to_string(),
        replaceability: parts[2].to_string(),
    })
}
/// Extract MIME information from event tags
pub fn extract_mime_from_event(event: &Event) -> Option<NostrMimeType> {
    let mut standard = None;
    let mut nostr_mime = None;
    for tag in event.tags.iter() {
        let slice = tag.as_slice();
        match slice.first().map(|s| s.as_str()) {
            Some("m") => {
                standard = slice.get(1).map(|s| s.to_string());
            }
            Some("M") => {
                if let Some(value) = slice.get(1) {
                    nostr_mime = parse_nostr_mime(value);
                }
            }
            _ => {}
        }
    }
    nostr_mime
        .map(|mut mime| {
            if let Some(std) = standard {
                mime.standard = std;
            }
            mime
        })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_mime_type_for_kind_30040() {
        let mime = mime_type_for_kind(30040);
        assert_eq!(mime.category, "meta-data");
        assert_eq!(mime.use_case, "index");
        assert_eq!(mime.replaceability, "replaceable");
        assert_eq!(mime.m_tag_value(), "meta-data/index/replaceable");
    }
    #[test]
    fn test_mime_type_for_kind_30041() {
        let mime = mime_type_for_kind(30041);
        assert_eq!(mime.category, "article");
        assert_eq!(mime.use_case, "publication-content");
        assert_eq!(mime.replaceability, "replaceable");
    }
    #[test]
    fn test_mime_type_for_kind_30818() {
        let mime = mime_type_for_kind(30818);
        assert_eq!(mime.category, "article");
        assert_eq!(mime.use_case, "wiki");
        assert_eq!(mime.replaceability, "replaceable");
    }
    #[test]
    fn test_mime_type_for_kind_1() {
        let mime = mime_type_for_kind(1);
        assert_eq!(mime.category, "note");
        assert_eq!(mime.use_case, "microblog");
        assert_eq!(mime.replaceability, "nonreplaceable");
    }
    #[test]
    fn test_is_replaceable_kind() {
        assert!(!is_replaceable_kind(1));
        assert!(!is_replaceable_kind(1111));
        assert!(is_replaceable_kind(10000));
        assert!(is_replaceable_kind(10002));
        assert!(is_replaceable_kind(30000));
        assert!(is_replaceable_kind(30818));
        assert!(!is_replaceable_kind(40000));
    }
    #[test]
    fn test_parse_nostr_mime() {
        let parsed = parse_nostr_mime("article/wiki/replaceable");
        assert!(parsed.is_some());
        let mime = parsed.unwrap();
        assert_eq!(mime.category, "article");
        assert_eq!(mime.use_case, "wiki");
        assert_eq!(mime.replaceability, "replaceable");
    }
    #[test]
    fn test_parse_nostr_mime_invalid() {
        assert!(parse_nostr_mime("invalid").is_none());
        assert!(parse_nostr_mime("only/two").is_none());
        assert!(parse_nostr_mime("too/many/parts/here").is_none());
    }
    #[test]
    fn test_generate_mime_tags() {
        let tags = generate_mime_tags(30818);
        assert_eq!(tags.len(), 2);
    }
}
