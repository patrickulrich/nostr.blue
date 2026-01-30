//! Pin Boards Store
//! Implements the Pinboards NIP with two-event architecture:
//! - Kind 30067: Pinboard Set (board metadata only)
//! - Kind 39067: Pin (individual content references)
#![allow(dead_code)]
use dioxus::prelude::*;
use lru::LruCache;
use nostr::Event as NostrEvent;
use nostr_sdk::prelude::*;
use serde::{Deserialize, Serialize};
use std::num::NonZeroUsize;
use std::time::Duration;
use crate::stores::nostr_client;
use crate::utils::nip73::ExternalContentId;
/// Kind 30067 - Pinboard Set (board metadata only, addressable event)
pub const KIND_PINBOARD: u16 = 30067;
/// Kind 39067 - Pin (individual content reference, regular event)
pub const KIND_PIN: u16 = 39067;
const PINBOARD_CACHE_SIZE: usize = 100;
const PIN_CACHE_SIZE: usize = 500;
/// Content type enum - inferred from pin reference
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PinContentType {
    Text,
    Link,
    Image,
    Video,
    Profile,
    Note,
    Recipe,
    Community,
    CodeRepo,
    Podcast,
    Music,
    CalendarEvent,
    Article,
    LiveStream,
    Badge,
    Pinboard,
    Book,
    Location,
}
impl std::fmt::Display for PinContentType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PinContentType::Text => write!(f, "text"),
            PinContentType::Link => write!(f, "link"),
            PinContentType::Image => write!(f, "image"),
            PinContentType::Video => write!(f, "video"),
            PinContentType::Profile => write!(f, "profile"),
            PinContentType::Note => write!(f, "note"),
            PinContentType::Recipe => write!(f, "recipe"),
            PinContentType::Community => write!(f, "community"),
            PinContentType::CodeRepo => write!(f, "code_repo"),
            PinContentType::Podcast => write!(f, "podcast"),
            PinContentType::Music => write!(f, "music"),
            PinContentType::CalendarEvent => write!(f, "calendar_event"),
            PinContentType::Article => write!(f, "article"),
            PinContentType::LiveStream => write!(f, "live_stream"),
            PinContentType::Badge => write!(f, "badge"),
            PinContentType::Pinboard => write!(f, "pinboard"),
            PinContentType::Book => write!(f, "book"),
            PinContentType::Location => write!(f, "location"),
        }
    }
}
impl PinContentType {
    /// Get display name for UI
    pub fn display_name(&self) -> &'static str {
        match self {
            PinContentType::Text => "Text",
            PinContentType::Link => "Link",
            PinContentType::Image => "Image",
            PinContentType::Video => "Video",
            PinContentType::Profile => "Profile",
            PinContentType::Note => "Note",
            PinContentType::Recipe => "Recipe",
            PinContentType::Community => "Community",
            PinContentType::CodeRepo => "Code Repository",
            PinContentType::Podcast => "Podcast",
            PinContentType::Music => "Music",
            PinContentType::CalendarEvent => "Calendar Event",
            PinContentType::Article => "Article",
            PinContentType::LiveStream => "Live Stream",
            PinContentType::Badge => "Badge",
            PinContentType::Pinboard => "Pinboard",
            PinContentType::Book => "Book",
            PinContentType::Location => "Location",
        }
    }
}
/// Pinboard (Kind 30067) - Board metadata only, no pins embedded
#[derive(Clone, Debug, PartialEq)]
pub struct Pinboard {
    pub d_tag: String,
    pub event_id: String,
    pub pubkey: String,
    pub naddr: String,
    pub a_tag: String,
    pub created_at: u64,
    pub title: String,
    pub description: Option<String>,
    pub image: Option<String>,
    pub tags: Vec<String>,
    /// If true, anyone can pin to this board and all pins are shown
    /// If false (default), only the owner's pins are displayed
    pub collaborative: bool,
    pub is_owner: bool,
    pub event: NostrEvent,
}
impl Pinboard {
    /// Get display title (falls back to d_tag if no title)
    pub fn display_title(&self) -> &str {
        if self.title.is_empty() { &self.d_tag } else { &self.title }
    }
}
/// Pin reference - what content is being pinned
#[derive(Clone, Debug, PartialEq)]
pub enum PinReference {
    /// Reference to a Nostr event by ID (e tag)
    Event { id: String, relay_hint: Option<String> },
    /// Reference to a parameterized replaceable event (a tag with non-30067 kind)
    Coordinate { address: String, relay_hint: Option<String> },
    /// Reference to external content via NIP-73 (i tag + k tag)
    External { content: ExternalContentId, hint: Option<String> },
}
impl PinReference {
    /// Infer content type from the reference
    pub fn infer_content_type(&self) -> PinContentType {
        match self {
            PinReference::External { content, .. } => {
                match content {
                    ExternalContentId::Url(url) => {
                        let lower = url.as_str().to_lowercase();
                        if is_image_url(&lower) {
                            PinContentType::Image
                        } else if is_video_url(&lower) {
                            PinContentType::Video
                        } else {
                            PinContentType::Link
                        }
                    }
                    ExternalContentId::Book(_) => PinContentType::Book,
                    ExternalContentId::PodcastFeed(_)
                    | ExternalContentId::PodcastEpisode(_)
                    | ExternalContentId::PodcastPublisher(_) => PinContentType::Podcast,
                    ExternalContentId::Movie(_) => PinContentType::Video,
                    ExternalContentId::Paper(_) => PinContentType::Article,
                    ExternalContentId::Geohash(_) => PinContentType::Location,
                    ExternalContentId::Hashtag(_) => PinContentType::Text,
                    ExternalContentId::BlockchainTransaction { .. }
                    | ExternalContentId::BlockchainAddress { .. } => PinContentType::Link,
                }
            }
            PinReference::Event { .. } => PinContentType::Note,
            PinReference::Coordinate { address, .. } => {
                let kind_opt = if address.starts_with("naddr1") {
                    Coordinate::from_bech32(address).ok().map(|c| c.kind.as_u16() as u32)
                } else {
                    address.split(':').next().and_then(|s| s.parse::<u32>().ok())
                };
                if let Some(kind) = kind_opt {
                    return match kind {
                        30023 => PinContentType::Article,
                        30078 => PinContentType::Recipe,
                        34550 => PinContentType::Community,
                        30617 => PinContentType::CodeRepo,
                        31922 | 31923 => PinContentType::CalendarEvent,
                        30311 => PinContentType::LiveStream,
                        30009 => PinContentType::Badge,
                        30067 => PinContentType::Pinboard,
                        32123 => PinContentType::Podcast,
                        31337 | 32267 => PinContentType::Music,
                        0 => PinContentType::Profile,
                        _ => PinContentType::Note,
                    };
                }
                PinContentType::Note
            }
        }
    }
    /// Get the display reference (URL or identifier)
    pub fn display_ref(&self) -> String {
        match self {
            PinReference::External { content, .. } => content.to_string(),
            PinReference::Event { id, .. } => id.clone(),
            PinReference::Coordinate { address, .. } => address.clone(),
        }
    }
}
/// Pin (Kind 39067) - Individual content reference
#[derive(Clone, Debug, PartialEq)]
pub struct Pin {
    pub event_id: String,
    pub pubkey: String,
    pub created_at: u64,
    pub board_addresses: Vec<String>,
    pub reference: PinReference,
    pub title: Option<String>,
    pub content: String,
    pub tags: Vec<String>,
    pub event: NostrEvent,
}
impl Pin {
    /// Get inferred content type from reference
    /// Note: For Kind 30023 events, this returns Article. Use fetch_pin_content_type()
    /// to get accurate type by checking the actual referenced event's tags.
    pub fn content_type(&self) -> PinContentType {
        self.reference.infer_content_type()
    }
    /// Check if this is a profile pin (no board reference)
    pub fn is_profile_pin(&self) -> bool {
        self.board_addresses.is_empty()
    }
    /// Get display title (falls back to content type name)
    pub fn display_title(&self) -> String {
        self.title
            .clone()
            .unwrap_or_else(|| self.content_type().display_name().to_string())
    }
}
/// Combined view for rendering a board with its pins
#[derive(Clone, Debug, PartialEq)]
pub struct PinboardWithPins {
    pub board: Pinboard,
    pub pins: Vec<Pin>,
}
impl PinboardWithPins {
    /// Get pin count
    pub fn pin_count(&self) -> usize {
        self.pins.len()
    }
    /// Check if board is empty
    pub fn is_empty(&self) -> bool {
        self.pins.is_empty()
    }
}
/// Input data for creating/updating a pinboard
#[derive(Clone, Debug)]
pub struct PinboardInput {
    pub title: String,
    pub description: Option<String>,
    pub image: Option<String>,
    pub tags: Vec<String>,
    /// If true, anyone can pin to this board and all pins are shown
    pub collaborative: bool,
}
/// Input data for creating a pin
#[derive(Clone, Debug)]
pub struct PinInput {
    pub board_addresses: Vec<String>,
    pub reference: PinReference,
    pub title: Option<String>,
    pub image: Option<String>,
    pub content: String,
    pub tags: Vec<String>,
}
/// Pinboards cache (keyed by a_tag)
pub static PINBOARDS_CACHE: GlobalSignal<LruCache<String, Pinboard>> = GlobalSignal::new(||
LruCache::new(NonZeroUsize::new(PINBOARD_CACHE_SIZE).unwrap()));
/// Pins cache (keyed by event_id)
pub static PINS_CACHE: GlobalSignal<LruCache<String, Pin>> = GlobalSignal::new(|| LruCache::new(
    NonZeroUsize::new(PIN_CACHE_SIZE).unwrap(),
));
/// Loading state
pub static LOADING_PINBOARDS: GlobalSignal<bool> = GlobalSignal::new(|| false);
/// Store initialization state
pub static PINBOARDS_INITIALIZED: GlobalSignal<bool> = GlobalSignal::new(|| false);
/// Get a pinboard from cache by a_tag
pub fn get_cached_pinboard(a_tag: &str) -> Option<Pinboard> {
    PINBOARDS_CACHE.read().peek(a_tag).cloned()
}
/// Get a pinboard from cache by naddr
pub fn get_cached_pinboard_by_naddr(naddr: &str) -> Option<Pinboard> {
    let cache = PINBOARDS_CACHE.read();
    cache.iter().find(|(_, board)| board.naddr == naddr).map(|(_, board)| board.clone())
}
/// Cache a pinboard
pub fn cache_pinboard(board: Pinboard) {
    PINBOARDS_CACHE.write().put(board.a_tag.clone(), board);
}
/// Cache multiple pinboards
pub fn cache_pinboards(boards: &[Pinboard]) {
    let mut cache = PINBOARDS_CACHE.write();
    for board in boards {
        cache.put(board.a_tag.clone(), board.clone());
    }
}
/// Get all cached pinboards
pub fn get_all_cached_pinboards() -> Vec<Pinboard> {
    let cache = PINBOARDS_CACHE.read();
    cache.iter().map(|(_, board)| board.clone()).collect()
}
/// Get cached boards by author
pub fn get_cached_pinboards_by_author(pubkey: &str) -> Vec<Pinboard> {
    let cache = PINBOARDS_CACHE.read();
    cache
        .iter()
        .filter(|(_, board)| board.pubkey == pubkey)
        .map(|(_, board)| board.clone())
        .collect()
}
/// Cache a pin
pub fn cache_pin(pin: Pin) {
    PINS_CACHE.write().put(pin.event_id.clone(), pin);
}
/// Cache multiple pins
pub fn cache_pins(pins: &[Pin]) {
    let mut cache = PINS_CACHE.write();
    for pin in pins {
        cache.put(pin.event_id.clone(), pin.clone());
    }
}
/// Get cached pins for a board
pub fn get_cached_pins_for_board(board_a_tag: &str) -> Vec<Pin> {
    let cache = PINS_CACHE.read();
    cache
        .iter()
        .filter(|(_, pin)| pin.board_addresses.contains(&board_a_tag.to_string()))
        .map(|(_, pin)| pin.clone())
        .collect()
}
/// Clear all caches
pub fn clear_cache() {
    PINBOARDS_CACHE.write().clear();
    PINS_CACHE.write().clear();
    *PINBOARDS_INITIALIZED.write() = false;
}
/// Remove a pinboard from cache
pub fn remove_pinboard_from_cache(a_tag: &str) {
    PINBOARDS_CACHE.write().pop(a_tag);
}
/// Remove a pin from cache
pub fn remove_pin_from_cache(event_id: &str) {
    PINS_CACHE.write().pop(event_id);
}
/// Check if URL is an image
fn is_image_url(url: &str) -> bool {
    let extensions = [".jpg", ".jpeg", ".png", ".gif", ".webp", ".svg", ".bmp", ".ico"];
    extensions.iter().any(|ext| url.ends_with(ext)) || url.contains("image")
        || url.contains("/i/")
}
/// Check if URL is a video
fn is_video_url(url: &str) -> bool {
    let extensions = [".mp4", ".webm", ".mov", ".avi", ".mkv", ".m4v"];
    extensions.iter().any(|ext| url.ends_with(ext)) || url.contains("youtube.com")
        || url.contains("youtu.be") || url.contains("vimeo.com")
}
/// Extract a tag value by tag name
fn extract_tag_value(tags: &Tags, tag_name: &str) -> Option<String> {
    tags.iter()
        .find(|t| {
            let slice = t.as_slice();
            slice.first().map(|s| s.as_str()) == Some(tag_name)
        })
        .and_then(|t| {
            let slice = t.as_slice();
            slice.get(1).map(|s| s.to_string())
        })
}
/// Extract all tag values for a repeatable tag
fn extract_all_tag_values(tags: &Tags, tag_name: &str) -> Vec<String> {
    tags.iter()
        .filter(|t| {
            let slice = t.as_slice();
            slice.first().map(|s| s.as_str()) == Some(tag_name)
        })
        .filter_map(|t| {
            let slice = t.as_slice();
            slice.get(1).map(|s| s.to_string())
        })
        .collect()
}
/// Parse a pinboard event (Kind 30067)
pub fn parse_pinboard_event(
    event: &NostrEvent,
    current_user_pubkey: Option<&str>,
) -> Option<Pinboard> {
    if event.kind.as_u16() != KIND_PINBOARD {
        return None;
    }
    let d_tag = event
        .tags
        .iter()
        .find(|t| {
            t.kind() == TagKind::SingleLetter(SingleLetterTag::lowercase(Alphabet::D))
        })
        .and_then(|t| t.content())
        .map(|s| s.to_string())?;
    let a_tag = format!("{}:{}:{}", KIND_PINBOARD, event.pubkey.to_hex(), d_tag);
    let naddr = match Coordinate::new(Kind::Custom(KIND_PINBOARD), event.pubkey)
        .identifier(&d_tag)
        .to_bech32()
    {
        Ok(n) => n,
        Err(e) => {
            log::error!("Failed to build naddr: {}", e);
            return None;
        }
    };
    let title = extract_tag_value(&event.tags, "title").unwrap_or_else(|| d_tag.clone());
    let description = extract_tag_value(&event.tags, "description");
    let image = extract_tag_value(&event.tags, "image");
    let tags = extract_all_tag_values(&event.tags, "t");
    let collaborative = event
        .tags
        .iter()
        .any(|t| t.as_slice().first().map(|s| s.as_str()) == Some("collaborative"));
    let pubkey_hex = event.pubkey.to_hex();
    let is_owner = current_user_pubkey.is_some_and(|p| p == pubkey_hex);
    Some(Pinboard {
        d_tag,
        event_id: event.id.to_hex(),
        pubkey: pubkey_hex,
        naddr,
        a_tag,
        created_at: event.created_at.as_secs(),
        title,
        description,
        image,
        tags,
        collaborative,
        is_owner,
        event: event.clone(),
    })
}
/// Parse a pin event (Kind 39067)
pub fn parse_pin_event(event: &NostrEvent) -> Option<Pin> {
    if event.kind.as_u16() != KIND_PIN {
        return None;
    }
    let mut board_addresses: Vec<String> = Vec::new();
    let mut content_coordinate: Option<(String, Option<String>)> = None;
    let mut event_ref: Option<(String, Option<String>)> = None;
    let mut external_content: Option<(ExternalContentId, Option<String>)> = None;
    for tag in event.tags.iter() {
        let slice = tag.as_slice();
        match slice.first().map(|s| s.as_str()) {
            Some("A") => {
                if let Some(address) = slice.get(1) {
                    if address.starts_with("30067:") {
                        board_addresses.push(address.to_string());
                    } else if address.starts_with("naddr1") {
                        if let Ok(coord) = Coordinate::from_bech32(address) {
                            if coord.kind.as_u16() == KIND_PINBOARD {
                                let a_tag = format!(
                                    "{}:{}:{}",
                                    coord.kind.as_u16(),
                                    coord.public_key.to_hex(),
                                    coord.identifier,
                                );
                                board_addresses.push(a_tag);
                            }
                        }
                    }
                }
            }
            Some("a") => {
                if let Some(address) = slice.get(1) {
                    let relay_hint = slice.get(2).map(|s| s.to_string());
                    content_coordinate = Some((address.to_string(), relay_hint));
                }
            }
            Some("e") => {
                if let Some(id) = slice.get(1) {
                    let relay_hint = slice.get(2).map(|s| s.to_string());
                    event_ref = Some((id.to_string(), relay_hint));
                }
            }
            _ => {}
        }
        if let Some(TagStandard::ExternalContent { content, hint, .. }) = tag
            .as_standardized()
        {
            external_content = Some((
                content.clone(),
                hint.as_ref().map(|u| u.to_string()),
            ));
        }
    }
    let reference = if let Some((id, relay_hint)) = event_ref {
        PinReference::Event {
            id,
            relay_hint,
        }
    } else if let Some((address, relay_hint)) = content_coordinate {
        PinReference::Coordinate {
            address,
            relay_hint,
        }
    } else if let Some((content, hint)) = external_content {
        PinReference::External {
            content,
            hint,
        }
    } else {
        log::warn!("Pin event {} has no content reference", event.id.to_hex());
        return None;
    };
    let title = extract_tag_value(&event.tags, "title");
    let tags = extract_all_tag_values(&event.tags, "t");
    Some(Pin {
        event_id: event.id.to_hex(),
        pubkey: event.pubkey.to_hex(),
        created_at: event.created_at.as_secs(),
        board_addresses,
        reference,
        title,
        content: event.content.clone(),
        tags,
        event: event.clone(),
    })
}
/// Determine the accurate content type for a pin by fetching the referenced event.
/// This is needed because Kind 30023 can be either an article or a recipe (identified by `nostrcooking` tag).
/// Returns the inferred type if fetch fails or referenced event doesn't change the inference.
pub async fn fetch_pin_content_type(pin: &Pin) -> PinContentType {
    if let PinReference::Coordinate { address, .. } = &pin.reference {
        let coord_opt = if address.starts_with("naddr1") {
            Coordinate::from_bech32(address).ok()
        } else {
            Coordinate::parse(address).ok()
        };
        if let Some(coord) = coord_opt {
            if coord.kind.as_u16() == 30023 {
                let filter = Filter::new()
                    .kind(coord.kind)
                    .author(coord.public_key)
                    .identifier(&coord.identifier)
                    .limit(1);
                if let Ok(events) = nostr_client::fetch_events_aggregated(
                        filter,
                        Duration::from_secs(5),
                    )
                    .await
                {
                    if let Some(event) = events.first() {
                        if event
                            .tags
                            .hashtags()
                            .any(|tag| tag == crate::utils::recipe::RECIPE_TAG_PREFIX)
                        {
                            return PinContentType::Recipe;
                        }
                        return PinContentType::Article;
                    }
                }
            }
        }
    }
    pin.content_type()
}
/// Enrich a list of pins with accurate content types by fetching referenced events.
/// For Kind 30023 references, this checks if they are recipes or articles.
pub async fn enrich_pins_content_types(pins: &[Pin]) -> Vec<(String, PinContentType)> {
    use futures::future::join_all;
    let futures: Vec<_> = pins
        .iter()
        .filter(|pin| {
            if let PinReference::Coordinate { address, .. } = &pin.reference {
                if address.starts_with("30023:") {
                    return true;
                }
                if address.starts_with("naddr1") {
                    if let Ok(coord) = Coordinate::from_bech32(address) {
                        return coord.kind.as_u16() == 30023;
                    }
                }
                false
            } else {
                false
            }
        })
        .map(|pin| {
            let event_id = pin.event_id.clone();
            async move {
                let content_type = fetch_pin_content_type(pin).await;
                (event_id, content_type)
            }
        })
        .collect();
    join_all(futures).await
}
/// Metadata extracted from the referenced event for display in pin cards
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PinMetadata {
    /// Accurate content type (resolves Article vs Recipe for Kind 30023)
    pub content_type: Option<PinContentType>,
    /// Title from the referenced event
    pub title: Option<String>,
    /// Cover image URL from the referenced event
    pub image: Option<String>,
    /// Summary/description from the referenced event
    pub summary: Option<String>,
}
/// Fetch metadata for a pin by retrieving the referenced event.
/// Extracts title, image, and summary from the referenced event's tags.
pub async fn fetch_pin_metadata(pin: &Pin) -> PinMetadata {
    match &pin.reference {
        PinReference::Coordinate { address, .. } => {
            let coord_opt = if address.starts_with("naddr1") {
                Coordinate::from_bech32(address).ok()
            } else {
                Coordinate::parse(address).ok()
            };
            if let Some(coord) = coord_opt {
                let filter = Filter::new()
                    .kind(coord.kind)
                    .author(coord.public_key)
                    .identifier(&coord.identifier)
                    .limit(1);
                if let Ok(events) = nostr_client::fetch_events_aggregated(
                        filter,
                        Duration::from_secs(5),
                    )
                    .await
                {
                    if let Some(event) = events.first() {
                        return extract_event_metadata(event, coord.kind.as_u16());
                    }
                }
            }
            PinMetadata::default()
        }
        PinReference::Event { id, .. } => {
            if let Ok(event_id) = EventId::from_hex(id) {
                let filter = Filter::new().id(event_id).limit(1);
                if let Ok(events) = nostr_client::fetch_events_aggregated(
                        filter,
                        Duration::from_secs(5),
                    )
                    .await
                {
                    if let Some(event) = events.first() {
                        return extract_event_metadata(event, event.kind.as_u16());
                    }
                }
            }
            PinMetadata::default()
        }
        PinReference::External { .. } => PinMetadata::default(),
    }
}
/// Extract metadata from an event based on its kind
fn extract_event_metadata(event: &NostrEvent, kind: u16) -> PinMetadata {
    let tags = &event.tags;
    let title = extract_tag_value(tags, "title");
    let image = extract_tag_value(tags, "image");
    let summary = extract_tag_value(tags, "summary");
    let content_type = match kind {
        30023 => {
            if tags.hashtags().any(|tag| tag == crate::utils::recipe::RECIPE_TAG_PREFIX)
            {
                Some(PinContentType::Recipe)
            } else {
                Some(PinContentType::Article)
            }
        }
        30078 => Some(PinContentType::Recipe),
        34550 => Some(PinContentType::Community),
        30617 => Some(PinContentType::CodeRepo),
        31922 | 31923 => Some(PinContentType::CalendarEvent),
        30311 => Some(PinContentType::LiveStream),
        30009 => Some(PinContentType::Badge),
        30067 => Some(PinContentType::Pinboard),
        32123 => Some(PinContentType::Podcast),
        31337 | 32267 => Some(PinContentType::Music),
        0 => Some(PinContentType::Profile),
        1 => Some(PinContentType::Note),
        _ => None,
    };
    PinMetadata {
        content_type,
        title,
        image,
        summary,
    }
}
/// Enrich a list of pins with full metadata by fetching referenced events.
/// This is more comprehensive than enrich_pins_content_types - it also gets image, title, summary.
pub async fn enrich_pins_metadata(
    pins: &[Pin],
) -> std::collections::HashMap<String, PinMetadata> {
    use futures::future::join_all;
    use std::collections::HashMap;
    let futures: Vec<_> = pins
        .iter()
        .filter(|pin| !matches!(pin.reference, PinReference::External { .. }))
        .map(|pin| {
            let event_id = pin.event_id.clone();
            async move {
                let metadata = fetch_pin_metadata(pin).await;
                (event_id, metadata)
            }
        })
        .collect();
    let results = join_all(futures).await;
    results
        .into_iter()
        .filter(|(_, meta)| {
            meta.title.is_some() || meta.image.is_some() || meta.content_type.is_some()
        })
        .collect::<HashMap<_, _>>()
}
/// Build a filter for fetching pinboards
pub fn pinboards_filter(limit: usize) -> Filter {
    Filter::new().kind(Kind::Custom(KIND_PINBOARD)).limit(limit)
}
/// Build a filter for a specific pinboard by coordinate
pub fn pinboard_by_coord_filter(pubkey: PublicKey, identifier: &str) -> Filter {
    Filter::new().kind(Kind::Custom(KIND_PINBOARD)).author(pubkey).identifier(identifier)
}
/// Build a filter for pinboards by author
pub fn pinboards_by_author_filter(pubkey: PublicKey, limit: usize) -> Filter {
    Filter::new().kind(Kind::Custom(KIND_PINBOARD)).author(pubkey).limit(limit)
}
/// Build a filter for pinboards with pagination
pub fn pinboards_paginated_filter(limit: usize, until: Option<u64>) -> Filter {
    let mut filter = Filter::new().kind(Kind::Custom(KIND_PINBOARD)).limit(limit);
    if let Some(ts) = until {
        filter = filter.until(Timestamp::from(ts));
    }
    filter
}
/// Build a filter for pinboards by hashtag
pub fn pinboards_by_hashtag_filter(hashtag: &str, limit: usize) -> Filter {
    Filter::new().kind(Kind::Custom(KIND_PINBOARD)).hashtag(hashtag).limit(limit)
}
/// Build a filter for pins referencing a board (uppercase A per NIP spec)
pub fn pins_for_board_filter(board_a_tag: &str, limit: usize) -> Filter {
    Filter::new()
        .kind(Kind::Custom(KIND_PIN))
        .custom_tag(SingleLetterTag::uppercase(Alphabet::A), board_a_tag)
        .limit(limit)
}
/// Build a filter for pins by author
pub fn pins_by_author_filter(pubkey: PublicKey, limit: usize) -> Filter {
    Filter::new().kind(Kind::Custom(KIND_PIN)).author(pubkey).limit(limit)
}
/// Build a filter for pins by author referencing a specific board (uppercase A per NIP spec)
pub fn pins_by_author_for_board_filter(
    pubkey: PublicKey,
    board_a_tag: &str,
    limit: usize,
) -> Filter {
    Filter::new()
        .kind(Kind::Custom(KIND_PIN))
        .author(pubkey)
        .custom_tag(SingleLetterTag::uppercase(Alphabet::A), board_a_tag)
        .limit(limit)
}
/// Build a filter for reactions on a pinboard
pub fn pinboard_reactions_filter(a_tag: &str, limit: usize) -> Filter {
    Filter::new()
        .kind(Kind::Reaction)
        .custom_tag(SingleLetterTag::lowercase(Alphabet::A), a_tag)
        .limit(limit)
}
/// Build a filter for zaps on a pinboard
pub fn pinboard_zaps_filter(a_tag: &str, limit: usize) -> Filter {
    Filter::new()
        .kind(Kind::ZapReceipt)
        .custom_tag(SingleLetterTag::lowercase(Alphabet::A), a_tag)
        .limit(limit)
}
/// Build a filter for reactions on a pin
pub fn pin_reactions_filter(event_id: &str, limit: usize) -> Filter {
    Filter::new()
        .kind(Kind::Reaction)
        .custom_tag(SingleLetterTag::lowercase(Alphabet::E), event_id)
        .limit(limit)
}
/// Fetch pinboards with aggregated DB + relay fetch
pub async fn fetch_pinboards(
    limit: usize,
) -> std::result::Result<Vec<Pinboard>, String> {
    *LOADING_PINBOARDS.write() = true;
    let filter = pinboards_filter(limit);
    let current_user = crate::stores::auth_store::get_pubkey();
    log::info!(
        "Discover: Fetching pinboards with filter kind={}, limit={}", KIND_PINBOARD,
        limit
    );
    let result = nostr_client::fetch_events_from_relays(filter, Duration::from_secs(15))
        .await;
    *LOADING_PINBOARDS.write() = false;
    match result {
        Ok(events) => {
            log::info!("Discover: Got {} raw events from relays", events.len());
            let unique_authors: std::collections::HashSet<_> = events
                .iter()
                .map(|e| e.pubkey.to_hex())
                .collect();
            log::info!("Discover: Events from {} unique authors", unique_authors.len());
            let boards: Vec<Pinboard> = events
                .iter()
                .filter_map(|e| parse_pinboard_event(e, current_user.as_deref()))
                .collect();
            cache_pinboards(&boards);
            *PINBOARDS_INITIALIZED.write() = true;
            log::info!("Discover: Parsed {} pinboards successfully", boards.len());
            Ok(boards)
        }
        Err(e) => {
            log::error!("Discover: Failed to fetch pinboards: {}", e);
            Err(e)
        }
    }
}
/// Fetch pinboards with pagination
pub async fn fetch_pinboards_page(
    limit: usize,
    until: Option<u64>,
) -> std::result::Result<Vec<Pinboard>, String> {
    let filter = pinboards_paginated_filter(limit, until);
    let current_user = crate::stores::auth_store::get_pubkey();
    let events = nostr_client::fetch_events_from_relays(filter, Duration::from_secs(15))
        .await?;
    let boards: Vec<Pinboard> = events
        .iter()
        .filter_map(|e| parse_pinboard_event(e, current_user.as_deref()))
        .collect();
    cache_pinboards(&boards);
    log::info!("Fetched {} pinboards (paginated)", boards.len());
    Ok(boards)
}
/// Fetch cookbooks (pinboards tagged with "cookbook")
pub async fn fetch_cookbooks(
    limit: usize,
) -> std::result::Result<Vec<Pinboard>, String> {
    let filter = Filter::new()
        .kind(Kind::Custom(KIND_PINBOARD))
        .hashtag("cookbook")
        .limit(limit);
    let current_user = crate::stores::auth_store::get_pubkey();
    let events = nostr_client::fetch_events_aggregated(filter, Duration::from_secs(15))
        .await?;
    let mut cookbooks: Vec<Pinboard> = events
        .iter()
        .filter_map(|e| parse_pinboard_event(e, current_user.as_deref()))
        .collect();
    cookbooks.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    cache_pinboards(&cookbooks);
    log::info!("Fetched {} cookbooks", cookbooks.len());
    Ok(cookbooks)
}
/// Fetch the current user's cookbooks (pinboards tagged with "cookbook")
pub async fn fetch_user_cookbooks() -> std::result::Result<Vec<Pinboard>, String> {
    let current_user = crate::stores::auth_store::get_pubkey().ok_or("Not logged in")?;
    let pubkey = nostr_sdk::PublicKey::from_hex(&current_user)
        .map_err(|e| format!("Invalid pubkey: {}", e))?;
    let filter = Filter::new()
        .kind(Kind::Custom(KIND_PINBOARD))
        .author(pubkey)
        .hashtag("cookbook");
    let events = nostr_client::fetch_events_aggregated(filter, Duration::from_secs(10))
        .await?;
    let mut cookbooks: Vec<Pinboard> = events
        .iter()
        .filter_map(|e| parse_pinboard_event(e, Some(&current_user)))
        .collect();
    cookbooks.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    cache_pinboards(&cookbooks);
    log::info!("Fetched {} user cookbooks", cookbooks.len());
    Ok(cookbooks)
}
/// Fetch a pinboard by naddr
pub async fn fetch_pinboard_by_naddr(
    naddr: &str,
) -> std::result::Result<Option<Pinboard>, String> {
    if let Some(cached) = get_cached_pinboard_by_naddr(naddr) {
        return Ok(Some(cached));
    }
    let coord = Coordinate::from_bech32(naddr)
        .map_err(|e| format!("Invalid naddr: {}", e))?;
    let filter = pinboard_by_coord_filter(coord.public_key, &coord.identifier);
    let current_user = crate::stores::auth_store::get_pubkey();
    let events = nostr_client::fetch_events_aggregated(filter, Duration::from_secs(10))
        .await?;
    if let Some(event) = events.first() {
        if let Some(board) = parse_pinboard_event(event, current_user.as_deref()) {
            cache_pinboard(board.clone());
            return Ok(Some(board));
        }
    }
    Ok(None)
}
/// Fetch pins for a board
pub async fn fetch_pins_for_board(
    board_a_tag: &str,
) -> std::result::Result<Vec<Pin>, String> {
    let filter = pins_for_board_filter(board_a_tag, 500);
    let events = nostr_client::fetch_events_aggregated(filter, Duration::from_secs(15))
        .await?;
    let mut pins: Vec<Pin> = events.iter().filter_map(parse_pin_event).collect();
    pins.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    cache_pins(&pins);
    log::info!("Fetched {} pins for board {}", pins.len(), board_a_tag);
    Ok(pins)
}
/// Fetch only the board owner's pins for a board
pub async fn fetch_owner_pins_for_board(
    board_a_tag: &str,
    owner_pubkey: &str,
) -> std::result::Result<Vec<Pin>, String> {
    let pk = PublicKey::parse(owner_pubkey)
        .map_err(|e| format!("Invalid pubkey: {}", e))?;
    let filter = pins_by_author_for_board_filter(pk, board_a_tag, 500);
    let events = nostr_client::fetch_events_aggregated(filter, Duration::from_secs(15))
        .await?;
    let mut pins: Vec<Pin> = events.iter().filter_map(parse_pin_event).collect();
    pins.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    cache_pins(&pins);
    log::info!("Fetched {} owner pins for board {}", pins.len(), board_a_tag);
    Ok(pins)
}
/// Fetch pins for a board with flexible author filtering
///
/// Modes:
/// - `owner_pubkey: Some(pk)` = Only owner's pins (default mode)
/// - `owner_pubkey: None` + `allowed_authors: Some(vec)` = Specific collaborators only
/// - `owner_pubkey: None` + `allowed_authors: None` = All pins (full collaborative mode)
pub async fn fetch_pins_for_board_filtered(
    board_a_tag: &str,
    owner_pubkey: Option<&str>,
    allowed_authors: Option<Vec<String>>,
) -> std::result::Result<Vec<Pin>, String> {
    let mut filter = Filter::new()
        .kind(Kind::Custom(KIND_PIN))
        .custom_tag(SingleLetterTag::uppercase(Alphabet::A), board_a_tag)
        .limit(500);
    if let Some(owner) = owner_pubkey {
        let pk = PublicKey::parse(owner)
            .map_err(|e| format!("Invalid owner pubkey: {}", e))?;
        filter = filter.author(pk);
    } else if let Some(ref authors) = allowed_authors {
        let pks: Vec<PublicKey> = authors
            .iter()
            .filter_map(|a| PublicKey::parse(a).ok())
            .collect();
        if !pks.is_empty() {
            filter = filter.authors(pks);
        }
    }
    let events = nostr_client::fetch_events_aggregated(filter, Duration::from_secs(15))
        .await?;
    let mut pins: Vec<Pin> = events.iter().filter_map(parse_pin_event).collect();
    pins.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    cache_pins(&pins);
    let mode = if owner_pubkey.is_some() {
        "owner-only"
    } else if allowed_authors.is_some() {
        "filtered-authors"
    } else {
        "all-authors"
    };
    log::info!("Fetched {} pins for board {} (mode: {})", pins.len(), board_a_tag, mode);
    Ok(pins)
}
/// Fetch a pinboard with its pins (two-stage loading)
pub async fn fetch_pinboard_with_pins(
    naddr: &str,
) -> std::result::Result<Option<PinboardWithPins>, String> {
    let board = match fetch_pinboard_by_naddr(naddr).await? {
        Some(b) => b,
        None => return Ok(None),
    };
    let pins = fetch_pins_for_board(&board.a_tag).await?;
    Ok(Some(PinboardWithPins { board, pins }))
}
/// Fetch pinboards by author
pub async fn fetch_pinboards_by_author(
    pubkey: &str,
    limit: usize,
) -> std::result::Result<Vec<Pinboard>, String> {
    let pk = PublicKey::parse(pubkey).map_err(|e| format!("Invalid pubkey: {}", e))?;
    let filter = pinboards_by_author_filter(pk, limit);
    let current_user = crate::stores::auth_store::get_pubkey();
    let events = nostr_client::fetch_events_aggregated(filter, Duration::from_secs(15))
        .await?;
    let boards: Vec<Pinboard> = events
        .iter()
        .filter_map(|e| parse_pinboard_event(e, current_user.as_deref()))
        .collect();
    cache_pinboards(&boards);
    log::info!("Fetched {} pinboards for author {}", boards.len(), pubkey);
    Ok(boards)
}
/// Fetch current user's pinboards
pub async fn fetch_my_pinboards() -> std::result::Result<Vec<Pinboard>, String> {
    let pubkey = crate::stores::auth_store::get_pubkey().ok_or("Not authenticated")?;
    fetch_pinboards_by_author(&pubkey, 100).await
}
/// Fetch all pins by the current user
pub async fn fetch_my_pins() -> std::result::Result<Vec<Pin>, String> {
    let pubkey = crate::stores::auth_store::get_pubkey().ok_or("Not authenticated")?;
    let pk = PublicKey::parse(&pubkey).map_err(|e| format!("Invalid pubkey: {}", e))?;
    let filter = pins_by_author_filter(pk, 500);
    let events = nostr_client::fetch_events_aggregated(filter, Duration::from_secs(15))
        .await?;
    let mut pins: Vec<Pin> = events.iter().filter_map(parse_pin_event).collect();
    pins.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    cache_pins(&pins);
    log::info!("Fetched {} pins for current user", pins.len());
    Ok(pins)
}
/// Search pinboards by title/description (local cache search)
pub fn search_pinboards_local(query: &str) -> Vec<Pinboard> {
    let query_lower = query.to_lowercase();
    let cache = PINBOARDS_CACHE.read();
    cache
        .iter()
        .filter(|(_, board)| {
            board.title.to_lowercase().contains(&query_lower)
                || board
                    .description
                    .as_ref()
                    .is_some_and(|d| d.to_lowercase().contains(&query_lower))
                || board.tags.iter().any(|t| t.to_lowercase().contains(&query_lower))
        })
        .map(|(_, board)| board.clone())
        .collect()
}
/// Create a new pinboard or update an existing one
pub async fn publish_pinboard(
    input: PinboardInput,
    existing_d_tag: Option<&str>,
) -> std::result::Result<String, String> {
    let client = nostr_client::get_client().ok_or("Client not initialized")?;
    if !*nostr_client::HAS_SIGNER.read() {
        return Err("No signer attached. Cannot publish pinboard.".to_string());
    }
    let d_tag = existing_d_tag
        .map(|s| s.to_string())
        .unwrap_or_else(|| crate::utils::slugify(&input.title));
    let mut tags: Vec<Tag> = vec![
        Tag::identifier(&d_tag),
        Tag::custom(TagKind::Custom("title".into()), vec![input.title.clone()]),
    ];
    if let Some(ref desc) = input.description {
        tags.push(
            Tag::custom(TagKind::Custom("description".into()), vec![desc.clone()]),
        );
    }
    if let Some(ref img) = input.image {
        tags.push(Tag::custom(TagKind::Custom("image".into()), vec![img.clone()]));
    }
    for tag in &input.tags {
        tags.push(Tag::hashtag(tag));
    }
    if input.collaborative {
        tags.push(
            Tag::custom(TagKind::Custom("collaborative".into()), Vec::<String>::new()),
        );
    }
    let builder = EventBuilder::new(Kind::Custom(KIND_PINBOARD), "").tags(tags);
    let output = client
        .send_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to publish pinboard: {}", e))?;
    let event_id = output.id().to_hex();
    log::info!("Pinboard published: {}", event_id);
    let signer = client
        .signer()
        .await
        .map_err(|e| format!("Failed to get signer: {}", e))?;
    let pubkey = signer
        .get_public_key()
        .await
        .map_err(|e| format!("Failed to get pubkey: {}", e))?;
    let naddr = nostr_client::make_naddr_with_hints(KIND_PINBOARD, &pubkey, &d_tag)
        .await?;
    Ok(naddr)
}
/// Create a new pin
pub async fn publish_pin(input: PinInput) -> std::result::Result<String, String> {
    let client = nostr_client::get_client().ok_or("Client not initialized")?;
    if !*nostr_client::HAS_SIGNER.read() {
        return Err("No signer attached. Cannot publish pin.".to_string());
    }
    let mut tags: Vec<Tag> = vec![];
    for board_addr in &input.board_addresses {
        let coord_opt = if board_addr.starts_with("naddr1") {
            Coordinate::from_bech32(board_addr).ok()
        } else {
            Coordinate::parse(board_addr).ok()
        };
        if let Some(coord) = coord_opt {
            tags.push(
                Tag::from_standardized(TagStandard::Coordinate {
                    coordinate: coord,
                    relay_url: None,
                    uppercase: true,
                }),
            );
        } else {
            log::warn!("Failed to parse board address: {}", board_addr);
            tags.push(
                Tag::custom(
                    TagKind::SingleLetter(SingleLetterTag::uppercase(Alphabet::A)),
                    vec![board_addr.clone()],
                ),
            );
        }
    }
    match &input.reference {
        PinReference::Event { id, relay_hint } => {
            let mut vals = vec![id.clone()];
            if let Some(relay) = relay_hint {
                vals.push(relay.clone());
            }
            tags.push(
                Tag::custom(
                    TagKind::SingleLetter(SingleLetterTag::lowercase(Alphabet::E)),
                    vals,
                ),
            );
        }
        PinReference::Coordinate { address, relay_hint } => {
            let mut vals = vec![address.clone()];
            if let Some(relay) = relay_hint {
                vals.push(relay.clone());
            }
            tags.push(
                Tag::custom(
                    TagKind::SingleLetter(SingleLetterTag::lowercase(Alphabet::A)),
                    vals,
                ),
            );
        }
        PinReference::External { content, hint } => {
            tags.push(
                Tag::from_standardized(TagStandard::ExternalContent {
                    content: content.clone(),
                    hint: hint.as_ref().and_then(|h| Url::parse(h).ok()),
                    uppercase: false,
                }),
            );
            tags.push(
                Tag::from_standardized(TagStandard::Nip73Kind {
                    kind: content.kind(),
                    uppercase: false,
                }),
            );
        }
    }
    if let Some(ref title) = input.title {
        tags.push(Tag::custom(TagKind::Custom("title".into()), vec![title.clone()]));
    }
    if let Some(ref image) = input.image {
        tags.push(Tag::custom(TagKind::Custom("image".into()), vec![image.clone()]));
    }
    for tag in &input.tags {
        tags.push(Tag::hashtag(tag));
    }
    let builder = EventBuilder::new(Kind::Custom(KIND_PIN), input.content).tags(tags);
    let output = client
        .send_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to publish pin: {}", e))?;
    let event_id = output.id().to_hex();
    log::info!("Pin published: {}", event_id);
    Ok(event_id)
}
/// Delete a pin
pub async fn delete_pin(pin_event_id: &str) -> std::result::Result<(), String> {
    let client = nostr_client::get_client().ok_or("Client not initialized")?;
    if !*nostr_client::HAS_SIGNER.read() {
        return Err("No signer attached. Cannot delete pin.".to_string());
    }
    let event_id = EventId::from_hex(pin_event_id)
        .map_err(|e| format!("Invalid event ID: {}", e))?;
    let deletion_request = EventDeletionRequest::new().id(event_id);
    let builder = EventBuilder::delete(deletion_request);
    client
        .send_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to delete pin: {}", e))?;
    remove_pin_from_cache(pin_event_id);
    log::info!("Pin deleted: {}", pin_event_id);
    Ok(())
}
/// Delete a pinboard
pub async fn delete_pinboard(board: &Pinboard) -> std::result::Result<String, String> {
    let client = nostr_client::get_client().ok_or("Client not initialized")?;
    if !*nostr_client::HAS_SIGNER.read() {
        return Err("No signer attached. Cannot delete pinboard.".to_string());
    }
    let current_pubkey = crate::stores::auth_store::get_pubkey().ok_or("Not logged in")?;
    if board.pubkey != current_pubkey {
        return Err("You can only delete your own pinboards".to_string());
    }
    let coord = Coordinate::new(Kind::Custom(KIND_PINBOARD), board.event.pubkey)
        .identifier(&board.d_tag);
    let deletion_request = EventDeletionRequest::new().coordinate(coord);
    let builder = EventBuilder::delete(deletion_request);
    let output = client
        .send_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to delete pinboard: {}", e))?;
    remove_pinboard_from_cache(&board.a_tag);
    log::info!("Pinboard deleted: {}", output.id().to_hex());
    Ok(output.id().to_hex())
}
/// Update pinboard metadata
pub async fn update_pinboard_metadata(
    naddr: &str,
    title: Option<String>,
    description: Option<String>,
    image: Option<String>,
    tags: Option<Vec<String>>,
) -> std::result::Result<String, String> {
    let board = fetch_pinboard_by_naddr(naddr).await?.ok_or("Pinboard not found")?;
    let current_pubkey = crate::stores::auth_store::get_pubkey().ok_or("Not logged in")?;
    if board.pubkey != current_pubkey {
        return Err("You can only edit your own pinboards".to_string());
    }
    let input = PinboardInput {
        title: title.unwrap_or(board.title.clone()),
        description: description.or(board.description.clone()),
        image: image.or(board.image.clone()),
        tags: tags.unwrap_or(board.tags.clone()),
        collaborative: board.collaborative,
    };
    publish_pinboard(input, Some(&board.d_tag)).await
}
/// Represents a reaction on a pinboard or pin
#[derive(Clone, Debug)]
pub struct BoardReaction {
    pub event_id: String,
    pub pubkey: String,
    pub content: String,
    pub created_at: u64,
}
/// Represents a zap receipt
#[derive(Clone, Debug)]
pub struct BoardZap {
    pub event_id: String,
    pub sender_pubkey: Option<String>,
    pub amount_msats: u64,
    pub comment: Option<String>,
    pub created_at: u64,
}
/// Fetch reactions for a pinboard
pub async fn fetch_pinboard_reactions(
    a_tag: &str,
) -> std::result::Result<Vec<BoardReaction>, String> {
    let client = nostr_client::get_client().ok_or("Nostr client not initialized")?;
    let filter = pinboard_reactions_filter(a_tag, 500);
    let events = client
        .fetch_events(filter, Duration::from_secs(10))
        .await
        .map_err(|e| format!("Failed to fetch reactions: {}", e))?;
    let reactions: Vec<BoardReaction> = events
        .iter()
        .map(|event| BoardReaction {
            event_id: event.id.to_hex(),
            pubkey: event.pubkey.to_hex(),
            content: event.content.clone(),
            created_at: event.created_at.as_secs(),
        })
        .collect();
    Ok(reactions)
}
/// Fetch zap receipts for a pinboard
pub async fn fetch_pinboard_zaps(
    a_tag: &str,
) -> std::result::Result<Vec<BoardZap>, String> {
    let client = nostr_client::get_client().ok_or("Nostr client not initialized")?;
    let filter = pinboard_zaps_filter(a_tag, 500);
    let events = client
        .fetch_events(filter, Duration::from_secs(10))
        .await
        .map_err(|e| format!("Failed to fetch zaps: {}", e))?;
    let zaps: Vec<BoardZap> = events
        .iter()
        .filter_map(|event| {
            let amount_msats = extract_zap_amount(event);
            if amount_msats == 0 {
                return None;
            }
            let sender_pubkey = extract_zap_sender(event);
            let comment = extract_zap_comment(event);
            Some(BoardZap {
                event_id: event.id.to_hex(),
                sender_pubkey,
                amount_msats,
                comment,
                created_at: event.created_at.as_secs(),
            })
        })
        .collect();
    Ok(zaps)
}
/// Calculate total zap amount in sats for a pinboard
pub async fn fetch_pinboard_zap_total(a_tag: &str) -> std::result::Result<u64, String> {
    let zaps = fetch_pinboard_zaps(a_tag).await?;
    let total_msats: u64 = zaps.iter().map(|z| z.amount_msats).sum();
    Ok(total_msats / 1000)
}
/// Count reactions for a pinboard
pub async fn fetch_pinboard_reaction_count(
    a_tag: &str,
) -> std::result::Result<usize, String> {
    let reactions = fetch_pinboard_reactions(a_tag).await?;
    Ok(reactions.len())
}
/// Check if current user has reacted to a pinboard
pub async fn has_user_reacted_to_pinboard(
    a_tag: &str,
) -> std::result::Result<bool, String> {
    let current_pubkey = crate::stores::auth_store::get_pubkey().ok_or("Not logged in")?;
    let reactions = fetch_pinboard_reactions(a_tag).await?;
    Ok(reactions.iter().any(|r| r.pubkey == current_pubkey))
}
/// Toggle reaction on a pinboard (add or remove)
pub async fn toggle_pinboard_reaction(
    board: &Pinboard,
    content: &str,
) -> std::result::Result<bool, String> {
    let client = nostr_client::get_client().ok_or("Nostr client not initialized")?;
    let current_pubkey = crate::stores::auth_store::get_pubkey().ok_or("Not logged in")?;
    let reactions = fetch_pinboard_reactions(&board.a_tag).await?;
    let existing_reaction = reactions.iter().find(|r| r.pubkey == current_pubkey);
    if let Some(reaction) = existing_reaction {
        let event_id = EventId::from_hex(&reaction.event_id)
            .map_err(|e| format!("Invalid event ID: {}", e))?;
        let deletion_request = EventDeletionRequest::new().id(event_id);
        let builder = EventBuilder::delete(deletion_request);
        client
            .send_event_builder(builder)
            .await
            .map_err(|e| format!("Failed to delete reaction: {}", e))?;
        Ok(false)
    } else {
        let author_pubkey = PublicKey::from_hex(&board.pubkey)
            .map_err(|e| format!("Invalid author pubkey: {}", e))?;
        let tags = vec![
            Tag::from_standardized(TagStandard::Coordinate {
                coordinate: Coordinate::new(Kind::Custom(KIND_PINBOARD), author_pubkey)
                    .identifier(&board.d_tag),
                relay_url: None,
                uppercase: false,
            }),
            Tag::public_key(author_pubkey),
        ];
        let builder = EventBuilder::new(Kind::Reaction, content).tags(tags);
        client
            .send_event_builder(builder)
            .await
            .map_err(|e| format!("Failed to send reaction: {}", e))?;
        Ok(true)
    }
}
/// Extract zap amount from a zap receipt event
fn extract_zap_amount(event: &NostrEvent) -> u64 {
    for tag in event.tags.iter() {
        if tag.kind() == TagKind::Custom("bolt11".into()) {
            if let Some(bolt11) = tag.content() {
                if let Some(amount) = parse_bolt11_amount(bolt11) {
                    return amount;
                }
            }
        }
        if tag.kind() == TagKind::Description {
            if let Some(desc) = tag.content() {
                if let Ok(zap_request) = serde_json::from_str::<
                    serde_json::Value,
                >(desc) {
                    if let Some(amount) = zap_request
                        .get("amount")
                        .and_then(|a| a.as_u64())
                    {
                        return amount;
                    }
                }
            }
        }
    }
    0
}
/// Parse amount from bolt11 invoice string (returns msats)
fn parse_bolt11_amount(bolt11: &str) -> Option<u64> {
    let lower = bolt11.to_lowercase();
    if !lower.starts_with("lnbc") && !lower.starts_with("lntb") {
        return None;
    }
    let prefix_len = 4;
    let rest = &lower[prefix_len..];
    let mut amount_end = 0;
    let mut multiplier_char = None;
    for (i, c) in rest.chars().enumerate() {
        if c.is_ascii_digit() {
            amount_end = i + 1;
        } else if ['m', 'u', 'n', 'p'].contains(&c) {
            multiplier_char = Some(c);
            amount_end = i;
            break;
        } else {
            amount_end = i;
            break;
        }
    }
    if amount_end == 0 {
        return None;
    }
    let amount_str = &rest[..amount_end];
    let amount: u64 = amount_str.parse().ok()?;
    let msats = match multiplier_char {
        Some('m') => amount * 100_000_000,
        Some('u') => amount * 100_000,
        Some('n') => amount * 100,
        Some('p') => amount / 10,
        Some(_) => return None,
        None => amount * 100_000_000_000,
    };
    Some(msats)
}
/// Extract sender pubkey from zap receipt
fn extract_zap_sender(event: &NostrEvent) -> Option<String> {
    for tag in event.tags.iter() {
        if tag.kind() == TagKind::Description {
            if let Some(desc) = tag.content() {
                if let Ok(zap_request) = serde_json::from_str::<
                    serde_json::Value,
                >(desc) {
                    if let Some(pubkey) = zap_request
                        .get("pubkey")
                        .and_then(|p| p.as_str())
                    {
                        return Some(pubkey.to_string());
                    }
                }
            }
        }
    }
    None
}
/// Extract comment from zap receipt
fn extract_zap_comment(event: &NostrEvent) -> Option<String> {
    for tag in event.tags.iter() {
        if tag.kind() == TagKind::Description {
            if let Some(desc) = tag.content() {
                if let Ok(zap_request) = serde_json::from_str::<
                    serde_json::Value,
                >(desc) {
                    if let Some(content) = zap_request
                        .get("content")
                        .and_then(|c| c.as_str())
                    {
                        if !content.is_empty() {
                            return Some(content.to_string());
                        }
                    }
                }
            }
        }
    }
    None
}
/// Alias for backwards compatibility during migration
pub type PinBoard = Pinboard;
pub type PinBoardInput = PinboardInput;
pub type PinBoardContentType = PinContentType;
/// Alias for old fetch function name
pub async fn fetch_pin_boards(
    limit: usize,
) -> std::result::Result<Vec<Pinboard>, String> {
    fetch_pinboards(limit).await
}
/// Alias for old fetch function name
pub async fn fetch_board_by_naddr(
    naddr: &str,
) -> std::result::Result<Option<Pinboard>, String> {
    fetch_pinboard_by_naddr(naddr).await
}
/// Alias for old fetch function name
pub async fn fetch_my_boards() -> std::result::Result<Vec<Pinboard>, String> {
    fetch_my_pinboards().await
}
/// Alias for old cache function
pub fn get_cached_board(a_tag: &str) -> Option<Pinboard> {
    get_cached_pinboard(a_tag)
}
/// Alias for old global signals
pub static PIN_BOARDS_CACHE: GlobalSignal<LruCache<String, Pinboard>> = GlobalSignal::new(||
LruCache::new(NonZeroUsize::new(PINBOARD_CACHE_SIZE).unwrap()));
pub static LOADING_PIN_BOARDS: GlobalSignal<bool> = GlobalSignal::new(|| false);
pub static PIN_BOARDS_INITIALIZED: GlobalSignal<bool> = GlobalSignal::new(|| false);
/// Get a shareable naddr with relay hints for a pinboard
/// Per NIP-19, relay hints help other clients locate the event
pub async fn get_shareable_naddr(
    board: &Pinboard,
) -> std::result::Result<String, String> {
    let pubkey = nostr::PublicKey::from_hex(&board.pubkey)
        .map_err(|e| format!("Invalid pubkey: {}", e))?;
    nostr_client::make_naddr_with_hints(KIND_PINBOARD, &pubkey, &board.d_tag).await
}
