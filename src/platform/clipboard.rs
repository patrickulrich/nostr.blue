/// Copy text to the system clipboard.
pub async fn copy_to_clipboard(text: &str) -> Result<(), String> {
    #[cfg(feature = "web")]
    {
        let window = web_sys::window().ok_or_else(|| "No window".to_string())?;
        let navigator = window.navigator();
        let clipboard = navigator.clipboard();
        wasm_bindgen_futures::JsFuture::from(clipboard.write_text(text))
            .await
            .map(|_| ())
            .map_err(|e| format!("{e:?}"))
    }
    #[cfg(feature = "desktop")]
    {
        let text = text.to_string();
        tokio::task::spawn_blocking(move || {
            let mut clipboard = arboard::Clipboard::new().map_err(|e| e.to_string())?;
            clipboard.set_text(&text).map_err(|e| e.to_string())
        })
        .await
        .map_err(|e| e.to_string())?
    }
    #[cfg(feature = "mobile")]
    {
        // On mobile, clipboard is handled via WebView's JavaScript bridge
        // Wrap in async IIFE to properly await the clipboard Promise
        let eval = dioxus::prelude::document::eval(&format!(
            "(async () => {{ await navigator.clipboard.writeText({}); }})()",
            serde_json::json!(text)
        ));
        eval.await.map(|_| ()).map_err(|e| format!("{e:?}"))
    }
    #[cfg(not(any(feature = "web", feature = "desktop", feature = "mobile")))]
    {
        Err("clipboard not supported on this platform".to_string())
    }
}

/// Read text from the system clipboard.
pub async fn read_text_from_clipboard() -> Result<String, String> {
    #[cfg(feature = "web")]
    {
        let window = web_sys::window().ok_or_else(|| "No window".to_string())?;
        let navigator = window.navigator();
        let clipboard = navigator.clipboard();
        let text = wasm_bindgen_futures::JsFuture::from(clipboard.read_text())
            .await
            .map_err(|e| format!("{e:?}"))?;
        text.as_string()
            .ok_or_else(|| "No text in clipboard".to_string())
    }
    #[cfg(feature = "desktop")]
    {
        tokio::task::spawn_blocking(move || {
            let mut clipboard = arboard::Clipboard::new().map_err(|e| e.to_string())?;
            clipboard.get_text().map_err(|e| e.to_string())
        })
        .await
        .map_err(|e| e.to_string())?
    }
    #[cfg(feature = "mobile")]
    {
        let eval =
            dioxus::prelude::document::eval("(async () => await navigator.clipboard.readText())()");
        let value = eval.await.map_err(|e| format!("{e:?}"))?;
        value
            .as_str()
            .map(|s| s.to_string())
            .ok_or_else(|| "No text in clipboard".to_string())
    }
    #[cfg(not(any(feature = "web", feature = "desktop", feature = "mobile")))]
    {
        Err("clipboard not supported on this platform".to_string())
    }
}
