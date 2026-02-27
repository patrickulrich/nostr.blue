/// Save text content to a file. On web, triggers a browser download.
/// On desktop, opens a save dialog. On mobile, uses WebView eval to trigger download.
pub fn save_file(filename: &str, content: &str, _mime_type: &str) -> Result<(), String> {
    #[cfg(feature = "web")]
    {
        crate::utils::download::download_blob(filename, content, _mime_type);
        Ok(())
    }
    #[cfg(feature = "desktop")]
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
        let escaped_content = content.replace('\\', "\\\\").replace('`', "\\`");
        let js = format!(
            r#"(function(){{var b=new Blob([`{escaped_content}`],{{type:'{_mime_type}'}});var u=URL.createObjectURL(b);var a=document.createElement('a');a.href=u;a.download='{filename}';document.body.appendChild(a);a.click();document.body.removeChild(a);URL.revokeObjectURL(u);}})();"#,
        );
        dioxus::prelude::document::eval(&js);
        Ok(())
    }
}
