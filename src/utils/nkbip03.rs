//! NKBIP-03 Citation Utilities
//!
//! Handles parsing and creation of citation events:
//! - Kind 30: Internal Nostr reference
//! - Kind 31: External web reference
//! - Kind 32: Hardcopy reference
//! - Kind 33: Prompt reference (AI citations)
//!
//! Also handles citation markup in AsciiDoc content:
//! - citation::end::nevent... - Endnotes
//! - citation::foot::nevent... - Footnotes
//! - citation::inline::nevent... - Inline "(Author, Year)"
//! - citation::quote::nevent... - Block quotes
//! - citation::prompt-end::nevent... - AI citation endnote
//! - citation::prompt-inline::nevent... - AI citation inline

use nostr_sdk::prelude::*;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::LazyLock;

// ============================================================================
// Event Kind Constants
// ============================================================================

/// Internal Nostr reference (kind 30)
pub const KIND_INTERNAL_REF: u16 = 30;

/// External web reference (kind 31)
pub const KIND_EXTERNAL_WEB: u16 = 31;

/// Hardcopy reference (kind 32)
pub const KIND_HARDCOPY: u16 = 32;

/// Prompt/AI reference (kind 33)
pub const KIND_PROMPT: u16 = 33;

// ============================================================================
// Citation Types
// ============================================================================

/// Type of citation
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum CitationType {
    /// Internal Nostr reference (kind 30)
    Internal,
    /// External web reference (kind 31)
    ExternalWeb,
    /// Hardcopy reference (kind 32)
    Hardcopy,
    /// AI prompt reference (kind 33)
    Prompt,
}

impl CitationType {
    /// Get the event kind for this citation type
    pub fn kind(&self) -> u16 {
        match self {
            CitationType::Internal => KIND_INTERNAL_REF,
            CitationType::ExternalWeb => KIND_EXTERNAL_WEB,
            CitationType::Hardcopy => KIND_HARDCOPY,
            CitationType::Prompt => KIND_PROMPT,
        }
    }

    /// Create from event kind
    pub fn from_kind(kind: u16) -> Option<Self> {
        match kind {
            KIND_INTERNAL_REF => Some(CitationType::Internal),
            KIND_EXTERNAL_WEB => Some(CitationType::ExternalWeb),
            KIND_HARDCOPY => Some(CitationType::Hardcopy),
            KIND_PROMPT => Some(CitationType::Prompt),
            _ => None,
        }
    }
}

/// Style of citation reference in content
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum CitationStyle {
    /// Endnote (listed at end of document)
    End,
    /// Footnote (at bottom of section)
    Foot,
    /// Footnote that links to endnote
    FootEnd,
    /// Inline reference "(Author, Year)"
    Inline,
    /// Block quote with citation
    Quote,
    /// AI prompt endnote
    PromptEnd,
    /// AI prompt inline
    PromptInline,
}

impl CitationStyle {
    /// Get the markup prefix for this style
    pub fn markup_prefix(&self) -> &'static str {
        match self {
            CitationStyle::End => "citation::end::",
            CitationStyle::Foot => "citation::foot::",
            CitationStyle::FootEnd => "citation::foot-end::",
            CitationStyle::Inline => "citation::inline::",
            CitationStyle::Quote => "citation::quote::",
            CitationStyle::PromptEnd => "citation::prompt-end::",
            CitationStyle::PromptInline => "citation::prompt-inline::",
        }
    }

    /// Parse from markup prefix
    pub fn from_prefix(prefix: &str) -> Option<Self> {
        match prefix {
            "end" => Some(CitationStyle::End),
            "foot" => Some(CitationStyle::Foot),
            "foot-end" => Some(CitationStyle::FootEnd),
            "inline" => Some(CitationStyle::Inline),
            "quote" => Some(CitationStyle::Quote),
            "prompt-end" => Some(CitationStyle::PromptEnd),
            "prompt-inline" => Some(CitationStyle::PromptInline),
            _ => None,
        }
    }
}

// ============================================================================
// Citation Data Structures
// ============================================================================

/// A citation reference found in content
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CitationReference {
    /// The raw matched text
    pub raw: String,
    /// Citation style (end, foot, inline, etc.)
    pub style: CitationStyle,
    /// The event identifier (nevent, note, naddr)
    pub identifier: String,
}

/// Base citation data (common to all types)
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CitationBase {
    /// Event ID (hex)
    pub event_id: String,
    /// Author pubkey (hex)
    pub pubkey: String,
    /// Title to display
    pub title: String,
    /// Author name to display
    pub author: String,
    /// Cited text content
    pub content: String,
    /// Access date (ISO 8601)
    pub accessed_on: Option<String>,
    /// Summary of the citation
    pub summary: Option<String>,
    /// Created timestamp
    pub created_at: u64,
}

/// Internal Nostr citation (kind 30)
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct InternalCitation {
    /// Base citation data
    #[serde(flatten)]
    pub base: CitationBase,
    /// Referenced event coordinate (c tag: kind:pubkey:event_id)
    pub coordinate: String,
    /// Publication date (ISO 8601)
    pub published_on: Option<String>,
    /// Location
    pub location: Option<String>,
    /// Geohash
    pub geohash: Option<String>,
}

/// External web citation (kind 31)
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ExternalWebCitation {
    /// Base citation data
    #[serde(flatten)]
    pub base: CitationBase,
    /// URL where citation was accessed
    pub url: String,
    /// Publication date (ISO 8601)
    pub published_on: Option<String>,
    /// Publisher name
    pub published_by: Option<String>,
    /// Version/edition
    pub version: Option<String>,
    /// OpenTimestamps event tag
    pub open_timestamp: Option<String>,
    /// Location
    pub location: Option<String>,
    /// Geohash
    pub geohash: Option<String>,
}

/// Hardcopy citation (kind 32)
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HardcopyCitation {
    /// Base citation data
    #[serde(flatten)]
    pub base: CitationBase,
    /// Page range
    pub page_range: Option<String>,
    /// Chapter/section title
    pub chapter_title: Option<String>,
    /// Editor name
    pub editor: Option<String>,
    /// Publication date (ISO 8601)
    pub published_on: Option<String>,
    /// Publisher name
    pub published_by: Option<String>,
    /// Journal name and volume
    pub published_in: Option<(String, Option<String>)>,
    /// DOI number
    pub doi: Option<String>,
    /// Version/edition
    pub version: Option<String>,
    /// Location
    pub location: Option<String>,
    /// Geohash
    pub geohash: Option<String>,
}

/// AI prompt citation (kind 33)
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PromptCitation {
    /// Base citation data
    #[serde(flatten)]
    pub base: CitationBase,
    /// Language model used
    pub llm: String,
    /// Model version
    pub version: Option<String>,
    /// Website URL (if accessed via web)
    pub url: Option<String>,
}

/// Unified citation enum
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Citation {
    Internal(InternalCitation),
    ExternalWeb(ExternalWebCitation),
    Hardcopy(HardcopyCitation),
    Prompt(PromptCitation),
}

impl Citation {
    /// Get the citation type
    pub fn citation_type(&self) -> CitationType {
        match self {
            Citation::Internal(_) => CitationType::Internal,
            Citation::ExternalWeb(_) => CitationType::ExternalWeb,
            Citation::Hardcopy(_) => CitationType::Hardcopy,
            Citation::Prompt(_) => CitationType::Prompt,
        }
    }

    /// Get the event kind
    pub fn kind(&self) -> u16 {
        self.citation_type().kind()
    }

    /// Get the base citation data
    pub fn base(&self) -> &CitationBase {
        match self {
            Citation::Internal(c) => &c.base,
            Citation::ExternalWeb(c) => &c.base,
            Citation::Hardcopy(c) => &c.base,
            Citation::Prompt(c) => &c.base,
        }
    }

    /// Get a short display string for the citation
    pub fn short_display(&self) -> String {
        let base = self.base();
        if !base.author.is_empty() {
            format!("{}, {}", base.author, base.title)
        } else {
            base.title.clone()
        }
    }
}

// ============================================================================
// Citation Markup Parsing
// ============================================================================

// Regex for citation markup: citation::style::identifier
static CITATION_MARKUP_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"citation::(end|foot|foot-end|inline|quote|prompt-end|prompt-inline)::(n(?:event|ote|addr)[a-zA-Z0-9]+)")
        .expect("Invalid citation markup regex")
});

/// Extract citation references from content
pub fn extract_citations(content: &str) -> Vec<CitationReference> {
    CITATION_MARKUP_REGEX
        .captures_iter(content)
        .filter_map(|cap| {
            let raw = cap.get(0)?.as_str().to_string();
            let style_str = cap.get(1)?.as_str();
            let identifier = cap.get(2)?.as_str().to_string();

            let style = CitationStyle::from_prefix(style_str)?;

            Some(CitationReference {
                raw,
                style,
                identifier,
            })
        })
        .collect()
}

/// Check if content contains any citation markup
pub fn has_citations(content: &str) -> bool {
    CITATION_MARKUP_REGEX.is_match(content)
}

// ============================================================================
// Citation Rendering
// ============================================================================

/// Resolved citation data for rendering (avoids circular dependency with store)
#[derive(Clone, Debug)]
pub struct ResolvedCitation {
    /// The identifier (nevent, naddr, note)
    pub identifier: String,
    /// Display title
    pub title: String,
    /// Author name
    pub author: String,
    /// Cited content/quote
    pub content: String,
    /// Citation type
    pub citation_type: CitationType,
    /// URL for external citations
    pub url: Option<String>,
    /// Publication date
    pub published_on: Option<String>,
    /// Publisher name
    pub published_by: Option<String>,
    /// DOI for academic citations
    pub doi: Option<String>,
    /// Page range for hardcopy citations
    pub page_range: Option<String>,
    /// LLM name for prompt citations
    pub llm: Option<String>,
}

impl ResolvedCitation {
    /// Create from a Citation
    pub fn from_citation(identifier: String, citation: &Citation) -> Self {
        let base = citation.base();
        let (url, published_on, published_by, doi, page_range, llm) = match citation {
            Citation::Internal(c) => (None, c.published_on.clone(), None, None, None, None),
            Citation::ExternalWeb(c) => (
                Some(c.url.clone()),
                c.published_on.clone(),
                c.published_by.clone(),
                None,
                None,
                None,
            ),
            Citation::Hardcopy(c) => (
                None,
                c.published_on.clone(),
                c.published_by.clone(),
                c.doi.clone(),
                c.page_range.clone(),
                None,
            ),
            Citation::Prompt(c) => (c.url.clone(), None, None, None, None, Some(c.llm.clone())),
        };

        ResolvedCitation {
            identifier,
            title: base.title.clone(),
            author: base.author.clone(),
            content: base.content.clone(),
            citation_type: citation.citation_type(),
            url,
            published_on,
            published_by,
            doi,
            page_range,
            llm,
        }
    }

    /// Format inline citation text "(Author, Year)" or "(Title)"
    pub fn inline_text(&self) -> String {
        if !self.author.is_empty() {
            if let Some(year) = self.extract_year() {
                format!("({}, {})", self.author, year)
            } else {
                format!("({})", self.author)
            }
        } else if !self.title.is_empty() {
            format!("({})", truncate_text(&self.title, 30))
        } else {
            format!("[{}]", truncate_identifier(&self.identifier))
        }
    }

    /// Format bibliographic entry for footnotes/endnotes
    pub fn bibliographic_entry(&self) -> String {
        let mut parts = Vec::new();

        // Author
        if !self.author.is_empty() {
            parts.push(self.author.clone());
        }

        // Year
        if let Some(year) = self.extract_year() {
            parts.push(format!("({})", year));
        }

        // Title
        if !self.title.is_empty() {
            parts.push(format!("\"{}\"", self.title));
        }

        // Publisher
        if let Some(ref pub_) = self.published_by {
            parts.push(pub_.clone());
        }

        // Page range
        if let Some(ref pages) = self.page_range {
            parts.push(format!("pp. {}", pages));
        }

        // DOI or URL
        if let Some(ref doi) = self.doi {
            parts.push(format!("doi:{}", doi));
        } else if let Some(ref url) = self.url {
            parts.push(url.clone());
        }

        // LLM for prompt citations
        if let Some(ref llm) = self.llm {
            parts.push(format!("AI: {}", llm));
        }

        if parts.is_empty() {
            self.identifier.clone()
        } else {
            parts.join(". ")
        }
    }

    fn extract_year(&self) -> Option<String> {
        self.published_on.as_ref().and_then(|date| {
            // Extract year from ISO date or various formats
            if date.len() >= 4 {
                Some(date[..4].to_string())
            } else {
                None
            }
        })
    }
}

/// Truncate text with ellipsis
fn truncate_text(text: &str, max_len: usize) -> String {
    if text.len() <= max_len {
        text.to_string()
    } else {
        format!("{}…", &text[..max_len - 1])
    }
}

/// Truncate identifier for display
fn truncate_identifier(identifier: &str) -> String {
    if identifier.len() > 12 {
        format!("{}…", &identifier[..12])
    } else {
        identifier.to_string()
    }
}

/// Render citation markup to HTML (basic version without resolved data).
///
/// Note: For full citation rendering with resolved data, use `render_citations_with_data()`.
/// This is a convenience wrapper for cases where citation data isn't needed.
#[allow(dead_code)]
pub fn render_citations(content: &str, _citations: &[Citation]) -> String {
    render_citations_with_data(content, &HashMap::new())
}

/// Render citation markup to HTML with resolved citation data
pub fn render_citations_with_data(
    content: &str,
    resolved: &HashMap<String, ResolvedCitation>,
) -> String {
    let mut footnote_counter = 0;
    let mut endnote_counter = 0;

    CITATION_MARKUP_REGEX
        .replace_all(content, |caps: &regex::Captures| {
            let style_str = caps.get(1).map(|m| m.as_str()).unwrap_or("");
            let identifier = caps.get(2).map(|m| m.as_str()).unwrap_or("");

            let style = CitationStyle::from_prefix(style_str);
            let citation_data = resolved.get(identifier);
            let is_resolved = citation_data.is_some();
            let resolved_class = if is_resolved { "" } else { " citation-unresolved" };

            match style {
                Some(CitationStyle::Inline) | Some(CitationStyle::PromptInline) => {
                    let display = citation_data
                        .map(|c| c.inline_text())
                        .unwrap_or_else(|| format!("[{}]", truncate_identifier(identifier)));
                    let title_attr = citation_data
                        .map(|c| format!(" title=\"{}\"", html_escape(&c.title)))
                        .unwrap_or_default();
                    format!(
                        r#"<a href="nostr:{}" class="citation citation-inline{}" data-identifier="{}"{} data-resolved="{}">{}</a>"#,
                        identifier, resolved_class, identifier, title_attr, is_resolved, display
                    )
                }
                Some(CitationStyle::Foot) => {
                    footnote_counter += 1;
                    let title_attr = citation_data
                        .map(|c| format!(" title=\"{}\"", html_escape(&c.title)))
                        .unwrap_or_default();
                    format!(
                        "<sup class=\"citation citation-footnote{}\" data-identifier=\"{}\"{}  data-resolved=\"{}\"><a href=\"#fn{}\">{}</a></sup>",
                        resolved_class, identifier, title_attr, is_resolved, footnote_counter, footnote_counter
                    )
                }
                Some(CitationStyle::FootEnd) => {
                    footnote_counter += 1;
                    endnote_counter += 1;
                    let title_attr = citation_data
                        .map(|c| format!(" title=\"{}\"", html_escape(&c.title)))
                        .unwrap_or_default();
                    format!(
                        "<sup class=\"citation citation-footnote citation-foot-end{}\" data-identifier=\"{}\"{}  data-resolved=\"{}\"><a href=\"#fn{}\">{}</a><a href=\"#en{}\" class=\"citation-end-ref\">[→]</a></sup>",
                        resolved_class, identifier, title_attr, is_resolved, footnote_counter, footnote_counter, endnote_counter
                    )
                }
                Some(CitationStyle::End) | Some(CitationStyle::PromptEnd) => {
                    endnote_counter += 1;
                    let title_attr = citation_data
                        .map(|c| format!(" title=\"{}\"", html_escape(&c.title)))
                        .unwrap_or_default();
                    format!(
                        "<sup class=\"citation citation-endnote{}\" data-identifier=\"{}\"{}  data-resolved=\"{}\"><a href=\"#en{}\">{}</a></sup>",
                        resolved_class, identifier, title_attr, is_resolved, endnote_counter, endnote_counter
                    )
                }
                Some(CitationStyle::Quote) => {
                    let quote_content = citation_data
                        .map(|c| html_escape(&c.content))
                        .unwrap_or_default();
                    let attribution = citation_data
                        .map(|c| {
                            let mut attr = String::new();
                            if !c.author.is_empty() {
                                attr.push_str(&format!("— {}", c.author));
                                if !c.title.is_empty() {
                                    attr.push_str(&format!(", <cite>{}</cite>", html_escape(&c.title)));
                                }
                            } else if !c.title.is_empty() {
                                attr.push_str(&format!("— <cite>{}</cite>", html_escape(&c.title)));
                            }
                            attr
                        })
                        .unwrap_or_default();
                    format!(
                        r#"<figure class="citation citation-quote{}" data-identifier="{}" data-resolved="{}"><blockquote cite="nostr:{}">{}</blockquote><figcaption>{}</figcaption></figure>"#,
                        resolved_class, identifier, is_resolved, identifier, quote_content, attribution
                    )
                }
                _ => caps.get(0).map(|m| m.as_str()).unwrap_or("").to_string(),
            }
        })
        .to_string()
}

/// Escape HTML special characters
fn html_escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

/// Generate footnotes HTML section (basic version).
///
/// Renders footnote-style citations at the bottom of sections.
/// For full resolved citation data, use `generate_footnotes_html_with_data()`.
#[allow(dead_code)]
pub fn generate_footnotes_html(citations: &[CitationReference]) -> String {
    generate_footnotes_html_with_data(citations, &HashMap::new())
}

/// Generate footnotes HTML section with resolved citation data
pub fn generate_footnotes_html_with_data(
    citations: &[CitationReference],
    resolved: &HashMap<String, ResolvedCitation>,
) -> String {
    let footnotes: Vec<_> = citations
        .iter()
        .filter(|c| matches!(c.style, CitationStyle::Foot | CitationStyle::FootEnd))
        .collect();

    if footnotes.is_empty() {
        return String::new();
    }

    let mut html = String::from("<section class=\"citation-footnotes\" aria-label=\"Footnotes\"><hr class=\"citation-divider\"><ol class=\"citation-list\">");

    for (i, citation) in footnotes.iter().enumerate() {
        let citation_data = resolved.get(&citation.identifier);
        let is_resolved = citation_data.is_some();
        let resolved_class = if is_resolved {
            ""
        } else {
            " citation-unresolved"
        };

        let content = citation_data
            .map(|c| c.bibliographic_entry())
            .unwrap_or_else(|| citation.identifier.clone());

        let type_badge = citation_data
            .map(|c| match c.citation_type {
                CitationType::Internal => "<span class=\"citation-type-badge citation-type-internal\" title=\"Nostr reference\">📌</span>",
                CitationType::ExternalWeb => "<span class=\"citation-type-badge citation-type-external\" title=\"Web reference\">🌐</span>",
                CitationType::Hardcopy => "<span class=\"citation-type-badge citation-type-hardcopy\" title=\"Book/Journal\">📖</span>",
                CitationType::Prompt => "<span class=\"citation-type-badge citation-type-prompt\" title=\"AI citation\">🤖</span>",
            })
            .unwrap_or("");

        html.push_str(&format!(
            "<li id=\"fn{}\" class=\"citation-entry{}\">{}<a href=\"nostr:{}\" class=\"citation-link\">{}</a><a href=\"#fnref{}\" class=\"citation-backref\" aria-label=\"Back to content\">↩</a></li>",
            i + 1,
            resolved_class,
            type_badge,
            citation.identifier,
            html_escape(&content),
            i + 1
        ));
    }

    html.push_str("</ol></section>");
    html
}

/// Generate endnotes HTML section (basic version).
///
/// Renders endnote-style citations as a references section at the end.
/// For full resolved citation data, use `generate_endnotes_html_with_data()`.
#[allow(dead_code)]
pub fn generate_endnotes_html(citations: &[CitationReference]) -> String {
    generate_endnotes_html_with_data(citations, &HashMap::new())
}

/// Generate endnotes HTML section with resolved citation data
pub fn generate_endnotes_html_with_data(
    citations: &[CitationReference],
    resolved: &HashMap<String, ResolvedCitation>,
) -> String {
    let endnotes: Vec<_> = citations
        .iter()
        .filter(|c| {
            matches!(
                c.style,
                CitationStyle::End | CitationStyle::PromptEnd | CitationStyle::FootEnd
            )
        })
        .collect();

    if endnotes.is_empty() {
        return String::new();
    }

    let mut html = String::from("<section class=\"citation-endnotes\" aria-label=\"References\"><h2 class=\"citation-heading\">References</h2><ol class=\"citation-list\">");

    for (i, citation) in endnotes.iter().enumerate() {
        let citation_data = resolved.get(&citation.identifier);
        let is_resolved = citation_data.is_some();
        let resolved_class = if is_resolved {
            ""
        } else {
            " citation-unresolved"
        };

        let content = citation_data
            .map(|c| c.bibliographic_entry())
            .unwrap_or_else(|| citation.identifier.clone());

        let type_badge = citation_data
            .map(|c| match c.citation_type {
                CitationType::Internal => "<span class=\"citation-type-badge citation-type-internal\" title=\"Nostr reference\">📌</span>",
                CitationType::ExternalWeb => "<span class=\"citation-type-badge citation-type-external\" title=\"Web reference\">🌐</span>",
                CitationType::Hardcopy => "<span class=\"citation-type-badge citation-type-hardcopy\" title=\"Book/Journal\">📖</span>",
                CitationType::Prompt => "<span class=\"citation-type-badge citation-type-prompt\" title=\"AI citation\">🤖</span>",
            })
            .unwrap_or("");

        html.push_str(&format!(
            "<li id=\"en{}\" class=\"citation-entry{}\">{}<a href=\"nostr:{}\" class=\"citation-link\">{}</a><a href=\"#enref{}\" class=\"citation-backref\" aria-label=\"Back to content\">↩</a></li>",
            i + 1,
            resolved_class,
            type_badge,
            citation.identifier,
            html_escape(&content),
            i + 1
        ));
    }

    html.push_str("</ol></section>");
    html
}

// Note: RenderedContent struct was removed - use asciidoc::RenderedContentWithCitations instead
// which has additional fields (citation_identifiers, has_citations) for better integration.

// ============================================================================
// Event Parsing Functions
// ============================================================================

/// Parse an internal citation from a Kind 30 event
pub fn parse_internal_citation(event: &Event) -> Result<InternalCitation, String> {
    if event.kind.as_u16() != KIND_INTERNAL_REF {
        return Err(format!(
            "Expected kind {}, got {}",
            KIND_INTERNAL_REF,
            event.kind.as_u16()
        ));
    }

    let base = parse_citation_base(event)?;

    let coordinate = get_tag_value(event, "c").ok_or("Missing required 'c' tag (coordinate)")?;

    Ok(InternalCitation {
        base,
        coordinate,
        published_on: get_tag_value(event, "published_on"),
        location: get_tag_value(event, "location"),
        geohash: get_tag_value(event, "g"),
    })
}

/// Parse an external web citation from a Kind 31 event
pub fn parse_external_web_citation(event: &Event) -> Result<ExternalWebCitation, String> {
    if event.kind.as_u16() != KIND_EXTERNAL_WEB {
        return Err(format!(
            "Expected kind {}, got {}",
            KIND_EXTERNAL_WEB,
            event.kind.as_u16()
        ));
    }

    let base = parse_citation_base(event)?;

    let url = get_tag_value(event, "u").ok_or("Missing required 'u' tag (URL)")?;

    Ok(ExternalWebCitation {
        base,
        url,
        published_on: get_tag_value(event, "published_on"),
        published_by: get_tag_value(event, "published_by"),
        version: get_tag_value(event, "version"),
        open_timestamp: get_tag_value(event, "open_timestamp"),
        location: get_tag_value(event, "location"),
        geohash: get_tag_value(event, "g"),
    })
}

/// Parse a hardcopy citation from a Kind 32 event
pub fn parse_hardcopy_citation(event: &Event) -> Result<HardcopyCitation, String> {
    if event.kind.as_u16() != KIND_HARDCOPY {
        return Err(format!(
            "Expected kind {}, got {}",
            KIND_HARDCOPY,
            event.kind.as_u16()
        ));
    }

    let base = parse_citation_base(event)?;

    // Parse published_in as (journal, volume) tuple
    let published_in = get_tag_with_optional_second(event, "published_in");

    Ok(HardcopyCitation {
        base,
        page_range: get_tag_value(event, "page_range"),
        chapter_title: get_tag_value(event, "chapter_title"),
        editor: get_tag_value(event, "editor"),
        published_on: get_tag_value(event, "published_on"),
        published_by: get_tag_value(event, "published_by"),
        published_in,
        doi: get_tag_value(event, "doi"),
        version: get_tag_value(event, "version"),
        location: get_tag_value(event, "location"),
        geohash: get_tag_value(event, "g"),
    })
}

/// Parse a prompt citation from a Kind 33 event
pub fn parse_prompt_citation(event: &Event) -> Result<PromptCitation, String> {
    if event.kind.as_u16() != KIND_PROMPT {
        return Err(format!(
            "Expected kind {}, got {}",
            KIND_PROMPT,
            event.kind.as_u16()
        ));
    }

    let base = parse_citation_base(event)?;

    let llm = get_tag_value(event, "llm").ok_or("Missing required 'llm' tag")?;

    Ok(PromptCitation {
        base,
        llm,
        version: get_tag_value(event, "version"),
        url: get_tag_value(event, "u"),
    })
}

/// Parse any citation from an event
pub fn parse_citation(event: &Event) -> Result<Citation, String> {
    let kind = event.kind.as_u16();
    let citation_type =
        CitationType::from_kind(kind).ok_or_else(|| format!("Unknown citation kind: {}", kind))?;

    match citation_type {
        CitationType::Internal => Ok(Citation::Internal(parse_internal_citation(event)?)),
        CitationType::ExternalWeb => Ok(Citation::ExternalWeb(parse_external_web_citation(event)?)),
        CitationType::Hardcopy => Ok(Citation::Hardcopy(parse_hardcopy_citation(event)?)),
        CitationType::Prompt => Ok(Citation::Prompt(parse_prompt_citation(event)?)),
    }
}

/// Parse base citation data from any citation event
fn parse_citation_base(event: &Event) -> Result<CitationBase, String> {
    let pubkey = event.pubkey.to_hex();

    let title = get_tag_value(event, "title").ok_or("Missing required 'title' tag")?;

    let author = get_tag_value(event, "author").ok_or("Missing required 'author' tag")?;

    Ok(CitationBase {
        event_id: event.id.to_hex(),
        pubkey,
        title,
        author,
        content: event.content.clone(),
        accessed_on: get_tag_value(event, "accessed_on"),
        summary: get_tag_value(event, "summary"),
        created_at: event.created_at.as_secs(),
    })
}

// ============================================================================
// Filter Builders (Low-level utilities)
// ============================================================================

/// Build a single filter for all citation events by an author.
///
/// Note: For high-level fetching with caching and parsing, use the
/// `citation_store::fetch_citations_by_author()` function instead.
/// This function returns a single Filter combining all citation kinds.
#[allow(dead_code)]
pub fn citations_filter(author: &PublicKey, limit: usize) -> Filter {
    Filter::new()
        .kinds(vec![
            Kind::Custom(KIND_INTERNAL_REF),
            Kind::Custom(KIND_EXTERNAL_WEB),
            Kind::Custom(KIND_HARDCOPY),
            Kind::Custom(KIND_PROMPT),
        ])
        .author(*author)
        .limit(limit)
}

/// Build a filter for a specific citation type.
///
/// Note: For high-level fetching with caching, use
/// `citation_store::fetch_citations_by_type()` instead.
#[allow(dead_code)]
pub fn citation_type_filter(citation_type: CitationType, limit: usize) -> Filter {
    Filter::new()
        .kind(Kind::Custom(citation_type.kind()))
        .limit(limit)
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Get first value of a tag by name
fn get_tag_value(event: &Event, tag_name: &str) -> Option<String> {
    event
        .tags
        .iter()
        .find(|t| t.as_slice().first().map(|s| s.as_str()) == Some(tag_name))
        .and_then(|t| t.as_slice().get(1).map(|s| s.to_string()))
}

/// Get tag with optional second value (e.g., published_in: journal, volume)
fn get_tag_with_optional_second(event: &Event, tag_name: &str) -> Option<(String, Option<String>)> {
    event
        .tags
        .iter()
        .find(|t| t.as_slice().first().map(|s| s.as_str()) == Some(tag_name))
        .and_then(|t| {
            let slice = t.as_slice();
            slice
                .get(1)
                .map(|first| (first.to_string(), slice.get(2).map(|s| s.to_string())))
        })
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_citations() {
        let content = "See citation::end::nevent1abc123 for more details.";
        let citations = extract_citations(content);
        assert_eq!(citations.len(), 1);
        assert_eq!(citations[0].style, CitationStyle::End);
        assert!(citations[0].identifier.starts_with("nevent1"));
    }

    #[test]
    fn test_extract_multiple_citations() {
        let content =
            "First citation::inline::nevent1abc and second citation::foot::naddr1xyz here.";
        let citations = extract_citations(content);
        assert_eq!(citations.len(), 2);
        assert_eq!(citations[0].style, CitationStyle::Inline);
        assert_eq!(citations[1].style, CitationStyle::Foot);
    }

    #[test]
    fn test_has_citations() {
        assert!(has_citations("citation::end::nevent1abc"));
        assert!(!has_citations("no citations here"));
    }

    #[test]
    fn test_citation_style_prefix() {
        assert_eq!(CitationStyle::End.markup_prefix(), "citation::end::");
        assert_eq!(CitationStyle::Inline.markup_prefix(), "citation::inline::");
    }

    #[test]
    fn test_citation_type_kind() {
        assert_eq!(CitationType::Internal.kind(), 30);
        assert_eq!(CitationType::ExternalWeb.kind(), 31);
        assert_eq!(CitationType::Hardcopy.kind(), 32);
        assert_eq!(CitationType::Prompt.kind(), 33);
    }

    #[test]
    fn test_citation_type_from_kind() {
        assert_eq!(CitationType::from_kind(30), Some(CitationType::Internal));
        assert_eq!(CitationType::from_kind(31), Some(CitationType::ExternalWeb));
        assert_eq!(CitationType::from_kind(32), Some(CitationType::Hardcopy));
        assert_eq!(CitationType::from_kind(33), Some(CitationType::Prompt));
        assert_eq!(CitationType::from_kind(99), None);
    }
}
