use dioxus::prelude::*;
use dioxus::signals::ReadableExt;
use dioxus_stores::Store;
use nostr_sdk::{Filter, Kind, Timestamp, SingleLetterTag, Alphabet};
use std::str::FromStr;
use std::time::Duration;

/// GIF metadata from Nostr (NIP-94 format)
#[derive(Clone, Debug, PartialEq)]
pub struct GifMetadata {
    pub url: String,
    pub thumbnail: Option<String>,
    pub dimensions: Option<(u64, u64)>,
    pub size: Option<usize>,
    pub blurhash: Option<String>,
    pub alt: Option<String>,
    pub summary: Option<String>,
    pub created_at: Timestamp,
}

/// Store for GIF search results with fine-grained reactivity
#[derive(Clone, Debug, Default, Store)]
pub struct GifResultsStore {
    pub data: Vec<GifMetadata>,
}

/// Store for recent GIFs with fine-grained reactivity
#[derive(Clone, Debug, Default, Store)]
pub struct RecentGifsStore {
    pub data: Vec<GifMetadata>,
}

/// Global state for GIF search results
pub static GIF_RESULTS: GlobalSignal<Store<GifResultsStore>> =
    Signal::global(|| Store::new(GifResultsStore::default()));
pub static GIF_LOADING: GlobalSignal<bool> = Signal::global(|| false);
pub static GIF_OLDEST_TIMESTAMP: GlobalSignal<Option<Timestamp>> = Signal::global(|| None);
pub static RECENT_GIFS: GlobalSignal<Store<RecentGifsStore>> =
    Signal::global(|| Store::new(RecentGifsStore::default()));
pub static CURRENT_SEARCH_QUERY: GlobalSignal<String> = Signal::global(String::new);
pub static GIF_SEARCH_SEQ: GlobalSignal<u64> = Signal::global(|| 0);

const MAX_RECENT_GIFS: usize = 20;

/// Fetch GIFs from Nostr using NIP-94 (Kind 1063)
pub async fn fetch_gifs(limit: usize, until: Option<Timestamp>, search_query: Option<String>) -> Result<Vec<GifMetadata>, String> {
    log::info!("Fetching GIFs from Nostr (limit: {}, until: {:?}, search: {:?})", limit, until, search_query);

    let client = match crate::stores::nostr_client::get_client() {
        Some(c) => c,
        None => {
            log::warn!("Client not initialized, skipping GIF fetch");
            return Err("Client not initialized".to_string());
        }
    };

    // Build filter for Kind 1063 (FileMetadata) with MIME type "image/gif"
    let mut filter = Filter::new()
        .kind(Kind::from(1063)) // FileMetadata
        .custom_tag(
            SingleLetterTag::lowercase(Alphabet::M),
            "image/gif"
        )
        .limit(limit);

    // Add NIP-50 search if provided (relay-side search)
    if let Some(ref query) = search_query {
        if !query.is_empty() {
            filter = filter.search(query);
            log::info!("Using NIP-50 relay search for: '{}'", query);
        }
    }

    // Add pagination if provided
    if let Some(until_ts) = until {
        filter = filter.until(until_ts);
    }

    // Try gifbuddy relay first (dedicated GIF relay with likely NIP-50 support)
    let gifbuddy_relays = vec!["wss://relay.gifbuddy.lol"];

    let events = match client.fetch_events_from(
        gifbuddy_relays.clone(),
        filter.clone(),
        Duration::from_secs(10)
    ).await {
        Ok(gifbuddy_events) => {
            log::info!("Fetched {} GIF events from gifbuddy relay", gifbuddy_events.len());

            // If we got good results from gifbuddy, use them
            if !gifbuddy_events.is_empty() {
                gifbuddy_events.into_iter().collect()
            } else {
                // Fallback to user's relays if gifbuddy returned nothing
                log::info!("No results from gifbuddy, trying user relays");
                crate::stores::nostr_client::fetch_events_aggregated(
                    filter,
                    Duration::from_secs(10)
                ).await?
            }
        }
        Err(e) => {
            // If gifbuddy fails, fallback to user's relays
            log::warn!("Failed to fetch from gifbuddy relay: {}, trying user relays", e);
            crate::stores::nostr_client::fetch_events_aggregated(
                filter,
                Duration::from_secs(10)
            ).await?
        }
    };

    log::info!("Fetched {} GIF events total", events.len());

    // Parse events into GifMetadata
    let mut gifs = Vec::new();
    for event in events {
        if let Some(gif) = parse_gif_event(&event) {
            gifs.push(gif);
        }
    }

    // Sort by created_at (newest first)
    gifs.sort_by(|a, b| b.created_at.cmp(&a.created_at));

    log::info!("Parsed {} valid GIF entries", gifs.len());

    Ok(gifs)
}

/// Parse a Nostr event into GifMetadata
fn parse_gif_event(event: &nostr::Event) -> Option<GifMetadata> {
    let mut url = None;
    let mut thumbnail = None;
    let mut dimensions = None;
    let mut size = None;
    let mut blurhash = None;
    let mut alt = None;
    let mut summary = None;

    // Parse tags to extract metadata
    for tag in event.tags.iter() {
        let tag_slice = tag.as_slice();
        if tag_slice.is_empty() {
            continue;
        }

        match tag_slice[0].as_str() {
            "url" => {
                if tag_slice.len() >= 2 {
                    url = Some(tag_slice[1].to_string());
                }
            }
            "thumb" => {
                if tag_slice.len() >= 2 {
                    thumbnail = Some(tag_slice[1].to_string());
                }
            }
            "dim" => {
                if tag_slice.len() >= 2 {
                    // Parse dimensions like "480x360"
                    if let Some((w, h)) = parse_dimensions(&tag_slice[1]) {
                        dimensions = Some((w, h));
                    }
                }
            }
            "size" => {
                if tag_slice.len() >= 2 {
                    if let Ok(s) = tag_slice[1].parse::<usize>() {
                        size = Some(s);
                    }
                }
            }
            "blurhash" => {
                if tag_slice.len() >= 2 {
                    blurhash = Some(tag_slice[1].to_string());
                }
            }
            "alt" => {
                if tag_slice.len() >= 2 {
                    alt = Some(tag_slice[1].to_string());
                }
            }
            "summary" => {
                if tag_slice.len() >= 2 {
                    summary = Some(tag_slice[1].to_string());
                }
            }
            _ => {}
        }
    }

    // URL is required
    let url = url?;

    Some(GifMetadata {
        url,
        thumbnail,
        dimensions,
        size,
        blurhash,
        alt,
        summary,
        created_at: event.created_at,
    })
}

/// Parse dimensions string like "480x360" into (width, height)
fn parse_dimensions(dim_str: &str) -> Option<(u64, u64)> {
    let parts: Vec<&str> = dim_str.split('x').collect();
    if parts.len() == 2 {
        let width = parts[0].parse::<u64>().ok()?;
        let height = parts[1].parse::<u64>().ok()?;
        Some((width, height))
    } else {
        None
    }
}

/// Load initial GIFs (from cache and network)
pub async fn load_initial_gifs() {
    *GIF_LOADING.write() = true;

    // Capture the current search query to detect if it changes during fetch
    let captured_query = CURRENT_SEARCH_QUERY.read().clone();
    let query = if captured_query.is_empty() { None } else { Some(captured_query.clone()) };

    match fetch_gifs(100, None, query).await {
        Ok(gifs) => {
            // Bail if the search query changed while we were fetching (prevents stale results)
            let current_query = CURRENT_SEARCH_QUERY.read().clone();
            if captured_query != current_query {
                log::debug!("Search query changed during initial load, discarding stale results");
                *GIF_LOADING.write() = false;
                return;
            }

            // Update oldest timestamp for pagination
            if let Some(oldest) = gifs.last() {
                *GIF_OLDEST_TIMESTAMP.write() = Some(oldest.created_at);
            }

            *GIF_RESULTS.read().data().write() = gifs;
        }
        Err(e) => {
            log::error!("Failed to load initial GIFs: {}", e);
        }
    }

    *GIF_LOADING.write() = false;
}

/// Search for GIFs with a specific query
pub async fn search_gifs(query: String) {
    // Increment sequence number to track this search request
    let request_seq = {
        let mut seq = GIF_SEARCH_SEQ.write();
        *seq = seq.wrapping_add(1);
        *seq
    };

    *GIF_LOADING.write() = true;
    *CURRENT_SEARCH_QUERY.write() = query.clone();

    let search_query = if query.is_empty() { None } else { Some(query) };

    match fetch_gifs(100, None, search_query).await {
        Ok(gifs) => {
            // Only update state if this is still the latest search request
            let current_seq = *GIF_SEARCH_SEQ.read();
            if request_seq != current_seq {
                log::debug!("Discarding stale search results (seq {} != {})", request_seq, current_seq);
                return;
            }

            // Update oldest timestamp for pagination
            if let Some(oldest) = gifs.last() {
                *GIF_OLDEST_TIMESTAMP.write() = Some(oldest.created_at);
            }

            *GIF_RESULTS.read().data().write() = gifs;
        }
        Err(e) => {
            log::error!("Failed to search GIFs: {}", e);
        }
    }

    *GIF_LOADING.write() = false;
}

/// Load more GIFs (pagination)
pub async fn load_more_gifs() {
    let until = *GIF_OLDEST_TIMESTAMP.read();
    if until.is_none() {
        log::warn!("No oldest timestamp set, cannot paginate");
        return;
    }

    *GIF_LOADING.write() = true;

    // Capture the current search query to detect if it changes while we're loading
    let captured_query = CURRENT_SEARCH_QUERY.read().clone();
    let query = if captured_query.is_empty() { None } else { Some(captured_query.clone()) };

    match fetch_gifs(100, until, query).await {
        Ok(new_gifs) => {
            // Bail if the search query changed while we were loading (prevents cross-contamination)
            let current_query = CURRENT_SEARCH_QUERY.read().clone();
            if captured_query != current_query {
                log::debug!("Search query changed during pagination, discarding stale results");
                *GIF_LOADING.write() = false;
                return;
            }

            if new_gifs.is_empty() {
                log::info!("No more GIFs to load");
                *GIF_LOADING.write() = false;
                return;
            }

            // Filter out duplicates (Filter::until is inclusive, so oldest event may be duplicated)
            let oldest_timestamp = until;
            let deduplicated_gifs: Vec<GifMetadata> = new_gifs.into_iter()
                .filter(|gif| Some(gif.created_at) != oldest_timestamp)
                .collect();

            if deduplicated_gifs.is_empty() {
                log::info!("No new GIFs after deduplication");
                *GIF_LOADING.write() = false;
                return;
            }

            // Update oldest timestamp
            if let Some(oldest) = deduplicated_gifs.last() {
                *GIF_OLDEST_TIMESTAMP.write() = Some(oldest.created_at);
            }

            // Append to existing results
            let store = GIF_RESULTS.read();
            let mut data = store.data();
            let mut current = data.write();
            current.extend(deduplicated_gifs);
        }
        Err(e) => {
            log::error!("Failed to load more GIFs: {}", e);
        }
    }

    *GIF_LOADING.write() = false;
}

/// Add a GIF to recent list
pub fn add_recent_gif(gif: GifMetadata) {
    let store = RECENT_GIFS.read();
    let mut data = store.data();
    let mut recent = data.write();

    // Remove if already exists (to move to front)
    recent.retain(|g| g.url != gif.url);

    // Add to front
    recent.insert(0, gif);

    // Limit size
    if recent.len() > MAX_RECENT_GIFS {
        recent.truncate(MAX_RECENT_GIFS);
    }
}

/// Gifbuddy relay for publishing uploaded GIFs
const GIFBUDDY_RELAY: &str = "wss://relay.gifbuddy.lol";

/// Publish a GIF as a NIP-94 FileMetadata event (kind 1063)
///
/// This publishes the uploaded GIF to relay.gifbuddy.lol and the user's relays
/// with the `gifbuddyupload` tag for discoverability.
///
/// # Arguments
/// * `url` - The URL of the uploaded GIF
/// * `mime_type` - MIME type (should be "image/gif")
/// * `hash` - SHA-256 hash of the file
/// * `caption` - Description/caption for the GIF
/// * `size` - Optional file size in bytes
/// * `dimensions` - Optional dimensions (width, height)
///
/// # Returns
/// * `Ok(String)` - Event ID of the published event
/// * `Err(String)` - Error message if publishing fails
pub async fn publish_gif_event(
    url: String,
    mime_type: String,
    hash: String,
    caption: String,
    size: Option<usize>,
    dimensions: Option<(u32, u32)>,
) -> Result<String, String> {
    use nostr_sdk::prelude::*;

    log::info!("Publishing GIF event for: {}", url);

    let client = crate::stores::nostr_client::get_client()
        .ok_or("Client not initialized")?;

    let signer = crate::stores::nostr_client::get_signer()
        .ok_or("Not authenticated. Please sign in to publish.")?;

    // Parse URL
    let file_url = Url::parse(&url).map_err(|e| format!("Invalid URL: {}", e))?;

    // Parse hash
    let sha256_hash = nostr::hashes::sha256::Hash::from_str(&hash)
        .map_err(|e| format!("Invalid hash: {}", e))?;

    // Build NIP-94 FileMetadata
    let mut metadata = nip94::FileMetadata::new(file_url, mime_type, sha256_hash);

    if let Some(s) = size {
        metadata = metadata.size(s);
    }

    if let Some((w, h)) = dimensions {
        metadata = metadata.dimensions(ImageDimensions {
            width: w as u64,
            height: h as u64,
        });
    }

    // Create the event builder
    let builder = EventBuilder::file_metadata(&caption, metadata);

    // Add gifbuddyupload tag for compatibility with gifbuddy ecosystem
    let tags = vec![
        Tag::hashtag("gifbuddyupload"),
        Tag::custom(TagKind::Custom("alt".into()), vec![caption.clone()]),
        Tag::custom(TagKind::Custom("summary".into()), vec![caption.clone()]),
    ];

    let builder = builder.tags(tags);

    // Sign the event
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

    let event_id = event.id.to_string();
    log::info!("Created GIF event: {}", event_id);

    // Ensure relays are connected
    crate::stores::nostr_client::ensure_relays_ready(&client).await;

    // First, try to add gifbuddy relay temporarily
    let gifbuddy_url = Url::parse(GIFBUDDY_RELAY).map_err(|e| format!("Invalid relay URL: {}", e))?;
    if let Err(e) = client.add_relay(&gifbuddy_url).await {
        log::warn!("Could not add gifbuddy relay: {}", e);
    }

    // Connect to gifbuddy relay
    if let Err(e) = client.connect_relay(&gifbuddy_url).await {
        log::warn!("Could not connect to gifbuddy relay: {}", e);
    }

    // Publish to all connected relays (including gifbuddy)
    match client.send_event(&event).await {
        Ok(output) => {
            log::info!("Published GIF event to {} relays", output.success.len());
            if !output.failed.is_empty() {
                log::warn!("Failed to publish to {} relays", output.failed.len());
            }
            Ok(event_id)
        }
        Err(e) => {
            log::error!("Failed to publish GIF event: {}", e);
            Err(format!("Failed to publish: {}", e))
        }
    }
}
