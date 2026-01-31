//! Download utilities for exporting wiki content as Markdown and triggering print-to-PDF
//!
//! Provides browser-based download functionality for wiki pages:
//! - Markdown export with AsciiDoc to Markdown conversion
//! - Print-to-PDF via browser print dialog (for formatted PDF output)

use regex::Regex;
use std::sync::LazyLock;
use wasm_bindgen::prelude::*;

// Regex patterns for AsciiDoc to Markdown conversion
static ASCIIDOC_HEADING: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?m)^(={1,6})\s+(.+?)(?:\s+=+)?$").expect("Invalid heading regex"));

static ASCIIDOC_BOLD: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\*([^*\n]+?)\*").expect("Invalid bold regex"));

static ASCIIDOC_LINK: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"link:(https?://[^\s\[]+)\[([^\]]*)\]").expect("Invalid link regex"));

static ASCIIDOC_IMAGE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"image::?([^\s\[]+)\[([^\]]*)\]").expect("Invalid image regex"));

static ASCIIDOC_CODE_BLOCK: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?s)\[source(?:,([^\]]*))?\]\s*\n----\n(.*?)\n----")
        .expect("Invalid code block regex")
});

static WIKILINK: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\[\[([^\]|]+)(?:\|([^\]]+))?\]\]").expect("Invalid wikilink regex"));

/// Download content as a Markdown file
///
/// Converts AsciiDoc content to Markdown format and triggers a browser download.
pub fn download_markdown(filename: &str, title: &str, content: &str) {
    let markdown = asciidoc_to_markdown(title, content);
    download_blob(filename, &markdown, "text/markdown;charset=utf-8");
}

/// Convert AsciiDoc content to Markdown format
///
/// Handles common AsciiDoc patterns:
/// - Headings: = Title -> # Title
/// - Bold: *text* -> **text**
/// - Links: link:url[text] -> [text](url)
/// - Images: image::url[alt] -> ![alt](url)
/// - Code blocks: [source,lang]\n----\ncode\n---- -> ```lang\ncode\n```
/// - Wikilinks: [[page]] -> [page](/wiki/page)
pub fn asciidoc_to_markdown(title: &str, content: &str) -> String {
    let mut markdown = String::new();

    // Add title as H1 if provided
    if !title.is_empty() {
        markdown.push_str(&format!("# {}\n\n", title));
    }

    let mut result = content.to_string();

    // Convert code blocks first (before other transformations)
    result = ASCIIDOC_CODE_BLOCK
        .replace_all(&result, |caps: &regex::Captures| {
            let lang = caps.get(1).map(|m| m.as_str()).unwrap_or("");
            let code = caps.get(2).map(|m| m.as_str()).unwrap_or("");
            format!("```{}\n{}\n```", lang, code)
        })
        .to_string();

    // Convert headings: = Title -> # Title
    result = ASCIIDOC_HEADING
        .replace_all(&result, |caps: &regex::Captures| {
            let level = caps.get(1).map(|m| m.as_str().len()).unwrap_or(1);
            let text = caps.get(2).map(|m| m.as_str()).unwrap_or("");
            format!("{} {}", "#".repeat(level), text)
        })
        .to_string();

    // Convert bold: *text* -> **text**
    result = ASCIIDOC_BOLD
        .replace_all(&result, |caps: &regex::Captures| {
            let text = caps.get(1).map(|m| m.as_str()).unwrap_or("");
            format!("**{}**", text)
        })
        .to_string();

    // Convert links: link:url[text] -> [text](url)
    result = ASCIIDOC_LINK
        .replace_all(&result, |caps: &regex::Captures| {
            let url = caps.get(1).map(|m| m.as_str()).unwrap_or("");
            let text = caps.get(2).map(|m| m.as_str()).unwrap_or(url);
            format!("[{}]({})", text, url)
        })
        .to_string();

    // Convert images: image::url[alt] -> ![alt](url)
    result = ASCIIDOC_IMAGE
        .replace_all(&result, |caps: &regex::Captures| {
            let url = caps.get(1).map(|m| m.as_str()).unwrap_or("");
            let alt = caps.get(2).map(|m| m.as_str()).unwrap_or("");
            format!("![{}]({})", alt, url)
        })
        .to_string();

    // Convert wikilinks: [[page]] or [[page|text]] -> [text](/wiki/page)
    result = WIKILINK
        .replace_all(&result, |caps: &regex::Captures| {
            let page = caps.get(1).map(|m| m.as_str()).unwrap_or("");
            let text = caps.get(2).map(|m| m.as_str()).unwrap_or(page);
            let slug = page
                .to_lowercase()
                .chars()
                .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
                .collect::<String>()
                .split('-')
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join("-");
            format!("[{}](/wiki/{})", text, slug)
        })
        .to_string();

    markdown.push_str(&result);
    markdown
}

/// Trigger a browser download for text content
///
/// Creates a Blob URL and programmatically clicks a download link.
pub fn download_blob(filename: &str, content: &str, mime_type: &str) {
    #[wasm_bindgen]
    extern "C" {
        #[wasm_bindgen(js_namespace = URL, js_name = createObjectURL)]
        fn create_object_url(blob: &web_sys::Blob) -> String;

        #[wasm_bindgen(js_namespace = URL, js_name = revokeObjectURL)]
        fn revoke_object_url(url: &str);
    }

    let window = match web_sys::window() {
        Some(w) => w,
        None => {
            log::error!("No window available for download");
            return;
        }
    };

    let document = match window.document() {
        Some(d) => d,
        None => {
            log::error!("No document available for download");
            return;
        }
    };

    // Create blob from content
    let parts = js_sys::Array::new();
    parts.push(&JsValue::from_str(content));

    let options = js_sys::Object::new();
    js_sys::Reflect::set(&options, &"type".into(), &mime_type.into()).ok();

    let blob = match web_sys::Blob::new_with_str_sequence_and_options(
        &parts,
        &options.unchecked_into(),
    ) {
        Ok(b) => b,
        Err(e) => {
            log::error!("Failed to create blob: {:?}", e);
            return;
        }
    };

    let url = create_object_url(&blob);

    // Create and click download link
    let a = match document.create_element("a") {
        Ok(el) => el,
        Err(e) => {
            log::error!("Failed to create anchor element: {:?}", e);
            revoke_object_url(&url);
            return;
        }
    };

    a.set_attribute("href", &url).ok();
    a.set_attribute("download", filename).ok();

    // Trigger download
    if let Ok(click_fn) = js_sys::Reflect::get(&a, &"click".into()) {
        let click_fn: js_sys::Function = click_fn.unchecked_into();
        click_fn.call0(&a).ok();
    }

    // Clean up
    revoke_object_url(&url);
}

/// Open print dialog for PDF export
///
/// Uses the browser's built-in print functionality which allows "Save as PDF".
/// This provides a high-quality PDF with proper formatting.
pub fn trigger_print() {
    if let Some(window) = web_sys::window() {
        window.print().ok();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_asciidoc_heading_conversion() {
        let result = asciidoc_to_markdown("", "= Title\n\n== Chapter");
        assert!(result.contains("# Title"));
        assert!(result.contains("## Chapter"));
    }

    #[test]
    fn test_asciidoc_bold_conversion() {
        let result = asciidoc_to_markdown("", "This is *bold* text");
        assert!(result.contains("**bold**"));
    }

    #[test]
    fn test_asciidoc_link_conversion() {
        let result = asciidoc_to_markdown("", "Check link:https://example.com[Example]");
        assert!(result.contains("[Example](https://example.com)"));
    }

    #[test]
    fn test_asciidoc_image_conversion() {
        let result = asciidoc_to_markdown("", "image::https://example.com/img.png[Alt text]");
        assert!(result.contains("![Alt text](https://example.com/img.png)"));
    }

    #[test]
    fn test_asciidoc_code_block_conversion() {
        let result = asciidoc_to_markdown("", "[source,rust]\n----\nfn main() {}\n----");
        assert!(result.contains("```rust\nfn main() {}\n```"));
    }

    #[test]
    fn test_wikilink_conversion() {
        let result = asciidoc_to_markdown("", "See [[Bitcoin]] for more info");
        assert!(result.contains("[Bitcoin](/wiki/bitcoin)"));
    }

    #[test]
    fn test_wikilink_with_text_conversion() {
        let result = asciidoc_to_markdown("", "Learn about [[Bitcoin|digital gold]]");
        assert!(result.contains("[digital gold](/wiki/bitcoin)"));
    }

    #[test]
    fn test_title_added() {
        let result = asciidoc_to_markdown("My Wiki Page", "Some content here");
        assert!(result.starts_with("# My Wiki Page\n\n"));
    }
}
