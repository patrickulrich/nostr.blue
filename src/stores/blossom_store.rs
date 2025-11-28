use dioxus::prelude::*;
use dioxus::signals::ReadableExt;
use nostr_blossom::prelude::*;
use nostr_sdk::{Url, Filter, Kind, PublicKey, FromBech32, Tag, TagKind};
use sha2::{Sha256, Digest};
use image::ImageFormat;
use std::io::Cursor;
use std::time::Duration;
use crate::stores::{nostr_client, auth_store};

/// Default Blossom server
pub const DEFAULT_SERVER: &str = "https://blossom.primal.net";

/// Kind 10063 - User Blossom Server List (NIP-B7)
pub const KIND_USER_BLOSSOM_SERVERS: u16 = 10063;

/// Global signal for the list of configured Blossom servers
/// Store for blossom servers with fine-grained reactivity
#[derive(Clone, Debug, Default, Store)]
pub struct BlossomServersStore {
    pub data: Vec<String>,
}

pub static BLOSSOM_SERVERS: GlobalSignal<Store<BlossomServersStore>> = Signal::global(|| { let mut store = BlossomServersStore::default(); store.data = vec![DEFAULT_SERVER.to_string()]; Store::new(store) });

/// Track if servers have been loaded from Nostr
pub static SERVERS_LOADED: GlobalSignal<bool> = Signal::global(|| false);

/// Global signal for upload progress (0-100)
pub static UPLOAD_PROGRESS: GlobalSignal<Option<f32>> = Signal::global(|| None);

/// Add a custom Blossom server
pub fn add_server(url: String) {
    let store = BLOSSOM_SERVERS.read();
    let mut data = store.data();
    let mut servers = data.write();
    if !servers.contains(&url) {
        servers.push(url);
    }
}

/// Remove a Blossom server
pub fn remove_server(url: &str) {
    let store = BLOSSOM_SERVERS.read();
    let mut data = store.data();
    let mut servers = data.write();
    servers.retain(|s| s != url);

    // Ensure we always have at least one server
    if servers.is_empty() {
        servers.push(DEFAULT_SERVER.to_string());
    }
}

/// Get the first server from the list (primary upload target)
pub fn get_primary_server() -> String {
    BLOSSOM_SERVERS.read().data().read().first().cloned().unwrap_or(DEFAULT_SERVER.to_string())
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

    // Check authentication early (before compression)
    if nostr_client::get_signer().is_none() {
        return Err("Not authenticated. Please sign in to upload media.".to_string());
    }

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

    upload_blob_with_auth(
        final_data,
        content_type,
        format!("Upload {} via nostr.blue", media_type),
        50.0,
    ).await
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

/// Internal helper to upload a blob with authentication
///
/// # Arguments
/// * `data` - Raw blob bytes
/// * `content_type` - MIME type
/// * `auth_content` - Authorization message content
/// * `start_progress` - Progress value to set at upload start (after any pre-processing)
///
/// # Returns
/// URL of the uploaded blob
async fn upload_blob_with_auth(
    data: Vec<u8>,
    content_type: String,
    auth_content: String,
    start_progress: f32,
) -> Result<String, String> {
    // Get signer for authentication
    let signer = nostr_client::get_signer()
        .ok_or("Not authenticated. Please sign in to upload.")?;

    UPLOAD_PROGRESS.write().replace(start_progress);

    // Get primary server
    let server_url = get_primary_server();
    let url = Url::parse(&server_url).map_err(|e| format!("Invalid server URL: {}", e))?;

    // Create Blossom client
    let client = BlossomClient::new(url);

    log::info!("Uploading to {} with authentication", server_url);
    UPLOAD_PROGRESS.write().replace(start_progress + 25.0);

    // Create authorization options for the upload
    let auth_options = Some(BlossomAuthorizationOptions {
        content: Some(auth_content),
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
                .map_err(|e| {
                    UPLOAD_PROGRESS.write().replace(0.0);
                    format!("Upload failed: {}", e)
                })?
        }
        #[cfg(target_family = "wasm")]
        crate::stores::signer::SignerType::BrowserExtension(browser_signer) => {
            client
                .upload_blob(data, Some(content_type), auth_options, Some(browser_signer.as_ref()))
                .await
                .map_err(|e| {
                    UPLOAD_PROGRESS.write().replace(0.0);
                    format!("Upload failed: {}", e)
                })?
        }
        crate::stores::signer::SignerType::NostrConnect(nostr_connect) => {
            client
                .upload_blob(data, Some(content_type), auth_options, Some(nostr_connect.as_ref()))
                .await
                .map_err(|e| {
                    UPLOAD_PROGRESS.write().replace(0.0);
                    format!("Upload failed: {}", e)
                })?
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

/// Upload audio to Blossom (no compression)
///
/// # Arguments
/// * `data` - Raw audio bytes
/// * `content_type` - MIME type (e.g., "audio/mp4", "audio/webm", "audio/ogg")
///
/// # Returns
/// URL of the uploaded audio
pub async fn upload_audio(
    data: Vec<u8>,
    content_type: String,
) -> Result<String, String> {
    log::info!("Uploading audio: {} bytes, type: {}", data.len(), content_type);

    // Reset progress
    UPLOAD_PROGRESS.write().replace(0.0);

    upload_blob_with_auth(
        data,
        content_type,
        "Upload voice message via nostr.blue".to_string(),
        25.0,
    ).await
}

/// Calculate SHA-256 hash of data
#[allow(dead_code)]
pub fn calculate_sha256(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    let result = hasher.finalize();
    format!("{:x}", result)
}

/// Fetch user's Blossom servers from kind 10063 (NIP-B7)
/// Should be called on authentication to load user's preferred servers
pub async fn fetch_user_servers() -> Result<Vec<String>, String> {
    let client = nostr_client::get_client().ok_or("Client not initialized")?;

    let pubkey_str = auth_store::get_pubkey().ok_or("Not authenticated")?;
    let pubkey = PublicKey::from_bech32(&pubkey_str)
        .or_else(|_| PublicKey::from_hex(&pubkey_str))
        .map_err(|e| format!("Invalid pubkey: {}", e))?;

    log::info!("Fetching user's Blossom servers (kind 10063)...");

    // Build filter for kind 10063
    let filter = Filter::new()
        .author(pubkey)
        .kind(Kind::from(KIND_USER_BLOSSOM_SERVERS))
        .limit(1);

    // Try database first
    if let Ok(db_events) = client.database().query(filter.clone()).await {
        if let Some(event) = db_events.into_iter().next() {
            let servers = parse_server_tags(&event.tags);
            if !servers.is_empty() {
                log::info!("Found {} Blossom servers in DB", servers.len());
                set_servers(servers.clone());
                *SERVERS_LOADED.write() = true;
                return Ok(servers);
            }
        }
    }

    // Fetch from relays
    nostr_client::ensure_relays_ready(&client).await;

    match client.fetch_events(filter, Duration::from_secs(5)).await {
        Ok(events) => {
            if let Some(event) = events.into_iter().next() {
                let servers = parse_server_tags(&event.tags);
                if !servers.is_empty() {
                    log::info!("Found {} Blossom servers from relay", servers.len());
                    set_servers(servers.clone());
                    *SERVERS_LOADED.write() = true;
                    return Ok(servers);
                }
            }
            log::info!("No Blossom servers found, using defaults");
            *SERVERS_LOADED.write() = true;
            Ok(vec![DEFAULT_SERVER.to_string()])
        }
        Err(e) => {
            log::warn!("Failed to fetch Blossom servers: {}", e);
            // Do NOT set SERVERS_LOADED = true on error - allow retry
            Err(format!("Failed to fetch servers: {}", e))
        }
    }
}

/// Parse server tags from a kind 10063 event with URL validation
fn parse_server_tags(tags: &nostr_sdk::Tags) -> Vec<String> {
    tags.iter()
        .filter_map(|tag| {
            // Look for ["server", "url"] tags
            if tag.kind() == TagKind::Custom("server".into()) {
                tag.content().and_then(|s| {
                    // Validate URL - Blossom servers should be HTTP(S)
                    match url::Url::parse(s) {
                        Ok(url) if url.scheme() == "https" || url.scheme() == "http" => {
                            Some(s.to_string())
                        }
                        Ok(url) => {
                            log::warn!("Invalid Blossom server scheme: {} (expected http/https)", url.scheme());
                            None
                        }
                        Err(e) => {
                            log::warn!("Invalid Blossom server URL '{}': {}", s, e);
                            None
                        }
                    }
                })
            } else {
                None
            }
        })
        .collect()
}

/// Set all servers (replaces existing list)
pub fn set_servers(servers: Vec<String>) {
    let store = BLOSSOM_SERVERS.read();
    let mut data = store.data();
    let mut current = data.write();
    current.clear();
    if servers.is_empty() {
        current.push(DEFAULT_SERVER.to_string());
    } else {
        current.extend(servers);
    }
}

/// Publish user's Blossom servers as kind 10063 (NIP-B7)
pub async fn publish_user_servers() -> Result<String, String> {
    let client = nostr_client::get_client().ok_or("Client not initialized")?;
    let signer = nostr_client::get_signer().ok_or("No signer available")?;

    let servers = BLOSSOM_SERVERS.read().data().read().clone();

    log::info!("Publishing {} Blossom servers to kind 10063", servers.len());

    // Build tags: ["server", "url"] for each server
    let tags: Vec<Tag> = servers.iter()
        .map(|url| Tag::custom(TagKind::Custom("server".into()), vec![url.clone()]))
        .collect();

    // Create kind 10063 event
    let builder = nostr_sdk::EventBuilder::new(Kind::from(KIND_USER_BLOSSOM_SERVERS), "")
        .tags(tags);

    // Sign and publish
    let event = match signer {
        crate::stores::signer::SignerType::Keys(keys) => {
            builder.sign(&keys).await
                .map_err(|e| format!("Failed to sign event: {}", e))?
        }
        #[cfg(target_family = "wasm")]
        crate::stores::signer::SignerType::BrowserExtension(browser_signer) => {
            builder.sign(browser_signer.as_ref()).await
                .map_err(|e| format!("Failed to sign event: {}", e))?
        }
        crate::stores::signer::SignerType::NostrConnect(nostr_connect) => {
            builder.sign(nostr_connect.as_ref()).await
                .map_err(|e| format!("Failed to sign event: {}", e))?
        }
    };

    nostr_client::ensure_relays_ready(&client).await;

    // Verify at least one relay is connected before publishing
    use nostr_relay_pool::RelayStatus as PoolRelayStatus;
    let relays = client.relays().await;
    let connected_count = relays.values()
        .filter(|r| r.status() == PoolRelayStatus::Connected)
        .count();
    if connected_count == 0 {
        return Err("No relays connected. Cannot publish server list.".to_string());
    }

    match client.send_event(&event).await {
        Ok(output) => {
            log::info!("Published Blossom server list: {}", output.id());
            Ok(output.id().to_string())
        }
        Err(e) => {
            log::error!("Failed to publish Blossom server list: {}", e);
            Err(format!("Failed to publish: {}", e))
        }
    }
}
