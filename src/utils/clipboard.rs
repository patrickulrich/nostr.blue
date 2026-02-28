//! Clipboard utilities for copying text and formatted content
//!
//! Provides a cross-platform way to copy text to the clipboard.
//! On web, uses the Web Clipboard API. On native, uses arboard.

/// Copy text to the system clipboard
pub async fn copy_to_clipboard(text: &str) -> Result<(), String> {
    crate::platform::clipboard::copy_to_clipboard(text).await
}

/// Copy formatted HTML content to the clipboard
///
/// Renders Markdown/AsciiDoc content to styled HTML and copies it.
pub async fn copy_formatted_content(content: &str) -> Result<(), String> {
    use crate::utils::asciidoc::render_content_styled;
    let html_content = render_content_styled(content);
    copy_to_clipboard(&html_content).await
}
