//! NIP-53 Live Activities Parsing Utilities
//!
//! Handles parsing of live activity events:
//! - Kind 30312: Meeting Space Event (addressable venue/room)
//! - Kind 30313: Meeting Room Event (addressable scheduled meeting)
//! - Kind 10312: Room Presence (replaceable presence indicator)
//!
//! Note: Kind 30311 (Live Streaming) is handled separately via nostr-sdk's LiveEvent parser
//! in the live stream components at /videos/live.
//!
//! NIP-53 defines a standard for live activities on Nostr.
use nostr_sdk::prelude::*;
use serde::{Deserialize, Serialize};
/// A room is considered live if a presence event was seen within this window.
/// 10 minutes — matches nostrnests `STALE_PRESENCE_SECONDS`
/// and the conventional presence-freshness window.
/// Speakers heartbeat every ~60-120s, so a 10-min
/// window tolerates several missed heartbeats before a room is considered dead.
const PRESENCE_LIVE_THRESHOLD_SECS: u64 = 600;

/// An "open" room whose `created_at` is older than this and has no recent
/// presence flips to Ended. 10 minutes — the standard "new room" grace so a
/// freshly-created room surfaces as Live before its first speaker heartbeat
/// arrives (ecosystem convention).
const ROOM_STALE_THRESHOLD_SECS: u64 = 600;
/// Live streaming event (audio/video streams, audio spaces)
/// Note: Used at /videos/live via nostr-sdk's LiveEvent parser
#[allow(dead_code)]
pub const KIND_LIVE_STREAM: u16 = 30311;
/// Meeting space event (container/venue for meetings)
pub const KIND_MEETING_SPACE: u16 = 30312;
/// Meeting room event (individual scheduled meeting)
pub const KIND_MEETING_ROOM: u16 = 30313;
/// Room presence indicator (for tracking who's in a meeting)
pub const KIND_ROOM_PRESENCE: u16 = 10312;
/// Live activity status
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum LiveStatus {
    /// Scheduled but not started
    #[default]
    Planned,
    /// Currently live
    Live,
    /// Activity has ended
    Ended,
}
impl LiveStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            LiveStatus::Planned => "planned",
            LiveStatus::Live => "live",
            LiveStatus::Ended => "ended",
        }
    }
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "planned" => Some(LiveStatus::Planned),
            "live" => Some(LiveStatus::Live),
            "ended" => Some(LiveStatus::Ended),
            _ => None,
        }
    }
}
impl std::fmt::Display for LiveStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}
/// Room accessibility status
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum RoomStatus {
    #[default]
    Open,
    Private,
    Closed,
}
impl RoomStatus {
    /// Wire value emitted on kind 30312 `status` tags.
    ///
    /// NIP-53's spec text lists `open`/`private`/`closed`, but the deployed
    /// The live-audio ecosystem canonically publishes
    /// `planned`/`live`/`ended` (the values NIP-53 defines for kinds
    /// 30311/30313). We emit the deployed values for cross-client
    /// interoperability; `from_str` accepts both sets.
    pub fn as_str(&self) -> &'static str {
        match self {
            RoomStatus::Open => "live",
            RoomStatus::Private => "planned",
            RoomStatus::Closed => "ended",
        }
    }
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            // "live" is not a NIP-53 `status` value (only open/private/closed),
            // but several nest hosts (e.g. nostrnests.com) publish `status=live`
            // on kind 30312 to signal an active room. Coerce it to Open so the
            // room surfaces as joinable instead of falling through to the
            // default and risking a wrong bucket.
            "live" | "open" => Some(RoomStatus::Open),
            // Live-audio hosts publish `planned` for scheduled
            // rooms; map it onto Private (our scheduled representation) so it
            // buckets as Planned instead of defaulting to Open and aging out
            // to Ended.
            "private" | "planned" => Some(RoomStatus::Private),
            "closed" | "ended" => Some(RoomStatus::Closed),
            _ => None,
        }
    }
}
impl std::fmt::Display for RoomStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}
/// Participant in a live activity
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LiveParticipant {
    /// Participant pubkey (hex)
    pub pubkey: String,
    /// Relay hint for the participant
    pub relay_hint: Option<String>,
    /// Role: Host, Speaker, Participant, Moderator, Owner
    pub role: Option<String>,
    /// Proof of agreement (signed SHA256 of a-tag)
    pub proof: Option<String>,
}
/// Meeting Space parsed from Kind 30312 events
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MeetingSpace {
    pub d_tag: String,
    pub event_id: String,
    pub pubkey: String,
    pub naddr: String,
    pub coordinate: String,
    pub created_at: u64,
    /// Room name
    pub room_name: String,
    /// Room description
    pub summary: Option<String>,
    /// Room image
    pub image: Option<String>,
    /// Room status (open/private/closed)
    pub status: RoomStatus,
    /// URL to access the room
    pub service_url: String,
    /// API endpoint for status/info
    pub endpoint_url: Option<String>,
    /// Recording URL (post-room recording)
    pub recording: Option<String>,
    /// Scheduled start time (unix seconds). Optional — NIP-53 lists `starts`
    /// only for kinds 30311/30313, but live-audio room events and the
    /// production nostrnests relays accept it on 30312 as a community
    /// extension for scheduled rooms. The builder
    /// (`add_meeting_space_optional_tags`) already emits it; the parser
    /// stores it back so round-trip works. Candidate for NKBIP-09.
    pub starts: Option<u64>,
    pub hashtags: Vec<String>,
    pub relays: Vec<String>,
    pub providers: Vec<LiveParticipant>,
}
/// Meeting Room Event parsed from Kind 30313 events
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MeetingRoomEvent {
    pub d_tag: String,
    pub event_id: String,
    pub pubkey: String,
    pub naddr: String,
    pub coordinate: String,
    pub created_at: u64,
    /// Meeting title
    pub title: String,
    /// Meeting description
    pub summary: Option<String>,
    /// Meeting image
    pub image: Option<String>,
    /// Full content
    pub content: String,
    /// Start timestamp (required)
    pub starts: u64,
    /// End timestamp (optional)
    pub ends: Option<u64>,
    /// Meeting status
    pub status: LiveStatus,
    /// Reference to parent space (30312 coordinate)
    pub space_coordinate: Option<String>,
    pub current_participants: Option<u32>,
    pub total_participants: Option<u32>,
    pub participants: Vec<LiveParticipant>,
    pub hashtags: Vec<String>,
}
impl MeetingRoomEvent {
    /// Check if currently live
    pub fn is_live(&self) -> bool {
        self.status == LiveStatus::Live
    }
    /// Get start timestamp for sorting
    pub fn start_timestamp(&self) -> u64 {
        self.starts
    }
    /// Get end timestamp
    pub fn end_timestamp(&self) -> Option<u64> {
        self.ends
    }
}
/// Room Presence parsed from Kind 10312 events
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RoomPresence {
    pub event_id: String,
    pub pubkey: String,
    pub created_at: u64,
    /// Room coordinate (a-tag)
    pub room_coordinate: String,
    /// Hand raised flag
    pub hand_raised: bool,
    /// Microphone muted
    pub muted: bool,
    /// Actively publishing audio
    pub publishing: bool,
    /// On stage (speaker slot)
    pub onstage: bool,
}
/// Nests server entry from Kind 10112 events
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NestsServer {
    pub relay_url: String,
    pub auth_url: String,
}
/// Parse a Kind 30312 event into a MeetingSpace
pub fn parse_meeting_space(event: &Event) -> Result<MeetingSpace, String> {
    if event.kind.as_u16() != KIND_MEETING_SPACE {
        return Err(format!(
            "Expected kind {}, got {}",
            KIND_MEETING_SPACE,
            event.kind.as_u16()
        ));
    }
    let pubkey = event.pubkey.to_hex();
    let d_tag = get_tag_value(event, "d").ok_or("Missing required 'd' tag")?;
    let coordinate = format!("{}:{}:{}", KIND_MEETING_SPACE, pubkey, d_tag);
    let naddr = build_naddr(KIND_MEETING_SPACE, &pubkey, &d_tag).unwrap_or_default();
    let room_name = get_tag_value(event, "title")
        .or_else(|| get_tag_value(event, "room"))
        .ok_or("Missing required 'title'/'room' tag")?;
    let summary = get_tag_value(event, "summary");
    let image = get_tag_value(event, "image");
    let status = get_tag_value(event, "status")
        .and_then(|s| RoomStatus::from_str(&s))
        .unwrap_or_default();
    let service_url = get_tag_value(event, "auth")
        .or_else(|| get_tag_value(event, "service"))
        .ok_or("Missing required 'auth'/'service' tag")?;
    let endpoint_url = get_tag_value(event, "streaming")
        .or_else(|| get_tag_value(event, "endpoint"));
    let recording = get_tag_value(event, "recording");
    let starts = get_tag_value(event, "starts").and_then(|s| s.parse::<u64>().ok());
    let hashtags = get_all_tag_values(event, "t");
    let relays = get_relay_tag_values(event);
    let providers = parse_participants(event);
    Ok(MeetingSpace {
        d_tag,
        event_id: event.id.to_hex(),
        pubkey,
        naddr,
        coordinate,
        created_at: event.created_at.as_secs(),
        room_name,
        summary,
        image,
        status,
        service_url,
        endpoint_url,
        recording,
        starts,
        hashtags,
        relays,
        providers,
    })
}
/// Parse a Kind 30313 event into a MeetingRoomEvent
pub fn parse_meeting_room_event(event: &Event) -> Result<MeetingRoomEvent, String> {
    if event.kind.as_u16() != KIND_MEETING_ROOM {
        return Err(format!(
            "Expected kind {}, got {}",
            KIND_MEETING_ROOM,
            event.kind.as_u16()
        ));
    }
    let pubkey = event.pubkey.to_hex();
    let d_tag = get_tag_value(event, "d").ok_or("Missing required 'd' tag")?;
    let coordinate = format!("{}:{}:{}", KIND_MEETING_ROOM, pubkey, d_tag);
    let naddr = build_naddr(KIND_MEETING_ROOM, &pubkey, &d_tag).unwrap_or_default();
    let title = get_tag_value(event, "title").ok_or("Missing required 'title' tag")?;
    let summary = get_tag_value(event, "summary");
    let image = get_tag_value(event, "image");
    let starts = get_tag_value(event, "starts")
        .and_then(|s| s.parse::<u64>().ok())
        .ok_or("Missing required 'starts' tag")?;
    let ends = get_tag_value(event, "ends").and_then(|s| s.parse::<u64>().ok());
    let status = get_tag_value(event, "status")
        .and_then(|s| LiveStatus::from_str(&s))
        .ok_or("Missing required 'status' tag")?;
    let space_coordinate = get_coordinate_tags(event)
        .into_iter()
        .find(|c| c.starts_with("30312:"));
    let current_participants =
        get_tag_value(event, "current_participants").and_then(|s| s.parse::<u32>().ok());
    let total_participants =
        get_tag_value(event, "total_participants").and_then(|s| s.parse::<u32>().ok());
    let participants = parse_participants(event);
    let hashtags = get_all_tag_values(event, "t");
    Ok(MeetingRoomEvent {
        d_tag,
        event_id: event.id.to_hex(),
        pubkey,
        naddr,
        coordinate,
        created_at: event.created_at.as_secs(),
        title,
        summary,
        image,
        content: event.content.clone(),
        starts,
        ends,
        status,
        space_coordinate,
        current_participants,
        total_participants,
        participants,
        hashtags,
    })
}
/// Parse a Kind 10312 event into a RoomPresence
pub fn parse_room_presence(event: &Event) -> Result<RoomPresence, String> {
    if event.kind.as_u16() != KIND_ROOM_PRESENCE {
        return Err(format!(
            "Expected kind {}, got {}",
            KIND_ROOM_PRESENCE,
            event.kind.as_u16()
        ));
    }
    let room_coordinate = get_coordinate_tags(event)
        .into_iter()
        .next()
        .ok_or("Missing required 'a' tag (room coordinate)")?;
    let hand_raised = get_tag_value(event, "hand")
        .map(|s| s == "1" || s.to_lowercase() == "true")
        .unwrap_or(false);
    let muted = get_tag_value(event, "muted")
        .map(|s| s == "1" || s.to_lowercase() == "true")
        .unwrap_or(false);
    let publishing = get_tag_value(event, "publishing")
        .map(|s| s == "1" || s.to_lowercase() == "true")
        .unwrap_or(false);
    let onstage = get_tag_value(event, "onstage")
        .map(|s| s == "1" || s.to_lowercase() == "true")
        .unwrap_or(false);
    Ok(RoomPresence {
        event_id: event.id.to_hex(),
        pubkey: event.pubkey.to_hex(),
        created_at: event.created_at.as_secs(),
        room_coordinate,
        hand_raised,
        muted,
        publishing,
        onstage,
    })
}
/// Get a tag value by name (first parameter after tag name)
fn get_tag_value(event: &Event, tag_name: &str) -> Option<String> {
    event
        .tags
        .iter()
        .find(|t| t.as_slice().first().map(|s| s.as_str()) == Some(tag_name))
        .and_then(|t| t.as_slice().get(1).map(|s| s.to_string()))
}
/// Get all values for tags with given name
fn get_all_tag_values(event: &Event, tag_name: &str) -> Vec<String> {
    event
        .tags
        .iter()
        .filter(|t| t.as_slice().first().map(|s| s.as_str()) == Some(tag_name))
        .filter_map(|t| t.as_slice().get(1).map(|s| s.to_string()))
        .collect()
}
/// Get all coordinate (a-tag) values
fn get_coordinate_tags(event: &Event) -> Vec<String> {
    event
        .tags
        .iter()
        .filter(|t| t.as_slice().first().map(|s| s.as_str()) == Some("a"))
        .filter_map(|t| t.as_slice().get(1).map(|s| s.to_string()))
        .collect()
}
/// Get relay URLs from relays tag: ["relays", url1, url2, ...]
fn get_relay_tag_values(event: &Event) -> Vec<String> {
    event
        .tags
        .iter()
        .find(|t| t.as_slice().first().map(|s| s.as_str()) == Some("relays"))
        .map(|t| t.as_slice().iter().skip(1).map(|s| s.to_string()).collect())
        .unwrap_or_default()
}
/// Parse participants from p-tags: ["p", pubkey, relay_hint?, role?, proof?]
fn parse_participants(event: &Event) -> Vec<LiveParticipant> {
    event
        .tags
        .iter()
        .filter(|t| t.as_slice().first().map(|s| s.as_str()) == Some("p"))
        .filter_map(|t| {
            let slice = t.as_slice();
            let pubkey = slice.get(1)?.to_string();
            let relay_hint = slice
                .get(2)
                .map(|s| s.to_string())
                .filter(|s| !s.is_empty());
            let role = slice
                .get(3)
                .map(|s| s.to_string())
                .filter(|s| !s.is_empty());
            let proof = slice
                .get(4)
                .map(|s| s.to_string())
                .filter(|s| !s.is_empty());
            Some(LiveParticipant {
                pubkey,
                relay_hint,
                role,
                proof,
            })
        })
        .collect()
}
/// Build naddr from kind, pubkey, and d-tag
fn build_naddr(kind: u16, pubkey: &str, d_tag: &str) -> Option<String> {
    let pk = PublicKey::from_hex(pubkey).ok()?;
    let coordinate = Coordinate::new(Kind::Custom(kind), pk).identifier(d_tag);
    let nip19 = Nip19Coordinate::new(coordinate, vec![]);
    nip19.to_bech32().ok()
}
/// Unified type for display in events page (combines meeting types)
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum LiveActivityEvent {
    Meeting(MeetingRoomEvent),
    Space(MeetingSpace),
}
impl LiveActivityEvent {
    pub fn title(&self) -> &str {
        match self {
            LiveActivityEvent::Meeting(e) => &e.title,
            LiveActivityEvent::Space(e) => &e.room_name,
        }
    }
    pub fn pubkey(&self) -> &str {
        match self {
            LiveActivityEvent::Meeting(e) => &e.pubkey,
            LiveActivityEvent::Space(e) => &e.pubkey,
        }
    }
    pub fn coordinate(&self) -> &str {
        match self {
            LiveActivityEvent::Meeting(e) => &e.coordinate,
            LiveActivityEvent::Space(e) => &e.coordinate,
        }
    }
    pub fn naddr(&self) -> &str {
        match self {
            LiveActivityEvent::Meeting(e) => &e.naddr,
            LiveActivityEvent::Space(e) => &e.naddr,
        }
    }
    pub fn start_timestamp(&self) -> u64 {
        match self {
            LiveActivityEvent::Meeting(e) => e.start_timestamp(),
            LiveActivityEvent::Space(e) => e.created_at,
        }
    }
    pub fn end_timestamp(&self) -> Option<u64> {
        match self {
            LiveActivityEvent::Meeting(e) => e.end_timestamp(),
            LiveActivityEvent::Space(_) => None,
        }
    }
    pub fn image(&self) -> Option<&str> {
        match self {
            LiveActivityEvent::Meeting(e) => e.image.as_deref(),
            LiveActivityEvent::Space(e) => e.image.as_deref(),
        }
    }
    pub fn is_live(&self) -> bool {
        match self {
            LiveActivityEvent::Meeting(e) => e.is_live(),
            LiveActivityEvent::Space(e) => e.status == RoomStatus::Open,
        }
    }
    pub fn status(&self) -> LiveStatus {
        match self {
            LiveActivityEvent::Meeting(e) => e.status,
            LiveActivityEvent::Space(e) => match e.status {
                RoomStatus::Open => LiveStatus::Live,
                RoomStatus::Private => LiveStatus::Planned,
                RoomStatus::Closed => LiveStatus::Ended,
            },
        }
    }
    pub fn hashtags(&self) -> &[String] {
        match self {
            LiveActivityEvent::Meeting(e) => &e.hashtags,
            LiveActivityEvent::Space(e) => &e.hashtags,
        }
    }
    /// Get location string (if any)
    /// Meetings are typically online, so they don't have physical locations
    pub fn location(&self) -> Option<&str> {
        None
    }
    /// Get geohash (if any)
    /// Meetings are typically online, so they don't have geohashes
    pub fn geohash(&self) -> Option<&str> {
        None
    }
    /// Get the URL to join this activity
    pub fn join_url(&self) -> Option<&str> {
        match self {
            LiveActivityEvent::Meeting(_) => None,
            LiveActivityEvent::Space(e) => Some(&e.service_url),
        }
    }
    /// Get description/summary
    pub fn summary(&self) -> Option<&str> {
        match self {
            LiveActivityEvent::Meeting(e) => e.summary.as_deref(),
            LiveActivityEvent::Space(e) => e.summary.as_deref(),
        }
    }
}
use nostr_sdk::nips::nip53::LiveEvent;
use nostr_sdk::secp256k1::schnorr::Signature;
use std::str::FromStr;
/// Extracted host information with verification status
#[derive(Clone, Debug)]
pub struct ParsedLiveHost {
    /// Host's public key (hex string)
    pub public_key: String,
    /// Relay URL hint for the host
    #[allow(dead_code)]
    pub relay_url: Option<String>,
    /// Proof signature (hex string) - used during verification, stored for display/debugging
    #[allow(dead_code)]
    pub proof: Option<String>,
    /// Whether the proof signature is valid per NIP-53
    pub is_verified: bool,
}
/// Parse a NIP-53 Kind 30311 live streaming event into a LiveEvent struct.
/// This is the canonical parser used by all live stream components.
///
/// Returns `Some(LiveEvent)` on successful parse, `None` on failure (with warning logged).
pub fn parse_nip53_live_event(event: &Event) -> Option<LiveEvent> {
    match LiveEvent::try_from(event.tags.clone().to_vec()) {
        Ok(le) => Some(le),
        Err(e) => {
            log::warn!("Failed to parse LiveEvent from tags: {}", e);
            None
        }
    }
}
/// Extract host from live event with case-insensitive fallback.
///
/// The nostr-sdk LiveEventMarker parser is case-sensitive and only matches "Host".
/// Many streaming services use lowercase "host", so we need a fallback.
///
/// This function:
/// 1. Tries the SDK-parsed host first (case-sensitive "Host")
/// 2. Falls back to manual case-insensitive p tag parsing
/// 3. Verifies the proof signature if provided
pub fn extract_live_event_host(event: &Event, live_event: &LiveEvent) -> Option<ParsedLiveHost> {
    if let Some(host) = &live_event.host {
        let is_verified = verify_host_proof(event, &host.public_key, host.proof.as_ref());
        return Some(ParsedLiveHost {
            public_key: host.public_key.to_string(),
            relay_url: host.relay_url.as_ref().map(|u| u.to_string()),
            proof: host.proof.as_ref().map(|s| s.to_string()),
            is_verified,
        });
    }
    for tag in event.tags.iter() {
        let tag_vec = tag.clone().to_vec();
        if tag_vec.first().map(|s| s.as_str()) != Some("p") {
            continue;
        }
        if let Some(marker) = tag_vec.get(3) {
            if marker.to_lowercase() == "host" {
                let pubkey_str = match tag_vec.get(1) {
                    Some(pk) => pk,
                    None => continue,
                };
                let relay_url = tag_vec
                    .get(2)
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_string());
                let proof_str = tag_vec
                    .get(4)
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_string());
                let parsed_pubkey = match PublicKey::parse(pubkey_str) {
                    Ok(pk) => pk,
                    Err(_) => continue,
                };
                let proof_sig = proof_str.as_ref().and_then(|p| Signature::from_str(p).ok());
                let is_verified = verify_host_proof(event, &parsed_pubkey, proof_sig.as_ref());
                return Some(ParsedLiveHost {
                    public_key: parsed_pubkey.to_string(),
                    relay_url,
                    proof: proof_str,
                    is_verified,
                });
            }
        }
    }
    None
}
/// Verify host proof per NIP-53.
///
/// Per NIP-53: "The proof is a signed SHA256 of the complete `a` Tag of the event
/// (`kind:pubkey:dTag`) by each `p`'s private key, encoded in hex."
///
/// Returns true if proof is valid, false if missing or invalid.
fn verify_host_proof(event: &Event, host_pubkey: &PublicKey, proof: Option<&Signature>) -> bool {
    use nostr_sdk::secp256k1::{Message, Secp256k1, XOnlyPublicKey};
    use sha2::{Digest, Sha256};
    let Some(proof_sig) = proof else {
        return false;
    };
    let d_tag = event
        .tags
        .iter()
        .find(|t| t.kind() == TagKind::SingleLetter(SingleLetterTag::lowercase(Alphabet::D)))
        .and_then(|t| t.content())
        .unwrap_or("");
    let message = format!("30311:{}:{}", event.pubkey, d_tag);
    let message_hash = Sha256::digest(message.as_bytes());
    let secp = Secp256k1::verification_only();
    let msg = match Message::from_digest_slice(message_hash.as_ref()) {
        Ok(m) => m,
        Err(_) => return false,
    };
    let pubkey_bytes = host_pubkey.to_bytes();
    let xonly_bytes = if pubkey_bytes.len() == 33 {
        &pubkey_bytes[1..]
    } else {
        &pubkey_bytes
    };
    match XOnlyPublicKey::from_slice(xonly_bytes) {
        Ok(xonly) => secp.verify_schnorr(proof_sig, &msg, &xonly).is_ok(),
        Err(_) => false,
    }
}
/// Kind 10112: Nests server list (replaceable)
#[allow(dead_code)]
pub const KIND_NESTS_SERVERS: u16 = 10112;
/// Kind 4312: Admin commands (ephemeral)
#[allow(dead_code)]
pub const KIND_NESTS_ADMIN: u16 = 4312;
/// Build tags for a Kind 30312 Meeting Space event
pub fn build_meeting_space_tags(
    d_tag: &str,
    room_name: &str,
    status: RoomStatus,
    auth_url: &str,
    host_pubkey: &str,
) -> Vec<Tag> {
    vec![
        Tag::identifier(d_tag),
        Tag::custom(TagKind::custom("title"), [room_name]),
        Tag::custom(TagKind::custom("status"), [status.as_str()]),
        Tag::custom(TagKind::custom("auth"), [auth_url]),
        Tag::custom(TagKind::custom("p"), [host_pubkey, "", "host"]),
        Tag::custom(
            TagKind::custom("alt"),
            ["Interactive room event"],
        ),
    ]
}
/// Rebuild all tags from an existing MeetingSpace with a new status.
/// Preserves participants, hashtags, and all optional fields.
/// Used for closing rooms and preserving participant data on edit.
pub fn rebuild_meeting_space_tags(
    ms: &MeetingSpace,
    new_status: RoomStatus,
) -> Vec<Tag> {
    rebuild_meeting_space_tags_inner(ms, new_status, None, None)
}

/// Rebuild 30312 tags with one participant's role changed (e.g. host promotes
/// a hand-raised listener to speaker). Preserves all other tags verbatim,
/// including optional fields, hashtags, relays, and other participants.
///
/// Refuses to reassign the host role (the event author is the implicit host —
/// matches the ecosystem role-transition convention).
///
/// If `target_pubkey` isn't currently in `ms.providers`, they are appended as
/// a new participant with the requested role. This supports the "promote a
/// listener who isn't yet a p-tag participant" flow.
///
/// When demoting (new_role != "host"/"admin"/"speaker"), pass `onstage: false`
/// to also clear their stage flag in presence — though this only updates the
/// 30312's p-tags, not the target's 10312 presence (they publish their own).
pub fn rebuild_meeting_space_tags_with_role(
    ms: &MeetingSpace,
    target_pubkey: &str,
    new_role: &str,
) -> Result<Vec<Tag>, String> {
    if new_role == "host" {
        return Err("Cannot reassign host role (host is implicit via event pubkey)".into());
    }
    if !matches!(new_role, "admin" | "speaker" | "participant") {
        return Err(format!("Unknown role: {} (expected admin/speaker/participant)", new_role));
    }
    Ok(rebuild_meeting_space_tags_inner(
        ms,
        ms.status,
        Some((target_pubkey, new_role)),
        None,
    ))
}

/// Internal helper that does the actual tag rebuilding. `role_override` is
/// `(target_pubkey, new_role)`; `status_override` lets future callers override
/// status independently of role changes.
fn rebuild_meeting_space_tags_inner(
    ms: &MeetingSpace,
    new_status: RoomStatus,
    role_override: Option<(&str, &str)>,
    _status_override: Option<RoomStatus>,
) -> Vec<Tag> {
    let mut tags = vec![
        Tag::identifier(&ms.d_tag),
        Tag::custom(TagKind::custom("title"), [&ms.room_name]),
        Tag::custom(TagKind::custom("status"), [new_status.as_str()]),
        Tag::custom(TagKind::custom("auth"), [&ms.service_url]),
        Tag::custom(
            TagKind::custom("alt"),
            ["Interactive room event"],
        ),
    ];
    // Participants, optionally with one role overridden.
    let mut override_applied = false;
    for provider in &ms.providers {
        let role = if let Some((target, new_role)) = role_override {
            if provider.pubkey == target {
                override_applied = true;
                new_role.to_string()
            } else {
                provider.role.clone().unwrap_or_else(|| "participant".into())
            }
        } else {
            provider.role.clone().unwrap_or_default()
        };
        let mut vals: Vec<String> = vec![provider.pubkey.clone()];
        vals.push(provider.relay_hint.clone().unwrap_or_default());
        vals.push(role);
        if let Some(ref proof) = provider.proof {
            vals.push(proof.clone());
        }
        tags.push(Tag::custom(TagKind::custom("p"), vals));
    }
    // If the target wasn't already a participant, append them.
    if let Some((target, new_role)) = role_override {
        if !override_applied {
            tags.push(Tag::custom(
                TagKind::custom("p"),
                vec![target.to_string(), String::new(), new_role.to_string()],
            ));
        }
    }
    // Optional fields (shared between rebuild and role-flip).
    append_optional_meeting_space_tags(&mut tags, ms);
    tags
}

/// Append optional fields (streaming, summary, image, recording, hashtags,
/// relays) from `ms` to `tags`. Shared by `rebuild_meeting_space_tags_inner`
/// and any future callers.
fn append_optional_meeting_space_tags(tags: &mut Vec<Tag>, ms: &MeetingSpace) {
    if let Some(ref url) = ms.endpoint_url {
        tags.push(Tag::custom(TagKind::custom("streaming"), [url.as_str()]));
    }
    if let Some(ref s) = ms.summary {
        tags.push(Tag::custom(TagKind::custom("summary"), [s.as_str()]));
    }
    if let Some(ref url) = ms.image {
        tags.push(Tag::custom(TagKind::custom("image"), [url.as_str()]));
    }
    if let Some(ref url) = ms.recording {
        tags.push(Tag::custom(TagKind::custom("recording"), [url.as_str()]));
    }
    if let Some(ts) = ms.starts {
        tags.push(Tag::custom(TagKind::custom("starts"), [ts.to_string()]));
    }
    for hashtag in &ms.hashtags {
        tags.push(Tag::custom(TagKind::custom("t"), [hashtag.as_str()]));
    }
    if !ms.relays.is_empty() {
        let relay_strs: Vec<String> = ms.relays.iter().map(|r| r.as_str().to_string()).collect();
        tags.push(Tag::custom(TagKind::custom("relays"), relay_strs));
    }
}
/// Add optional tags to a meeting space event
pub fn add_meeting_space_optional_tags(
    tags: &mut Vec<Tag>,
    streaming_url: Option<&str>,
    summary: Option<&str>,
    image: Option<&str>,
    starts: Option<u64>,
    recording: Option<&str>,
    relays: &[String],
) {
    if let Some(url) = streaming_url {
        tags.push(Tag::custom(TagKind::custom("streaming"), [url]));
    }
    if let Some(s) = summary {
        tags.push(Tag::custom(TagKind::custom("summary"), [s]));
    }
    if let Some(url) = image {
        tags.push(Tag::custom(TagKind::custom("image"), [url]));
    }
    if let Some(ts) = starts {
        tags.push(Tag::custom(
            TagKind::custom("starts"),
            [ts.to_string()],
        ));
    }
    if let Some(url) = recording {
        tags.push(Tag::custom(TagKind::custom("recording"), [url]));
    }
    if !relays.is_empty() {
        let relay_strs: Vec<String> = relays.iter().map(|r| r.as_str().to_string()).collect();
        tags.push(Tag::custom(
            TagKind::custom("relays"),
            relay_strs,
        ));
    }
}
/// Build tags for a Kind 10312 Room Presence event
#[allow(dead_code)]
pub fn build_room_presence_tags(
    room_coordinate: &str,
    hand: bool,
    muted: bool,
    publishing: bool,
    onstage: bool,
) -> Vec<Tag> {
    vec![
        Tag::custom(TagKind::custom("a"), [room_coordinate]),
        Tag::custom(
            TagKind::custom("hand"),
            [if hand { "1" } else { "0" }],
        ),
        Tag::custom(
            TagKind::custom("muted"),
            [if muted { "1" } else { "0" }],
        ),
        Tag::custom(
            TagKind::custom("publishing"),
            [if publishing { "1" } else { "0" }],
        ),
        Tag::custom(
            TagKind::custom("onstage"),
            [if onstage { "1" } else { "0" }],
        ),
    ]
}
/// Build tags for a Kind 10112 Nests server list event
#[allow(dead_code)]
pub fn build_nests_servers_tags(servers: &[NestsServer]) -> Vec<Tag> {
    let mut tags: Vec<Tag> = vec![Tag::identifier("nests-servers")];
    for server in servers {
        tags.push(Tag::custom(
            TagKind::custom("server"),
            [&server.relay_url, &server.auth_url],
        ));
    }
    tags
}
/// Parse a Kind 10112 event into a list of Nests servers.
///
/// Accepts both 3-element `["server", relay_url, auth_url]` tags (the form
/// this client publishes) and legacy 2-element `["server", relay_url]` tags.
/// For the 2-element form, the auth URL is derived from the relay URL by
/// replacing the leading `moq.` prefix with `moq-auth.` and dropping the port
/// (ecosystem convention).
pub fn parse_nests_servers(event: &Event) -> Vec<NestsServer> {
    event
        .tags
        .iter()
        .filter(|t| {
            let name = t.as_slice().first().map(|s| s.as_str());
            name == Some("server") || name == Some("relay")
        })
        .filter_map(|t| {
            let slice = t.as_slice();
            let relay_url = slice.get(1)?.to_string();
            let auth_url = if let Some(a) = slice.get(2) {
                if a.is_empty() {
                    derive_auth_url(&relay_url)?
                } else {
                    a.to_string()
                }
            } else {
                // 2-element form — derive auth URL.
                derive_auth_url(&relay_url)?
            };
            Some(NestsServer {
                relay_url,
                auth_url,
            })
        })
        .collect()
}

/// Derive the moq-auth URL from a MoQ relay URL.
///
/// `https://moq.example.com:4443` → `https://moq-auth.example.com`
/// `https://relay.example.com`     → `https://moq-auth.relay.example.com`
///
/// Ecosystem convention for auth-URL derivation.
pub fn derive_auth_url(relay_url: &str) -> Option<String> {
    let parsed = url::Url::parse(relay_url).ok()?;
    let host = parsed.host_str()?;
    let auth_host = if let Some(rest) = host.strip_prefix("moq.") {
        format!("moq-auth.{}", rest)
    } else {
        format!("moq-auth.{}", host)
    };
    Some(format!("https://{}", auth_host))
}
/// Build tags for a Kind 4312 admin command event
#[allow(dead_code)]
pub fn build_admin_command_tags(
    room_coordinate: &str,
    target_pubkey: &str,
    action: &str,
) -> Vec<Tag> {
    vec![
        Tag::custom(TagKind::custom("a"), [room_coordinate]),
        Tag::custom(TagKind::custom("p"), [target_pubkey]),
        Tag::custom(TagKind::custom("action"), [action]),
    ]
}
/// Determine the display status for a nest room using presence-based liveness
pub fn nest_effective_status(
    room_status: RoomStatus,
    last_presence: Option<u64>,
    created_at: u64,
) -> LiveStatus {
    if room_status == RoomStatus::Closed {
        return LiveStatus::Ended;
    }
    let now = crate::platform::timestamp::now_secs();
    if let Some(ts) = last_presence {
        if now.saturating_sub(ts) < PRESENCE_LIVE_THRESHOLD_SECS {
            return LiveStatus::Live;
        }
    }
    if room_status == RoomStatus::Open {
        if now.saturating_sub(created_at) > ROOM_STALE_THRESHOLD_SECS {
            return LiveStatus::Ended;
        }
        LiveStatus::Live
    } else {
        LiveStatus::Planned
    }
}

/// Verify that a kind 4312 admin command should be honored.
///
/// Implements the client-side authority check that relays do NOT enforce:
/// the signer must be the room's host (event author) OR hold a `host`/`admin`
/// role on the active 30312. Also enforces a 60s freshness gate and dedup
/// by event id (ecosystem admin-command conventions).
///
/// Only `kick` is honored as a wire action (the ecosystem convention
/// doesn't implement `mute`/`unmute`).
pub fn should_honor_admin_command(
    event: &Event,
    space: &MeetingSpace,
    seen: &mut std::collections::HashSet<nostr_sdk::EventId>,
) -> Option<AdminAction> {
    // Freshness: only honor commands from the last 60s.
    let now = crate::platform::timestamp::now_secs();
    if event.created_at.as_secs() < now.saturating_sub(60) {
        return None;
    }
    // Dedup by event id.
    if !seen.insert(event.id) {
        return None;
    }
    // Authority: signer is the room host (event author) OR has host/admin role.
    let signer_hex = event.pubkey.to_hex();
    let is_authority = signer_hex == space.pubkey
        || space.providers.iter().any(|p| {
            p.pubkey == signer_hex && matches!(p.role.as_deref(), Some("host") | Some("admin"))
        });
    if !is_authority {
        return None;
    }
    // Action: only honor `kick` (reference client doesn't implement mute/unmute).
    let action = event
        .tags
        .iter()
        .find(|t| t.as_slice().first().map(|s| s.as_str()) == Some("action"))
        .and_then(|t| t.as_slice().get(1))
        .map(|s| s.as_str());
    match action {
        Some("kick") => Some(AdminAction::Kick),
        _ => None,
    }
}

/// Admin actions a client honors from kind 4312 events.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AdminAction {
    /// Disconnect the target user from the room. The only action the
    /// ecosystem admin-command set defines.
    Kick,
}

// ---------------------------------------------------------------------------
// Feed filter helpers (Phase 2 — lobby/feed filter parity). These are
// pure functions over `MeetingSpace` so they
// can be unit-tested without a relay connection.
// ---------------------------------------------------------------------------

/// Presence freshness window for "is this room still live?". Matches
/// 3× the 60s presence heartbeat.
#[allow(dead_code)]
pub const PRESENCE_FRESHNESS_WINDOW_SECS: u64 = 180;

/// A room is considered joinable from the feed only if it carries the
/// minimum field set the ecosystem requires (rule 2): `room_name`,
/// `status`, `service_url`, `endpoint_url`, with HTTPS on both URLs.
/// Drops d-tag-only events relays sometimes leak and legacy
/// `wss+livekit://` URLs a moq-lite client can't reach.
pub fn is_joinable(ms: &MeetingSpace) -> bool {
    !ms.room_name.is_empty()
        && !ms.service_url.is_empty()
        && ms.service_url.starts_with("https://")
        && ms
            .endpoint_url
            .as_ref()
            .map(|u| u.starts_with("https://"))
            .unwrap_or(false)
}

/// A "stale-live" room: status says open/private but no presence has been
/// seen in the last 3 minutes. Used to demote rooms in the feed and route
/// taps to the read-only lobby instead of joining a dead room's audio.
/// Fresh-speakers gate for feed eligibility.
#[allow(dead_code)]
pub fn is_stale_live(ms: &MeetingSpace, last_presence: Option<u64>) -> bool {
    if matches!(ms.status, RoomStatus::Closed) {
        return false;
    }
    let now = crate::platform::timestamp::now_secs();
    match last_presence {
        Some(ts) => now.saturating_sub(ts) > PRESENCE_FRESHNESS_WINDOW_SECS,
        // No presence ever seen — stale unless the room was just created
        // (A created-at grace lets new rooms surface before
        // the first speaker heartbeat arrives).
        None => now.saturating_sub(ms.created_at) > PRESENCE_FRESHNESS_WINDOW_SECS,
    }
}

/// Planned rooms whose `starts` is more than 1h in the past (host never
/// opened) or more than 30d in the future (likely spam) are dropped from
/// the feed (planned-room staleness and
/// `PLANNED_MAX_FUTURE_SECONDS`.
pub fn is_within_planned_window(ms: &MeetingSpace, now: u64) -> bool {
    if !matches!(ms.status, RoomStatus::Private) || ms.starts.is_none() {
        return true;
    }
    let starts = ms.starts.unwrap();
    const PLANNED_STALE_SECS: u64 = 60 * 60;
    const PLANNED_MAX_FUTURE_SECS: u64 = 30 * 24 * 60 * 60;
    starts > now.saturating_sub(PLANNED_STALE_SECS) && starts < now + PLANNED_MAX_FUTURE_SECS
}

/// Ended rooms older than 7 days drop out of the historic feed bucket.
/// Ended-room history window. The audience can
/// still reach a recording via direct link; the feed just stops surfacing
/// it.
pub fn is_within_ended_window(ms: &MeetingSpace, now: u64) -> bool {
    if !matches!(ms.status, RoomStatus::Closed) {
        return true;
    }
    const ENDED_HISTORY_SECS: u64 = 7 * 24 * 60 * 60;
    now.saturating_sub(ms.created_at) < ENDED_HISTORY_SECS
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_live_status_from_str() {
        assert_eq!(LiveStatus::from_str("planned"), Some(LiveStatus::Planned));
        assert_eq!(LiveStatus::from_str("LIVE"), Some(LiveStatus::Live));
        assert_eq!(LiveStatus::from_str("ended"), Some(LiveStatus::Ended));
        assert_eq!(LiveStatus::from_str("invalid"), None);
    }
    #[test]
    fn test_room_status_from_str() {
        assert_eq!(RoomStatus::from_str("open"), Some(RoomStatus::Open));
        assert_eq!(RoomStatus::from_str("PRIVATE"), Some(RoomStatus::Private));
        assert_eq!(RoomStatus::from_str("closed"), Some(RoomStatus::Closed));
        assert_eq!(RoomStatus::from_str("invalid"), None);
    }
    #[test]
    fn test_room_status_planned_alias_to_private() {
        // Live-audio hosts publish status=planned for scheduled
        // rooms; it must map to Private (our scheduled representation) so the
        // feed buckets it as Planned instead of defaulting to Open.
        assert_eq!(RoomStatus::from_str("planned"), Some(RoomStatus::Private));
        assert_eq!(RoomStatus::from_str("Planned"), Some(RoomStatus::Private));
        assert_eq!(RoomStatus::from_str("PLANNED"), Some(RoomStatus::Private));
    }
    #[test]
    fn test_room_status_as_str_emits_deployed_wire_values() {
        assert_eq!(RoomStatus::Open.as_str(), "live");
        assert_eq!(RoomStatus::Private.as_str(), "planned");
        assert_eq!(RoomStatus::Closed.as_str(), "ended");
        // Round-trip: every emitted value parses back to the same variant.
        for status in [RoomStatus::Open, RoomStatus::Private, RoomStatus::Closed] {
            assert_eq!(RoomStatus::from_str(status.as_str()), Some(status));
        }
    }
    #[test]
    fn test_live_status_display() {
        assert_eq!(LiveStatus::Live.to_string(), "live");
        assert_eq!(LiveStatus::Planned.to_string(), "planned");
        assert_eq!(LiveStatus::Ended.to_string(), "ended");
    }
    #[test]
    fn test_room_status_ended_alias() {
        assert_eq!(RoomStatus::from_str("ended"), Some(RoomStatus::Closed));
        assert_eq!(RoomStatus::from_str("ENDED"), Some(RoomStatus::Closed));
    }
    #[test]
    fn test_room_status_live_alias_to_open() {
        // nostrnests.com and other hosts publish status=live on kind 30312.
        // Coerce to Open so the room is treated as joinable.
        assert_eq!(RoomStatus::from_str("live"), Some(RoomStatus::Open));
        assert_eq!(RoomStatus::from_str("LIVE"), Some(RoomStatus::Open));
        assert_eq!(RoomStatus::from_str("Live"), Some(RoomStatus::Open));
    }
    #[test]
    fn test_build_meeting_space_tags() {
        let tags = build_meeting_space_tags(
            "my-room",
            "Test Room",
            RoomStatus::Open,
            "https://auth.example.com",
            "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
        );
        let d = tags.iter().find(|t| t.kind() == TagKind::SingleLetter(SingleLetterTag::lowercase(Alphabet::D))).and_then(|t| t.content()).unwrap();
        assert_eq!(d, "my-room");
        let title = tags.iter().find(|t| t.as_slice().first().map(|s| s.as_str()) == Some("title")).and_then(|t| t.as_slice().get(1).map(|s| s.as_str())).unwrap();
        assert_eq!(title, "Test Room");
        let status = tags.iter().find(|t| t.as_slice().first().map(|s| s.as_str()) == Some("status")).and_then(|t| t.as_slice().get(1).map(|s| s.as_str())).unwrap();
        assert_eq!(status, "live");
        assert!(tags.iter().any(|t| {
            let s = t.as_slice();
            s.first().map(|x| x.as_str()) == Some("p") && s.get(3).map(|x| x.as_str()) == Some("host")
        }));
    }
    #[test]
    fn test_build_room_presence_tags() {
        let tags = build_room_presence_tags("30312:abc123:my-room", true, false, true, false);
        let a_tag = tags.iter().find(|t| t.as_slice().first().map(|s| s.as_str()) == Some("a")).and_then(|t| t.as_slice().get(1).map(|s| s.to_string())).unwrap();
        assert_eq!(a_tag, "30312:abc123:my-room");
        assert!(tags.iter().any(|t| t.as_slice().first().map(|s| s.as_str()) == Some("hand") && t.as_slice().get(1).map(|s| s.as_str()) == Some("1")));
        assert!(tags.iter().any(|t| t.as_slice().first().map(|s| s.as_str()) == Some("publishing") && t.as_slice().get(1).map(|s| s.as_str()) == Some("1")));
    }
    #[test]
    fn test_build_nests_servers_tags() {
        let servers = vec![
            NestsServer { relay_url: "wss://relay.example.com".into(), auth_url: "https://auth.example.com".into() },
        ];
        let tags = build_nests_servers_tags(&servers);
        assert!(tags.iter().any(|t| t.kind() == TagKind::SingleLetter(SingleLetterTag::lowercase(Alphabet::D))));
        let server_tag = tags.iter().find(|t| t.as_slice().first().map(|s| s.as_str()) == Some("server")).unwrap();
        assert_eq!(server_tag.as_slice()[1], "wss://relay.example.com");
        assert_eq!(server_tag.as_slice()[2], "https://auth.example.com");
    }
    #[test]
    fn test_parse_nests_servers() {
        let keys = Keys::generate();
        let event = EventBuilder::new(Kind::Custom(10112), "")
            .tags(vec![
                Tag::identifier("nests-servers"),
                Tag::custom(TagKind::custom("server"), ["wss://relay.example.com", "https://auth.example.com"]),
            ])
            .sign_with_keys(&keys)
            .unwrap();
        let servers = parse_nests_servers(&event);
        assert_eq!(servers.len(), 1);
        assert_eq!(servers[0].relay_url, "wss://relay.example.com");
        assert_eq!(servers[0].auth_url, "https://auth.example.com");
    }
    #[test]
    fn test_build_admin_command_tags() {
        let tags = build_admin_command_tags(
            "30312:abc:room1",
            "def456",
            "mute",
        );
        let a_tag = tags.iter().find(|t| t.as_slice().first().map(|s| s.as_str()) == Some("a")).and_then(|t| t.as_slice().get(1).map(|s| s.to_string())).unwrap();
        assert_eq!(a_tag, "30312:abc:room1");
        let p_tag = tags.iter().find(|t| t.as_slice().first().map(|s| s.as_str()) == Some("p")).and_then(|t| t.as_slice().get(1).map(|s| s.to_string())).unwrap();
        assert_eq!(p_tag, "def456");
        let action = tags.iter().find(|t| t.as_slice().first().map(|s| s.as_str()) == Some("action")).and_then(|t| t.as_slice().get(1).map(|s| s.to_string())).unwrap();
        assert_eq!(action, "mute");
    }
    #[test]
    fn test_rebuild_meeting_space_tags_preserves_participants() {
        let ms = MeetingSpace {
            d_tag: "room-42".into(),
            event_id: "evt123".into(),
            pubkey: "aa".repeat(32),
            naddr: "naddr1test".into(),
            coordinate: "30312:aa".to_string() + &"aa".repeat(31) + ":room-42",
            created_at: 1000,
            room_name: "My Room".into(),
            summary: Some("A test room".into()),
            image: Some("https://img.example.com/pic.png".into()),
            status: RoomStatus::Open,
            service_url: "https://auth.example.com".into(),
            endpoint_url: Some("https://stream.example.com".into()),
            recording: Some("https://rec.example.com/rec.mp4".into()),
            starts: None,
            hashtags: vec!["test".into(), "nostr".into()],
            relays: vec!["wss://relay.example.com".into()],
            providers: vec![
                LiveParticipant {
                    pubkey: "bb".repeat(32),
                    relay_hint: Some("wss://r1.example.com".into()),
                    role: Some("host".into()),
                    proof: None,
                },
                LiveParticipant {
                    pubkey: "cc".repeat(32),
                    relay_hint: None,
                    role: Some("speaker".into()),
                    proof: Some("proof123".into()),
                },
            ],
        };
        let tags = rebuild_meeting_space_tags(&ms, RoomStatus::Closed);
        let p_tags: Vec<_> = tags.iter().filter(|t| t.as_slice().first().map(|s| s.as_str()) == Some("p")).collect();
        assert_eq!(p_tags.len(), 2);
        let status_tag = tags.iter().find(|t| t.as_slice().first().map(|s| s.as_str()) == Some("status")).and_then(|t| t.as_slice().get(1).map(|s| s.to_string())).unwrap();
        assert_eq!(status_tag, "ended");
        assert!(tags.iter().any(|t| t.as_slice().first().map(|s| s.as_str()) == Some("summary")));
        assert!(tags.iter().any(|t| t.as_slice().first().map(|s| s.as_str()) == Some("image")));
        assert!(tags.iter().any(|t| t.as_slice().first().map(|s| s.as_str()) == Some("recording")));
        assert!(tags.iter().any(|t| t.as_slice().first().map(|s| s.as_str()) == Some("streaming")));
        let t_tags: Vec<_> = tags.iter().filter(|t| t.as_slice().first().map(|s| s.as_str()) == Some("t")).collect();
        assert_eq!(t_tags.len(), 2);
    }
    #[test]
    fn test_add_meeting_space_optional_tags() {
        let mut tags = build_meeting_space_tags("room1", "Room", RoomStatus::Open, "https://auth.example.com", &"aa".repeat(32));
        add_meeting_space_optional_tags(
            &mut tags,
            Some("https://stream.example.com"),
            Some("A summary"),
            Some("https://img.example.com/pic.png"),
            Some(1700000000),
            Some("https://rec.example.com/rec.mp4"),
            &["wss://relay.example.com".to_string()],
        );
        assert!(tags.iter().any(|t| t.as_slice().first().map(|s| s.as_str()) == Some("streaming")));
        assert!(tags.iter().any(|t| t.as_slice().first().map(|s| s.as_str()) == Some("summary")));
        assert!(tags.iter().any(|t| t.as_slice().first().map(|s| s.as_str()) == Some("image")));
        assert!(tags.iter().any(|t| t.as_slice().first().map(|s| s.as_str()) == Some("starts")));
        assert!(tags.iter().any(|t| t.as_slice().first().map(|s| s.as_str()) == Some("recording")));
        assert!(tags.iter().any(|t| t.as_slice().first().map(|s| s.as_str()) == Some("relays")));
    }

    /// Helper for the role-flip tests below.
    fn sample_room_for_role_flip() -> MeetingSpace {
        MeetingSpace {
            d_tag: "room-42".into(),
            event_id: "evt123".into(),
            pubkey: "aa".repeat(32),
            naddr: "naddr1test".into(),
            coordinate: format!("30312:{}:room-42", "aa".repeat(32)),
            created_at: 1000,
            room_name: "My Room".into(),
            summary: Some("A test room".into()),
            image: None,
            status: RoomStatus::Open,
            service_url: "https://auth.example.com".into(),
            endpoint_url: Some("https://stream.example.com".into()),
            recording: None,
            starts: None,
            hashtags: vec![],
            relays: vec![],
            providers: vec![
                LiveParticipant {
                    pubkey: "bb".repeat(32),
                    relay_hint: None,
                    role: Some("host".into()),
                    proof: None,
                },
                LiveParticipant {
                    pubkey: "cc".repeat(32),
                    relay_hint: None,
                    role: Some("speaker".into()),
                    proof: None,
                },
            ],
        }
    }

    #[test]
    fn test_rebuild_with_role_promotes_existing_participant() {
        let ms = sample_room_for_role_flip();
        // Promote a brand-new participant.
        let target = "dd".repeat(32);
        let tags = rebuild_meeting_space_tags_with_role(&ms, &target, "speaker").unwrap();
        // The target should appear with role "speaker".
        let target_tag = tags
            .iter()
            .filter(|t| t.as_slice().first().map(|s| s.as_str()) == Some("p"))
            .find(|t| t.as_slice().get(1).map(|s| s.as_str()) == Some(target.as_str()))
            .expect("target p-tag should exist");
        assert_eq!(target_tag.as_slice().get(3).map(|s| s.as_str()), Some("speaker"));
        // Existing participants' roles preserved.
        let host_tag = tags.iter().filter(|t| t.as_slice().first().map(|s| s.as_str()) == Some("p"))
            .find(|t| t.as_slice().get(1).map(|s| s.as_str()) == Some("bb".repeat(32).as_str())).unwrap();
        assert_eq!(host_tag.as_slice().get(3).map(|s| s.as_str()), Some("host"));
        // 3 p-tags total (host, existing speaker, new speaker).
        let p_count = tags.iter().filter(|t| t.as_slice().first().map(|s| s.as_str()) == Some("p")).count();
        assert_eq!(p_count, 3);
    }

    #[test]
    fn test_rebuild_with_role_overrides_existing_role() {
        let ms = sample_room_for_role_flip();
        // Demote the existing speaker (cc) to participant.
        let target = "cc".repeat(32);
        let tags = rebuild_meeting_space_tags_with_role(&ms, &target, "participant").unwrap();
        // No new p-tag added.
        let p_count = tags.iter().filter(|t| t.as_slice().first().map(|s| s.as_str()) == Some("p")).count();
        assert_eq!(p_count, 2);
        // Target's role updated.
        let target_tag = tags.iter().filter(|t| t.as_slice().first().map(|s| s.as_str()) == Some("p"))
            .find(|t| t.as_slice().get(1).map(|s| s.as_str()) == Some(target.as_str())).unwrap();
        assert_eq!(target_tag.as_slice().get(3).map(|s| s.as_str()), Some("participant"));
    }

    #[test]
    fn test_rebuild_with_role_refuses_host_reassignment() {
        let ms = sample_room_for_role_flip();
        let result = rebuild_meeting_space_tags_with_role(&ms, "bb".repeat(32).as_str(), "host");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_lowercase().contains("host"));
    }

    #[test]
    fn test_rebuild_with_role_rejects_unknown_role() {
        let ms = sample_room_for_role_flip();
        let result = rebuild_meeting_space_tags_with_role(&ms, "cc".repeat(32).as_str(), "bogus");
        assert!(result.is_err());
    }

    #[test]
    fn test_rebuild_with_role_preserves_optional_fields() {
        let mut ms = sample_room_for_role_flip();
        ms.hashtags = vec!["nostr".into()];
        ms.recording = Some("https://rec.example.com/r.mp4".into());
        ms.image = Some("https://img.example.com/i.png".into());
        let tags = rebuild_meeting_space_tags_with_role(&ms, "cc".repeat(32).as_str(), "participant").unwrap();
        assert!(tags.iter().any(|t| t.as_slice().first().map(|s| s.as_str()) == Some("streaming")));
        assert!(tags.iter().any(|t| t.as_slice().first().map(|s| s.as_str()) == Some("summary")));
        assert!(tags.iter().any(|t| t.as_slice().first().map(|s| s.as_str()) == Some("image")));
        assert!(tags.iter().any(|t| t.as_slice().first().map(|s| s.as_str()) == Some("recording")));
        assert!(tags.iter().any(|t| t.as_slice().first().map(|s| s.as_str()) == Some("t")));
    }

    #[test]
    fn test_parse_meeting_space_extracts_starts_tag() {
        let keys = Keys::generate();
        let event = EventBuilder::new(Kind::Custom(30312), "")
            .tags(vec![
                Tag::identifier("room-1"),
                Tag::custom(TagKind::custom("title"), ["Test"]),
                Tag::custom(TagKind::custom("status"), ["open"]),
                Tag::custom(TagKind::custom("auth"), ["https://auth.example.com"]),
                Tag::custom(TagKind::custom("streaming"), ["https://stream.example.com"]),
                Tag::custom(TagKind::custom("starts"), ["1700000000"]),
            ])
            .sign_with_keys(&keys)
            .unwrap();
        let ms = parse_meeting_space(&event).unwrap();
        assert_eq!(ms.starts, Some(1700000000));
    }

    #[test]
    fn test_parse_meeting_space_starts_optional() {
        let keys = Keys::generate();
        let event = EventBuilder::new(Kind::Custom(30312), "")
            .tags(vec![
                Tag::identifier("room-1"),
                Tag::custom(TagKind::custom("title"), ["Test"]),
                Tag::custom(TagKind::custom("status"), ["open"]),
                Tag::custom(TagKind::custom("auth"), ["https://auth.example.com"]),
                Tag::custom(TagKind::custom("streaming"), ["https://stream.example.com"]),
            ])
            .sign_with_keys(&keys)
            .unwrap();
        let ms = parse_meeting_space(&event).unwrap();
        assert_eq!(ms.starts, None);
    }

    #[test]
    fn test_parse_meeting_space_planned_status() {
        // Hosts publish status=planned (+ starts) for scheduled rooms.
        // Without the alias the parser defaults to Open, which the feed ages
        // out to Ended after 10 minutes — misclassifying every scheduled
        // scheduled room.
        let keys = Keys::generate();
        let event = EventBuilder::new(Kind::Custom(30312), "")
            .tags(vec![
                Tag::identifier("room-1"),
                Tag::custom(TagKind::custom("title"), ["Test"]),
                Tag::custom(TagKind::custom("status"), ["planned"]),
                Tag::custom(TagKind::custom("auth"), ["https://auth.example.com"]),
                Tag::custom(TagKind::custom("streaming"), ["https://stream.example.com"]),
                Tag::custom(TagKind::custom("starts"), ["1700000000"]),
            ])
            .sign_with_keys(&keys)
            .unwrap();
        let ms = parse_meeting_space(&event).unwrap();
        assert_eq!(ms.status, RoomStatus::Private);
        // Private (no presence) buckets as Planned via nest_effective_status;
        // the scheduled window applies via is_within_planned_window.
        assert!(is_within_planned_window(&ms, 1_700_000_000));
    }

    #[test]
    fn test_rebuild_meeting_space_tags_round_trips_starts() {
        let mut ms = sample_room_for_role_flip();
        ms.starts = Some(1_700_000_000);
        let tags = rebuild_meeting_space_tags(&ms, ms.status);
        assert!(
            tags.iter().any(|t| {
                t.as_slice().first().map(|s| s.as_str()) == Some("starts")
                    && t.as_slice().get(1).map(|s| s.as_str()) == Some("1700000000")
            }),
            "starts tag should be emitted on rebuild, got: {:?}",
            tags.iter().map(|t| t.as_slice()).collect::<Vec<_>>()
        );
    }

    fn joinable_room() -> MeetingSpace {
        MeetingSpace {
            starts: None,
            ..sample_room_for_role_flip()
        }
    }

    #[test]
    fn test_is_joinable_accepts_valid_room() {
        let ms = joinable_room();
        assert!(is_joinable(&ms));
    }

    #[test]
    fn test_is_joinable_rejects_non_https_service() {
        let mut ms = joinable_room();
        ms.service_url = "wss://auth.example.com".into();
        assert!(!is_joinable(&ms));
    }

    #[test]
    fn test_is_joinable_rejects_missing_endpoint() {
        let mut ms = joinable_room();
        ms.endpoint_url = None;
        assert!(!is_joinable(&ms));
    }

    #[test]
    fn test_is_joinable_rejects_empty_name() {
        let mut ms = joinable_room();
        ms.room_name = "".into();
        assert!(!is_joinable(&ms));
    }

    #[test]
    fn test_is_within_planned_window_allows_recent_past() {
        let mut ms = joinable_room();
        ms.status = RoomStatus::Private;
        ms.starts = Some(1_000_000_000); // arbitrary past ts
        let now = 1_000_000_000 + 30 * 60; // 30 min after starts
        assert!(is_within_planned_window(&ms, now));
    }

    #[test]
    fn test_is_within_planned_window_rejects_stale_past() {
        let mut ms = joinable_room();
        ms.status = RoomStatus::Private;
        ms.starts = Some(1_000_000_000);
        let now = 1_000_000_000 + 2 * 60 * 60; // 2h after starts
        assert!(!is_within_planned_window(&ms, now));
    }

    #[test]
    fn test_is_within_planned_window_rejects_far_future() {
        let mut ms = joinable_room();
        ms.status = RoomStatus::Private;
        ms.starts = Some(1_000_000_000);
        let now = 1_000_000_000 - 60 * 24 * 60 * 60; // 60d before starts
        assert!(!is_within_planned_window(&ms, now));
    }

    #[test]
    fn test_is_within_planned_window_passes_through_open_rooms() {
        let ms = joinable_room(); // status Open
        assert!(is_within_planned_window(&ms, 1_000_000_000));
    }

    #[test]
    fn test_is_within_ended_window_allows_recent() {
        let mut ms = joinable_room();
        ms.status = RoomStatus::Closed;
        ms.created_at = 1_000_000_000;
        let now = 1_000_000_000 + 3 * 24 * 60 * 60; // 3d after creation
        assert!(is_within_ended_window(&ms, now));
    }

    #[test]
    fn test_is_within_ended_window_rejects_old() {
        let mut ms = joinable_room();
        ms.status = RoomStatus::Closed;
        ms.created_at = 1_000_000_000;
        let now = 1_000_000_000 + 10 * 24 * 60 * 60; // 10d after creation
        assert!(!is_within_ended_window(&ms, now));
    }
}
