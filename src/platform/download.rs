/// Save text content to a file. On web, triggers a browser download.
/// On desktop, opens a save dialog. On mobile, uses WebView eval to trigger download.
pub fn save_file(filename: &str, content: &str, _mime_type: &str) -> Result<(), String> {
    #[cfg(all(feature = "web", not(feature = "desktop")))]
    {
        crate::utils::download::download_blob(filename, content, _mime_type);
        Ok(())
    }
    #[cfg(all(feature = "desktop", not(feature = "web")))]
    {
        let path = rfd::FileDialog::new()
            .set_file_name(filename)
            .save_file()
            .ok_or_else(|| "Save cancelled".to_string())?;
        std::fs::write(&path, content).map_err(|e| e.to_string())
    }
    #[cfg(all(feature = "mobile", not(feature = "desktop"), not(feature = "web")))]
    {
        // On mobile WebView, trigger a download via JavaScript blob URL
        let content_json = serde_json::to_string(content).unwrap_or_default();
        let filename_json = serde_json::to_string(filename).unwrap_or_default();
        let mime_json = serde_json::to_string(_mime_type).unwrap_or_default();
        let js = format!(
            r#"(function(){{var b=new Blob([{content_json}],{{type:{mime_json}}});var u=URL.createObjectURL(b);try{{var a=document.createElement('a');a.href=u;a.download={filename_json};document.body.appendChild(a);a.click();document.body.removeChild(a);}}finally{{URL.revokeObjectURL(u);}}}})();"#,
        );
        dioxus::prelude::document::eval(&js);
        Ok(())
    }
}
