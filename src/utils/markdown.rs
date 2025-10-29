/// Markdown rendering utilities for NIP-23 long-form content
use pulldown_cmark::{Parser, Options, html};

/// Render markdown to safe HTML
/// Uses pulldown-cmark for parsing and ammonia for sanitization
pub fn render_markdown(markdown: &str) -> String {
    // Set up markdown options (GitHub-flavored markdown)
    let mut options = Options::empty();
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_FOOTNOTES);
    options.insert(Options::ENABLE_TASKLISTS);
    options.insert(Options::ENABLE_HEADING_ATTRIBUTES);

    // Parse markdown to HTML
    let parser = Parser::new_ext(markdown, options);
    let mut html_output = String::new();
    html::push_html(&mut html_output, parser);

    // Sanitize HTML to prevent XSS
    sanitize_html(&html_output)
}

/// Sanitize HTML using ammonia
/// Allows safe tags and attributes while removing potentially dangerous content
pub fn sanitize_html(html: &str) -> String {
    use ammonia::Builder;
    use maplit::{hashmap, hashset};

    Builder::default()
        // Allow common HTML tags
        .tags(hashset![
            "h1", "h2", "h3", "h4", "h5", "h6",
            "p", "br", "hr",
            "strong", "em", "u", "s", "del", "ins",
            "a",
            "ul", "ol", "li",
            "blockquote",
            "code", "pre",
            "table", "thead", "tbody", "tr", "th", "td",
            "img",
            "div", "span",
            "sup", "sub",
        ])
        // Allow specific attributes on specific tags
        // Note: "rel" is NOT in the "a" attributes because link_rel() handles it automatically
        .tag_attributes(hashmap![
            "a" => hashset!["href", "title", "target"],
            "img" => hashset!["src", "alt", "title", "width", "height"],
            "code" => hashset!["class"],
            "pre" => hashset!["class"],
            "div" => hashset!["class"],
            "span" => hashset!["class"],
            "th" => hashset!["align"],
            "td" => hashset!["align"],
        ])
        // Allow all http(s) URL schemes
        .url_schemes(hashset!["http", "https", "mailto"])
        // Set rel="noopener noreferrer" for external links (handled automatically)
        .link_rel(Some("noopener noreferrer"))
        // Clean the HTML
        .clean(html)
        .to_string()
}

/// Extract plain text from markdown (for previews)
#[allow(dead_code)]
pub fn markdown_to_text(markdown: &str) -> String {
    use pulldown_cmark::{Event, Tag};

    let parser = Parser::new(markdown);
    let mut text = String::new();

    for event in parser {
        match event {
            Event::Text(t) | Event::Code(t) => {
                text.push_str(&t);
                text.push(' ');
            }
            Event::SoftBreak | Event::HardBreak => {
                text.push(' ');
            }
            Event::Start(Tag::Paragraph) => {
                if !text.is_empty() && !text.ends_with(' ') {
                    text.push(' ');
                }
            }
            _ => {}
        }
    }

    text.trim().to_string()
}

/// Wrap rendered HTML in a styled container
/// Returns HTML with prose styling classes
#[allow(dead_code)]
pub fn wrap_with_prose_styles(html: &str) -> String {
    format!(
        r#"<div class="prose prose-neutral dark:prose-invert max-w-none">{}</div>"#,
        html
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_basic_markdown() {
        let md = "# Hello\n\nThis is **bold** and this is *italic*.";
        let html = render_markdown(md);
        assert!(html.contains("<h1>"));
        assert!(html.contains("<strong>"));
        assert!(html.contains("<em>"));
    }

    #[test]
    fn test_sanitize_script_tags() {
        let dangerous = "<p>Safe</p><script>alert('xss')</script>";
        let safe = sanitize_html(dangerous);
        assert!(!safe.contains("<script"));
        assert!(safe.contains("<p>"));
    }

    #[test]
    fn test_markdown_to_text() {
        let md = "# Title\n\nThis is **bold** text with [a link](https://example.com).";
        let text = markdown_to_text(md);
        assert_eq!(text, "Title This is bold text with a link .");
    }

    #[test]
    fn test_render_links() {
        let md = "[Click here](https://example.com)";
        let html = render_markdown(md);
        assert!(html.contains("<a"));
        assert!(html.contains("href=\"https://example.com\""));
    }

    #[test]
    fn test_render_images() {
        let md = "![Alt text](https://example.com/image.jpg)";
        let html = render_markdown(md);
        assert!(html.contains("<img"));
        assert!(html.contains("src=\"https://example.com/image.jpg\""));
        assert!(html.contains("alt=\"Alt text\""));
    }

    #[test]
    fn test_render_table() {
        let md = "| Header 1 | Header 2 |\n|----------|----------|\n| Cell 1   | Cell 2   |";
        let html = render_markdown(md);
        assert!(html.contains("<table"));
        assert!(html.contains("<th>"));
        assert!(html.contains("<td>"));
    }
}
