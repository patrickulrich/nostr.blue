use serde::{Deserialize, Serialize};

/// Metadata extracted from a URL
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct UrlMetadata {
    /// Page title (from og:title, twitter:title, or <title>)
    pub title: Option<String>,
    /// Page description (from og:description, twitter:description, or meta description)
    pub description: Option<String>,
    /// Main image URL (from og:image, twitter:image)
    pub image: Option<String>,
    /// Site name (from og:site_name)
    pub site_name: Option<String>,
    /// URL that was fetched (may differ from requested if redirected)
    pub url: String,
}

/// Fetch metadata from a URL by parsing HTML meta tags
///
/// This function fetches the HTML content and extracts Open Graph tags,
/// Twitter Card tags, and standard HTML meta tags.
///
/// # Arguments
/// * `url` - The URL to fetch metadata from (with or without https://)
///
/// # Returns
/// * `Ok(UrlMetadata)` - Extracted metadata (fields may be None if not found)
/// * `Err(String)` - Error message if fetch or parse fails
pub async fn fetch_url_metadata(url: String) -> Result<UrlMetadata, String> {
    // Ensure URL has a scheme
    let full_url = if url.starts_with("http://") || url.starts_with("https://") {
        url.clone()
    } else {
        format!("https://{}", url)
    };

    log::info!("Fetching metadata for URL: {}", full_url);

    // Fetch HTML content
    #[cfg(target_arch = "wasm32")]
    let html = fetch_html_wasm(&full_url).await?;

    #[cfg(not(target_arch = "wasm32"))]
    let html = fetch_html_native(&full_url).await?;

    // Parse HTML and extract metadata
    let metadata = parse_html_metadata(&html, full_url);

    log::info!("Extracted metadata: title={:?}, description={:?}, image={:?}",
        metadata.title.as_ref().map(|s| s.chars().take(50).collect::<String>()),
        metadata.description.as_ref().map(|s| s.chars().take(50).collect::<String>()),
        metadata.image.is_some()
    );

    Ok(metadata)
}

/// Fetch HTML content using gloo-net (WASM)
#[cfg(target_arch = "wasm32")]
async fn fetch_html_wasm(url: &str) -> Result<String, String> {
    use gloo_net::http::Request;

    let response = Request::get(url)
        .send()
        .await
        .map_err(|e| format!("Failed to fetch URL: {}", e))?;

    if !response.ok() {
        return Err(format!("HTTP error: {}", response.status()));
    }

    response
        .text()
        .await
        .map_err(|e| format!("Failed to read response body: {}", e))
}

/// Fetch HTML content using reqwest (native)
#[cfg(not(target_arch = "wasm32"))]
async fn fetch_html_native(url: &str) -> Result<String, String> {
    let client = reqwest::Client::builder()
        .user_agent("Mozilla/5.0 (compatible; NostrBlueBot/1.0)")
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))?;

    let response = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("Failed to fetch URL: {}", e))?;

    if !response.status().is_success() {
        return Err(format!("HTTP error: {}", response.status()));
    }

    response
        .text()
        .await
        .map_err(|e| format!("Failed to read response body: {}", e))
}

/// Parse HTML and extract metadata from meta tags
fn parse_html_metadata(html: &str, url: String) -> UrlMetadata {
    let mut metadata = UrlMetadata {
        url,
        ..Default::default()
    };

    // Extract <title> tag
    if let Some(title) = extract_tag_content(html, "<title", "</title>") {
        metadata.title = Some(clean_text(&title));
    }

    // Extract meta tags
    let meta_tags = extract_meta_tags(html);

    // Priority order for title: og:title > twitter:title > title tag
    if let Some(og_title) = meta_tags.iter()
        .find(|tag| tag.property.as_deref() == Some("og:title"))
        .and_then(|tag| tag.content.clone())
    {
        metadata.title = Some(clean_text(&og_title));
    } else if let Some(twitter_title) = meta_tags.iter()
        .find(|tag| tag.name.as_deref() == Some("twitter:title"))
        .and_then(|tag| tag.content.clone())
    {
        metadata.title = Some(clean_text(&twitter_title));
    }

    // Priority order for description: og:description > twitter:description > meta description
    if let Some(og_desc) = meta_tags.iter()
        .find(|tag| tag.property.as_deref() == Some("og:description"))
        .and_then(|tag| tag.content.clone())
    {
        metadata.description = Some(clean_text(&og_desc));
    } else if let Some(twitter_desc) = meta_tags.iter()
        .find(|tag| tag.name.as_deref() == Some("twitter:description"))
        .and_then(|tag| tag.content.clone())
    {
        metadata.description = Some(clean_text(&twitter_desc));
    } else if let Some(desc) = meta_tags.iter()
        .find(|tag| tag.name.as_deref() == Some("description"))
        .and_then(|tag| tag.content.clone())
    {
        metadata.description = Some(clean_text(&desc));
    }

    // Priority order for image: og:image > twitter:image
    if let Some(og_image) = meta_tags.iter()
        .find(|tag| tag.property.as_deref() == Some("og:image"))
        .and_then(|tag| tag.content.clone())
    {
        metadata.image = Some(og_image.trim().to_string());
    } else if let Some(twitter_image) = meta_tags.iter()
        .find(|tag| tag.name.as_deref() == Some("twitter:image"))
        .and_then(|tag| tag.content.clone())
    {
        metadata.image = Some(twitter_image.trim().to_string());
    }

    // Extract site name
    if let Some(site_name) = meta_tags.iter()
        .find(|tag| tag.property.as_deref() == Some("og:site_name"))
        .and_then(|tag| tag.content.clone())
    {
        metadata.site_name = Some(clean_text(&site_name));
    }

    metadata
}

/// Represents a meta tag
#[derive(Debug)]
struct MetaTag {
    name: Option<String>,
    property: Option<String>,
    content: Option<String>,
}

/// Find the closing '>' bracket while ignoring those inside quotes
fn find_unquoted_close_bracket(tag: &str) -> Option<usize> {
    let bytes = tag.as_bytes();
    let mut in_single_quote = false;
    let mut in_double_quote = false;

    for (i, &byte) in bytes.iter().enumerate() {
        match byte {
            b'\'' if !in_double_quote => in_single_quote = !in_single_quote,
            b'"' if !in_single_quote => in_double_quote = !in_double_quote,
            b'>' if !in_single_quote && !in_double_quote => return Some(i),
            _ => {}
        }
    }

    None
}

/// Extract all meta tags from HTML
fn extract_meta_tags(html: &str) -> Vec<MetaTag> {
    let mut tags = Vec::new();
    let mut pos = 0;

    // Search for "<meta" case-insensitively by scanning byte positions
    while pos < html.len() {
        // Look for '<' character
        if let Some(lt_pos) = html[pos..].find('<') {
            let meta_pos = pos + lt_pos;
            let remaining = &html[meta_pos..];

            // Check if this is a "<meta" tag (case-insensitive)
            if remaining.len() >= 5 && remaining[..5].eq_ignore_ascii_case("<meta") {
                // Find the closing '>' while handling quoted attributes
                let close_offset = find_unquoted_close_bracket(remaining);

                if let Some(offset) = close_offset {
                    let tag_content = &remaining[..offset];

                    let name = extract_attribute(tag_content, "name");
                    let property = extract_attribute(tag_content, "property");
                    let content = extract_attribute(tag_content, "content");

                    if name.is_some() || property.is_some() {
                        tags.push(MetaTag { name, property, content });
                    }

                    pos = meta_pos + offset + 1;
                } else {
                    // Unterminated tag, break out safely
                    break;
                }
            } else {
                // Not a meta tag, move past this '<'
                pos = meta_pos + 1;
            }
        } else {
            // No more '<' characters found
            break;
        }
    }

    tags
}

/// Extract attribute value from HTML tag
fn extract_attribute(tag: &str, attr_name: &str) -> Option<String> {
    let tag_lower = tag.to_ascii_lowercase();
    let attr_name_lower = attr_name.to_ascii_lowercase();

    // Find the attribute name
    let mut search_pos = 0;
    while let Some(attr_pos) = tag_lower[search_pos..].find(&attr_name_lower) {
        let actual_pos = search_pos + attr_pos;

        // Check if this is a word boundary (preceded by whitespace or start of tag)
        let is_word_start = actual_pos == 0 ||
            tag_lower.as_bytes()[actual_pos - 1].is_ascii_whitespace() ||
            tag_lower.as_bytes()[actual_pos - 1] == b'<';

        if !is_word_start {
            search_pos = actual_pos + 1;
            continue;
        }

        // Skip past the attribute name
        let mut pos = actual_pos + attr_name_lower.len();

        // Skip whitespace after attribute name
        while pos < tag.len() && tag.as_bytes()[pos].is_ascii_whitespace() {
            pos += 1;
        }

        // Check for '='
        if pos >= tag.len() || tag.as_bytes()[pos] != b'=' {
            search_pos = actual_pos + 1;
            continue;
        }
        pos += 1; // Skip '='

        // Skip whitespace after '='
        while pos < tag.len() && tag.as_bytes()[pos].is_ascii_whitespace() {
            pos += 1;
        }

        if pos >= tag.len() {
            return None;
        }

        let remaining = &tag[pos..];

        // Check for quote character
        if let Some(first_char) = remaining.chars().next() {
            if first_char == '"' || first_char == '\'' {
                // Quoted value
                if let Some(close_pos) = remaining[1..].find(first_char) {
                    return Some(remaining[1..close_pos + 1].to_string());
                }
            } else {
                // Unquoted value - read until whitespace or '>'
                let end_pos = remaining
                    .find(|c: char| c.is_ascii_whitespace() || c == '>')
                    .unwrap_or(remaining.len());
                return Some(remaining[..end_pos].to_string());
            }
        }

        return None;
    }

    None
}

/// Extract content between opening and closing tags
fn extract_tag_content(html: &str, open_tag: &str, close_tag: &str) -> Option<String> {
    let html_lower = html.to_ascii_lowercase();
    let open_tag_lower = open_tag.to_ascii_lowercase();
    let close_tag_lower = close_tag.to_ascii_lowercase();

    if let Some(start_pos) = html_lower.find(&open_tag_lower) {
        // Find the end of the opening tag
        if let Some(tag_end) = html_lower[start_pos..].find('>') {
            let content_start = start_pos + tag_end + 1;

            // Find the closing tag
            if let Some(close_pos) = html_lower[content_start..].find(&close_tag_lower) {
                let content = &html[content_start..content_start + close_pos];
                return Some(content.to_string());
            }
        }
    }

    None
}

/// Clean text by decoding HTML entities and trimming whitespace
fn clean_text(text: &str) -> String {
    let mut result = text.trim().to_string();

    // Decode common HTML entities
    result = result.replace("&amp;", "&");
    result = result.replace("&lt;", "<");
    result = result.replace("&gt;", ">");
    result = result.replace("&quot;", "\"");
    result = result.replace("&#39;", "'");
    result = result.replace("&apos;", "'");
    result = result.replace("&nbsp;", " ");

    // Remove extra whitespace
    result = result.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_attribute() {
        let tag = r#"<meta property="og:title" content="Test Title">"#;
        assert_eq!(extract_attribute(tag, "property"), Some("og:title".to_string()));
        assert_eq!(extract_attribute(tag, "content"), Some("Test Title".to_string()));
    }

    #[test]
    fn test_extract_tag_content() {
        let html = r#"<title>Page Title</title>"#;
        assert_eq!(extract_tag_content(html, "<title", "</title>"), Some("Page Title".to_string()));
    }

    #[test]
    fn test_clean_text() {
        assert_eq!(clean_text("  Hello &amp; World  "), "Hello & World");
        assert_eq!(clean_text("Test&nbsp;&nbsp;Text"), "Test Text");
    }

    #[test]
    fn test_parse_html_metadata() {
        let html = r#"
            <html>
            <head>
                <title>Test Page</title>
                <meta property="og:title" content="OG Title">
                <meta property="og:description" content="OG Description">
                <meta property="og:image" content="https://example.com/image.jpg">
                <meta property="og:site_name" content="Example Site">
            </head>
            </html>
        "#;

        let metadata = parse_html_metadata(html, "https://example.com".to_string());

        assert_eq!(metadata.title, Some("OG Title".to_string()));
        assert_eq!(metadata.description, Some("OG Description".to_string()));
        assert_eq!(metadata.image, Some("https://example.com/image.jpg".to_string()));
        assert_eq!(metadata.site_name, Some("Example Site".to_string()));
    }
}
