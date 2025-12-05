//! Clipboard utilities for copying text
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
    wasm_bindgen_futures::JsFuture::from(clipboard.write_text(text))
        .await
        .map(|_| ())
}
