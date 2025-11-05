use serde::{Deserialize, Serialize};
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::{Request, RequestInit, RequestMode, Response};

const NOSTR_BAND_API: &str = "https://api.nostr.band";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrendingNote {
    pub event: TrendingEvent,
    pub author: TrendingAuthor,
    pub profile: Option<TrendingProfile>,
    pub stats: Option<TrendingStats>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrendingEvent {
    pub id: String,
    pub pubkey: String,
    pub created_at: u64,
    pub kind: u16,
    pub tags: Vec<Vec<String>>,
    pub content: String,
    pub sig: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrendingAuthor {
    pub pubkey: String,
    pub content: String, // JSON string containing profile data
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrendingProfile {
    pub name: Option<String>,
    pub display_name: Option<String>,
    pub picture: Option<String>,
    pub nip05: Option<String>,
    pub about: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrendingStats {
    pub replies: Option<u32>,
    pub reactions: Option<u32>,
    pub reposts: Option<u32>,
    pub zaps: Option<u32>,
}

#[derive(Debug, Clone, Deserialize)]
struct NostrBandApiNote {
    event: TrendingEvent,
    author: TrendingAuthor,
    stats: Option<NostrBandStats>,
}

#[derive(Debug, Clone, Deserialize)]
struct NostrBandStats {
    replies_count: Option<u32>,
    reactions_count: Option<u32>,
    reposts_count: Option<u32>,
    zaps_msats: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
struct NostrBandResponse {
    notes: Vec<NostrBandApiNote>,
}

/// Fetch trending notes from Nostr.Band API
/// Returns the top trending posts in the last 24 hours
pub async fn get_trending_notes(limit: Option<usize>) -> Result<Vec<TrendingNote>, String> {
    let url = format!("{}/v0/trending/notes", NOSTR_BAND_API);

    // Create request
    let opts = RequestInit::new();
    opts.set_method("GET");
    opts.set_mode(RequestMode::Cors);

    let request = Request::new_with_str_and_init(&url, &opts)
        .map_err(|e| format!("Failed to create request: {:?}", e))?;

    request
        .headers()
        .set("Accept", "application/json")
        .map_err(|e| format!("Failed to set header: {:?}", e))?;

    // Fetch from API
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

    // Parse JSON response
    let json = JsFuture::from(resp.json().map_err(|e| format!("Failed to get JSON: {:?}", e))?)
        .await
        .map_err(|e| format!("Failed to parse JSON: {:?}", e))?;

    let response: NostrBandResponse = serde_wasm_bindgen::from_value(json)
        .map_err(|e| format!("Failed to deserialize: {:?}", e))?;

    // Transform and parse notes
    let mut trending_notes: Vec<TrendingNote> = Vec::new();

    for note in response.notes {
        // Parse the author's content field to get profile data
        let profile = match serde_json::from_str::<TrendingProfile>(&note.author.content) {
            Ok(p) => Some(p),
            Err(e) => {
                log::warn!("Failed to parse author profile: {}", e);
                None
            }
        };

        let stats = note.stats.map(|s| TrendingStats {
            replies: s.replies_count,
            reactions: s.reactions_count,
            reposts: s.reposts_count,
            zaps: s.zaps_msats.map(|z| (z / 1000) as u32),
        });

        trending_notes.push(TrendingNote {
            event: note.event,
            author: note.author,
            profile,
            stats,
        });
    }

    // Apply limit if specified
    if let Some(limit) = limit {
        trending_notes.truncate(limit);
    }

    log::info!("Fetched {} trending notes from Nostr.Band", trending_notes.len());
    Ok(trending_notes)
}

/// Get display name for a trending note author
pub fn get_display_name(note: &TrendingNote) -> String {
    if let Some(profile) = &note.profile {
        if let Some(display_name) = &profile.display_name {
            return display_name.clone();
        }
        if let Some(name) = &profile.name {
            return name.clone();
        }
    }

    // Fallback to truncated pubkey
    let pubkey = &note.event.pubkey;
    if pubkey.len() > 16 {
        format!("{}...{}", &pubkey[..8], &pubkey[pubkey.len()-8..])
    } else {
        pubkey.clone()
    }
}

/// Truncate content to a maximum length
pub fn truncate_content(content: &str, max_length: usize) -> String {
    if content.len() <= max_length {
        content.to_string()
    } else {
        // Find the last valid UTF-8 character boundary before max_length
        let truncate_at = content
            .char_indices()
            .take_while(|(idx, _)| *idx < max_length)
            .last()
            .map(|(idx, ch)| idx + ch.len_utf8())
            .unwrap_or(0);

        format!("{}...", content[..truncate_at].trim())
    }
}
