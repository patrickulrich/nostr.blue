//! NKBIP-08 Book Wikilink Utilities
//!
//! Handles parsing and rendering of book:: macro syntax for internal publication references.
//!
//! Syntax: `book::[<collection>:]<title> [<chapter>][:<section_range>] [| <version>]`
//!
//! Indexable Tags:
//! - `C`: Collection (e.g., "bible", "shakespeare-complete")
//! - `T`: Title (mandatory)
//! - `c`: Chapter
//! - `s`: Section
//! - `v`: Version/edition
//!
//! Examples:
//! - `book::bible:genesis 2:4-9 | kjv` → Genesis 2:4-9 KJV
//! - `book::jane-eyre 21:8 | penguin-classics` → Chapter 21, paragraph 8
use nostr_sdk::prelude::*;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::sync::LazyLock;
/// Matches book:: macro syntax
/// Groups: 1=collection (optional), 2=title, 3=chapter (optional), 4=sections (optional), 5=version (optional)
static BOOK_WIKILINK_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
            r"book::(?:([a-z0-9-]+):)?([a-z0-9-]+)(?:\s+(\d+))?(?::([0-9,-]+))?(?:\s*\|\s*([a-z0-9-]+))?",
        )
        .unwrap()
});
/// Matches book:: macros in content for extraction
static BOOK_EXTRACT_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
            r"book::(?:[a-z0-9-]+:)?[a-z0-9-]+(?:\s+\d+)?(?::[0-9,-]+)?(?:\s*\|\s*[a-z0-9-]+)?",
        )
        .unwrap()
});
/// Collection tag (uppercase C)
pub const TAG_COLLECTION: &str = "C";
/// Title tag (uppercase T)
pub const TAG_TITLE: &str = "T";
/// Chapter tag (lowercase c)
pub const TAG_CHAPTER: &str = "c";
/// Section tag (lowercase s)
pub const TAG_SECTION: &str = "s";
/// Version tag (lowercase v)
pub const TAG_VERSION: &str = "v";
/// Parsed book wikilink reference
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BookReference {
    /// Raw book:: macro string
    pub raw: String,
    /// Collection identifier (C tag) - e.g., "bible", "shakespeare-complete"
    pub collection: Option<String>,
    /// Title identifier (T tag) - mandatory, e.g., "genesis", "jane-eyre"
    pub title: String,
    /// Chapter number (c tag)
    pub chapter: Option<String>,
    /// Section range(s) (s tag) - e.g., "4-9", "1,3,5"
    pub sections: Vec<String>,
    /// Version/edition (v tag) - e.g., "kjv", "penguin-classics"
    pub version: Option<String>,
}
impl BookReference {
    /// Create a new BookReference
    pub fn new(title: &str) -> Self {
        Self {
            raw: format!("book::{}", title),
            collection: None,
            title: title.to_string(),
            chapter: None,
            sections: Vec::new(),
            version: None,
        }
    }
    /// Set collection
    pub fn with_collection(mut self, collection: &str) -> Self {
        self.collection = Some(collection.to_string());
        self
    }
    /// Set chapter
    pub fn with_chapter(mut self, chapter: &str) -> Self {
        self.chapter = Some(chapter.to_string());
        self
    }
    /// Add section
    pub fn with_section(mut self, section: &str) -> Self {
        self.sections.push(section.to_string());
        self
    }
    /// Set version
    pub fn with_version(mut self, version: &str) -> Self {
        self.version = Some(version.to_string());
        self
    }
    /// Generate display text for the reference
    pub fn display_text(&self) -> String {
        let mut parts = Vec::new();
        let title_display = self.title.replace('-', " ");
        let title_display = title_display
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
        parts.push(title_display);
        if let Some(ref chapter) = self.chapter {
            let mut chapter_str = chapter.clone();
            if !self.sections.is_empty() {
                chapter_str.push(':');
                chapter_str.push_str(&self.sections.join(","));
            }
            parts.push(chapter_str);
        } else if !self.sections.is_empty() {
            parts.push(self.sections.join(","));
        }
        if let Some(ref version) = self.version {
            parts.push(format!("({})", version.to_uppercase().replace('-', " ")));
        }
        parts.join(" ")
    }
    /// Convert to nostr tags for indexing
    pub fn to_tags(&self) -> Vec<Tag> {
        let mut tags = Vec::new();
        tags.push(
            Tag::custom(TagKind::Custom(TAG_TITLE.into()), vec![self.title.clone()]),
        );
        if let Some(ref collection) = self.collection {
            tags.push(
                Tag::custom(
                    TagKind::Custom(TAG_COLLECTION.into()),
                    vec![collection.clone()],
                ),
            );
        }
        if let Some(ref chapter) = self.chapter {
            tags.push(
                Tag::custom(TagKind::Custom(TAG_CHAPTER.into()), vec![chapter.clone()]),
            );
        }
        for section in &self.sections {
            tags.push(
                Tag::custom(TagKind::Custom(TAG_SECTION.into()), vec![section.clone()]),
            );
        }
        if let Some(ref version) = self.version {
            tags.push(
                Tag::custom(TagKind::Custom(TAG_VERSION.into()), vec![version.clone()]),
            );
        }
        tags
    }
    /// Build a filter to find the referenced publication section
    pub fn to_filter(&self, author_pubkey: Option<&str>) -> Filter {
        let mut filter = Filter::new()
            .kind(Kind::Custom(30041))
            .custom_tag(SingleLetterTag::uppercase(Alphabet::T), self.title.clone());
        if let Some(ref collection) = self.collection {
            filter = filter
                .custom_tag(SingleLetterTag::uppercase(Alphabet::C), collection.clone());
        }
        if let Some(ref chapter) = self.chapter {
            filter = filter
                .custom_tag(SingleLetterTag::lowercase(Alphabet::C), chapter.clone());
        }
        if let Some(ref version) = self.version {
            filter = filter
                .custom_tag(SingleLetterTag::lowercase(Alphabet::V), version.clone());
        }
        if let Some(pubkey) = author_pubkey {
            if let Ok(pk) = PublicKey::from_hex(pubkey) {
                filter = filter.author(pk);
            }
        }
        filter
    }
}
/// Parse a single book:: wikilink macro
///
/// # Examples
/// - `book::genesis` → title="genesis"
/// - `book::bible:genesis` → collection="bible", title="genesis"
/// - `book::genesis 2:4-9` → title="genesis", chapter="2", sections=["4-9"]
/// - `book::bible:genesis 2:4-9 | kjv` → full reference
pub fn parse_book_wikilink(input: &str) -> Option<BookReference> {
    let caps = BOOK_WIKILINK_REGEX.captures(input)?;
    let collection = caps.get(1).map(|m| m.as_str().to_string());
    let title = caps.get(2)?.as_str().to_string();
    let chapter = caps.get(3).map(|m| m.as_str().to_string());
    let sections = caps
        .get(4)
        .map(|m| parse_section_range(m.as_str()))
        .unwrap_or_default();
    let version = caps.get(5).map(|m| m.as_str().to_string());
    Some(BookReference {
        raw: input.to_string(),
        collection,
        title,
        chapter,
        sections,
        version,
    })
}
/// Parse section range string into individual sections
/// "4-9" → ["4", "5", "6", "7", "8", "9"]
/// "1,3,5" → ["1", "3", "5"]
/// "1-3,7" → ["1", "2", "3", "7"]
fn parse_section_range(input: &str) -> Vec<String> {
    let mut sections = Vec::new();
    for part in input.split(',') {
        let part = part.trim();
        if part.contains('-') {
            let range_parts: Vec<&str> = part.split('-').collect();
            if range_parts.len() == 2 {
                if let (Ok(start), Ok(end)) = (
                    range_parts[0].parse::<u32>(),
                    range_parts[1].parse::<u32>(),
                ) {
                    for i in start..=end {
                        sections.push(i.to_string());
                    }
                }
            }
        } else if let Ok(_num) = part.parse::<u32>() {
            sections.push(part.to_string());
        }
    }
    sections
}
/// Extract all book:: wikilinks from content
pub fn extract_book_wikilinks(content: &str) -> Vec<BookReference> {
    BOOK_EXTRACT_REGEX
        .find_iter(content)
        .filter_map(|m| parse_book_wikilink(m.as_str()))
        .collect()
}
/// Render book:: wikilinks in content to clickable HTML
pub fn render_book_wikilinks(content: &str) -> String {
    let mut result = content.to_string();
    for cap in BOOK_EXTRACT_REGEX.find_iter(content) {
        let raw = cap.as_str();
        if let Some(reference) = parse_book_wikilink(raw) {
            let html = format!(
                r#"<a href="/publication/search?{}" class="book-wikilink" title="{}">{}</a>"#,
                reference.to_query_string(),
                html_escape(raw),
                html_escape(&reference.display_text()),
            );
            result = result.replace(raw, &html);
        }
    }
    result
}
impl BookReference {
    /// Convert to URL query string for search
    pub fn to_query_string(&self) -> String {
        let mut params = Vec::new();
        params.push(format!("T={}", urlencoding::encode(&self.title)));
        if let Some(ref collection) = self.collection {
            params.push(format!("C={}", urlencoding::encode(collection)));
        }
        if let Some(ref chapter) = self.chapter {
            params.push(format!("c={}", urlencoding::encode(chapter)));
        }
        for section in &self.sections {
            params.push(format!("s={}", urlencoding::encode(section)));
        }
        if let Some(ref version) = self.version {
            params.push(format!("v={}", urlencoding::encode(version)));
        }
        params.join("&")
    }
}
/// Escape HTML special characters
fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
/// Extract BookReference from event tags
pub fn extract_book_reference_from_tags(tags: &[Tag]) -> Option<BookReference> {
    let mut title: Option<String> = None;
    let mut collection: Option<String> = None;
    let mut chapter: Option<String> = None;
    let mut sections: Vec<String> = Vec::new();
    let mut version: Option<String> = None;
    for tag in tags {
        let slice = tag.as_slice();
        match slice.first().map(|s| s.as_str()) {
            Some("T") => {
                title = slice.get(1).map(|s| s.to_string());
            }
            Some("C") => {
                collection = slice.get(1).map(|s| s.to_string());
            }
            Some("c") => {
                chapter = slice.get(1).map(|s| s.to_string());
            }
            Some("s") => {
                if let Some(s) = slice.get(1) {
                    sections.push(s.to_string());
                }
            }
            Some("v") => {
                version = slice.get(1).map(|s| s.to_string());
            }
            _ => {}
        }
    }
    title
        .map(|t| {
            let raw = build_book_macro(&t, &collection, &chapter, &sections, &version);
            BookReference {
                raw,
                title: t,
                collection,
                chapter,
                sections,
                version,
            }
        })
}
/// Build book:: macro string from components
fn build_book_macro(
    title: &str,
    collection: &Option<String>,
    chapter: &Option<String>,
    sections: &[String],
    version: &Option<String>,
) -> String {
    let mut parts = Vec::new();
    if let Some(ref c) = collection {
        parts.push(format!("book::{}:{}", c, title));
    } else {
        parts.push(format!("book::{}", title));
    }
    if let Some(ref ch) = chapter {
        if !sections.is_empty() {
            parts.push(format!(" {}:{}", ch, sections.join(",")));
        } else {
            parts.push(format!(" {}", ch));
        }
    } else if !sections.is_empty() {
        parts.push(format!(":{}", sections.join(",")));
    }
    if let Some(ref v) = version {
        parts.push(format!(" | {}", v));
    }
    parts.concat()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_parse_simple_book() {
        let reference = parse_book_wikilink("book::genesis").unwrap();
        assert_eq!(reference.title, "genesis");
        assert!(reference.collection.is_none());
        assert!(reference.chapter.is_none());
        assert!(reference.sections.is_empty());
        assert!(reference.version.is_none());
    }
    #[test]
    fn test_parse_book_with_collection() {
        let reference = parse_book_wikilink("book::bible:genesis").unwrap();
        assert_eq!(reference.collection, Some("bible".to_string()));
        assert_eq!(reference.title, "genesis");
    }
    #[test]
    fn test_parse_book_with_chapter() {
        let reference = parse_book_wikilink("book::genesis 2").unwrap();
        assert_eq!(reference.title, "genesis");
        assert_eq!(reference.chapter, Some("2".to_string()));
    }
    #[test]
    fn test_parse_book_with_chapter_and_sections() {
        let reference = parse_book_wikilink("book::genesis 2:4-9").unwrap();
        assert_eq!(reference.title, "genesis");
        assert_eq!(reference.chapter, Some("2".to_string()));
        assert_eq!(reference.sections, vec!["4", "5", "6", "7", "8", "9"]);
    }
    #[test]
    fn test_parse_full_book_reference() {
        let reference = parse_book_wikilink("book::bible:genesis 2:4-9 | kjv").unwrap();
        assert_eq!(reference.collection, Some("bible".to_string()));
        assert_eq!(reference.title, "genesis");
        assert_eq!(reference.chapter, Some("2".to_string()));
        assert_eq!(reference.sections, vec!["4", "5", "6", "7", "8", "9"]);
        assert_eq!(reference.version, Some("kjv".to_string()));
    }
    #[test]
    fn test_parse_jane_eyre_example() {
        let reference = parse_book_wikilink("book::jane-eyre 21:8 | penguin-classics")
            .unwrap();
        assert_eq!(reference.title, "jane-eyre");
        assert_eq!(reference.chapter, Some("21".to_string()));
        assert_eq!(reference.sections, vec!["8"]);
        assert_eq!(reference.version, Some("penguin-classics".to_string()));
    }
    #[test]
    fn test_parse_section_range() {
        assert_eq!(parse_section_range("4-9"), vec!["4", "5", "6", "7", "8", "9"]);
        assert_eq!(parse_section_range("1,3,5"), vec!["1", "3", "5"]);
        assert_eq!(parse_section_range("1-3,7"), vec!["1", "2", "3", "7"]);
    }
    #[test]
    fn test_display_text() {
        let reference = BookReference::new("jane-eyre")
            .with_chapter("21")
            .with_section("8")
            .with_version("penguin-classics");
        assert_eq!(reference.display_text(), "Jane Eyre 21:8 (PENGUIN CLASSICS)");
    }
    #[test]
    fn test_display_text_genesis() {
        let reference = BookReference::new("genesis")
            .with_collection("bible")
            .with_chapter("2")
            .with_section("4")
            .with_section("5")
            .with_section("6")
            .with_section("7")
            .with_section("8")
            .with_section("9")
            .with_version("kjv");
        assert_eq!(reference.display_text(), "Genesis 2:4,5,6,7,8,9 (KJV)");
    }
    #[test]
    fn test_extract_book_wikilinks() {
        let content = "Check out book::genesis 1:1 and also book::jane-eyre for more info.";
        let references = extract_book_wikilinks(content);
        assert_eq!(references.len(), 2);
        assert_eq!(references[0].title, "genesis");
        assert_eq!(references[1].title, "jane-eyre");
    }
    #[test]
    fn test_to_tags() {
        let reference = BookReference::new("genesis")
            .with_collection("bible")
            .with_chapter("2")
            .with_section("4")
            .with_version("kjv");
        let tags = reference.to_tags();
        assert_eq!(tags.len(), 5);
    }
    #[test]
    fn test_to_query_string() {
        let reference = BookReference::new("genesis")
            .with_collection("bible")
            .with_chapter("2")
            .with_version("kjv");
        let query = reference.to_query_string();
        assert!(query.contains("T=genesis"));
        assert!(query.contains("C=bible"));
        assert!(query.contains("c=2"));
        assert!(query.contains("v=kjv"));
    }
    #[test]
    fn test_render_book_wikilinks() {
        let content = "See book::genesis for details.";
        let rendered = render_book_wikilinks(content);
        assert!(rendered.contains("<a href="));
        assert!(rendered.contains("book-wikilink"));
        assert!(rendered.contains("Genesis"));
    }
    #[test]
    fn test_invalid_book_wikilink() {
        assert!(parse_book_wikilink("not a book link").is_none());
        assert!(parse_book_wikilink("book::").is_none());
    }
}
