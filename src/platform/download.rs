/// Save text content to a file. On web, triggers a browser download.
/// On desktop, opens a save dialog. On mobile (Android), triggers Share Intent.

#[cfg(all(feature = "web", feature = "desktop"))]
compile_error!("Cannot enable both 'web' and 'desktop' features simultaneously");

#[cfg(all(feature = "web", feature = "mobile"))]
compile_error!("Cannot enable both 'web' and 'mobile' features simultaneously");

#[cfg(all(feature = "desktop", feature = "mobile"))]
compile_error!("Cannot enable both 'desktop' and 'mobile' features simultaneously");

#[cfg(all(
    not(feature = "web"),
    not(feature = "desktop"),
    not(feature = "mobile")
))]
compile_error!("Must enable exactly one of 'web', 'desktop', or 'mobile' feature");

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
    #[cfg(feature = "mobile")]
    {
        // On mobile (Android), trigger a Share Intent to let user save/share the file
        crate::platform::download_file(filename, content.as_bytes(), _mime_type)
            .map_err(|e| format!("Download failed: {}", e))
    }
}
