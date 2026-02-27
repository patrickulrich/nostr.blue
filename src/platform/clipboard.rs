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
            let mut clipboard =
                arboard::Clipboard::new().map_err(|e| e.to_string())?;
            clipboard.set_text(&text).map_err(|e| e.to_string())
        })
        .await
        .map_err(|e| e.to_string())?
    }
    #[cfg(all(feature = "mobile", not(feature = "desktop"), not(feature = "web")))]
    {
        // On mobile, clipboard is handled via WebView's JavaScript bridge
        let eval = dioxus::prelude::document::eval(
            &format!("navigator.clipboard.writeText({})", serde_json::json!(text)),
        );
        eval.await.map(|_| ()).map_err(|e| format!("{e:?}"))
    }
}
