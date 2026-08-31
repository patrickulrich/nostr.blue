//! Imperative textarea DOM operations shared by all composers.
//!
//! The composer textareas are deliberately **uncontrolled**: the DOM element owns
//! the text, and Rust keeps a mirror signal in sync via `oninput`. Any programmatic
//! mutation (emoji/GIF/media/mention insert, clear) MUST go through this module so
//! the DOM value and the caret are written in one atomic step. This avoids the
//! controlled-`value` caret-reset race (a `node.value = x` write from a stale render
//! diff always strands the caret at the end of the textarea).
//!
//! - `web`: synchronous `web_sys` access.
//! - desktop/mobile WebView: async `document::eval` round-trips through the
//!   WebView IPC layer, with the payload JSON-escaped to make string embedding safe.

use crate::utils::text::{utf8_to_utf16_index, utf16_to_utf8_index};
#[cfg(feature = "web")]
use wasm_bindgen::JsCast;

/// Encode a string as a JavaScript string-literal expression (including quotes).
///
/// Uses serde_json escaping so quotes, newlines, backslashes and control
/// characters are safely embedded when interpolating into `document::eval` scripts.
pub fn js_string_literal(s: &str) -> String {
    serde_json::to_string(s).unwrap_or_else(|_| "\"\"".to_string())
}

/// Read the current caret (selection start) as a UTF-8 byte index into `text`.
///
/// Returns `None` when the element cannot be found or the platform cannot read
/// the selection synchronously.
#[cfg(feature = "web")]
pub fn read_caret_sync(textarea_id: &str, text: &str) -> Option<usize> {
    let textarea = textarea_element(textarea_id)?;
    let start = textarea.selection_start().ok().flatten()? as usize;
    Some(utf16_to_utf8_index(text, start))
}

#[cfg(feature = "web")]
fn textarea_element(textarea_id: &str) -> Option<web_sys::HtmlTextAreaElement> {
    web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.get_element_by_id(textarea_id))
        .and_then(|el| el.dyn_into::<web_sys::HtmlTextAreaElement>().ok())
}

/// Read the caret (selection start) as a UTF-8 byte index into `text`, awaiting
/// the WebView IPC round-trip when the platform requires it. Falls back to
/// `fallback` (the last tracked caret) when the read fails.
pub async fn read_caret(textarea_id: &str, text: &str, fallback: usize) -> usize {
    #[cfg(feature = "web")]
    {
        read_caret_sync(textarea_id, text)
            .unwrap_or_else(|| crate::hooks::use_composer_editor::to_char_boundary(text, fallback))
    }
    #[cfg(not(feature = "web"))]
    {
        let script = format!(
            "return document.getElementById('{}')?.selectionStart ?? -1",
            js_string_literal(textarea_id),
        );
        match dioxus::prelude::document::eval(&script).await {
            Ok(v) => {
                let pos = v.as_f64().filter(|&p| p >= 0.0).map(|p| p as usize);
                match pos {
                    Some(p) => utf16_to_utf8_index(text, p),
                    None => crate::hooks::use_composer_editor::to_char_boundary(text, fallback),
                }
            }
            Err(_) => crate::hooks::use_composer_editor::to_char_boundary(text, fallback),
        }
    }
}

/// Atomically write `value` and place the caret at `caret_utf8` (UTF-8 byte index
/// into `value`), refocusing the textarea.
#[cfg(feature = "web")]
pub fn write_value_and_caret_sync(textarea_id: &str, value: &str, caret_utf8: usize) {
    if let Some(textarea) = textarea_element(textarea_id) {
        textarea.set_value(value);
        let utf16 = utf8_to_utf16_index(value, caret_utf8) as u32;
        let _ = textarea.set_selection_range(utf16, utf16);
        let _ = textarea.focus();
    }
}

/// Atomically write `value` and place the caret at `caret_utf8`, refocusing the
/// textarea. Awaits the WebView IPC round-trip on desktop/mobile.
pub async fn write_value_and_caret(textarea_id: &str, value: &str, caret_utf8: usize) {
    #[cfg(feature = "web")]
    {
        write_value_and_caret_sync(textarea_id, value, caret_utf8);
    }
    #[cfg(not(feature = "web"))]
    {
        let utf16 = utf8_to_utf16_index(value, caret_utf8);
        let script = format!(
            "const el = document.getElementById({id}); \
             if (el) {{ el.value = {value}; el.setSelectionRange({caret}, {caret}); el.focus(); }}",
            id = js_string_literal(textarea_id),
            value = js_string_literal(value),
            caret = utf16,
        );
        let _ = dioxus::prelude::document::eval(&script).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn js_string_literal_escapes_quotes_and_newlines() {
        assert_eq!(js_string_literal("plain"), "\"plain\"");
        assert_eq!(js_string_literal("a\"b"), "\"a\\\"b\"");
        assert_eq!(js_string_literal("line1\nline2"), "\"line1\\nline2\"");
        assert_eq!(js_string_literal("back\\slash"), "\"back\\\\slash\"");
    }

    #[test]
    fn js_string_literal_handles_control_chars_and_unicode() {
        assert_eq!(js_string_literal("\u{1}"), "\"\\u0001\"");
        assert_eq!(js_string_literal("\u{1F600}"), "\"😀\"");
    }
}
