//! NIP-96 HTTP File Storage Integration
//!
//! Implements file upload to NIP-96 compatible servers (like nostr.build)
//! with NIP-98 HTTP Authentication.
//!
//! ## Overview
//! - Upload files using NIP-96 protocol
//! - Authenticate requests using NIP-98 (HTTP Auth)
//! - Parse response for NIP-94 file metadata
//!
//! ## References
//! - NIP-96: https://github.com/nostr-protocol/nips/blob/master/96.md
//! - NIP-98: https://github.com/nostr-protocol/nips/blob/master/98.md

use dioxus::prelude::*;
use serde::Deserialize;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::{FormData, Request, RequestInit, Response};

use crate::stores::nostr_client;

// Disambiguate Result type
type Result<T, E> = std::result::Result<T, E>;

/// Default NIP-96 server (nostr.build)
#[allow(dead_code)]
pub const DEFAULT_NIP96_SERVER: &str = "https://nostr.build";
pub const NOSTR_BUILD_API_URL: &str = "https://nostr.build/api/v2/nip96/upload";

/// Global signal for NIP-96 upload progress (0-100)
pub static NIP96_UPLOAD_PROGRESS: GlobalSignal<Option<f32>> = Signal::global(|| None);

/// Current upload ID to prevent timer race conditions
/// Each upload gets a unique ID; timer only clears progress if ID matches
pub static CURRENT_UPLOAD_ID: GlobalSignal<Option<uuid::Uuid>> = Signal::global(|| None);

/// NIP-96 server configuration (from /.well-known/nostr/nip96.json)
#[allow(dead_code)]
#[derive(Clone, Debug, Deserialize)]
pub struct Nip96ServerConfig {
    pub api_url: String,
    #[serde(default)]
    pub download_url: Option<String>,
    #[serde(default)]
    pub supported_nips: Vec<u32>,
    #[serde(default)]
    pub tos_url: Option<String>,
    #[serde(default)]
    pub content_types: Vec<String>,
}

/// NIP-96 upload response
#[derive(Clone, Debug, Deserialize)]
pub struct Nip96UploadResponse {
    pub status: String,
    #[serde(default)]
    pub message: Option<String>,
    #[serde(default)]
    #[allow(dead_code)] // Part of NIP-96 spec, may be used for async uploads
    pub processing_url: Option<String>,
    #[serde(default)]
    pub nip94_event: Option<Nip94EventData>,
}

/// NIP-94 event data from upload response
#[derive(Clone, Debug, Deserialize)]
pub struct Nip94EventData {
    pub tags: Vec<Vec<String>>,
    #[serde(default)]
    #[allow(dead_code)] // Part of NIP-94 spec
    pub content: String,
}

/// Extracted file metadata from NIP-94 tags
#[derive(Clone, Debug, Default)]
pub struct UploadedFileMetadata {
    pub url: String,
    pub original_hash: Option<String>,  // ox tag
    pub transformed_hash: Option<String>,  // x tag
    pub mime_type: Option<String>,  // m tag
    pub size: Option<usize>,  // size tag
    pub dimensions: Option<(u32, u32)>,  // dim tag (width x height)
    pub blurhash: Option<String>,  // blurhash tag
    pub thumbnail: Option<String>,  // thumb tag
}

impl UploadedFileMetadata {
    /// Parse metadata from NIP-94 event tags
    pub fn from_tags(tags: &[Vec<String>]) -> Option<Self> {
        let mut metadata = UploadedFileMetadata::default();

        for tag in tags {
            if tag.len() < 2 {
                continue;
            }

            match tag[0].as_str() {
                "url" => metadata.url = tag[1].clone(),
                "ox" => metadata.original_hash = Some(tag[1].clone()),
                "x" => metadata.transformed_hash = Some(tag[1].clone()),
                "m" => metadata.mime_type = Some(tag[1].clone()),
                "size" => metadata.size = tag[1].parse().ok(),
                "dim" => {
                    if let Some((w, h)) = tag[1].split_once('x') {
                        if let (Ok(width), Ok(height)) = (w.parse(), h.parse()) {
                            metadata.dimensions = Some((width, height));
                        }
                    }
                }
                "blurhash" => metadata.blurhash = Some(tag[1].clone()),
                "thumb" => metadata.thumbnail = Some(tag[1].clone()),
                _ => {}
            }
        }

        if metadata.url.is_empty() {
            return None;
        }

        Some(metadata)
    }
}

/// Upload a file to nostr.build using NIP-96 with NIP-98 authentication
///
/// # Arguments
/// * `file_data` - Raw file bytes
/// * `mime_type` - MIME type (e.g., "image/gif")
/// * `caption` - Description/caption for the file
/// * `alt` - Alt text for accessibility
///
/// # Returns
/// * `Ok(UploadedFileMetadata)` - Metadata from successful upload
/// * `Err(String)` - Error message if upload fails
pub async fn upload_to_nip96(
    file_data: Vec<u8>,
    mime_type: String,
    caption: String,
    alt: String,
) -> Result<UploadedFileMetadata, String> {
    log::info!("Starting NIP-96 upload: {} bytes, type: {}", file_data.len(), mime_type);

    // Reset progress
    *NIP96_UPLOAD_PROGRESS.write() = Some(0.0);

    // Get signer for NIP-98 authentication
    let signer = match nostr_client::get_signer() {
        Some(s) => s,
        None => {
            *NIP96_UPLOAD_PROGRESS.write() = None;
            return Err("Not authenticated. Please sign in to upload files.".to_string());
        }
    };

    *NIP96_UPLOAD_PROGRESS.write() = Some(10.0);

    // Calculate SHA-256 hash of file
    use sha2::{Sha256, Digest};
    let mut hasher = Sha256::new();
    hasher.update(&file_data);
    let file_hash = hasher.finalize();
    let file_hash_hex = hex::encode(file_hash);

    log::info!("File hash: {}", file_hash_hex);
    *NIP96_UPLOAD_PROGRESS.write() = Some(20.0);

    // Create NIP-98 authorization header
    let authorization = match create_nip98_auth(&signer, NOSTR_BUILD_API_URL, &file_hash_hex).await {
        Ok(auth) => auth,
        Err(e) => {
            *NIP96_UPLOAD_PROGRESS.write() = None;
            return Err(e);
        }
    };

    log::info!("NIP-98 auth created");
    *NIP96_UPLOAD_PROGRESS.write() = Some(30.0);

    // Upload using web_sys fetch API with FormData
    let metadata = match upload_with_fetch(
        file_data,
        mime_type,
        caption,
        alt,
        authorization,
    ).await {
        Ok(m) => m,
        Err(e) => {
            *NIP96_UPLOAD_PROGRESS.write() = None;
            return Err(e);
        }
    };

    *NIP96_UPLOAD_PROGRESS.write() = Some(100.0);

    // Clear progress after a short delay
    // Use spawn_forever so timer survives component unmount, and track upload ID
    // to prevent race condition where new upload's progress gets cleared by old timer
    let upload_id = uuid::Uuid::new_v4();
    *CURRENT_UPLOAD_ID.write() = Some(upload_id);
    dioxus_core::spawn_forever(async move {
        gloo_timers::future::TimeoutFuture::new(1000).await;
        // Only clear if this is still the current upload
        if *CURRENT_UPLOAD_ID.read() == Some(upload_id) {
            *NIP96_UPLOAD_PROGRESS.write() = None;
            *CURRENT_UPLOAD_ID.write() = None;
        }
    });

    log::info!("NIP-96 upload successful: {}", metadata.url);
    Ok(metadata)
}

/// Create NIP-98 authorization header
async fn create_nip98_auth(
    signer: &crate::stores::signer::SignerType,
    api_url: &str,
    _file_hash: &str,
) -> Result<String, String> {
    use nostr_sdk::prelude::*;

    let url = Url::parse(api_url).map_err(|e| format!("Invalid URL: {}", e))?;

    // Create HTTP data for NIP-98
    // Note: payload hash is optional per NIP-98 spec
    let http_data = nip98::HttpData::new(url, nip98::HttpMethod::POST);

    // Generate authorization header based on signer type
    let authorization = match signer {
        crate::stores::signer::SignerType::Keys(keys) => {
            http_data.to_authorization(keys).await
                .map_err(|e| format!("Failed to create NIP-98 auth: {}", e))?
        }
        #[cfg(target_family = "wasm")]
        crate::stores::signer::SignerType::BrowserExtension(browser_signer) => {
            http_data.to_authorization(browser_signer.as_ref()).await
                .map_err(|e| format!("Failed to create NIP-98 auth: {}", e))?
        }
        crate::stores::signer::SignerType::NostrConnect(nostr_connect) => {
            http_data.to_authorization(nostr_connect.as_ref()).await
                .map_err(|e| format!("Failed to create NIP-98 auth: {}", e))?
        }
    };

    Ok(authorization)
}

/// Upload file using web_sys Fetch API with FormData (for WASM compatibility)
async fn upload_with_fetch(
    file_data: Vec<u8>,
    mime_type: String,
    caption: String,
    alt: String,
    authorization: String,
) -> Result<UploadedFileMetadata, String> {
    let window = web_sys::window().ok_or("No window object")?;

    // Create FormData
    let form_data = FormData::new().map_err(|e| format!("Failed to create FormData: {:?}", e))?;

    // Create Blob from file data
    let uint8_array = js_sys::Uint8Array::from(file_data.as_slice());
    let blob_parts = js_sys::Array::new();
    blob_parts.push(&uint8_array);

    let blob_options = web_sys::BlobPropertyBag::new();
    blob_options.set_type(&mime_type);

    let blob = web_sys::Blob::new_with_u8_array_sequence_and_options(&blob_parts, &blob_options)
        .map_err(|e| format!("Failed to create Blob: {:?}", e))?;

    // Append file to FormData
    form_data.append_with_blob_and_filename("file", &blob, "upload.gif")
        .map_err(|e| format!("Failed to append file: {:?}", e))?;

    // Append other form fields
    form_data.append_with_str("caption", &caption)
        .map_err(|e| format!("Failed to append caption: {:?}", e))?;
    form_data.append_with_str("alt", &alt)
        .map_err(|e| format!("Failed to append alt: {:?}", e))?;
    form_data.append_with_str("content_type", &mime_type)
        .map_err(|e| format!("Failed to append content_type: {:?}", e))?;
    form_data.append_with_str("no_transform", "true")
        .map_err(|e| format!("Failed to append no_transform: {:?}", e))?;

    *NIP96_UPLOAD_PROGRESS.write() = Some(50.0);

    // Create request options
    let opts = RequestInit::new();
    opts.set_method("POST");
    opts.set_body(&form_data);

    // Create request with Authorization header
    let request = Request::new_with_str_and_init(NOSTR_BUILD_API_URL, &opts)
        .map_err(|e| format!("Failed to create request: {:?}", e))?;

    request.headers().set("Authorization", &authorization)
        .map_err(|e| format!("Failed to set Authorization header: {:?}", e))?;

    *NIP96_UPLOAD_PROGRESS.write() = Some(60.0);

    // Execute fetch request
    let resp_value = JsFuture::from(window.fetch_with_request(&request))
        .await
        .map_err(|e| format!("Fetch failed: {:?}", e))?;

    let response: Response = resp_value.dyn_into()
        .map_err(|_| "Response is not a Response object")?;

    *NIP96_UPLOAD_PROGRESS.write() = Some(80.0);

    // Check response status
    if !response.ok() {
        let status = response.status();
        let status_text = response.status_text();
        // Try to get response body for better error diagnostics
        if let Ok(text_promise) = response.text() {
            if let Ok(body_js) = JsFuture::from(text_promise).await {
                if let Some(body) = body_js.as_string() {
                    log::error!("Upload failed: {} {} - body: {}", status, status_text, body);
                    return Err(format!("Upload failed: {} {} - {}", status, status_text, body));
                }
            }
        }
        return Err(format!("Upload failed: {} {}", status, status_text));
    }

    // Parse JSON response
    let json_value = JsFuture::from(response.json().map_err(|e| format!("Failed to get JSON: {:?}", e))?)
        .await
        .map_err(|e| format!("Failed to parse JSON: {:?}", e))?;

    let upload_response: Nip96UploadResponse = serde_wasm_bindgen::from_value(json_value)
        .map_err(|e| format!("Failed to deserialize response: {}", e))?;

    *NIP96_UPLOAD_PROGRESS.write() = Some(90.0);

    // Check response status
    if upload_response.status != "success" {
        let msg = upload_response.message.unwrap_or_else(|| "Unknown error".to_string());
        return Err(format!("Upload failed: {}", msg));
    }

    // Extract metadata from nip94_event
    let nip94_event = upload_response.nip94_event
        .ok_or("No nip94_event in response")?;

    let metadata = UploadedFileMetadata::from_tags(&nip94_event.tags)
        .ok_or("Failed to parse file metadata from response")?;

    Ok(metadata)
}

/// Get the list of available upload servers
#[allow(dead_code)]
pub fn get_upload_servers() -> Vec<(String, String)> {
    let mut servers = vec![
        ("nostr.build".to_string(), NOSTR_BUILD_API_URL.to_string()),
    ];

    // Add user's Blossom server if configured
    let blossom_server = crate::stores::blossom_store::get_primary_server();
    if blossom_server != crate::stores::blossom_store::DEFAULT_SERVER {
        servers.push(("Custom Blossom".to_string(), blossom_server));
    } else {
        servers.push(("Blossom (Primal)".to_string(), blossom_server));
    }

    servers
}
