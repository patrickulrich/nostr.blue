#[cfg(feature = "web")]
use crate::stores::nostr_client::get_client;
use crate::utils::truncate_pubkey;
#[cfg(feature = "web")]
use nostr_sdk::{EventId, Filter, Kind};
use serde::{Deserialize, Serialize};
#[cfg(feature = "web")]
use wasm_bindgen::JsCast;
#[cfg(feature = "web")]
use wasm_bindgen_futures::JsFuture;
#[cfg(feature = "web")]
use web_sys::{Request, RequestInit, RequestMode, Response};
#[cfg(feature = "web")]
const NOSTR_WINE_API: &str = "https://api.nostr.wine";
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrendingNote {
    pub event: TrendingEvent,
    pub author: TrendingAuthor,
    pub profile: Option<TrendingProfile>,
    pub stats: Option<TrendingStats>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrendingEvent {
    pub id: String,
    pub pubkey: String,
    pub created_at: u64,
    pub kind: u16,
    pub tags: Vec<Vec<String>>,
    pub content: String,
    pub sig: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrendingAuthor {
    pub pubkey: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrendingProfile {
    pub name: Option<String>,
    pub display_name: Option<String>,
    pub picture: Option<String>,
    pub nip05: Option<String>,
    pub about: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrendingStats {
    pub replies: Option<u32>,
    pub reactions: Option<u32>,
    pub reposts: Option<u32>,
    pub zaps: Option<u32>,
}
/// Response item from nostr.wine trending API
#[cfg(feature = "web")]
#[derive(Debug, Clone, Deserialize)]
struct NostrWineTrendingItem {
    event_id: String,
    replies: u32,
    reactions: u32,
    reposts: u32,
    #[allow(dead_code)]
    zap_amount: u64,
    zap_count: u32,
}
/// Fetch trending notes from nostr.wine API
/// Returns the top trending posts ordered by reactions
///
/// Parameters:
/// - limit: Number of events to return (1-200, default 10)
///
/// Note: Hours is fixed at 24 and order is fixed to "reactions" for best engagement signal.
#[cfg(feature = "web")]
pub async fn get_trending_notes(limit: Option<usize>) -> Result<Vec<TrendingNote>, String> {
    let limit = limit.unwrap_or(10).clamp(1, 200);
    let url = format!(
        "{}/trending?limit={}&hours=24&order=reactions",
        NOSTR_WINE_API, limit,
    );
    log::info!("Fetching trending from nostr.wine: {}", url);
    let opts = RequestInit::new();
    opts.set_method("GET");
    opts.set_mode(RequestMode::Cors);
    let request = Request::new_with_str_and_init(&url, &opts)
        .map_err(|e| format!("Failed to create request: {:?}", e))?;
    request
        .headers()
        .set("Accept", "application/json")
        .map_err(|e| format!("Failed to set header: {:?}", e))?;
    let window = web_sys::window().ok_or("No window object")?;
    let resp_value = JsFuture::from(window.fetch_with_request(&request))
        .await
        .map_err(|e| format!("Fetch failed: {:?}", e))?;
    let resp: Response = resp_value
        .dyn_into()
        .map_err(|_| "Failed to cast to Response")?;
    if !resp.ok() {
        return Err(format!("API returned status: {}", resp.status()));
    }
    let json = JsFuture::from(
        resp.json()
            .map_err(|e| format!("Failed to get JSON: {:?}", e))?,
    )
    .await
    .map_err(|e| format!("Failed to parse JSON: {:?}", e))?;
    let trending_items: Vec<NostrWineTrendingItem> = serde_wasm_bindgen::from_value(json)
        .map_err(|e| format!("Failed to deserialize trending items: {:?}", e))?;
    log::info!(
        "Got {} trending event IDs from nostr.wine",
        trending_items.len()
    );
    if trending_items.is_empty() {
        return Ok(Vec::new());
    }
    let stats_map: std::collections::HashMap<String, TrendingStats> = trending_items
        .iter()
        .map(|item| {
            (
                item.event_id.clone(),
                TrendingStats {
                    replies: Some(item.replies),
                    reactions: Some(item.reactions),
                    reposts: Some(item.reposts),
                    zaps: Some(item.zap_count),
                },
            )
        })
        .collect();
    let client = get_client().ok_or("Client not initialized")?;
    let event_ids: Vec<EventId> = trending_items
        .iter()
        .filter_map(|item| EventId::from_hex(&item.event_id).ok())
        .collect();
    let filter = Filter::new().ids(event_ids.clone()).kind(Kind::TextNote);
    let events = client
        .fetch_events(filter, std::time::Duration::from_secs(10))
        .await
        .map_err(|e| format!("Failed to fetch events: {}", e))?;
    log::info!("Fetched {} events from relays", events.len());
    let mut trending_notes: Vec<TrendingNote> = Vec::new();
    let events_map: std::collections::HashMap<String, _> =
        events.into_iter().map(|e| (e.id.to_hex(), e)).collect();
    for item in &trending_items {
        if let Some(event) = events_map.get(&item.event_id) {
            let stats = stats_map.get(&item.event_id).cloned();
            let tags: Vec<Vec<String>> = event
                .tags
                .iter()
                .map(|tag| tag.as_slice().iter().map(|s| s.to_string()).collect())
                .collect();
            let trending_event = TrendingEvent {
                id: event.id.to_hex(),
                pubkey: event.pubkey.to_hex(),
                created_at: event.created_at.as_secs(),
                kind: event.kind.as_u16(),
                tags,
                content: event.content.clone(),
                sig: event.sig.to_string(),
            };
            let author = TrendingAuthor {
                pubkey: event.pubkey.to_hex(),
            };
            trending_notes.push(TrendingNote {
                event: trending_event,
                author,
                profile: None,
                stats,
            });
        }
    }
    if trending_notes.len() < trending_items.len() {
        log::warn!(
            "Trending: only fetched {} of {} items from relays (missing {})",
            trending_notes.len(),
            trending_items.len(),
            trending_items.len() - trending_notes.len()
        );
    }
    log::info!(
        "Built {} trending notes from nostr.wine",
        trending_notes.len()
    );
    Ok(trending_notes)
}

/// Fetch trending notes (native stub)
#[cfg(not(feature = "web"))]
pub async fn get_trending_notes(_limit: Option<usize>) -> Result<Vec<TrendingNote>, String> {
    Err("Trending notes not yet supported on native".to_string())
}

/// Get display name for a trending note author
#[allow(dead_code)]
pub fn get_display_name(note: &TrendingNote) -> String {
    if let Some(profile) = &note.profile {
        if let Some(display_name) = &profile.display_name {
            return display_name.clone();
        }
        if let Some(name) = &profile.name {
            return name.clone();
        }
    }
    truncate_pubkey(&note.event.pubkey)
}
/// Truncate content to a maximum length
pub fn truncate_content(content: &str, max_length: usize) -> String {
    if content.len() <= max_length {
        content.to_string()
    } else {
        let truncate_at = content
            .char_indices()
            .take_while(|(idx, _)| *idx < max_length)
            .last()
            .map(|(idx, ch)| idx + ch.len_utf8())
            .unwrap_or(0);
        format!("{}...", content[..truncate_at].trim())
    }
}
