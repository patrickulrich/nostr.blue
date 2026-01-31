//! Clipboard utilities for copying text and formatted content
//!
//! Provides a cross-platform way to copy text to the clipboard using
//! the Web Clipboard API.
use wasm_bindgen::JsValue;
/// Copy text to the system clipboard
///
/// Uses the Web Clipboard API to copy the provided text.
///
/// # Arguments
/// * `text` - The text to copy to the clipboard
///
/// # Returns
/// * `Ok(())` if the text was successfully copied
/// * `Err(JsValue)` if the operation failed
pub async fn copy_to_clipboard(text: &str) -> Result<(), JsValue> {
    let window = web_sys::window().ok_or_else(|| JsValue::from_str("No window"))?;
    let navigator = window.navigator();
    let clipboard = navigator.clipboard();
    wasm_bindgen_futures::JsFuture::from(clipboard.write_text(text)).await.map(|_| ())
}
/// Copy formatted HTML content to the clipboard
///
/// Renders Markdown/AsciiDoc content to styled HTML and copies it.
/// Falls back to plain text copy if the Clipboard API doesn't support
/// rich content (ClipboardItem requires web_sys feature flags).
///
/// # Arguments
/// * `content` - The Markdown or AsciiDoc content to render and copy
///
/// # Returns
/// * `Ok(())` if the content was successfully copied
/// * `Err(JsValue)` if the operation failed
pub async fn copy_formatted_content(content: &str) -> Result<(), JsValue> {
    use crate::utils::asciidoc::render_content_styled;
    let html_content = render_content_styled(content);
    copy_to_clipboard(&html_content).await
}
