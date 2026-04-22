use dioxus::prelude::*;
use dioxus::signals::ReadableExt;
use dioxus_stores::Store;
use nostr_sdk::{Alphabet, Filter, Kind, SingleLetterTag, Timestamp};
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
    /// Description from event content field (NIP-94 stores description here)
    pub description: Option<String>,
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
pub async fn fetch_gifs(
    limit: usize,
    until: Option<Timestamp>,
    search_query: Option<String>,
) -> Result<Vec<GifMetadata>, String> {
    log::info!(
        "Fetching GIFs from Nostr (limit: {}, until: {:?}, search: {:?})",
        limit,
        until,
        search_query
    );
    let client = match crate::stores::nostr_client::get_client() {
        Some(c) => c,
        None => {
            log::warn!("Client not initialized, skipping GIF fetch");
            return Err("Client not initialized".to_string());
        }
    };
    let mut filter = Filter::new()
        .kind(Kind::from(1063))
        .custom_tag(SingleLetterTag::lowercase(Alphabet::M), "image/gif")
        .limit(limit);
    let search_query_lower = search_query.as_ref().map(|q| q.to_lowercase());
    if let Some(ref query) = search_query {
        if !query.is_empty() {
            filter = filter.search(query);
            log::info!(
                "Using NIP-50 search for: '{}' (relays without NIP-50 will be filtered client-side)",
                query
            );
        }
    }
    if let Some(until_ts) = until {
        filter = filter.until(until_ts);
    }
    crate::stores::relay::ensure_gif_relay(&client).await;
    log::info!("Fetching GIFs from all connected relays (including gifbuddy)");
    let events =
        crate::stores::nostr_client::fetch_events_aggregated(filter, Duration::from_secs(10))
            .await?;
    log::info!("Fetched {} GIF events total", events.len());
    let mut gifs = Vec::new();
    for event in events {
        if let Some(gif) = parse_gif_event(&event) {
            gifs.push(gif);
        }
    }
    gifs.sort_by_key(|b| std::cmp::Reverse(b.created_at));
    let mut seen_urls = std::collections::HashSet::new();
    let gifs: Vec<GifMetadata> = gifs
        .into_iter()
        .filter(|gif| seen_urls.insert(gif.url.clone()))
        .collect();
    log::info!("After dedup: {} unique GIFs", gifs.len());
    let gifs = if let Some(ref query) = search_query_lower {
        if !query.is_empty() {
            let filtered: Vec<GifMetadata> = gifs
                .into_iter()
                .filter(|gif| {
                    let description_match = gif
                        .description
                        .as_ref()
                        .map(|d| d.to_lowercase().contains(query))
                        .unwrap_or(false);
                    let alt_match = gif
                        .alt
                        .as_ref()
                        .map(|a| a.to_lowercase().contains(query))
                        .unwrap_or(false);
                    let summary_match = gif
                        .summary
                        .as_ref()
                        .map(|s| s.to_lowercase().contains(query))
                        .unwrap_or(false);
                    let url_match = gif.url.to_lowercase().contains(query);
                    description_match || alt_match || summary_match || url_match
                })
                .collect();
            log::info!("Filtered to {} GIFs matching '{}'", filtered.len(), query);
            filtered
        } else {
            gifs
        }
    } else {
        gifs
    };
    log::info!("Returning {} GIF entries", gifs.len());
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
    for tag in event.tags.iter() {
        let tag_slice = tag.as_slice();
        if tag_slice.is_empty() {
            continue;
        }
        match tag_slice[0].as_str() {
            "url" if tag_slice.len() >= 2 => {
                url = Some(tag_slice[1].to_string());
            }
            "thumb" if tag_slice.len() >= 2 => {
                thumbnail = Some(tag_slice[1].to_string());
            }
            "dim" if tag_slice.len() >= 2 => {
                if let Some((w, h)) = parse_dimensions(&tag_slice[1]) {
                    dimensions = Some((w, h));
                }
            }
            "size" if tag_slice.len() >= 2 => {
                if let Ok(s) = tag_slice[1].parse::<usize>() {
                    size = Some(s);
                }
            }
            "blurhash" if tag_slice.len() >= 2 => {
                blurhash = Some(tag_slice[1].to_string());
            }
            "alt" if tag_slice.len() >= 2 => {
                alt = Some(tag_slice[1].to_string());
            }
            "summary" if tag_slice.len() >= 2 => {
                summary = Some(tag_slice[1].to_string());
            }
            _ => {}
        }
    }
    let url = url?;
    let description = if event.content.is_empty() {
        None
    } else {
        Some(event.content.to_string())
    };
    Some(GifMetadata {
        url,
        thumbnail,
        dimensions,
        size,
        blurhash,
        alt,
        summary,
        description,
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
    let captured_query = CURRENT_SEARCH_QUERY.read().clone();
    let query = if captured_query.is_empty() {
        None
    } else {
        Some(captured_query.clone())
    };
    match fetch_gifs(100, None, query).await {
        Ok(gifs) => {
            let current_query = CURRENT_SEARCH_QUERY.read().clone();
            if captured_query != current_query {
                log::debug!("Search query changed during initial load, discarding stale results");
                *GIF_LOADING.write() = false;
                return;
            }
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
    let request_seq = {
        let mut seq = GIF_SEARCH_SEQ.write();
        *seq = seq.wrapping_add(1);
        *seq
    };
    *GIF_LOADING.write() = true;
    *CURRENT_SEARCH_QUERY.write() = query.clone();
    let search_query = if query.is_empty() {
        None
    } else {
        Some(query.clone())
    };
    match fetch_gifs(100, None, search_query).await {
        Ok(gifs) => {
            let current_seq = *GIF_SEARCH_SEQ.read();
            if request_seq != current_seq {
                log::debug!(
                    "Discarding stale search results (seq {} != {})",
                    request_seq,
                    current_seq
                );
                return;
            }
            let current_query = CURRENT_SEARCH_QUERY.read().clone();
            if search_gifs_query_mismatch(&current_query, &query) {
                log::debug!("Search query changed during fetch, discarding stale results");
                *GIF_LOADING.write() = false;
                return;
            }
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

fn search_gifs_query_mismatch(current_query: &str, requested_query: &str) -> bool {
    current_query != requested_query
}
/// Load more GIFs (pagination)
pub async fn load_more_gifs() {
    let until = *GIF_OLDEST_TIMESTAMP.read();
    if until.is_none() {
        log::warn!("No oldest timestamp set, cannot paginate");
        return;
    }
    *GIF_LOADING.write() = true;
    let captured_query = CURRENT_SEARCH_QUERY.read().clone();
    let query = if captured_query.is_empty() {
        None
    } else {
        Some(captured_query.clone())
    };
    match fetch_gifs(100, until, query).await {
        Ok(new_gifs) => {
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
            let oldest_timestamp = until;
            let deduplicated_gifs: Vec<GifMetadata> = new_gifs
                .into_iter()
                .filter(|gif| Some(gif.created_at) != oldest_timestamp)
                .collect();
            if deduplicated_gifs.is_empty() {
                log::info!("No new GIFs after deduplication");
                *GIF_LOADING.write() = false;
                return;
            }
            if let Some(oldest) = deduplicated_gifs.last() {
                *GIF_OLDEST_TIMESTAMP.write() = Some(oldest.created_at);
            }
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
    recent.retain(|g| g.url != gif.url);
    recent.insert(0, gif);
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
    let client = crate::stores::nostr_client::get_client().ok_or("Client not initialized")?;
    let signer = crate::stores::nostr_client::get_signer()
        .ok_or("Not authenticated. Please sign in to publish.")?;
    let file_url = Url::parse(&url).map_err(|e| format!("Invalid URL: {}", e))?;
    let sha256_hash =
        nostr::hashes::sha256::Hash::from_str(&hash).map_err(|e| format!("Invalid hash: {}", e))?;
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
    let builder = EventBuilder::file_metadata(&caption, metadata);
    let tags = vec![
        Tag::hashtag("gifbuddyupload"),
        Tag::custom(TagKind::Custom("alt".into()), vec![caption.clone()]),
        Tag::custom(TagKind::Custom("summary".into()), vec![caption.clone()]),
    ];
    let builder = builder.tags(tags);
    let event = match signer {
        crate::stores::signer::SignerType::Keys(keys) => builder
            .sign(&keys)
            .await
            .map_err(|e| format!("Failed to sign event: {}", e))?,
        #[cfg(target_family = "wasm")]
        crate::stores::signer::SignerType::BrowserExtension(browser_signer) => builder
            .sign(browser_signer.as_ref())
            .await
            .map_err(|e| format!("Failed to sign event: {}", e))?,
        crate::stores::signer::SignerType::NostrConnect(nostr_connect) => builder
            .sign(nostr_connect.as_ref())
            .await
            .map_err(|e| format!("Failed to sign event: {}", e))?,
        #[cfg(feature = "mobile_platform")]
        crate::stores::signer::SignerType::AndroidSigner(android_signer) => builder
            .sign(android_signer.as_ref())
            .await
            .map_err(|e| format!("Failed to sign event: {}", e))?,
    };
    let event_id = event.id.to_string();
    log::info!("Created GIF event: {}", event_id);
    crate::stores::nostr_client::ensure_relays_ready(&client).await;
    let gifbuddy_url =
        Url::parse(GIFBUDDY_RELAY).map_err(|e| format!("Invalid relay URL: {}", e))?;
    if let Err(e) = client.add_relay(&gifbuddy_url).await {
        log::warn!("Could not add gifbuddy relay: {}", e);
    }
    if let Err(e) = client.connect_relay(&gifbuddy_url).await {
        log::warn!("Could not connect to gifbuddy relay: {}", e);
    }
    crate::stores::publish_queue::enqueue(
        event,
        crate::stores::publish_queue::types::QueueEventType::Media,
        None,
        std::collections::HashMap::new(),
    ).await;
    Ok(event_id)
}
