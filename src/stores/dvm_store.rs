//! NIP-90 Data Vending Machine (DVM) Store - Content Discovery Focus
//!
//! Provides state management for:
//! - DVM provider discovery (kind 31990 with #k=5300)
//! - Content discovery requests (kind 5300)
//! - Feed response parsing (kind 6300)

use dioxus::prelude::*;
use nostr_sdk::{Event, EventId, Filter, Kind, PublicKey, Tag, Timestamp};
use crate::stores::nostr_client;
use std::time::Duration;
use url::Url;

// ============================================================================
// Constants
// ============================================================================

/// Content discovery job kind (NIP-90)
pub const KIND_CONTENT_DISCOVERY: u16 = 5300;

/// Content discovery result kind (5300 + 1000)
pub const KIND_CONTENT_DISCOVERY_RESULT: u16 = 6300;

/// NIP-89 Handler information / DVM announcement
pub const KIND_APP_HANDLER: u16 = 31990;

/// Default content discovery DVM (same as Snort uses)
pub const DEFAULT_CONTENT_DVM: &str = "0d9ec486275b70f0c4faec277fc4c63b9f14cb1ca1ec029f7d76210e957e5257";

/// Relays known to have DVM providers
const DVM_RELAYS: &[&str] = &[
    "wss://relay.damus.io",
    "wss://nos.lol",
    "wss://relay.nostr.band",
    "wss://nostr.wine",
    "wss://relay.primal.net",
];

// ============================================================================
// Data Types
// ============================================================================

/// DVM Service Provider (parsed from kind 31990)
#[derive(Clone, Debug, PartialEq)]
pub struct DvmProvider {
    pub pubkey: PublicKey,
    pub event_id: EventId,
    pub name: String,
    pub about: Option<String>,
    pub picture: Option<String>,
    pub created_at: Timestamp,
}

impl DvmProvider {
    /// Parse from a kind 31990 event with k=5300 tag
    pub fn from_event(event: &Event) -> Option<Self> {
        if event.kind.as_u16() != KIND_APP_HANDLER {
            return None;
        }

        // Check for k tag with 5300 (content discovery)
        let has_content_discovery = event.tags.iter().any(|tag| {
            let slice = tag.as_slice();
            slice.len() >= 2 && slice[0] == "k" && slice[1] == "5300"
        });

        if !has_content_discovery {
            return None;
        }

        // Parse content as JSON metadata
        let metadata: serde_json::Value = serde_json::from_str(&event.content)
            .unwrap_or(serde_json::Value::Null);

        Some(Self {
            pubkey: event.pubkey,
            event_id: event.id,
            name: metadata.get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("Unknown DVM")
                .to_string(),
            about: metadata.get("about")
                .and_then(|v| v.as_str())
                .map(String::from),
            picture: metadata.get("picture")
                .or_else(|| metadata.get("image"))
                .and_then(|v| v.as_str())
                .filter(|url_str| {
                    // Validate URL properly (pattern from url_metadata.rs)
                    Url::parse(url_str)
                        .map(|url| url.scheme() == "https" || url.scheme() == "http")
                        .unwrap_or(false)
                })
                .map(String::from),
            created_at: event.created_at,
        })
    }
}

// ============================================================================
// Global State
// ============================================================================

/// Selected DVM provider pubkey (None = use default)
pub static SELECTED_DVM_PROVIDER: GlobalSignal<Option<PublicKey>> = Signal::global(|| None);

/// Available content discovery DVM providers
pub static DVM_PROVIDERS: GlobalSignal<Vec<DvmProvider>> = Signal::global(Vec::new);

/// Current feed events from DVM response
pub static DVM_FEED_EVENTS: GlobalSignal<Vec<Event>> = Signal::global(Vec::new);

/// Loading state for feed
pub static DVM_FEED_LOADING: GlobalSignal<bool> = Signal::global(|| false);

/// Loading state for providers
pub static DVM_PROVIDERS_LOADING: GlobalSignal<bool> = Signal::global(|| false);

/// Error message if any
pub static DVM_FEED_ERROR: GlobalSignal<Option<String>> = Signal::global(|| None);

/// Last request event ID (for response matching)
pub static DVM_LAST_REQUEST_ID: GlobalSignal<Option<EventId>> = Signal::global(|| None);

// ============================================================================
// Functions
// ============================================================================

/// Get the effective DVM provider pubkey (selected or default)
#[allow(dead_code)]
pub fn get_effective_provider() -> PublicKey {
    SELECTED_DVM_PROVIDER.read()
        .unwrap_or_else(|| PublicKey::from_hex(DEFAULT_CONTENT_DVM)
            .expect("Invalid DEFAULT_CONTENT_DVM constant"))
}

/// Discover content discovery DVM providers (kind 31990 with #k=5300)
pub async fn discover_content_dvms() -> Result<Vec<DvmProvider>, String> {
    // Atomic check to prevent duplicate loads (pattern from reactions_store.rs)
    {
        let mut loading = DVM_PROVIDERS_LOADING.write();
        if *loading {
            return Ok(DVM_PROVIDERS.read().clone());
        }
        *loading = true;
    }

    let client = nostr_client::get_client()
        .ok_or_else(|| {
            *DVM_PROVIDERS_LOADING.write() = false;
            "Client not initialized".to_string()
        })?;

    // Add DVM relays
    for relay_url in DVM_RELAYS {
        if let Ok(url) = nostr_sdk::RelayUrl::parse(relay_url) {
            let _ = client.add_relay(url).await;
        }
    }

    nostr_client::ensure_relays_ready(&client).await;

    // Query for kind 31990 with #k=5300 (content discovery capability)
    let filter = Filter::new()
        .kind(Kind::from(KIND_APP_HANDLER))
        .custom_tag(
            nostr_sdk::SingleLetterTag::lowercase(nostr_sdk::Alphabet::K),
            "5300"
        )
        .limit(100);

    log::info!("Discovering content discovery DVMs (kind 31990 with #k=5300)");

    let events = client.fetch_events(filter, Duration::from_secs(15))
        .await
        .map_err(|e| {
            *DVM_PROVIDERS_LOADING.write() = false;
            format!("Failed to fetch DVMs: {}", e)
        })?;

    log::info!("Fetched {} potential content discovery DVM events", events.len());

    // Parse providers
    let mut providers: Vec<DvmProvider> = events
        .into_iter()
        .filter_map(|event| DvmProvider::from_event(&event))
        .collect();

    // Deduplicate by pubkey (keep newest)
    providers.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    let mut seen_pubkeys = std::collections::HashSet::new();
    providers.retain(|p| seen_pubkeys.insert(p.pubkey));

    log::info!("Found {} unique content discovery DVMs", providers.len());

    *DVM_PROVIDERS.write() = providers.clone();
    *DVM_PROVIDERS_LOADING.write() = false;

    Ok(providers)
}

/// Request content feed from DVM (kind 5300 → 6300)
pub async fn request_content_feed(provider: Option<PublicKey>) -> Result<Vec<Event>, String> {
    *DVM_FEED_LOADING.write() = true;
    *DVM_FEED_ERROR.write() = None;

    let client = nostr_client::get_client()
        .ok_or("Client not initialized")?;

    // Get target DVM pubkey
    let target_pubkey = provider.unwrap_or_else(|| {
        PublicKey::from_hex(DEFAULT_CONTENT_DVM)
            .expect("Invalid DEFAULT_CONTENT_DVM constant")
    });

    log::info!("Requesting content discovery from DVM: {}", target_pubkey.to_hex());

    // Build kind 5300 job request
    let tags = vec![
        Tag::public_key(target_pubkey),
    ];

    let builder = nostr_sdk::EventBuilder::new(Kind::from(KIND_CONTENT_DISCOVERY), "")
        .tags(tags);

    // Check if we have a signer
    if !*nostr_client::HAS_SIGNER.read() {
        // Without signer, we can't submit jobs - try fetching recent results from this DVM
        log::info!("No signer available, fetching recent DVM results instead");
        return fetch_recent_dvm_results(target_pubkey).await;
    }

    // Publish the job request
    let output = client.send_event_builder(builder).await
        .map_err(|e| {
            *DVM_FEED_EVENTS.write() = Vec::new();
            *DVM_FEED_LOADING.write() = false;
            *DVM_FEED_ERROR.write() = Some(format!("Failed to submit job: {}", e));
            format!("Failed to submit job: {}", e)
        })?;

    let request_id = *output.id();
    log::info!("Content discovery request submitted: {}", request_id.to_hex());
    *DVM_LAST_REQUEST_ID.write() = Some(request_id);

    // Wait for response (kind 6300) tagged with our request
    let response_filter = Filter::new()
        .kind(Kind::from(KIND_CONTENT_DISCOVERY_RESULT))
        .event(request_id)
        .author(target_pubkey)
        .limit(1);

    // Poll for response with timeout
    let mut attempts = 0;
    let max_attempts = 30; // 30 seconds total

    loop {
        attempts += 1;
        if attempts > max_attempts {
            *DVM_FEED_EVENTS.write() = Vec::new();
            *DVM_FEED_LOADING.write() = false;
            *DVM_FEED_ERROR.write() = Some("DVM response timeout".to_string());
            return Err("DVM response timeout - no response received".to_string());
        }

        // Brief delay between polls
        #[cfg(target_arch = "wasm32")]
        {
            use gloo_timers::future::TimeoutFuture;
            TimeoutFuture::new(1000).await;
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            tokio::time::sleep(Duration::from_secs(1)).await;
        }

        // Check for response
        if let Ok(responses) = client.fetch_events(response_filter.clone(), Duration::from_secs(2)).await {
            if let Some(response) = responses.into_iter().next() {
                log::info!("Received DVM response: {}", response.id.to_hex());
                let feed_events = parse_feed_response(&response, &client).await?;

                *DVM_FEED_EVENTS.write() = feed_events.clone();
                *DVM_FEED_LOADING.write() = false;

                return Ok(feed_events);
            }
        }

        log::debug!("Waiting for DVM response... attempt {}/{}", attempts, max_attempts);
    }
}

/// Fetch recent results from a DVM (fallback when not signed in)
async fn fetch_recent_dvm_results(dvm_pubkey: PublicKey) -> Result<Vec<Event>, String> {
    let client = nostr_client::get_client()
        .ok_or_else(|| {
            *DVM_FEED_EVENTS.write() = Vec::new();
            *DVM_FEED_LOADING.write() = false;
            *DVM_FEED_ERROR.write() = Some("Client not initialized".to_string());
            "Client not initialized".to_string()
        })?;

    // Fetch recent kind 6300 events from this DVM
    let filter = Filter::new()
        .kind(Kind::from(KIND_CONTENT_DISCOVERY_RESULT))
        .author(dvm_pubkey)
        .limit(1);

    let responses = client.fetch_events(filter, Duration::from_secs(10))
        .await
        .map_err(|e| {
            *DVM_FEED_EVENTS.write() = Vec::new();
            *DVM_FEED_LOADING.write() = false;
            *DVM_FEED_ERROR.write() = Some(format!("Failed to fetch DVM results: {}", e));
            format!("Failed to fetch DVM results: {}", e)
        })?;

    if let Some(response) = responses.into_iter().next() {
        let feed_events = parse_feed_response(&response, &client).await?;
        *DVM_FEED_EVENTS.write() = feed_events.clone();
        *DVM_FEED_LOADING.write() = false;
        return Ok(feed_events);
    }

    *DVM_FEED_EVENTS.write() = Vec::new();
    *DVM_FEED_LOADING.write() = false;
    *DVM_FEED_ERROR.write() = Some("No DVM results found".to_string());
    Err("No DVM results found".to_string())
}

/// Parse kind 6300 response to fetch referenced events
async fn parse_feed_response(response: &Event, client: &nostr_sdk::Client) -> Result<Vec<Event>, String> {
    // Response content should be JSON array of tags: [["e", "id1"], ["e", "id2"], ...]
    let json_tags: Vec<Vec<String>> = serde_json::from_str(&response.content)
        .unwrap_or_else(|_| {
            // Fallback: try parsing from event tags instead
            response.tags.iter()
                .filter_map(|tag| {
                    let parts: Vec<String> = tag.as_slice().iter()
                        .map(|s| s.to_string())
                        .collect();
                    if parts.first().map(|s| s.as_str()) == Some("e") {
                        Some(parts)
                    } else {
                        None
                    }
                })
                .collect()
        });

    // Extract event IDs
    let event_ids: Vec<EventId> = json_tags.iter()
        .filter_map(|tag| {
            if tag.first().map(|s| s.as_str()) == Some("e") {
                tag.get(1).and_then(|id| EventId::from_hex(id).ok())
            } else {
                None
            }
        })
        .collect();

    log::info!("DVM response contains {} event references", event_ids.len());

    if event_ids.is_empty() {
        return Ok(Vec::new());
    }

    // Fetch the referenced events
    let filter = Filter::new()
        .ids(event_ids.clone())
        .limit(event_ids.len());

    let events = client.fetch_events(filter, Duration::from_secs(10))
        .await
        .map_err(|e| format!("Failed to fetch feed events: {}", e))?;

    let mut event_vec: Vec<Event> = events.into_iter().collect();

    // Sort by created_at descending (newest first)
    event_vec.sort_by(|a, b| b.created_at.cmp(&a.created_at));

    log::info!("Fetched {} feed events from DVM recommendation", event_vec.len());

    Ok(event_vec)
}

/// Set selected DVM provider
pub fn set_selected_provider(pubkey: Option<PublicKey>) {
    *SELECTED_DVM_PROVIDER.write() = pubkey;
}

/// Clear feed state (for refresh)
pub fn clear_feed() {
    *DVM_FEED_EVENTS.write() = Vec::new();
    *DVM_FEED_ERROR.write() = None;
    *DVM_FEED_LOADING.write() = false;
    *DVM_LAST_REQUEST_ID.write() = None;
}
