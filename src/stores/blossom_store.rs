use dioxus::prelude::*;
use nostr_blossom::prelude::*;
use nostr_sdk::Url;
use sha2::{Sha256, Digest};
use image::ImageFormat;
use std::io::Cursor;
use crate::stores::nostr_client;

/// Default Blossom server
pub const DEFAULT_SERVER: &str = "https://blossom.primal.net";

/// Global signal for the list of configured Blossom servers
pub static BLOSSOM_SERVERS: GlobalSignal<Vec<String>> = Signal::global(|| vec![DEFAULT_SERVER.to_string()]);

/// Global signal for upload progress (0-100)
pub static UPLOAD_PROGRESS: GlobalSignal<Option<f32>> = Signal::global(|| None);

/// Add a custom Blossom server
pub fn add_server(url: String) {
    let mut servers = BLOSSOM_SERVERS.write();
    if !servers.contains(&url) {
        servers.push(url);
    }
}

/// Remove a Blossom server
pub fn remove_server(url: &str) {
    let mut servers = BLOSSOM_SERVERS.write();
    servers.retain(|s| s != url);

    // Ensure we always have at least one server
    if servers.is_empty() {
        servers.push(DEFAULT_SERVER.to_string());
    }
}

/// Get the first server from the list (primary upload target)
pub fn get_primary_server() -> String {
    BLOSSOM_SERVERS.read().first().cloned().unwrap_or(DEFAULT_SERVER.to_string())
}

/// Upload media (image or video) to Blossom with optional compression
///
/// # Arguments
/// * `data` - Raw media bytes
/// * `content_type` - MIME type (e.g., "image/png", "image/jpeg", "video/mp4")
/// * `quality` - Compression quality (0-100). 100 = original, no compression. Only applies to images.
///
/// # Returns
/// URL of the uploaded media
pub async fn upload_image(
    data: Vec<u8>,
    content_type: String,
    quality: u8,
) -> Result<String, String> {
    let is_video = content_type.starts_with("video/");
    let media_type = if is_video { "video" } else { "image" };

    log::info!("Uploading {}: {} bytes{}", media_type, data.len(),
        if is_video { "" } else { &format!(", quality: {}%", quality) });

    // Reset progress
    UPLOAD_PROGRESS.write().replace(0.0);

    // Get signer for authentication
    let signer = nostr_client::get_signer()
        .ok_or("Not authenticated. Please sign in to upload media.")?;

    // Compress image if quality < 100 and not a video
    let final_data = if !is_video && quality < 100 {
        log::info!("Compressing image to {}% quality", quality);
        UPLOAD_PROGRESS.write().replace(25.0);
        compress_image(data, content_type.clone(), quality).await?
    } else {
        if is_video {
            log::info!("Skipping compression for video file");
            UPLOAD_PROGRESS.write().replace(25.0);
        }
        data
    };

    log::info!("Final {} size: {} bytes", media_type, final_data.len());
    UPLOAD_PROGRESS.write().replace(50.0);

    // Get primary server
    let server_url = get_primary_server();
    let url = Url::parse(&server_url).map_err(|e| format!("Invalid server URL: {}", e))?;

    // Create Blossom client
    let client = BlossomClient::new(url);

    // Upload blob with authentication
    log::info!("Uploading to {} with authentication", server_url);
    UPLOAD_PROGRESS.write().replace(75.0);

    // Create authorization options for the upload
    let auth_options = Some(BlossomAuthorizationOptions {
        content: Some(format!("Upload {} via nostr.blue", media_type)),
        expiration: None, // No expiration
        action: None, // Default action (upload)
        scope: None, // No specific scope restriction
    });

    // Upload with proper authentication based on signer type
    let descriptor = match signer {
        crate::stores::signer::SignerType::Keys(keys) => {
            client
                .upload_blob(final_data, Some(content_type), auth_options, Some(&keys))
                .await
                .map_err(|e| format!("Upload failed: {}", e))?
        }
        #[cfg(target_family = "wasm")]
        crate::stores::signer::SignerType::BrowserExtension(browser_signer) => {
            client
                .upload_blob(final_data, Some(content_type), auth_options, Some(browser_signer.as_ref()))
                .await
                .map_err(|e| format!("Upload failed: {}", e))?
        }
        crate::stores::signer::SignerType::NostrConnect(nostr_connect) => {
            client
                .upload_blob(final_data, Some(content_type), auth_options, Some(nostr_connect.as_ref()))
                .await
                .map_err(|e| format!("Upload failed: {}", e))?
        }
    };

    UPLOAD_PROGRESS.write().replace(100.0);

    log::info!("Upload successful: {}", descriptor.url);

    // Clear progress after a short delay
    spawn(async move {
        gloo_timers::future::TimeoutFuture::new(1000).await;
        *UPLOAD_PROGRESS.write() = None;
    });

    Ok(descriptor.url.to_string())
}

/// Compress an image to the specified quality level
///
/// # Arguments
/// * `data` - Original image bytes
/// * `content_type` - Original MIME type
/// * `quality` - Target quality (0-100)
///
/// # Returns
/// Compressed image bytes
async fn compress_image(
    data: Vec<u8>,
    content_type: String,
    quality: u8,
) -> Result<Vec<u8>, String> {
    // Load image
    let img = image::load_from_memory(&data)
        .map_err(|e| format!("Failed to load image: {}", e))?;

    // Determine output format
    let format = if content_type.contains("png") {
        ImageFormat::Png
    } else {
        ImageFormat::Jpeg
    };

    // Compress based on quality
    let mut compressed_data = Vec::new();
    let mut cursor = Cursor::new(&mut compressed_data);

    match format {
        ImageFormat::Jpeg => {
            // JPEG uses quality parameter directly
            let encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut cursor, quality);
            img.write_with_encoder(encoder)
                .map_err(|e| format!("JPEG encoding failed: {}", e))?;
        }
        ImageFormat::Png => {
            // PNG doesn't have quality, so we'll just re-encode
            // For more compression, we could resize the image
            img.write_to(&mut cursor, format)
                .map_err(|e| format!("PNG encoding failed: {}", e))?;
        }
        _ => {
            return Err("Unsupported image format".to_string());
        }
    }

    Ok(compressed_data)
}

/// Upload audio to Blossom (no compression)
///
/// # Arguments
/// * `data` - Raw audio bytes
/// * `content_type` - MIME type (e.g., "audio/mp4", "audio/webm", "audio/ogg")
///
/// # Returns
/// URL of the uploaded audio
#[allow(dead_code)]
pub async fn upload_audio(
    data: Vec<u8>,
    content_type: String,
) -> Result<String, String> {
    log::info!("Uploading audio: {} bytes, type: {}", data.len(), content_type);

    // Reset progress
    UPLOAD_PROGRESS.write().replace(0.0);

    // Get signer for authentication
    let signer = nostr_client::get_signer()
        .ok_or("Not authenticated. Please sign in to upload audio.")?;

    UPLOAD_PROGRESS.write().replace(25.0);

    // Get primary server
    let server_url = get_primary_server();
    let url = Url::parse(&server_url).map_err(|e| format!("Invalid server URL: {}", e))?;

    // Create Blossom client
    let client = BlossomClient::new(url);

    log::info!("Uploading to {} with authentication", server_url);
    UPLOAD_PROGRESS.write().replace(50.0);

    // Create authorization options for the upload
    let auth_options = Some(BlossomAuthorizationOptions {
        content: Some("Upload voice message via nostr.blue".to_string()),
        expiration: None, // No expiration
        action: None, // Default action (upload)
        scope: None, // No specific scope restriction
    });

    // Upload with proper authentication based on signer type
    let descriptor = match signer {
        crate::stores::signer::SignerType::Keys(keys) => {
            client
                .upload_blob(data, Some(content_type), auth_options, Some(&keys))
                .await
                .map_err(|e| format!("Upload failed: {}", e))?
        }
        #[cfg(target_family = "wasm")]
        crate::stores::signer::SignerType::BrowserExtension(browser_signer) => {
            client
                .upload_blob(data, Some(content_type), auth_options, Some(browser_signer.as_ref()))
                .await
                .map_err(|e| format!("Upload failed: {}", e))?
        }
        crate::stores::signer::SignerType::NostrConnect(nostr_connect) => {
            client
                .upload_blob(data, Some(content_type), auth_options, Some(nostr_connect.as_ref()))
                .await
                .map_err(|e| format!("Upload failed: {}", e))?
        }
    };

    UPLOAD_PROGRESS.write().replace(100.0);

    log::info!("Audio upload successful: {}", descriptor.url);

    // Clear progress after a short delay
    spawn(async move {
        gloo_timers::future::TimeoutFuture::new(1000).await;
        *UPLOAD_PROGRESS.write() = None;
    });

    Ok(descriptor.url.to_string())
}

/// Calculate SHA-256 hash of data
#[allow(dead_code)]
pub fn calculate_sha256(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    let result = hasher.finalize();
    format!("{:x}", result)
}
