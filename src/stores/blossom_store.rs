use dioxus::prelude::*;
use dioxus::signals::ReadableExt;
use nostr_blossom::prelude::*;
use nostr_sdk::{Url, Filter, Kind, PublicKey, FromBech32, Tag, TagKind, Timestamp, EventBuilder};
use sha2::{Sha256, Digest};
use image::ImageFormat;
use std::io::Cursor;
use std::time::Duration;
use std::collections::{HashMap, HashSet};
use serde::{Serialize, Deserialize};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use crate::stores::{nostr_client, auth_store};

#[cfg(target_arch = "wasm32")]
use gloo_storage::{LocalStorage, Storage};

/// Default Blossom server
pub const DEFAULT_SERVER: &str = "https://blossom.primal.net";

/// Kind 10063 - User Blossom Server List (NIP-B7)
pub const KIND_USER_BLOSSOM_SERVERS: u16 = 10063;

/// Local storage key for blossom servers (persists across page reloads)
#[allow(dead_code)] // Used in WASM-only code paths
const BLOSSOM_SERVERS_STORAGE_KEY: &str = "nostr_blue_blossom_servers";

/// HTTP request timeout in milliseconds
#[cfg(target_arch = "wasm32")]
const REQUEST_TIMEOUT_MS: u32 = 10_000; // 10 seconds

/// Save servers to local storage (WASM only)
#[cfg(target_arch = "wasm32")]
fn save_servers_to_storage(servers: &[String]) {
    if let Err(e) = LocalStorage::set(BLOSSOM_SERVERS_STORAGE_KEY, servers.to_vec()) {
        log::warn!("Failed to save blossom servers to local storage: {:?}", e);
    } else {
        log::debug!("Saved {} blossom servers to local storage", servers.len());
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn save_servers_to_storage(_servers: &[String]) {
    // No-op on non-WASM platforms
}

/// Load servers from local storage (WASM only)
#[cfg(target_arch = "wasm32")]
pub fn load_servers_from_storage() -> Option<Vec<String>> {
    match LocalStorage::get::<Vec<String>>(BLOSSOM_SERVERS_STORAGE_KEY) {
        Ok(servers) if !servers.is_empty() => {
            log::info!("Loaded {} blossom servers from local storage", servers.len());
            Some(servers)
        }
        Ok(_) => None,
        Err(e) => {
            log::debug!("No blossom servers in local storage: {:?}", e);
            None
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn load_servers_from_storage() -> Option<Vec<String>> {
    None
}

/// Global signal for the list of configured Blossom servers
/// Store for blossom servers with fine-grained reactivity
#[derive(Clone, Debug, Default, Store)]
pub struct BlossomServersStore {
    pub data: Vec<String>,
}

pub static BLOSSOM_SERVERS: GlobalSignal<Store<BlossomServersStore>> = Signal::global(|| {
    Store::new(BlossomServersStore { data: vec![DEFAULT_SERVER.to_string()] })
});

/// Track if servers have been loaded from Nostr
pub static SERVERS_LOADED: GlobalSignal<bool> = Signal::global(|| false);

/// Global signal for upload progress (0-100)
pub static UPLOAD_PROGRESS: GlobalSignal<Option<f32>> = Signal::global(|| None);

// ============================================================================
// Media Item Types and State
// ============================================================================

/// Represents a media item stored on Blossom servers (BUD-01/BUD-02 compatible)
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MediaItem {
    /// SHA-256 hash of the file (unique identifier)
    pub sha256: String,
    /// MIME type (e.g., "image/png", "video/mp4")
    pub mime_type: String,
    /// Primary URL where file is hosted
    pub url: String,
    /// File size in bytes
    pub size: u64,
    /// Upload timestamp (Unix epoch seconds)
    pub uploaded: u64,
    /// List of mirror URLs where file is also available
    #[serde(default)]
    pub mirrors: Vec<String>,
}

/// Media type filter for tab display
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Hash)]
pub enum MediaFilter {
    #[default]
    All,
    Images,
    Videos,
    Audio,
    Files, // Non-image, non-video, non-audio files
}

impl MediaFilter {
    pub fn matches(&self, mime_type: &str) -> bool {
        match self {
            MediaFilter::All => true,
            MediaFilter::Images => mime_type.starts_with("image/"),
            MediaFilter::Videos => mime_type.starts_with("video/"),
            MediaFilter::Audio => mime_type.starts_with("audio/"),
            MediaFilter::Files => {
                !mime_type.starts_with("image/")
                && !mime_type.starts_with("video/")
                && !mime_type.starts_with("audio/")
            }
        }
    }

    #[allow(dead_code)]
    pub fn label(&self) -> &'static str {
        match self {
            MediaFilter::All => "All",
            MediaFilter::Images => "Images",
            MediaFilter::Videos => "Videos",
            MediaFilter::Audio => "Audio",
            MediaFilter::Files => "Files",
        }
    }
}

/// Blossom page tab selection
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum BlossomTab {
    #[default]
    Images,
    Videos,
    Audio,
    Files,
    Servers,
}

impl BlossomTab {
    pub fn label(&self) -> &'static str {
        match self {
            BlossomTab::Images => "Images",
            BlossomTab::Videos => "Videos",
            BlossomTab::Audio => "Audio",
            BlossomTab::Files => "Files",
            BlossomTab::Servers => "Servers",
        }
    }

    #[allow(dead_code)]
    pub fn to_filter(self) -> Option<MediaFilter> {
        match self {
            BlossomTab::Images => Some(MediaFilter::Images),
            BlossomTab::Videos => Some(MediaFilter::Videos),
            BlossomTab::Audio => Some(MediaFilter::Audio),
            BlossomTab::Files => Some(MediaFilter::Files),
            BlossomTab::Servers => None,
        }
    }
}

/// Global signal for media items fetched from servers
pub static MEDIA_ITEMS: GlobalSignal<Vec<MediaItem>> = Signal::global(Vec::new);

/// Global signal for media loading state
pub static MEDIA_LOADING: GlobalSignal<bool> = Signal::global(|| false);

/// Global signal for media error state
pub static MEDIA_ERROR: GlobalSignal<Option<String>> = Signal::global(|| None);

/// Last time files were fetched (Unix timestamp)
pub static LAST_FETCH_TIME: GlobalSignal<u64> = Signal::global(|| 0);

/// Kind 24242 - Blossom Authorization Event
pub const KIND_BLOSSOM_AUTH: u16 = 24242;

/// Add a custom Blossom server
pub fn add_server(url: String) {
    let store = BLOSSOM_SERVERS.read();
    let mut data = store.data();
    let mut servers = data.write();
    if !servers.contains(&url) {
        servers.push(url);
        save_servers_to_storage(&servers);
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
    save_servers_to_storage(&servers);
}

/// Get the first server from the list (primary upload target)
pub fn get_primary_server() -> String {
    let server = BLOSSOM_SERVERS.read().data().read().first().cloned().unwrap_or(DEFAULT_SERVER.to_string());
    log::debug!("get_primary_server returning: {}", server);
    server
}

/// Get all configured Blossom servers
pub fn get_servers() -> Vec<String> {
    BLOSSOM_SERVERS.read().data().read().clone()
}

/// Set a server as preferred (moves to first position in list)
/// Changes are persisted to local storage immediately, and to Nostr via `publish_user_servers()`
pub fn set_as_preferred(url: &str) {
    log::info!("set_as_preferred called with: {}", url);
    let store = BLOSSOM_SERVERS.read();
    let mut data = store.data();
    let mut servers = data.write();
    log::info!("Current servers before reorder: {:?}", *servers);
    if let Some(pos) = servers.iter().position(|s| s == url) {
        log::info!("Found server at position {}, moving to first", pos);
        let server = servers.remove(pos);
        servers.insert(0, server);
        log::info!("Servers after reorder: {:?}", *servers);
        // Persist to local storage so preference survives page reload
        save_servers_to_storage(&servers);
    } else {
        log::warn!("Server not found in list: {}", url);
        log::warn!("Available servers: {:?}", *servers);
    }
}

/// Upload media (image or video) to Blossom with optional compression
///
/// # Arguments
/// * `data` - Raw media bytes
/// * `content_type` - MIME type (e.g., "image/png", "image/jpeg", "video/mp4")
/// * `quality` - Compression quality (0-100). 100 = original, no compression. Only applies to images.
/// * `server_url` - Optional server URL to upload to (uses primary server if None)
///
/// # Returns
/// URL of the uploaded media
pub async fn upload_image(
    data: Vec<u8>,
    content_type: String,
    quality: u8,
    server_url: Option<String>,
) -> Result<String, String> {
    let is_video = content_type.starts_with("video/");
    let media_type = if is_video { "video" } else { "image" };

    let quality_str = if is_video { String::new() } else { format!(", quality: {}%", quality) };
    log::info!("Uploading {}: {} bytes{}", media_type, data.len(), quality_str);

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
        server_url,
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
/// * `server_url` - Optional server URL to upload to (uses primary server if None)
///
/// # Returns
/// URL of the uploaded blob
async fn upload_blob_with_auth(
    data: Vec<u8>,
    content_type: String,
    auth_content: String,
    start_progress: f32,
    server_url: Option<String>,
) -> Result<String, String> {
    // Get signer for authentication
    let signer = nostr_client::get_signer()
        .ok_or("Not authenticated. Please sign in to upload.")?;

    UPLOAD_PROGRESS.write().replace(start_progress);

    // Use provided server or fall back to primary
    let server_url = server_url.unwrap_or_else(get_primary_server);
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
/// * `server_url` - Optional server URL to upload to (uses primary server if None)
///
/// # Returns
/// URL of the uploaded audio
pub async fn upload_audio(
    data: Vec<u8>,
    content_type: String,
    server_url: Option<String>,
) -> Result<String, String> {
    log::info!("Uploading audio: {} bytes, type: {}", data.len(), content_type);

    // Reset progress
    UPLOAD_PROGRESS.write().replace(0.0);

    upload_blob_with_auth(
        data,
        content_type,
        "Upload voice message via nostr.blue".to_string(),
        25.0,
        server_url,
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
/// Local storage is checked first to preserve user's preferred order
pub async fn fetch_user_servers() -> Result<Vec<String>, String> {
    // Check local storage first - user's local preferences take priority
    if let Some(local_servers) = load_servers_from_storage() {
        log::info!("Using {} blossom servers from local storage (preferred order preserved)", local_servers.len());
        set_servers(local_servers.clone());
        *SERVERS_LOADED.write() = true;
        return Ok(local_servers);
    }

    let client = nostr_client::get_client().ok_or("Client not initialized")?;

    let pubkey_str = auth_store::get_pubkey().ok_or("Not authenticated")?;
    let pubkey = PublicKey::from_bech32(&pubkey_str)
        .or_else(|_| PublicKey::from_hex(&pubkey_str))
        .map_err(|e| format!("Invalid pubkey: {}", e))?;

    log::info!("Fetching user's Blossom servers from Nostr (kind 10063)...");

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

// ============================================================================
// Media File Operations (BUD-02 / BUD-04 / BUD-05)
// ============================================================================

/// Response structure from Blossom server /list endpoint
#[derive(Debug, Deserialize)]
struct BlobDescriptor {
    sha256: String,
    #[serde(rename = "type")]
    mime_type: Option<String>,
    url: String,
    size: u64,
    uploaded: u64,
}

/// Create a signed authorization header for Blossom API calls (kind 24242)
///
/// # Arguments
/// * `action` - The action type: "list", "upload", "delete", "get"
/// * `sha256` - Optional file hash for file-specific operations
/// * `content` - Human-readable reason for the authorization
///
/// # Returns
/// Authorization header value: "Nostr {base64_event}"
pub async fn get_auth_header(
    action: &str,
    sha256: Option<&str>,
    content: &str,
) -> Result<String, String> {
    let signer = nostr_client::get_signer()
        .ok_or("Not authenticated. Please sign in.")?;

    // Build tags for the auth event
    let mut tags: Vec<Tag> = vec![
        Tag::custom(TagKind::Custom("t".into()), vec![action.to_string()]),
        Tag::custom(
            TagKind::Custom("expiration".into()),
            vec![(Timestamp::now().as_secs() + 600).to_string()], // 10 minutes
        ),
    ];

    // Add file hash tag if provided
    if let Some(hash) = sha256 {
        // Support comma-separated hashes for batch operations
        for h in hash.split(',') {
            let h = h.trim();
            if !h.is_empty() {
                // Validate: must be exactly 64 hex characters (BUD-01 spec)
                if h.len() != 64 {
                    return Err(format!("Invalid SHA256 hash length: {} (expected 64)", h.len()));
                }
                if !h.chars().all(|c| c.is_ascii_hexdigit()) {
                    return Err("Invalid SHA256 hash: contains non-hex characters".to_string());
                }
                tags.push(Tag::custom(TagKind::Custom("x".into()), vec![h.to_string()]));
            }
        }
    }

    // Create the event builder
    let builder = EventBuilder::new(Kind::from(KIND_BLOSSOM_AUTH), content)
        .tags(tags);

    // Sign the event based on signer type
    let event = match signer {
        crate::stores::signer::SignerType::Keys(keys) => {
            builder.sign(&keys).await
                .map_err(|e| format!("Failed to sign auth event: {}", e))?
        }
        #[cfg(target_family = "wasm")]
        crate::stores::signer::SignerType::BrowserExtension(browser_signer) => {
            builder.sign(browser_signer.as_ref()).await
                .map_err(|e| format!("Failed to sign auth event: {}", e))?
        }
        crate::stores::signer::SignerType::NostrConnect(nostr_connect) => {
            builder.sign(nostr_connect.as_ref()).await
                .map_err(|e| format!("Failed to sign auth event: {}", e))?
        }
    };

    // Serialize and base64 encode
    let event_json = serde_json::to_string(&event)
        .map_err(|e| format!("Failed to serialize auth event: {}", e))?;
    let base64_event = BASE64.encode(event_json.as_bytes());

    Ok(format!("Nostr {}", base64_event))
}

/// Fetch list of files from all configured Blossom servers
///
/// Merges results from all servers, tracking mirrors for files that exist on multiple servers.
/// Uses BUD-05 /list/{pubkey} endpoint.
pub async fn list_files() -> Result<Vec<MediaItem>, String> {
    *MEDIA_LOADING.write() = true;
    *MEDIA_ERROR.write() = None;

    // Use inner function to ensure MEDIA_LOADING is always reset
    let result = list_files_inner().await;

    // Always reset loading state, even on error
    *MEDIA_LOADING.write() = false;

    if let Err(ref e) = result {
        *MEDIA_ERROR.write() = Some(e.clone());
    }

    result
}

/// Inner implementation of list_files that can fail without leaving loading state stuck
async fn list_files_inner() -> Result<Vec<MediaItem>, String> {
    let pubkey_str = auth_store::get_pubkey()
        .ok_or("Not authenticated")?;

    let pubkey = PublicKey::from_bech32(&pubkey_str)
        .or_else(|_| PublicKey::from_hex(&pubkey_str))
        .map_err(|e| format!("Invalid pubkey: {}", e))?;

    let servers = get_servers();
    if servers.is_empty() {
        return Ok(vec![]);
    }

    // Get auth header for list operation
    let auth_header = get_auth_header("list", None, "List files via nostr.blue").await?;

    // Fetch from all servers sequentially, tracking success
    let mut all_items: HashMap<String, MediaItem> = HashMap::new();
    let mut success_count = 0;
    let mut last_error = String::new();

    for server in &servers {
        let server_url = server.trim_end_matches('/');
        let list_url = format!("{}/list/{}", server_url, pubkey.to_hex());

        log::info!("Fetching files from: {}", list_url);

        match fetch_server_list(&list_url, &auth_header).await {
            Ok(blobs) => {
                success_count += 1;
                log::info!("Found {} files on {}", blobs.len(), server);
                for blob in blobs {
                    if let Some(existing) = all_items.get_mut(&blob.sha256) {
                        // File exists on another server, add as mirror
                        if !existing.mirrors.contains(&blob.url) && existing.url != blob.url {
                            existing.mirrors.push(blob.url);
                        }
                    } else {
                        // New file
                        all_items.insert(blob.sha256.clone(), MediaItem {
                            sha256: blob.sha256,
                            mime_type: blob.mime_type.unwrap_or_else(|| "application/octet-stream".to_string()),
                            url: blob.url,
                            size: blob.size,
                            uploaded: blob.uploaded,
                            mirrors: vec![],
                        });
                    }
                }
            }
            Err(e) => {
                log::warn!("Failed to fetch from {}: {}", server, e);
                last_error = e;
            }
        }
    }

    // If no server succeeded, return error instead of empty success
    if success_count == 0 && !servers.is_empty() {
        return Err(format!(
            "Failed to fetch from all {} server(s). Last error: {}",
            servers.len(),
            last_error
        ));
    }

    // Convert to vec and sort by upload time (newest first)
    let mut items: Vec<MediaItem> = all_items.into_values().collect();
    items.sort_by(|a, b| b.uploaded.cmp(&a.uploaded));

    // Update global state
    *MEDIA_ITEMS.write() = items.clone();
    *LAST_FETCH_TIME.write() = Timestamp::now().as_secs();

    Ok(items)
}

/// Internal helper to fetch file list from a single server
#[allow(unused_variables)]
async fn fetch_server_list(url: &str, auth_header: &str) -> Result<Vec<BlobDescriptor>, String> {
    #[cfg(target_arch = "wasm32")]
    {
        use gloo_net::http::Request;
        use futures::future::{select, Either};
        use gloo_timers::future::TimeoutFuture;

        let request_future = Request::get(url)
            .header("Authorization", auth_header)
            .send();
        let timeout_future = TimeoutFuture::new(REQUEST_TIMEOUT_MS);

        match select(std::pin::pin!(request_future), std::pin::pin!(timeout_future)).await {
            Either::Left((response_result, _)) => {
                let response = response_result.map_err(|e| format!("Request failed: {}", e))?;
                if !response.ok() {
                    return Err(format!("Server returned status: {}", response.status()));
                }
                let blobs: Vec<BlobDescriptor> = response
                    .json()
                    .await
                    .map_err(|e| format!("Failed to parse response: {}", e))?;
                Ok(blobs)
            }
            Either::Right((_, _)) => {
                Err("Request timeout".to_string())
            }
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        // Non-WASM implementation using reqwest or similar
        Err("Not implemented for non-WASM targets".to_string())
    }
}

/// Delete a single file from all configured servers
///
/// Uses BUD-02 DELETE /{sha256} endpoint with kind 24242 auth.
pub async fn delete_file(sha256: &str) -> Result<(), String> {
    let servers = get_servers();
    if servers.is_empty() {
        return Err("No servers configured".to_string());
    }

    // Get auth header for delete operation
    let auth_header = get_auth_header("delete", Some(sha256), "Delete file via nostr.blue").await?;

    let mut success_count = 0;
    let mut last_error = String::new();

    for server in &servers {
        let server_url = server.trim_end_matches('/');
        let delete_url = format!("{}/{}", server_url, sha256);

        log::info!("Deleting {} from {}", sha256, server);

        match delete_from_server(&delete_url, &auth_header).await {
            Ok(_) => {
                log::info!("Successfully deleted from {}", server);
                success_count += 1;
            }
            Err(e) => {
                log::warn!("Failed to delete from {}: {}", server, e);
                last_error = e;
            }
        }
    }

    if success_count > 0 {
        // Remove from local state
        MEDIA_ITEMS.write().retain(|item| item.sha256 != sha256);
        Ok(())
    } else {
        Err(format!("Failed to delete from all servers: {}", last_error))
    }
}

/// Delete multiple files from all configured servers
pub async fn delete_files(sha256_list: &[String]) -> Result<usize, String> {
    let mut deleted_count = 0;

    for sha256 in sha256_list {
        match delete_file(sha256).await {
            Ok(_) => deleted_count += 1,
            Err(e) => log::warn!("Failed to delete {}: {}", sha256, e),
        }
    }

    Ok(deleted_count)
}

/// Internal helper to delete file from a single server
#[allow(unused_variables)]
async fn delete_from_server(url: &str, auth_header: &str) -> Result<(), String> {
    #[cfg(target_arch = "wasm32")]
    {
        use gloo_net::http::Request;
        use futures::future::{select, Either};
        use gloo_timers::future::TimeoutFuture;

        let request_future = Request::delete(url)
            .header("Authorization", auth_header)
            .send();
        let timeout_future = TimeoutFuture::new(REQUEST_TIMEOUT_MS);

        match select(std::pin::pin!(request_future), std::pin::pin!(timeout_future)).await {
            Either::Left((response_result, _)) => {
                let response = response_result.map_err(|e| format!("Request failed: {}", e))?;
                if response.ok() {
                    Ok(())
                } else {
                    Err(format!("Server returned status: {}", response.status()))
                }
            }
            Either::Right((_, _)) => {
                Err("Request timeout".to_string())
            }
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        Err("Not implemented for non-WASM targets".to_string())
    }
}

/// Mirror a file to other servers that don't have it yet
///
/// Uses BUD-04 PUT /mirror endpoint.
///
/// # Arguments
/// * `sha256` - Hash of the file to mirror
/// * `source_url` - URL of the file on its current server
/// * `target_servers` - Optional list of servers to mirror to (uses all configured servers if None)
pub async fn mirror_file(
    sha256: &str,
    source_url: &str,
    target_servers: Option<Vec<String>>,
) -> Result<Vec<String>, String> {
    let servers = target_servers.unwrap_or_else(get_servers);
    if servers.is_empty() {
        return Err("No target servers".to_string());
    }

    // Get auth header for upload (mirroring is treated as upload)
    let auth_header = get_auth_header("upload", Some(sha256), "Mirror file via nostr.blue").await?;

    let mut new_mirrors: Vec<String> = vec![];

    for server in &servers {
        let server_url = server.trim_end_matches('/');

        // Skip if this is the source server
        if source_url.starts_with(server_url) {
            continue;
        }

        let mirror_url = format!("{}/mirror", server_url);

        log::info!("Mirroring {} to {}", sha256, server);

        match mirror_to_server(&mirror_url, source_url, &auth_header).await {
            Ok(new_url) => {
                log::info!("Successfully mirrored to {}: {}", server, new_url);
                new_mirrors.push(new_url.clone());

                // Update local state
                let mut items = MEDIA_ITEMS.write();
                if let Some(item) = items.iter_mut().find(|i| i.sha256 == sha256) {
                    if !item.mirrors.contains(&new_url) {
                        item.mirrors.push(new_url);
                    }
                }
            }
            Err(e) => {
                log::warn!("Failed to mirror to {}: {}", server, e);
            }
        }
    }

    if new_mirrors.is_empty() {
        Err("Failed to mirror to any server".to_string())
    } else {
        Ok(new_mirrors)
    }
}

/// Response from mirror endpoint
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct MirrorResponse {
    url: String,
    #[allow(dead_code)]
    sha256: Option<String>,
}

/// Internal helper to mirror file to a single server
#[allow(unused_variables)]
async fn mirror_to_server(mirror_url: &str, source_url: &str, auth_header: &str) -> Result<String, String> {
    #[cfg(target_arch = "wasm32")]
    {
        use gloo_net::http::Request;
        use futures::future::{select, Either};
        use gloo_timers::future::TimeoutFuture;

        let body = serde_json::json!({ "url": source_url });

        let request_future = Request::put(mirror_url)
            .header("Authorization", auth_header)
            .header("Content-Type", "application/json")
            .body(body.to_string())
            .map_err(|e| format!("Failed to build request: {}", e))?
            .send();
        let timeout_future = TimeoutFuture::new(REQUEST_TIMEOUT_MS);

        match select(std::pin::pin!(request_future), std::pin::pin!(timeout_future)).await {
            Either::Left((response_result, _)) => {
                let response = response_result.map_err(|e| format!("Request failed: {}", e))?;
                if !response.ok() {
                    return Err(format!("Server returned status: {}", response.status()));
                }
                let mirror_response: MirrorResponse = response
                    .json()
                    .await
                    .map_err(|e| format!("Failed to parse response: {}", e))?;
                Ok(mirror_response.url)
            }
            Either::Right((_, _)) => {
                Err("Request timeout".to_string())
            }
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        Err("Not implemented for non-WASM targets".to_string())
    }
}

/// Mirror multiple files to all servers
pub async fn mirror_files(items: &[MediaItem]) -> Result<usize, String> {
    let mut mirrored_count = 0;

    for item in items {
        match mirror_file(&item.sha256, &item.url, None).await {
            Ok(mirrors) => {
                if !mirrors.is_empty() {
                    mirrored_count += 1;
                }
            }
            Err(e) => log::warn!("Failed to mirror {}: {}", item.sha256, e),
        }
    }

    Ok(mirrored_count)
}

/// Check if a media item is fully mirrored (exists on all configured servers)
pub fn is_fully_mirrored(item: &MediaItem) -> bool {
    let servers = get_servers();
    if servers.len() <= 1 {
        return true; // Only one server, already "fully mirrored"
    }

    // Count unique servers where file exists
    let mut server_domains: HashSet<String> = HashSet::new();

    // Add primary URL's domain
    if let Ok(url) = url::Url::parse(&item.url) {
        if let Some(host) = url.host_str() {
            server_domains.insert(host.to_string());
        }
    }

    // Add mirror URL domains
    for mirror in &item.mirrors {
        if let Ok(url) = url::Url::parse(mirror) {
            if let Some(host) = url.host_str() {
                server_domains.insert(host.to_string());
            }
        }
    }

    // Check if file exists on all configured server domains
    let configured_domains: HashSet<String> = servers
        .iter()
        .filter_map(|s| url::Url::parse(s).ok())
        .filter_map(|u| u.host_str().map(|h| h.to_string()))
        .collect();

    configured_domains.is_subset(&server_domains)
}

/// Get count of media items by filter type
#[allow(dead_code)]
pub fn get_media_counts() -> HashMap<MediaFilter, usize> {
    let items = MEDIA_ITEMS.read();
    let mut counts = HashMap::new();

    counts.insert(MediaFilter::All, items.len());
    counts.insert(
        MediaFilter::Images,
        items.iter().filter(|i| MediaFilter::Images.matches(&i.mime_type)).count(),
    );
    counts.insert(
        MediaFilter::Videos,
        items.iter().filter(|i| MediaFilter::Videos.matches(&i.mime_type)).count(),
    );
    counts.insert(
        MediaFilter::Audio,
        items.iter().filter(|i| MediaFilter::Audio.matches(&i.mime_type)).count(),
    );
    counts.insert(
        MediaFilter::Files,
        items.iter().filter(|i| MediaFilter::Files.matches(&i.mime_type)).count(),
    );

    counts
}

/// Get total storage used across all media items
pub fn get_total_storage() -> u64 {
    MEDIA_ITEMS.read().iter().map(|i| i.size).sum()
}

/// Format bytes as human-readable string
pub fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.2} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}
