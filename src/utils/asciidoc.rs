//! AsciiDoc Rendering Utilities
//!
//! Custom AsciiDoc parser for rendering wiki and publication content.
//! Handles common AsciiDoc patterns used in Nostr publications:
//! - Headings (= to ======)
//! - Bold, italic, monospace
//! - Links and images
//! - Lists (ordered and unordered)
//! - Code blocks
//! - Blockquotes
//! - Tables (basic)
//! - Admonitions (NOTE, TIP, WARNING, CAUTION, IMPORTANT)
//!
//! Also supports Markdown content detection and rendering for compatibility
//! with wiki articles that use Markdown instead of AsciiDoc.
//!
//! Uses ammonia for HTML sanitization to prevent XSS.

use regex::Regex;
use std::collections::HashMap;
use std::sync::LazyLock;

use super::markdown::{render_markdown, sanitize_html};
use super::nip54::render_wikilinks_to_html;
use super::nkbip03::{
    extract_citations, has_citations, render_citations_with_data,
    generate_footnotes_html_with_data, generate_endnotes_html_with_data,
    ResolvedCitation, CitationReference,
};

// ============================================================================
// Content Format Detection
// ============================================================================

/// Detected content format
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ContentFormat {
    AsciiDoc,
    Markdown,
}

/// Detect whether content is Markdown or AsciiDoc
///
/// Heuristics used:
/// - Markdown headings: `# `, `## `, etc. at start of line
/// - Markdown bold: `**text**`
/// - Markdown links: `[text](url)`
/// - AsciiDoc headings: `= `, `== `, etc. at start of line
/// - AsciiDoc links: `link:url[text]`
pub fn detect_content_format(content: &str) -> ContentFormat {
    // Count indicators for each format
    let mut markdown_score = 0;
    let mut asciidoc_score = 0;

    for line in content.lines() {
        let trimmed = line.trim();

        // Markdown heading: # Title (but not #hashtag)
        if trimmed.starts_with('#') && trimmed.chars().nth(1).map(|c| c == ' ' || c == '#').unwrap_or(false) {
            markdown_score += 3;
        }

        // AsciiDoc heading: = Title
        if trimmed.starts_with('=') && trimmed.chars().nth(1).map(|c| c == ' ' || c == '=').unwrap_or(false) {
            asciidoc_score += 3;
        }
    }

    // Check for Markdown bold: **text**
    if content.contains("**") {
        markdown_score += 2;
    }

    // Check for Markdown links: [text](url)
    if content.contains("](") {
        markdown_score += 2;
    }

    // Check for Markdown images: ![alt](url)
    if content.contains("![") {
        markdown_score += 2;
    }

    // Check for AsciiDoc links: link:url[text]
    if content.contains("link:") && content.contains('[') {
        asciidoc_score += 2;
    }

    // Check for AsciiDoc images: image:url[alt]
    if content.contains("image:") || content.contains("image::") {
        asciidoc_score += 2;
    }

    // Check for AsciiDoc code blocks: [source] or ----
    if content.contains("[source") || content.contains("\n----\n") {
        asciidoc_score += 2;
    }

    // Check for Markdown code blocks: ```
    if content.contains("```") {
        markdown_score += 2;
    }

    // Check for AsciiDoc admonitions: NOTE:, TIP:, etc.
    if content.contains("NOTE:") || content.contains("TIP:") || content.contains("WARNING:") {
        asciidoc_score += 1;
    }

    // Default to Markdown if scores are equal or Markdown wins
    // (Markdown is more common in the wild)
    if asciidoc_score > markdown_score {
        ContentFormat::AsciiDoc
    } else {
        ContentFormat::Markdown
    }
}

/// Render content with automatic format detection
///
/// Detects whether content is Markdown or AsciiDoc and renders accordingly.
/// Optionally processes wikilinks in both formats.
pub fn render_content_auto(content: &str) -> String {
    render_content_auto_with_options(content, true)
}

/// Render content with automatic format detection and optional wikilink support
///
/// Detects whether content is Markdown or AsciiDoc and renders accordingly.
/// When `enable_wikilinks` is true, wikilinks are processed.
pub fn render_content_auto_with_options(content: &str, enable_wikilinks: bool) -> String {
    let format = detect_content_format(content);

    match format {
        ContentFormat::Markdown => {
            if enable_wikilinks {
                let with_wikilinks = render_wikilinks_to_html(content);
                render_markdown(&with_wikilinks)
            } else {
                render_markdown(content)
            }
        }
        ContentFormat::AsciiDoc => {
            if enable_wikilinks {
                render_asciidoc_with_wikilinks(content)
            } else {
                render_asciidoc(content)
            }
        }
    }
}

// ============================================================================
// Regex Patterns (Lazy Initialized)
// ============================================================================

// Heading patterns: = Title (h1) through ====== Title (h6)
static HEADING_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?m)^(={1,6})\s+(.+?)(?:\s+=+)?$").expect("Invalid heading regex")
});

// Bold: *text* or **text**
static BOLD_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\*\*?([^*\n]+?)\*\*?").expect("Invalid bold regex")
});

// Italic: _text_ or __text__ (but not inside words)
static ITALIC_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?:^|[^a-zA-Z0-9])_([^_\n]+?)_(?:[^a-zA-Z0-9]|$)").expect("Invalid italic regex")
});

// Monospace: `text` or +text+
static MONO_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"`([^`\n]+?)`|\+([^+\n]+?)\+").expect("Invalid mono regex")
});

// Links: link:url[text] or url[text]
static LINK_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?:link:)?(https?://[^\s\[]+)\[([^\]]*)\]").expect("Invalid link regex")
});

// Images: image:url[alt] or image::url[alt]
static IMAGE_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"image::?([^\s\[]+)\[([^\]]*)\]").expect("Invalid image regex")
});

// Code blocks: [source,lang] followed by ---- delimited block
static CODE_BLOCK_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?s)\[source(?:,([^\]]*))?\]\s*\n----\n(.*?)\n----")
        .expect("Invalid code block regex")
});

// Literal blocks: .... delimited
static LITERAL_BLOCK_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?s)\.\.\.\.\n(.*?)\n\.\.\.\.").expect("Invalid literal block regex")
});

// Unordered list items: * item or - item (with nesting via **)
static UNORDERED_LIST_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?m)^(\*+|-)\s+(.+)$").expect("Invalid unordered list regex")
});

// Ordered list items: . item or 1. item
static ORDERED_LIST_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?m)^(\.+|\d+\.)\s+(.+)$").expect("Invalid ordered list regex")
});

// Blockquote: > text or [quote] block
static BLOCKQUOTE_LINE_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?m)^>\s*(.*)$").expect("Invalid blockquote regex")
});

// Admonitions: NOTE: text or [NOTE] followed by ====
static ADMONITION_INLINE_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?m)^(NOTE|TIP|WARNING|CAUTION|IMPORTANT):\s+(.+)$")
        .expect("Invalid admonition regex")
});

// Horizontal rule: ''' or ---
static HR_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?m)^(?:'{3,}|-{3,})$").expect("Invalid hr regex")
});

// Line breaks: + at end of line
static LINE_BREAK_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\s+\+\s*$").expect("Invalid line break regex")
});

// Table: |===... (simple table parsing)
static TABLE_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?s)\|===\s*\n(.*?)\n\|===").expect("Invalid table regex")
});

// ============================================================================
// Main Rendering Functions
// ============================================================================

/// Render AsciiDoc content to sanitized HTML
///
/// Processes common AsciiDoc patterns and returns safe HTML.
/// Does NOT process wikilinks - use `render_asciidoc_with_wikilinks` for that.
pub fn render_asciidoc(content: &str) -> String {
    let html = render_asciidoc_internal(content);
    sanitize_html(&html)
}

/// Render AsciiDoc with wikilink processing
///
/// First processes [[wikilinks]] to HTML links, then renders AsciiDoc.
pub fn render_asciidoc_with_wikilinks(content: &str) -> String {
    // First, process wikilinks
    let with_wikilinks = render_wikilinks_to_html(content);
    // Then render AsciiDoc
    render_asciidoc(&with_wikilinks)
}

/// Internal rendering without sanitization (for testing)
fn render_asciidoc_internal(content: &str) -> String {
    let mut html = content.to_string();

    // Process in order of precedence (block-level first, then inline)

    // 1. Code blocks (must be first to protect content inside)
    html = render_code_blocks(&html);

    // 2. Literal blocks
    html = render_literal_blocks(&html);

    // 3. Tables
    html = render_tables(&html);

    // 4. Admonitions
    html = render_admonitions(&html);

    // 5. Blockquotes
    html = render_blockquotes(&html);

    // 6. Headings
    html = render_headings(&html);

    // 7. Horizontal rules
    html = render_hr(&html);

    // 8. Lists (unordered then ordered)
    html = render_lists(&html);

    // 9. Inline elements
    html = render_images(&html);
    html = render_links(&html);
    html = render_bold(&html);
    html = render_italic(&html);
    html = render_mono(&html);

    // 10. Line breaks
    html = render_line_breaks(&html);

    // 11. Paragraphs (wrap remaining text)
    html = render_paragraphs(&html);

    html
}

// ============================================================================
// Block Element Rendering
// ============================================================================

fn render_headings(content: &str) -> String {
    HEADING_REGEX
        .replace_all(content, |caps: &regex::Captures| {
            let level = caps.get(1).map(|m| m.as_str().len()).unwrap_or(1);
            let text = caps.get(2).map(|m| m.as_str()).unwrap_or("");
            let id = slugify_heading(text);
            format!(r#"<h{} id="{}">{}</h{}>"#, level, id, text, level)
        })
        .to_string()
}

fn render_code_blocks(content: &str) -> String {
    CODE_BLOCK_REGEX
        .replace_all(content, |caps: &regex::Captures| {
            let lang = caps.get(1).map(|m| m.as_str()).unwrap_or("");
            let code = caps.get(2).map(|m| m.as_str()).unwrap_or("");
            let escaped = html_escape(code);

            if lang.is_empty() {
                format!(r#"<pre><code>{}</code></pre>"#, escaped)
            } else {
                format!(
                    r#"<pre><code class="language-{}">{}</code></pre>"#,
                    lang, escaped
                )
            }
        })
        .to_string()
}

fn render_literal_blocks(content: &str) -> String {
    LITERAL_BLOCK_REGEX
        .replace_all(content, |caps: &regex::Captures| {
            let text = caps.get(1).map(|m| m.as_str()).unwrap_or("");
            format!(r#"<pre>{}</pre>"#, html_escape(text))
        })
        .to_string()
}

fn render_tables(content: &str) -> String {
    TABLE_REGEX
        .replace_all(content, |caps: &regex::Captures| {
            let table_content = caps.get(1).map(|m| m.as_str()).unwrap_or("");
            render_table_content(table_content)
        })
        .to_string()
}

fn render_table_content(content: &str) -> String {
    let mut html = String::from("<table>");
    let mut in_header = true;
    let mut current_row: Vec<String> = Vec::new();

    for line in content.lines() {
        let line = line.trim();

        if line.is_empty() {
            // Empty line might separate header from body
            if !current_row.is_empty() {
                let tag = if in_header { "th" } else { "td" };
                html.push_str("<tr>");
                for cell in &current_row {
                    html.push_str(&format!("<{0}>{1}</{0}>", tag, cell));
                }
                html.push_str("</tr>");
                current_row.clear();
                in_header = false;
            }
            continue;
        }

        if line.starts_with('|') {
            // Parse cells from the line
            let cells: Vec<&str> = line
                .trim_start_matches('|')
                .trim_end_matches('|')
                .split('|')
                .collect();

            for cell in cells {
                current_row.push(cell.trim().to_string());
            }

            // Output the row
            let tag = if in_header { "th" } else { "td" };
            html.push_str("<tr>");
            for cell in &current_row {
                html.push_str(&format!("<{0}>{1}</{0}>", tag, cell));
            }
            html.push_str("</tr>");
            current_row.clear();
            in_header = false;
        }
    }

    html.push_str("</table>");
    html
}

fn render_admonitions(content: &str) -> String {
    ADMONITION_INLINE_REGEX
        .replace_all(content, |caps: &regex::Captures| {
            let kind = caps.get(1).map(|m| m.as_str()).unwrap_or("NOTE");
            let text = caps.get(2).map(|m| m.as_str()).unwrap_or("");
            let class = kind.to_lowercase();
            format!(
                r#"<div class="admonition admonition-{}"><span class="admonition-label">{}</span> {}</div>"#,
                class, kind, text
            )
        })
        .to_string()
}

fn render_blockquotes(content: &str) -> String {
    // Collect consecutive blockquote lines
    let lines: Vec<&str> = content.lines().collect();
    let mut result = Vec::new();
    let mut in_blockquote = false;
    let mut quote_lines: Vec<String> = Vec::new();

    for line in lines {
        if let Some(caps) = BLOCKQUOTE_LINE_REGEX.captures(line) {
            if !in_blockquote {
                in_blockquote = true;
            }
            let text = caps.get(1).map(|m| m.as_str()).unwrap_or("");
            quote_lines.push(text.to_string());
        } else {
            if in_blockquote {
                result.push(format!(
                    "<blockquote>{}</blockquote>",
                    quote_lines.join("<br>")
                ));
                quote_lines.clear();
                in_blockquote = false;
            }
            result.push(line.to_string());
        }
    }

    if in_blockquote && !quote_lines.is_empty() {
        result.push(format!(
            "<blockquote>{}</blockquote>",
            quote_lines.join("<br>")
        ));
    }

    result.join("\n")
}

fn render_hr(content: &str) -> String {
    HR_REGEX.replace_all(content, "<hr>").to_string()
}

fn render_lists(content: &str) -> String {
    let mut result = content.to_string();

    // Process unordered lists
    result = render_unordered_lists(&result);

    // Process ordered lists
    result = render_ordered_lists(&result);

    result
}

fn render_unordered_lists(content: &str) -> String {
    let lines: Vec<&str> = content.lines().collect();
    let mut result = Vec::new();
    let mut in_list = false;

    for line in lines {
        if let Some(caps) = UNORDERED_LIST_REGEX.captures(line) {
            if !in_list {
                result.push("<ul>".to_string());
                in_list = true;
            }
            let text = caps.get(2).map(|m| m.as_str()).unwrap_or("");
            result.push(format!("<li>{}</li>", text));
        } else {
            if in_list {
                result.push("</ul>".to_string());
                in_list = false;
            }
            result.push(line.to_string());
        }
    }

    if in_list {
        result.push("</ul>".to_string());
    }

    result.join("\n")
}

fn render_ordered_lists(content: &str) -> String {
    let lines: Vec<&str> = content.lines().collect();
    let mut result = Vec::new();
    let mut in_list = false;

    for line in lines {
        if let Some(caps) = ORDERED_LIST_REGEX.captures(line) {
            if !in_list {
                result.push("<ol>".to_string());
                in_list = true;
            }
            let text = caps.get(2).map(|m| m.as_str()).unwrap_or("");
            result.push(format!("<li>{}</li>", text));
        } else {
            if in_list {
                result.push("</ol>".to_string());
                in_list = false;
            }
            result.push(line.to_string());
        }
    }

    if in_list {
        result.push("</ol>".to_string());
    }

    result.join("\n")
}

fn render_paragraphs(content: &str) -> String {
    let lines: Vec<&str> = content.lines().collect();
    let mut result = Vec::new();
    let mut paragraph: Vec<&str> = Vec::new();

    for line in lines {
        let trimmed = line.trim();

        // Check if line is already HTML or should be treated as inline
        let is_block_element = trimmed.starts_with('<')
            && (trimmed.starts_with("<h")
                || trimmed.starts_with("<p")
                || trimmed.starts_with("<ul")
                || trimmed.starts_with("<ol")
                || trimmed.starts_with("<li")
                || trimmed.starts_with("</")
                || trimmed.starts_with("<table")
                || trimmed.starts_with("<tr")
                || trimmed.starts_with("<th")
                || trimmed.starts_with("<td")
                || trimmed.starts_with("<pre")
                || trimmed.starts_with("<blockquote")
                || trimmed.starts_with("<div")
                || trimmed.starts_with("<hr"));

        if trimmed.is_empty() || is_block_element {
            // Flush current paragraph
            if !paragraph.is_empty() {
                result.push(format!("<p>{}</p>", paragraph.join(" ")));
                paragraph.clear();
            }
            if !trimmed.is_empty() {
                result.push(trimmed.to_string());
            }
        } else {
            paragraph.push(trimmed);
        }
    }

    // Flush any remaining paragraph
    if !paragraph.is_empty() {
        result.push(format!("<p>{}</p>", paragraph.join(" ")));
    }

    result.join("\n")
}

// ============================================================================
// Inline Element Rendering
// ============================================================================

fn render_images(content: &str) -> String {
    IMAGE_REGEX
        .replace_all(content, |caps: &regex::Captures| {
            let url = caps.get(1).map(|m| m.as_str()).unwrap_or("");
            let alt = caps.get(2).map(|m| m.as_str()).unwrap_or("");
            format!(r#"<img src="{}" alt="{}">"#, url, html_escape(alt))
        })
        .to_string()
}

fn render_links(content: &str) -> String {
    LINK_REGEX
        .replace_all(content, |caps: &regex::Captures| {
            let url = caps.get(1).map(|m| m.as_str()).unwrap_or("");
            let text = caps.get(2).map(|m| m.as_str()).unwrap_or(url);
            format!(r#"<a href="{}">{}</a>"#, url, html_escape(text))
        })
        .to_string()
}

fn render_bold(content: &str) -> String {
    BOLD_REGEX
        .replace_all(content, |caps: &regex::Captures| {
            let text = caps.get(1).map(|m| m.as_str()).unwrap_or("");
            format!("<strong>{}</strong>", text)
        })
        .to_string()
}

fn render_italic(content: &str) -> String {
    // More careful replacement to avoid replacing underscores in URLs
    ITALIC_REGEX
        .replace_all(content, |caps: &regex::Captures| {
            let full = caps.get(0).map(|m| m.as_str()).unwrap_or("");
            let text = caps.get(1).map(|m| m.as_str()).unwrap_or("");

            // Preserve leading/trailing characters that aren't underscores
            let leading = if full.starts_with('_') { "" } else { &full[..1] };
            let trailing = if full.ends_with('_') { "" } else { &full[full.len()-1..] };

            format!("{}<em>{}</em>{}", leading, text, trailing)
        })
        .to_string()
}

fn render_mono(content: &str) -> String {
    MONO_REGEX
        .replace_all(content, |caps: &regex::Captures| {
            let text = caps
                .get(1)
                .or_else(|| caps.get(2))
                .map(|m| m.as_str())
                .unwrap_or("");
            format!("<code>{}</code>", html_escape(text))
        })
        .to_string()
}

fn render_line_breaks(content: &str) -> String {
    LINE_BREAK_REGEX.replace_all(content, "<br>\n").to_string()
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Simple HTML escaping
fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

/// Generate a slug from heading text for anchor IDs
fn slugify_heading(text: &str) -> String {
    text.to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

/// Extract plain text from content with automatic format detection (for previews)
///
/// Detects whether content is Markdown or AsciiDoc and strips formatting accordingly.
pub fn content_to_plain_text(content: &str) -> String {
    let format = detect_content_format(content);
    match format {
        ContentFormat::Markdown => markdown_to_plain_text(content),
        ContentFormat::AsciiDoc => asciidoc_to_text(content),
    }
}

/// Extract plain text from Markdown (for previews)
pub fn markdown_to_plain_text(content: &str) -> String {
    let mut text = content.to_string();

    // Remove code blocks (``` ... ```)
    let code_block_regex = Regex::new(r"(?s)```[^\n]*\n.*?```").unwrap();
    text = code_block_regex.replace_all(&text, " ").to_string();

    // Remove inline code (`code`)
    let inline_code_regex = Regex::new(r"`([^`]+)`").unwrap();
    text = inline_code_regex.replace_all(&text, "$1").to_string();

    // Remove headings (# ## ### etc.)
    let heading_regex = Regex::new(r"(?m)^#{1,6}\s+(.+)$").unwrap();
    text = heading_regex.replace_all(&text, "$1 ").to_string();

    // Remove bold (**text** or __text__) - must be done before italic
    let bold_double_star = Regex::new(r"\*\*([^*]+)\*\*").unwrap();
    text = bold_double_star.replace_all(&text, "$1").to_string();
    let bold_double_under = Regex::new(r"__([^_]+)__").unwrap();
    text = bold_double_under.replace_all(&text, "$1").to_string();

    // Remove italic (*text* or _text_) - single markers only
    let italic_star = Regex::new(r"\*([^*\n]+)\*").unwrap();
    text = italic_star.replace_all(&text, "$1").to_string();
    let italic_under = Regex::new(r"(?:^|[^a-zA-Z0-9])_([^_\n]+)_(?:[^a-zA-Z0-9]|$)").unwrap();
    text = italic_under.replace_all(&text, " $1 ").to_string();

    // Remove links [text](url)
    let link_regex = Regex::new(r"\[([^\]]+)\]\([^)]+\)").unwrap();
    text = link_regex.replace_all(&text, "$1").to_string();

    // Remove images ![alt](url)
    let image_regex = Regex::new(r"!\[([^\]]*)\]\([^)]+\)").unwrap();
    text = image_regex.replace_all(&text, "$1").to_string();

    // Remove wikilinks [[target|display]] or [[target]]
    let wikilink_regex = Regex::new(r"\[\[([^\]|]+)(?:\|([^\]]+))?\]\]").unwrap();
    text = wikilink_regex.replace_all(&text, |caps: &regex::Captures| {
        caps.get(2).map(|m| m.as_str()).unwrap_or(
            caps.get(1).map(|m| m.as_str()).unwrap_or("")
        ).to_string()
    }).to_string();

    // Remove blockquotes (> text)
    let blockquote_regex = Regex::new(r"(?m)^>\s*(.*)$").unwrap();
    text = blockquote_regex.replace_all(&text, "$1 ").to_string();

    // Remove list markers (* - + or 1. 2. etc.)
    let list_regex = Regex::new(r"(?m)^[\s]*[-*+]\s+(.+)$").unwrap();
    text = list_regex.replace_all(&text, "$1 ").to_string();
    let ordered_list_regex = Regex::new(r"(?m)^[\s]*\d+\.\s+(.+)$").unwrap();
    text = ordered_list_regex.replace_all(&text, "$1 ").to_string();

    // Remove horizontal rules
    let hr_regex = Regex::new(r"(?m)^[-*_]{3,}$").unwrap();
    text = hr_regex.replace_all(&text, " ").to_string();

    // Clean up whitespace
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Extract plain text from AsciiDoc (for previews)
pub fn asciidoc_to_text(content: &str) -> String {
    let mut text = content.to_string();

    // Remove code blocks
    text = CODE_BLOCK_REGEX.replace_all(&text, " ").to_string();
    text = LITERAL_BLOCK_REGEX.replace_all(&text, " ").to_string();

    // Remove headings markers
    text = HEADING_REGEX.replace_all(&text, "$2 ").to_string();

    // Remove formatting markers
    text = BOLD_REGEX.replace_all(&text, "$1").to_string();
    text = ITALIC_REGEX.replace_all(&text, "$1").to_string();
    text = MONO_REGEX.replace_all(&text, "$1$2").to_string();

    // Remove link/image syntax
    text = LINK_REGEX.replace_all(&text, "$2").to_string();
    text = IMAGE_REGEX.replace_all(&text, "$2").to_string();

    // Remove list markers
    text = UNORDERED_LIST_REGEX.replace_all(&text, "$2 ").to_string();
    text = ORDERED_LIST_REGEX.replace_all(&text, "$2 ").to_string();

    // Remove blockquote markers
    text = BLOCKQUOTE_LINE_REGEX.replace_all(&text, "$1 ").to_string();

    // Remove admonition markers
    text = ADMONITION_INLINE_REGEX.replace_all(&text, "$2 ").to_string();

    // Remove horizontal rules
    text = HR_REGEX.replace_all(&text, " ").to_string();

    // Clean up whitespace
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Wrap rendered HTML in a styled container
#[allow(dead_code)]
pub fn wrap_with_prose_styles(html: &str) -> String {
    format!(
        r#"<div class="prose prose-neutral dark:prose-invert max-w-none asciidoc-content">{}</div>"#,
        html
    )
}

/// Render content with auto-detection and wrap with prose styles
///
/// Convenience function that renders content (auto-detecting Markdown vs AsciiDoc)
/// and wraps the output in a styled prose container. Useful for contexts where
/// you need complete styled HTML output in one step.
#[allow(dead_code)]
pub fn render_content_styled(content: &str) -> String {
    let html = render_content_auto(content);
    wrap_with_prose_styles(&html)
}

/// Render AsciiDoc content and wrap with prose styles
///
/// Convenience function for AsciiDoc-only content that renders and
/// wraps the output in a styled prose container.
#[allow(dead_code)]
pub fn render_asciidoc_styled(content: &str) -> String {
    let html = render_asciidoc(content);
    wrap_with_prose_styles(&html)
}

// ============================================================================
// Citation-Aware Rendering
// ============================================================================

/// Result of rendering content with citations
#[derive(Clone, Debug, Default)]
pub struct RenderedContentWithCitations {
    /// Main rendered HTML content
    pub html: String,
    /// Citation identifiers extracted from content
    pub citation_identifiers: Vec<String>,
    /// Citation references found in content
    pub citations: Vec<CitationReference>,
    /// Whether content has any citations
    pub has_citations: bool,
    /// Generated footnotes HTML (if citations were resolved)
    pub footnotes_html: String,
    /// Generated endnotes HTML (if citations were resolved)
    pub endnotes_html: String,
}

/// Check if content contains citation markup
pub fn content_has_citations(content: &str) -> bool {
    has_citations(content)
}

/// Extract citation identifiers from content
pub fn extract_citation_identifiers(content: &str) -> Vec<String> {
    extract_citations(content)
        .into_iter()
        .map(|c| c.identifier)
        .collect()
}

/// Render content with automatic format detection and citation support
///
/// Returns rendered HTML along with citation information.
/// If `resolved_citations` is provided, citations will be rendered with full data.
/// Otherwise, citations render as placeholders that can be hydrated later.
/// The `enable_wikilinks` parameter controls whether [[wikilinks]] are processed.
pub fn render_content_with_citations(
    content: &str,
    resolved_citations: &HashMap<String, ResolvedCitation>,
    enable_wikilinks: bool,
) -> RenderedContentWithCitations {
    let format = detect_content_format(content);
    let has_citations_flag = has_citations(content);
    let citation_refs = extract_citations(content);
    let citation_identifiers: Vec<String> = citation_refs.iter().map(|c| c.identifier.clone()).collect();

    // Process citations in the content
    let content_with_citations = if has_citations_flag {
        render_citations_with_data(content, resolved_citations)
    } else {
        content.to_string()
    };

    // Render the content based on format
    let html = match format {
        ContentFormat::Markdown => {
            if enable_wikilinks {
                let with_wikilinks = render_wikilinks_to_html(&content_with_citations);
                render_markdown(&with_wikilinks)
            } else {
                render_markdown(&content_with_citations)
            }
        }
        ContentFormat::AsciiDoc => {
            if enable_wikilinks {
                render_asciidoc_with_wikilinks(&content_with_citations)
            } else {
                render_asciidoc(&content_with_citations)
            }
        }
    };

    // Generate footnotes and endnotes if we have citations
    let (footnotes_html, endnotes_html) = if has_citations_flag {
        (
            generate_footnotes_html_with_data(&citation_refs, resolved_citations),
            generate_endnotes_html_with_data(&citation_refs, resolved_citations),
        )
    } else {
        (String::new(), String::new())
    };

    RenderedContentWithCitations {
        html,
        citation_identifiers,
        citations: citation_refs,
        has_citations: has_citations_flag,
        footnotes_html,
        endnotes_html,
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_headings() {
        let asciidoc = "= Title\n\n== Chapter 1\n\n=== Section 1.1";
        let html = render_asciidoc_internal(asciidoc);
        assert!(html.contains("<h1"));
        assert!(html.contains("<h2"));
        assert!(html.contains("<h3"));
    }

    #[test]
    fn test_render_bold() {
        let asciidoc = "This is *bold* text.";
        let html = render_asciidoc_internal(asciidoc);
        assert!(html.contains("<strong>bold</strong>"));
    }

    #[test]
    fn test_render_italic() {
        let asciidoc = "This is _italic_ text.";
        let html = render_asciidoc_internal(asciidoc);
        assert!(html.contains("<em>italic</em>"));
    }

    #[test]
    fn test_render_mono() {
        let asciidoc = "This is `code` text.";
        let html = render_asciidoc_internal(asciidoc);
        assert!(html.contains("<code>code</code>"));
    }

    #[test]
    fn test_render_link() {
        let asciidoc = "Check out link:https://nostr.com[Nostr]";
        let html = render_asciidoc_internal(asciidoc);
        assert!(html.contains(r#"<a href="https://nostr.com">Nostr</a>"#));
    }

    #[test]
    fn test_render_image() {
        let asciidoc = "image::https://example.com/image.jpg[Alt text]";
        let html = render_asciidoc_internal(asciidoc);
        assert!(html.contains(r#"<img src="https://example.com/image.jpg""#));
        assert!(html.contains(r#"alt="Alt text""#));
    }

    #[test]
    fn test_render_code_block() {
        let asciidoc = "[source,rust]\n----\nfn main() {\n    println!(\"Hello\");\n}\n----";
        let html = render_asciidoc_internal(asciidoc);
        assert!(html.contains(r#"<pre><code class="language-rust">"#));
        assert!(html.contains("fn main()"));
    }

    #[test]
    fn test_render_unordered_list() {
        let asciidoc = "* Item 1\n* Item 2\n* Item 3";
        let html = render_asciidoc_internal(asciidoc);
        assert!(html.contains("<ul>"));
        assert!(html.contains("<li>Item 1</li>"));
        assert!(html.contains("</ul>"));
    }

    #[test]
    fn test_render_ordered_list() {
        let asciidoc = ". First\n. Second\n. Third";
        let html = render_asciidoc_internal(asciidoc);
        assert!(html.contains("<ol>"));
        assert!(html.contains("<li>First</li>"));
        assert!(html.contains("</ol>"));
    }

    #[test]
    fn test_render_blockquote() {
        let asciidoc = "> This is a quote\n> with two lines";
        let html = render_asciidoc_internal(asciidoc);
        assert!(html.contains("<blockquote>"));
        assert!(html.contains("This is a quote"));
    }

    #[test]
    fn test_render_admonition() {
        let asciidoc = "NOTE: This is important information.";
        let html = render_asciidoc_internal(asciidoc);
        assert!(html.contains("admonition-note"));
        assert!(html.contains("This is important information"));
    }

    #[test]
    fn test_render_hr() {
        let asciidoc = "Before\n\n'''\n\nAfter";
        let html = render_asciidoc_internal(asciidoc);
        assert!(html.contains("<hr>"));
    }

    #[test]
    fn test_asciidoc_to_text() {
        let asciidoc = "= Title\n\nThis is *bold* and _italic_ text with a link:https://example.com[link].";
        let text = asciidoc_to_text(asciidoc);
        assert!(text.contains("Title"));
        assert!(text.contains("bold"));
        assert!(text.contains("italic"));
        assert!(text.contains("link"));
        assert!(!text.contains('*'));
        assert!(!text.contains('_'));
    }

    #[test]
    fn test_slugify_heading() {
        assert_eq!(slugify_heading("Hello World"), "hello-world");
        assert_eq!(slugify_heading("Chapter 1: Introduction"), "chapter-1-introduction");
    }
}
