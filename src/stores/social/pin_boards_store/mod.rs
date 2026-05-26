//! Pin Boards Store
//! Implements the Pinboards NIP with two-event architecture:
//! - Kind 30067: Pinboard Set (board metadata only)
//! - Kind 39067: Pin (individual content references)
//!
//! ## Submodules
//! - `fetch`: All async fetch, subscribe, and query functions
//! - `publish`: All publish, create, update, delete functions
#![allow(dead_code)]
#![allow(unused_imports)]
mod fetch;
mod publish;
use crate::stores::nostr_client;
use crate::utils::nip73::ExternalContentId;
use dioxus::prelude::*;
pub use fetch::*;
use lru::LruCache;
use nostr::Event as NostrEvent;
use nostr_sdk::prelude::*;
pub use publish::*;
use serde::{Deserialize, Serialize};
use std::num::NonZeroUsize;
use std::time::Duration;
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
        if self.title.is_empty() {
            &self.d_tag
        } else {
            &self.title
        }
    }
}
/// Pin reference - what content is being pinned
#[derive(Clone, Debug, PartialEq)]
pub enum PinReference {
    /// Reference to a Nostr event by ID (e tag)
    Event {
        id: String,
        relay_hint: Option<String>,
    },
    /// Reference to a parameterized replaceable event (a tag with non-30067 kind)
    Coordinate {
        address: String,
        relay_hint: Option<String>,
    },
    /// Reference to external content via NIP-73 (i tag + k tag)
    External {
        content: ExternalContentId,
        hint: Option<String>,
    },
}
impl PinReference {
    /// Infer content type from the reference
    pub fn infer_content_type(&self) -> PinContentType {
        match self {
            PinReference::External { content, .. } => match content {
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
            },
            PinReference::Event { .. } => PinContentType::Note,
            PinReference::Coordinate { address, .. } => {
                let kind_opt = if address.starts_with("naddr1") {
                    Coordinate::from_bech32(address)
                        .ok()
                        .map(|c| c.kind.as_u16() as u32)
                } else {
                    address
                        .split(':')
                        .next()
                        .and_then(|s| s.parse::<u32>().ok())
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
/// Alias for backwards compatibility during migration
pub type PinBoard = Pinboard;
pub type PinBoardInput = PinboardInput;
pub type PinBoardContentType = PinContentType;
/// Pinboards cache (keyed by a_tag)
pub static PINBOARDS_CACHE: GlobalSignal<LruCache<String, Pinboard>> =
    GlobalSignal::new(|| LruCache::new(NonZeroUsize::new(PINBOARD_CACHE_SIZE).unwrap()));
/// Pins cache (keyed by event_id)
pub static PINS_CACHE: GlobalSignal<LruCache<String, Pin>> =
    GlobalSignal::new(|| LruCache::new(NonZeroUsize::new(PIN_CACHE_SIZE).unwrap()));
/// Loading state
pub static LOADING_PINBOARDS: GlobalSignal<bool> = GlobalSignal::new(|| false);
/// Store initialization state
pub static PINBOARDS_INITIALIZED: GlobalSignal<bool> = GlobalSignal::new(|| false);
/// Alias for old global signals
pub static PIN_BOARDS_CACHE: GlobalSignal<LruCache<String, Pinboard>> =
    GlobalSignal::new(|| LruCache::new(NonZeroUsize::new(PINBOARD_CACHE_SIZE).unwrap()));
pub static LOADING_PIN_BOARDS: GlobalSignal<bool> = GlobalSignal::new(|| false);
pub static PIN_BOARDS_INITIALIZED: GlobalSignal<bool> = GlobalSignal::new(|| false);
/// Get a pinboard from cache by a_tag
pub fn get_cached_pinboard(a_tag: &str) -> Option<Pinboard> {
    PINBOARDS_CACHE.read().peek(a_tag).cloned()
}
/// Get a pinboard from cache by naddr
pub fn get_cached_pinboard_by_naddr(naddr: &str) -> Option<Pinboard> {
    let cache = PINBOARDS_CACHE.read();
    cache
        .iter()
        .find(|(_, board)| board.naddr == naddr)
        .map(|(_, board)| board.clone())
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
/// Alias for old cache function
pub fn get_cached_board(a_tag: &str) -> Option<Pinboard> {
    get_cached_pinboard(a_tag)
}
/// Check if URL is an image
fn is_image_url(url: &str) -> bool {
    let extensions = [
        ".jpg", ".jpeg", ".png", ".gif", ".webp", ".svg", ".bmp", ".ico",
    ];
    extensions.iter().any(|ext| url.ends_with(ext)) || url.contains("image") || url.contains("/i/")
}
/// Check if URL is a video
fn is_video_url(url: &str) -> bool {
    let extensions = [
        ".mp4", ".m4v", ".webm", ".mov", ".avi", ".mkv", ".ogg", ".ogv", ".3gp", ".3gpp", ".flv",
    ];
    extensions.iter().any(|ext| url.ends_with(ext))
        || url.contains("youtube.com")
        || url.contains("youtu.be")
        || url.contains("vimeo.com")
}
/// Extract a tag value by tag name
pub(crate) fn extract_tag_value(tags: &Tags, tag_name: &str) -> Option<String> {
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
pub(crate) fn extract_all_tag_values(tags: &Tags, tag_name: &str) -> Vec<String> {
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
        .find(|t| t.kind() == TagKind::SingleLetter(SingleLetterTag::lowercase(Alphabet::D)))
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
        if let Some(TagStandard::ExternalContent { content, hint, .. }) = tag.as_standardized() {
            external_content = Some((content.clone(), hint.as_ref().map(|u| u.to_string())));
        }
    }
    let reference = if let Some((id, relay_hint)) = event_ref {
        PinReference::Event { id, relay_hint }
    } else if let Some((address, relay_hint)) = content_coordinate {
        PinReference::Coordinate {
            address,
            relay_hint,
        }
    } else if let Some((content, hint)) = external_content {
        PinReference::External { content, hint }
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
/// Extract metadata from an event based on its kind
pub(crate) fn extract_event_metadata(event: &NostrEvent, kind: u16) -> PinMetadata {
    let tags = &event.tags;
    let title = extract_tag_value(tags, "title");
    let image = extract_tag_value(tags, "image");
    let summary = extract_tag_value(tags, "summary");
    let content_type = match kind {
        30023 => {
            if tags
                .hashtags()
                .any(|tag| tag == crate::utils::recipe::RECIPE_TAG_PREFIX)
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
/// Build a filter for fetching pinboards
pub fn pinboards_filter(limit: usize) -> Filter {
    Filter::new().kind(Kind::Custom(KIND_PINBOARD)).limit(limit)
}
/// Build a filter for a specific pinboard by coordinate
pub fn pinboard_by_coord_filter(pubkey: PublicKey, identifier: &str) -> Filter {
    Filter::new()
        .kind(Kind::Custom(KIND_PINBOARD))
        .author(pubkey)
        .identifier(identifier)
}
/// Build a filter for pinboards by author
pub fn pinboards_by_author_filter(pubkey: PublicKey, limit: usize) -> Filter {
    Filter::new()
        .kind(Kind::Custom(KIND_PINBOARD))
        .author(pubkey)
        .limit(limit)
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
    Filter::new()
        .kind(Kind::Custom(KIND_PINBOARD))
        .hashtag(hashtag)
        .limit(limit)
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
    Filter::new()
        .kind(Kind::Custom(KIND_PIN))
        .author(pubkey)
        .limit(limit)
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
