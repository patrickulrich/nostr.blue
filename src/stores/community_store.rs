//! Community Store
//! Handles NIP-72 Moderated Communities - caching, filtering, and state management
//!
//! NIP-72 Event Kinds:
//! - 34550: Community definition (addressable)
//! - 4550: Moderator approval events
//! - 4551: Post removal events
//! - 1111: Posts/comments (NIP-22)
//! - 34551: Approved members list

#![allow(dead_code)]

use dioxus::prelude::*;
use lru::LruCache;
use nostr_sdk::prelude::*;
use nostr::Event as NostrEvent;
use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::num::NonZeroUsize;
use std::time::Duration;

use crate::utils::format::truncate_pubkey;

// ============================================================================
// Constants
// ============================================================================

pub const KIND_COMMUNITY_DEFINITION: u16 = 34550;
pub const KIND_COMMUNITY_POST: u16 = 1111;  // NIP-22 comment
pub const KIND_APPROVAL: u16 = 4550;
pub const KIND_REMOVAL: u16 = 4551;
pub const KIND_APPROVED_MEMBERS: u16 = 34551;

// Join request workflow kinds (Chorus-compatible)
pub const KIND_JOIN_REQUEST: u16 = 4552;
pub const KIND_DECLINED_MEMBERS: u16 = 4553;
pub const KIND_BANNED_MEMBERS: u16 = 4554;

const COMMUNITY_CACHE_SIZE: usize = 100;
const POST_CACHE_SIZE: usize = 500;

// ============================================================================
// Data Structures
// ============================================================================

#[derive(Clone, Debug, PartialEq)]
pub struct Community {
    pub id: String,                    // Event ID hex
    pub pubkey: String,                // Owner pubkey hex
    pub d_tag: String,                 // Community identifier
    pub name: Option<String>,
    pub description: Option<String>,
    pub image: Option<String>,
    pub banner: Option<String>,
    pub rules: Option<String>,
    pub moderators: Vec<String>,       // Pubkeys with "moderator" marker
    pub a_tag: String,                 // "34550:pubkey:d_tag"
    pub naddr: String,                 // bech32 encoded
    pub created_at: u64,
    pub event: NostrEvent,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CommunityPost {
    pub id: String,                    // Event ID hex
    pub pubkey: String,                // Author pubkey hex
    pub content: String,
    pub community_a_tag: String,       // Parent community
    pub parent_id: Option<String>,     // For replies (e-tag)
    pub parent_pubkey: Option<String>, // Parent author (p-tag)
    pub kind: u16,                     // 1111 or 1 (legacy)
    pub created_at: u64,
    pub event: NostrEvent,
    pub approval_status: ApprovalStatus,
    pub is_top_level: bool,            // True if A == a tags (top-level post)
}

#[derive(Clone, Debug, PartialEq)]
pub enum ApprovalStatus {
    AutoApproved(AutoApprovalReason),
    Approved(Vec<Approval>),
    Pending,
    Removed(Removal),
}

#[derive(Clone, Debug, PartialEq)]
pub enum AutoApprovalReason {
    Owner,
    Moderator,
    ApprovedMember,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Approval {
    pub event_id: String,
    pub moderator_pubkey: String,
    pub approved_at: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Removal {
    pub event_id: String,
    pub moderator_pubkey: String,
    pub removed_at: u64,
    pub reason: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum UserRole {
    Owner,
    Moderator,
    ApprovedMember,
    Pending,     // Join request submitted, awaiting approval
    Declined,    // Join request was declined
    Visitor,
}

/// Membership status for a user in a community (more detailed than UserRole)
#[derive(Clone, Debug, PartialEq)]
pub enum MembershipStatus {
    Owner,
    Moderator,
    Member,
    Pending { request_id: String, requested_at: u64 },
    Declined { reason: Option<String> },
    Banned { reason: Option<String> },
    None,
}

/// Join request event (kind 4552)
#[derive(Clone, Debug, PartialEq)]
pub struct JoinRequest {
    pub id: String,                 // Event ID
    pub community_a_tag: String,    // Community being joined
    pub user_pubkey: String,        // Requesting user
    pub reason: Option<String>,     // Join reason message
    pub created_at: u64,
    pub event: Option<NostrEvent>,  // Original event (optional, not available when self-submitted)
}

/// Community with membership context (for display with role badges)
#[derive(Clone, Debug, PartialEq)]
pub struct CommunityWithMembership {
    pub community: Community,
    pub membership_status: MembershipStatus,
    pub is_pinned: bool,
    pub pending_request_count: Option<u32>,  // For moderators/owners
}

// ============================================================================
// Global Signals
// ============================================================================

/// Community definitions cache (keyed by a_tag)
pub static COMMUNITIES_CACHE: GlobalSignal<LruCache<String, Community>> =
    GlobalSignal::new(|| LruCache::new(NonZeroUsize::new(COMMUNITY_CACHE_SIZE).unwrap()));

/// Posts cache (keyed by event_id)
pub static POSTS_CACHE: GlobalSignal<LruCache<String, CommunityPost>> =
    GlobalSignal::new(|| LruCache::new(NonZeroUsize::new(POST_CACHE_SIZE).unwrap()));

/// Approvals cache (keyed by post event_id -> Vec<Approval>)
pub static APPROVALS_CACHE: GlobalSignal<HashMap<String, Vec<Approval>>> =
    GlobalSignal::new(HashMap::new);

/// Removals cache (keyed by post event_id -> Removal)
pub static REMOVALS_CACHE: GlobalSignal<HashMap<String, Removal>> =
    GlobalSignal::new(HashMap::new);

/// Approved members by community (a_tag -> Set of pubkeys)
pub static APPROVED_MEMBERS_CACHE: GlobalSignal<HashMap<String, HashSet<String>>> =
    GlobalSignal::new(HashMap::new);

/// Pending join requests by community (a_tag -> Vec<JoinRequest>)
pub static PENDING_JOIN_REQUESTS_CACHE: GlobalSignal<HashMap<String, Vec<JoinRequest>>> =
    GlobalSignal::new(HashMap::new);

/// User's own pending join requests (a_tag -> JoinRequest)
pub static USER_PENDING_REQUESTS: GlobalSignal<HashMap<String, JoinRequest>> =
    GlobalSignal::new(HashMap::new);

/// Declined members by community (a_tag -> Set of pubkeys)
pub static DECLINED_MEMBERS_CACHE: GlobalSignal<HashMap<String, HashSet<String>>> =
    GlobalSignal::new(HashMap::new);

/// Banned members by community (a_tag -> Set of pubkeys)
pub static BANNED_MEMBERS_CACHE: GlobalSignal<HashMap<String, HashSet<String>>> =
    GlobalSignal::new(HashMap::new);

/// Whether community store is initialized
pub static COMMUNITY_INITIALIZED: GlobalSignal<bool> = GlobalSignal::new(|| false);

/// Loading state
pub static LOADING_COMMUNITIES: GlobalSignal<bool> = GlobalSignal::new(|| false);
pub static LOADING_POSTS: GlobalSignal<bool> = GlobalSignal::new(|| false);

// ============================================================================
// Cache Functions
// ============================================================================

/// Get a community from cache by a_tag
pub fn get_cached_community(a_tag: &str) -> Option<Community> {
    COMMUNITIES_CACHE.read().peek(a_tag).cloned()
}

/// Get a community from cache by naddr
pub fn get_cached_community_by_naddr(naddr: &str) -> Option<Community> {
    let cache = COMMUNITIES_CACHE.read();
    cache
        .iter()
        .find(|(_, comm)| comm.naddr == naddr)
        .map(|(_, comm)| comm.clone())
}

/// Cache a community
pub fn cache_community(community: Community) {
    COMMUNITIES_CACHE.write().put(community.a_tag.clone(), community);
}

/// Cache multiple communities
pub fn cache_communities(communities: &[Community]) {
    let mut cache = COMMUNITIES_CACHE.write();
    for community in communities {
        cache.put(community.a_tag.clone(), community.clone());
    }
}

/// Get a post from cache by event_id
pub fn get_cached_post(event_id: &str) -> Option<CommunityPost> {
    POSTS_CACHE.read().peek(event_id).cloned()
}

/// Cache a post
pub fn cache_post(post: CommunityPost) {
    POSTS_CACHE.write().put(post.id.clone(), post);
}

/// Cache multiple posts
pub fn cache_posts(posts: &[CommunityPost]) {
    let mut cache = POSTS_CACHE.write();
    for post in posts {
        cache.put(post.id.clone(), post.clone());
    }
}

/// Get all cached communities
pub fn get_all_cached_communities() -> Vec<Community> {
    let cache = COMMUNITIES_CACHE.read();
    cache.iter().map(|(_, comm)| comm.clone()).collect()
}

// ============================================================================
// Parsing Functions
// ============================================================================

/// Parse community definition (kind 34550)
pub fn parse_community_event(event: &NostrEvent) -> Option<Community> {
    use nostr_sdk::{TagKind, SingleLetterTag, Alphabet};

    if event.kind.as_u16() != KIND_COMMUNITY_DEFINITION {
        return None;
    }

    // Extract d-tag (required)
    let d_tag = event.tags.iter()
        .find(|t| t.kind() == TagKind::SingleLetter(SingleLetterTag::lowercase(Alphabet::D)))
        .and_then(|t| t.content())
        .map(|s| s.to_string())?;

    // Extract optional multi-letter tags
    let name = extract_tag_value(&event.tags, "name");
    let description = extract_tag_value(&event.tags, "description");
    let image = extract_tag_value(&event.tags, "image");
    let banner = extract_tag_value(&event.tags, "banner");
    let rules = extract_tag_value(&event.tags, "rules");

    // Extract moderators (p-tags with "moderator" marker)
    let moderators: Vec<String> = event.tags.iter()
        .filter(|t| t.kind() == TagKind::p())
        .filter(|t| {
            let slice = t.as_slice();
            slice.get(3).map(|s| s.as_str()) == Some("moderator")
        })
        .filter_map(|t| t.content().map(|s| s.to_string()))
        .collect();

    // Build a_tag
    let a_tag = format!("{}:{}:{}", KIND_COMMUNITY_DEFINITION, event.pubkey.to_hex(), d_tag);

    // Build naddr
    let naddr = match Coordinate::new(Kind::Custom(KIND_COMMUNITY_DEFINITION), event.pubkey)
        .identifier(&d_tag)
        .to_bech32()
    {
        Ok(n) => n,
        Err(_) => return None,
    };

    Some(Community {
        id: event.id.to_hex(),
        pubkey: event.pubkey.to_hex(),
        d_tag,
        name,
        description,
        image,
        banner,
        rules,
        moderators,
        a_tag,
        naddr,
        created_at: event.created_at.as_secs(),
        event: event.clone(),
    })
}

/// Parse community post (kind 1111 or 1)
pub fn parse_community_post(event: &NostrEvent, community_a_tag: &str) -> Option<CommunityPost> {
    let kind = event.kind.as_u16();
    if kind != KIND_COMMUNITY_POST && kind != 1 {
        return None;
    }

    // Extract uppercase A tag (root community scope - NIP-22)
    let big_a_tag = extract_uppercase_tag(&event.tags, "A");

    // Extract lowercase a tag (parent scope)
    let small_a_tag = event.tags.iter()
        .find(|t| t.kind() == TagKind::a())
        .and_then(|t| t.content())
        .map(|s| s.to_string());

    // Verify this post belongs to the specified community
    let post_community = big_a_tag.as_ref().or(small_a_tag.as_ref());
    if post_community != Some(&community_a_tag.to_string()) {
        return None;
    }

    // Determine if top-level (A == a or no parent event)
    let is_top_level = big_a_tag == small_a_tag ||
        event.tags.iter().all(|t| t.kind() != TagKind::e());

    // Extract parent info for replies
    let parent_id = event.tags.iter()
        .find(|t| t.kind() == TagKind::e())
        .and_then(|t| t.content())
        .map(|s| s.to_string());

    // For replies, get parent author from lowercase p tag
    let parent_pubkey = if !is_top_level {
        event.tags.iter()
            .filter(|t| t.kind() == TagKind::p())
            .last()
            .and_then(|t| t.content())
            .map(|s| s.to_string())
    } else {
        None
    };

    Some(CommunityPost {
        id: event.id.to_hex(),
        pubkey: event.pubkey.to_hex(),
        content: event.content.clone(),
        community_a_tag: community_a_tag.to_string(),
        parent_id,
        parent_pubkey,
        kind,
        created_at: event.created_at.as_secs(),
        event: event.clone(),
        approval_status: ApprovalStatus::Pending,  // Will be computed separately
        is_top_level,
    })
}

/// Parse approval event (kind 4550)
pub fn parse_approval_event(event: &NostrEvent) -> Option<(String, Approval)> {
    if event.kind.as_u16() != KIND_APPROVAL {
        return None;
    }

    // Get the post event ID from e tag
    let post_id = event.tags.iter()
        .find(|t| t.kind() == TagKind::e())
        .and_then(|t| t.content())
        .map(|s| s.to_string())?;

    Some((post_id, Approval {
        event_id: event.id.to_hex(),
        moderator_pubkey: event.pubkey.to_hex(),
        approved_at: event.created_at.as_secs(),
    }))
}

/// Parse removal event (kind 4551)
pub fn parse_removal_event(event: &NostrEvent) -> Option<(String, Removal)> {
    if event.kind.as_u16() != KIND_REMOVAL {
        return None;
    }

    let post_id = event.tags.iter()
        .find(|t| t.kind() == TagKind::e())
        .and_then(|t| t.content())
        .map(|s| s.to_string())?;

    Some((post_id, Removal {
        event_id: event.id.to_hex(),
        moderator_pubkey: event.pubkey.to_hex(),
        removed_at: event.created_at.as_secs(),
        reason: if event.content.is_empty() { None } else { Some(event.content.clone()) },
    }))
}

/// Helper to extract uppercase tags (A, K, P for NIP-22)
fn extract_uppercase_tag(tags: &Tags, tag_name: &str) -> Option<String> {
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

/// Helper to extract multi-letter tag values
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

// ============================================================================
// Role Detection (Cascading Auto-Approval)
// ============================================================================

/// Determine user's role in a community
pub fn get_user_role(user_pubkey: &str, community: &Community) -> UserRole {
    // Owner check
    if community.pubkey == user_pubkey {
        return UserRole::Owner;
    }

    // Moderator check
    if community.moderators.contains(&user_pubkey.to_string()) {
        return UserRole::Moderator;
    }

    // Approved member check
    let approved_members = APPROVED_MEMBERS_CACHE.read();
    if let Some(members) = approved_members.get(&community.a_tag) {
        if members.contains(user_pubkey) {
            return UserRole::ApprovedMember;
        }
    }

    UserRole::Visitor
}

/// Check if a post should be auto-approved based on author's role
pub fn compute_approval_status(post: &CommunityPost, community: &Community) -> ApprovalStatus {
    let role = get_user_role(&post.pubkey, community);

    match role {
        UserRole::Owner => ApprovalStatus::AutoApproved(AutoApprovalReason::Owner),
        UserRole::Moderator => ApprovalStatus::AutoApproved(AutoApprovalReason::Moderator),
        UserRole::ApprovedMember => ApprovalStatus::AutoApproved(AutoApprovalReason::ApprovedMember),
        // Pending/Declined users are treated as visitors for approval purposes
        UserRole::Pending | UserRole::Declined | UserRole::Visitor => {
            // Check for removal first
            let removals = REMOVALS_CACHE.read();
            if let Some(removal) = removals.get(&post.id) {
                return ApprovalStatus::Removed(removal.clone());
            }

            // Check for manual approvals
            let approvals = APPROVALS_CACHE.read();
            if let Some(post_approvals) = approvals.get(&post.id) {
                // Verify at least one approval is from a moderator/owner
                let valid_approvals: Vec<_> = post_approvals.iter()
                    .filter(|a| {
                        community.pubkey == a.moderator_pubkey ||
                        community.moderators.contains(&a.moderator_pubkey)
                    })
                    .cloned()
                    .collect();

                if !valid_approvals.is_empty() {
                    return ApprovalStatus::Approved(valid_approvals);
                }
            }

            ApprovalStatus::Pending
        }
    }
}

/// Check if current user can moderate (is owner or moderator)
pub fn can_moderate(user_pubkey: &str, community: &Community) -> bool {
    matches!(
        get_user_role(user_pubkey, community),
        UserRole::Owner | UserRole::Moderator
    )
}

/// Get detailed membership status for a user in a community
/// Checks: Owner > Moderator > Member > Banned > Declined > Pending > None
pub fn get_membership_status(user_pubkey: &str, community: &Community) -> MembershipStatus {
    // Owner check
    if community.pubkey == user_pubkey {
        return MembershipStatus::Owner;
    }

    // Moderator check
    if community.moderators.contains(&user_pubkey.to_string()) {
        return MembershipStatus::Moderator;
    }

    // Approved member check
    let approved_members = APPROVED_MEMBERS_CACHE.read();
    if let Some(members) = approved_members.get(&community.a_tag) {
        if members.contains(user_pubkey) {
            return MembershipStatus::Member;
        }
    }
    drop(approved_members);

    // Banned check
    let banned_members = BANNED_MEMBERS_CACHE.read();
    if let Some(banned) = banned_members.get(&community.a_tag) {
        if banned.contains(user_pubkey) {
            return MembershipStatus::Banned { reason: None };
        }
    }
    drop(banned_members);

    // Declined check
    let declined_members = DECLINED_MEMBERS_CACHE.read();
    if let Some(declined) = declined_members.get(&community.a_tag) {
        if declined.contains(user_pubkey) {
            return MembershipStatus::Declined { reason: None };
        }
    }
    drop(declined_members);

    // Pending join request check
    let user_pending = USER_PENDING_REQUESTS.read();
    if let Some(request) = user_pending.get(&community.a_tag) {
        if request.user_pubkey == user_pubkey {
            return MembershipStatus::Pending {
                request_id: request.id.clone(),
                requested_at: request.created_at,
            };
        }
    }

    MembershipStatus::None
}

/// Convert MembershipStatus to UserRole (for backward compatibility)
pub fn membership_status_to_role(status: &MembershipStatus) -> UserRole {
    match status {
        MembershipStatus::Owner => UserRole::Owner,
        MembershipStatus::Moderator => UserRole::Moderator,
        MembershipStatus::Member => UserRole::ApprovedMember,
        MembershipStatus::Pending { .. } => UserRole::Pending,
        MembershipStatus::Declined { .. } => UserRole::Declined,
        MembershipStatus::Banned { .. } => UserRole::Visitor,
        MembershipStatus::None => UserRole::Visitor,
    }
}

/// Sort communities by membership priority for display
/// Priority: Pinned > Owner > Moderator > Member > Pending > Others
pub fn sort_communities_by_membership(
    communities: Vec<Community>,
    user_pubkey: Option<&str>,
    pinned_a_tags: &HashSet<String>,
) -> Vec<CommunityWithMembership> {
    let mut result: Vec<CommunityWithMembership> = communities
        .into_iter()
        .map(|community| {
            let membership_status = user_pubkey
                .map(|pk| get_membership_status(pk, &community))
                .unwrap_or(MembershipStatus::None);

            let is_pinned = pinned_a_tags.contains(&community.a_tag);

            // Get pending request count for moderators/owners
            let pending_request_count = match &membership_status {
                MembershipStatus::Owner | MembershipStatus::Moderator => {
                    PENDING_JOIN_REQUESTS_CACHE.read()
                        .get(&community.a_tag)
                        .map(|requests| requests.len() as u32)
                }
                _ => None,
            };

            CommunityWithMembership {
                community,
                membership_status,
                is_pinned,
                pending_request_count,
            }
        })
        .collect();

    // Sort by priority: pinned first, then by role, then by name
    result.sort_by(|a, b| {
        // Pinned communities first
        match (a.is_pinned, b.is_pinned) {
            (true, false) => return std::cmp::Ordering::Less,
            (false, true) => return std::cmp::Ordering::Greater,
            _ => {}
        }

        // Then by membership priority
        let priority = |status: &MembershipStatus| -> u8 {
            match status {
                MembershipStatus::Owner => 0,
                MembershipStatus::Moderator => 1,
                MembershipStatus::Member => 2,
                MembershipStatus::Pending { .. } => 3,
                MembershipStatus::Declined { .. } => 4,
                MembershipStatus::Banned { .. } => 5,
                MembershipStatus::None => 6,
            }
        };

        let a_priority = priority(&a.membership_status);
        let b_priority = priority(&b.membership_status);

        match a_priority.cmp(&b_priority) {
            std::cmp::Ordering::Equal => {
                // Same priority - sort by name
                let a_name = a.community.name.as_ref().unwrap_or(&a.community.d_tag);
                let b_name = b.community.name.as_ref().unwrap_or(&b.community.d_tag);
                a_name.to_lowercase().cmp(&b_name.to_lowercase())
            }
            other => other,
        }
    });

    result
}

// ============================================================================
// Filter Builders (Nostr queries)
// ============================================================================

/// Build filter for fetching communities (with optional limit)
pub fn communities_filter(limit: usize) -> Filter {
    Filter::new()
        .kind(Kind::Custom(KIND_COMMUNITY_DEFINITION))
        .limit(limit)
}

/// Build filter for a specific community by coordinate
pub fn community_by_coord_filter(pubkey: PublicKey, identifier: &str) -> Filter {
    Filter::new()
        .kind(Kind::Custom(KIND_COMMUNITY_DEFINITION))
        .author(pubkey)
        .identifier(identifier)
}

/// Build filter for community posts (both kind 1111 and kind 1 for backwards compat)
/// Uses uppercase A tag for NIP-22 root scope
pub fn posts_filter_by_community(community_a_tag: &str, limit: usize, until: Option<u64>) -> Filter {
    let mut filter = Filter::new()
        .kinds(vec![Kind::Comment, Kind::TextNote])
        .custom_tag(SingleLetterTag::uppercase(Alphabet::A), community_a_tag)
        .limit(limit);

    if let Some(ts) = until {
        filter = filter.until(Timestamp::from(ts));
    }

    filter
}

/// Build filter for top-level posts only (where k tag is "34550")
pub fn top_level_posts_filter(community_a_tag: &str, limit: usize) -> Filter {
    Filter::new()
        .kinds(vec![Kind::Comment, Kind::TextNote])
        .custom_tag(SingleLetterTag::uppercase(Alphabet::A), community_a_tag)
        .custom_tag(SingleLetterTag::lowercase(Alphabet::K), "34550")
        .limit(limit)
}

/// Build filter for replies to a specific post
pub fn replies_filter(parent_event_id: &str, community_a_tag: &str, limit: usize) -> Filter {
    Filter::new()
        .kinds(vec![Kind::Comment, Kind::TextNote])
        .custom_tag(SingleLetterTag::uppercase(Alphabet::A), community_a_tag)
        .custom_tag(SingleLetterTag::lowercase(Alphabet::E), parent_event_id)
        .limit(limit)
}

/// Build filter for approval events (kind 4550)
pub fn approvals_filter_by_community(community_a_tag: &str, limit: usize) -> Filter {
    Filter::new()
        .kind(Kind::Custom(KIND_APPROVAL))
        .custom_tag(SingleLetterTag::lowercase(Alphabet::A), community_a_tag)
        .limit(limit)
}

/// Build filter for removal events (kind 4551)
pub fn removals_filter_by_community(community_a_tag: &str) -> Filter {
    Filter::new()
        .kind(Kind::Custom(KIND_REMOVAL))
        .custom_tag(SingleLetterTag::lowercase(Alphabet::A), community_a_tag)
}

/// Build filter for approved members list (kind 34551)
pub fn approved_members_filter(community_a_tag: &str) -> Filter {
    // Parse a_tag to get pubkey
    let parts: Vec<&str> = community_a_tag.split(':').collect();
    if parts.len() != 3 {
        return Filter::new().kind(Kind::Custom(KIND_APPROVED_MEMBERS)).limit(0);
    }

    let pubkey = match PublicKey::from_hex(parts[1]) {
        Ok(pk) => pk,
        Err(_) => return Filter::new().kind(Kind::Custom(KIND_APPROVED_MEMBERS)).limit(0),
    };

    Filter::new()
        .kind(Kind::Custom(KIND_APPROVED_MEMBERS))
        .author(pubkey)
        .custom_tag(SingleLetterTag::lowercase(Alphabet::A), community_a_tag)
        .limit(1)
}

// ============================================================================
// Fetch Functions
// ============================================================================

/// Fetch all communities
pub async fn fetch_communities(limit: usize) -> std::result::Result<Vec<Community>, String> {
    *LOADING_COMMUNITIES.write() = true;

    let filter = communities_filter(limit);
    let result = crate::stores::nostr_client::fetch_events_aggregated(
        filter,
        Duration::from_secs(15)
    ).await;

    *LOADING_COMMUNITIES.write() = false;

    match result {
        Ok(events) => {
            let communities: Vec<Community> = events
                .iter()
                .filter_map(parse_community_event)
                .collect();

            // Cache communities
            cache_communities(&communities);
            *COMMUNITY_INITIALIZED.write() = true;

            log::info!("Fetched {} communities", communities.len());
            Ok(communities)
        }
        Err(e) => {
            log::error!("Failed to fetch communities: {}", e);
            Err(e)
        }
    }
}

/// Fetch a specific community by naddr
pub async fn fetch_community_by_naddr(naddr: &str) -> std::result::Result<Option<Community>, String> {
    // Check cache first
    if let Some(cached) = get_cached_community_by_naddr(naddr) {
        return Ok(Some(cached));
    }

    // Parse naddr
    let coord = Coordinate::from_bech32(naddr)
        .map_err(|e| format!("Invalid naddr: {}", e))?;

    let filter = community_by_coord_filter(coord.public_key, &coord.identifier);

    let events = crate::stores::nostr_client::fetch_events_aggregated(
        filter,
        Duration::from_secs(10)
    ).await?;

    if let Some(event) = events.first() {
        if let Some(community) = parse_community_event(event) {
            cache_community(community.clone());
            return Ok(Some(community));
        }
    }

    Ok(None)
}

/// Fetch a specific community by a_tag
pub async fn fetch_community_by_a_tag(a_tag: &str) -> std::result::Result<Option<Community>, String> {
    // Check cache first
    if let Some(cached) = get_cached_community(a_tag) {
        return Ok(Some(cached));
    }

    // Parse a_tag: "34550:pubkey:d_tag"
    let parts: Vec<&str> = a_tag.split(':').collect();
    if parts.len() != 3 {
        return Err("Invalid a_tag format".to_string());
    }

    let pubkey = PublicKey::from_hex(parts[1])
        .map_err(|e| format!("Invalid pubkey in a_tag: {}", e))?;
    let identifier = parts[2];

    let filter = community_by_coord_filter(pubkey, identifier);

    let events = crate::stores::nostr_client::fetch_events_aggregated(
        filter,
        Duration::from_secs(10)
    ).await?;

    if let Some(event) = events.first() {
        if let Some(community) = parse_community_event(event) {
            cache_community(community.clone());
            return Ok(Some(community));
        }
    }

    Ok(None)
}

/// Fetch posts for a community with approval status computation
pub async fn fetch_community_posts(
    community: &Community,
    limit: usize,
    include_pending: bool,
    until: Option<u64>,
) -> std::result::Result<Vec<CommunityPost>, String> {
    *LOADING_POSTS.write() = true;

    // Fetch posts, approvals, and removals in parallel
    let posts_filter = posts_filter_by_community(&community.a_tag, limit, until);
    let approvals_filter = approvals_filter_by_community(&community.a_tag, 500);
    let removals_filter = removals_filter_by_community(&community.a_tag);

    let (posts_result, approvals_result, removals_result) = futures::join!(
        crate::stores::nostr_client::fetch_events_aggregated(posts_filter, Duration::from_secs(15)),
        crate::stores::nostr_client::fetch_events_aggregated(approvals_filter, Duration::from_secs(10)),
        crate::stores::nostr_client::fetch_events_aggregated(removals_filter, Duration::from_secs(5)),
    );

    *LOADING_POSTS.write() = false;

    // Cache approvals
    if let Ok(approval_events) = approvals_result {
        let mut approvals_cache = APPROVALS_CACHE.write();
        for event in &approval_events {
            if let Some((post_id, approval)) = parse_approval_event(event) {
                approvals_cache.entry(post_id).or_default().push(approval);
            }
        }
    }

    // Cache removals
    if let Ok(removal_events) = removals_result {
        let mut removals_cache = REMOVALS_CACHE.write();
        for event in &removal_events {
            if let Some((post_id, removal)) = parse_removal_event(event) {
                removals_cache.insert(post_id, removal);
            }
        }
    }

    let posts_events = posts_result?;

    // Parse and compute approval status for each post
    let mut posts: Vec<CommunityPost> = posts_events
        .iter()
        .filter_map(|e| parse_community_post(e, &community.a_tag))
        .map(|mut post| {
            post.approval_status = compute_approval_status(&post, community);
            post
        })
        .collect();

    // Filter based on include_pending flag
    if !include_pending {
        posts.retain(|p| !matches!(p.approval_status, ApprovalStatus::Pending | ApprovalStatus::Removed(_)));
    }

    // Sort by created_at (newest first)
    posts.sort_by(|a, b| b.created_at.cmp(&a.created_at));

    // Cache posts
    cache_posts(&posts);

    log::info!("Fetched {} posts for community {}", posts.len(), community.a_tag);
    Ok(posts)
}

/// Fetch pending posts for moderation queue
pub async fn fetch_pending_posts(community: &Community) -> std::result::Result<Vec<CommunityPost>, String> {
    let all_posts = fetch_community_posts(community, 200, true, None).await?;

    Ok(all_posts
        .into_iter()
        .filter(|p| matches!(p.approval_status, ApprovalStatus::Pending))
        .collect())
}

// ============================================================================
// Publishing Functions
// ============================================================================

/// Create a new community (kind 34550)
pub async fn create_community(
    identifier: &str,
    name: &str,
    description: Option<&str>,
    image: Option<&str>,
    rules: Option<&str>,
    moderators: Vec<String>,
) -> std::result::Result<String, String> {
    let client = crate::stores::nostr_client::get_client()
        .ok_or("Client not initialized")?;

    if !*crate::stores::nostr_client::HAS_SIGNER.read() {
        return Err("No signer attached".to_string());
    }

    let mut tags: Vec<Tag> = vec![
        Tag::identifier(identifier),
        Tag::custom(TagKind::Custom("name".into()), vec![name]),
    ];

    if let Some(desc) = description {
        tags.push(Tag::custom(TagKind::Custom("description".into()), vec![desc]));
    }

    if let Some(img) = image {
        tags.push(Tag::custom(TagKind::Custom("image".into()), vec![img]));
    }

    if let Some(r) = rules {
        tags.push(Tag::custom(TagKind::Custom("rules".into()), vec![r]));
    }

    // Add moderators with "moderator" marker
    for mod_pubkey in moderators {
        tags.push(Tag::custom(TagKind::p(), vec![mod_pubkey, "".to_string(), "moderator".to_string()]));
    }

    let builder = EventBuilder::new(Kind::Custom(KIND_COMMUNITY_DEFINITION), "")
        .tags(tags);

    let output = client.send_event_builder(builder).await
        .map_err(|e| format!("Failed to publish community: {}", e))?;

    log::info!("Community created: {}", output.id().to_hex());
    Ok(output.id().to_hex())
}

/// Post to a community (kind 1111 with NIP-22 tags)
pub async fn post_to_community(
    community: &Community,
    content: &str,
) -> std::result::Result<String, String> {
    let client = crate::stores::nostr_client::get_client()
        .ok_or("Client not initialized")?;

    if !*crate::stores::nostr_client::HAS_SIGNER.read() {
        return Err("No signer attached".to_string());
    }

    // Parse community coordinate
    let coord = Coordinate::new(Kind::Custom(KIND_COMMUNITY_DEFINITION),
        PublicKey::from_hex(&community.pubkey).map_err(|e| e.to_string())?)
        .identifier(&community.d_tag);

    // Use EventBuilder::comment with CommentTarget::coordinate for NIP-22
    let target = CommentTarget::coordinate(Cow::Owned(coord), None);
    let builder = EventBuilder::comment(content, target, None);

    let output = client.send_event_builder(builder).await
        .map_err(|e| format!("Failed to publish post: {}", e))?;

    log::info!("Community post published: {}", output.id().to_hex());
    Ok(output.id().to_hex())
}

/// Reply to a community post (kind 1111 with NIP-22 reply tags)
pub async fn reply_to_post(
    community: &Community,
    parent_post: &CommunityPost,
    content: &str,
) -> std::result::Result<String, String> {
    let client = crate::stores::nostr_client::get_client()
        .ok_or("Client not initialized")?;

    if !*crate::stores::nostr_client::HAS_SIGNER.read() {
        return Err("No signer attached".to_string());
    }

    // Parse community coordinate (root)
    let community_coord = Coordinate::new(Kind::Custom(KIND_COMMUNITY_DEFINITION),
        PublicKey::from_hex(&community.pubkey).map_err(|e| e.to_string())?)
        .identifier(&community.d_tag);

    // Parse parent event ID
    let parent_id = EventId::from_hex(&parent_post.id)
        .map_err(|e| format!("Invalid parent event ID: {}", e))?;

    let parent_pubkey = PublicKey::from_hex(&parent_post.pubkey)
        .map_err(|e| format!("Invalid parent pubkey: {}", e))?;

    // Root = community, Parent = the post we're replying to
    let root_target = CommentTarget::coordinate(Cow::Owned(community_coord), None);
    let parent_target = CommentTarget::event(parent_id, Kind::Comment, Some(parent_pubkey), None);

    let builder = EventBuilder::comment(content, parent_target, Some(root_target));

    let output = client.send_event_builder(builder).await
        .map_err(|e| format!("Failed to publish reply: {}", e))?;

    log::info!("Community reply published: {}", output.id().to_hex());
    Ok(output.id().to_hex())
}

/// Approve a post (kind 4550)
pub async fn approve_post(
    community: &Community,
    post: &CommunityPost,
) -> std::result::Result<String, String> {
    let client = crate::stores::nostr_client::get_client()
        .ok_or("Client not initialized")?;

    if !*crate::stores::nostr_client::HAS_SIGNER.read() {
        return Err("No signer attached".to_string());
    }

    // Verify caller is moderator/owner
    let current_pubkey = crate::stores::auth_store::get_pubkey()
        .ok_or("Not logged in")?;

    if !can_moderate(&current_pubkey, community) {
        return Err("You are not a moderator of this community".to_string());
    }

    // Include the full post JSON in content (per NIP-72)
    let post_json = serde_json::to_string(&post.event)
        .unwrap_or_default();

    // Build coordinate for community
    let coord = Coordinate::new(Kind::Custom(KIND_COMMUNITY_DEFINITION),
        PublicKey::from_hex(&community.pubkey).map_err(|e| e.to_string())?)
        .identifier(&community.d_tag);

    let tags: Vec<Tag> = vec![
        Tag::coordinate(coord, None),
        Tag::event(EventId::from_hex(&post.id).map_err(|e| e.to_string())?),
        Tag::public_key(PublicKey::from_hex(&post.pubkey).map_err(|e| e.to_string())?),
        Tag::custom(TagKind::Custom("k".into()), vec![post.kind.to_string()]),
    ];

    let builder = EventBuilder::new(Kind::Custom(KIND_APPROVAL), &post_json)
        .tags(tags);

    let output = client.send_event_builder(builder).await
        .map_err(|e| format!("Failed to approve post: {}", e))?;

    log::info!("Post approved: {}", output.id().to_hex());
    Ok(output.id().to_hex())
}

/// Remove a post (kind 4551)
pub async fn remove_post(
    community: &Community,
    post: &CommunityPost,
    reason: Option<&str>,
) -> std::result::Result<String, String> {
    let client = crate::stores::nostr_client::get_client()
        .ok_or("Client not initialized")?;

    if !*crate::stores::nostr_client::HAS_SIGNER.read() {
        return Err("No signer attached".to_string());
    }

    // Verify caller is moderator/owner
    let current_pubkey = crate::stores::auth_store::get_pubkey()
        .ok_or("Not logged in")?;

    if !can_moderate(&current_pubkey, community) {
        return Err("You are not a moderator of this community".to_string());
    }

    let content = reason.unwrap_or("");

    // Build coordinate for community
    let coord = Coordinate::new(Kind::Custom(KIND_COMMUNITY_DEFINITION),
        PublicKey::from_hex(&community.pubkey).map_err(|e| e.to_string())?)
        .identifier(&community.d_tag);

    let tags: Vec<Tag> = vec![
        Tag::coordinate(coord, None),
        Tag::event(EventId::from_hex(&post.id).map_err(|e| e.to_string())?),
        Tag::public_key(PublicKey::from_hex(&post.pubkey).map_err(|e| e.to_string())?),
        Tag::custom(TagKind::Custom("k".into()), vec![post.kind.to_string()]),
    ];

    let builder = EventBuilder::new(Kind::Custom(KIND_REMOVAL), content)
        .tags(tags);

    let output = client.send_event_builder(builder).await
        .map_err(|e| format!("Failed to remove post: {}", e))?;

    log::info!("Post removed: {}", output.id().to_hex());
    Ok(output.id().to_hex())
}

// ============================================================================
// Enhancement Functions (Phase 1)
// ============================================================================

/// Fetch communities where user is owner, moderator, or approved member
/// Used to show "Your Communities" at top of /communities page
pub async fn fetch_user_communities(user_pubkey: &str) -> std::result::Result<Vec<Community>, String> {
    let client = crate::stores::nostr_client::get_client()
        .ok_or("Client not initialized")?;

    // Wait for at least one relay to be connected before fetching
    crate::stores::nostr_client::ensure_relays_ready(&client).await;

    let pubkey = PublicKey::from_hex(user_pubkey)
        .map_err(|e| format!("Invalid pubkey: {}", e))?;

    // Filter 1: Communities where user is owner (author)
    let owned_filter = Filter::new()
        .kind(Kind::Custom(KIND_COMMUNITY_DEFINITION))
        .author(pubkey);

    // Filter 2: Communities where user is in p tags (moderator/member)
    // Note: Not all relays support #p tag filtering for kind 34550
    let mod_filter = Filter::new()
        .kind(Kind::Custom(KIND_COMMUNITY_DEFINITION))
        .pubkey(pubkey);

    // Filter 3: Approved member lists where user is listed (kind 34551 with user's p-tag)
    // This finds communities where the user is an approved member
    let member_filter = Filter::new()
        .kind(Kind::Custom(KIND_APPROVED_MEMBERS))
        .pubkey(pubkey);

    // Filter 4: Fetch recent communities to check client-side (fallback for relays that don't index p-tags)
    let recent_filter = Filter::new()
        .kind(Kind::Custom(KIND_COMMUNITY_DEFINITION))
        .limit(200);

    // Fetch all in parallel
    let (owned_result, mod_result, member_result, recent_result) = futures::join!(
        client.fetch_events(owned_filter, Duration::from_secs(5)),
        client.fetch_events(mod_filter, Duration::from_secs(5)),
        client.fetch_events(member_filter, Duration::from_secs(5)),
        client.fetch_events(recent_filter, Duration::from_secs(5)),
    );

    let owned_events = owned_result.map_err(|e| format!("Failed to fetch owned: {}", e))?;
    let mod_events = mod_result.unwrap_or_default(); // Don't fail if p-tag query not supported
    let member_events = member_result.unwrap_or_default(); // Approved member lists containing user
    let recent_events = recent_result.unwrap_or_default();

    log::info!("User communities query: owned={}, mod_filter={}, member_lists={}, recent={}",
        owned_events.len(), mod_events.len(), member_events.len(), recent_events.len());

    // Dedupe by a_tag and parse
    let mut seen = HashSet::new();
    let mut communities = Vec::new();
    let user_pk_str = user_pubkey.to_lowercase();

    // Process owned communities (author matches)
    for event in owned_events.into_iter() {
        if let Some(community) = parse_community_event(&event) {
            if seen.insert(community.a_tag.clone()) {
                communities.push(community);
            }
        }
    }

    // Process mod-filtered results
    for event in mod_events.into_iter() {
        if let Some(community) = parse_community_event(&event) {
            // Verify user is actually a moderator (has p-tag with "moderator" role)
            if community.moderators.iter().any(|m| m.to_lowercase() == user_pk_str)
                && seen.insert(community.a_tag.clone()) {
                    communities.push(community);
                }
        }
    }

    // Process approved member list events - extract community a_tags where user is approved
    // These are kind 34551 events with the community's a_tag in 'a' tag and user in p-tags
    let mut member_community_a_tags: HashSet<String> = HashSet::new();
    for event in member_events.into_iter() {
        log::info!("Processing member list event {} (kind {})", event.id.to_hex(), event.kind.as_u16());

        // Log all tags for debugging
        let tag_summary: Vec<String> = event.tags.iter()
            .map(|t| format!("{}:{}", t.kind(), t.content().unwrap_or("(none)")))
            .collect();
        log::info!("  Event tags: {:?}", tag_summary);

        // Try 'a' tag first (standard approach)
        if let Some(a_tag) = event.tags.iter()
            .find(|t| t.kind() == TagKind::a())
            .and_then(|t| t.content())
        {
            let a_tag_str = a_tag.to_string();
            log::info!("Found community a_tag in approved member list: {}", a_tag_str);

            // Verify it's a community definition (kind 34550)
            if a_tag_str.starts_with(&format!("{}:", KIND_COMMUNITY_DEFINITION)) {
                member_community_a_tags.insert(a_tag_str.clone());

                // Cache the approved members for this community
                let members: HashSet<String> = event.tags.iter()
                    .filter(|t| t.kind() == TagKind::p())
                    .filter_map(|t| t.content().map(|s| s.to_lowercase()))
                    .collect();

                log::info!("Caching {} approved members for community {}", members.len(), &a_tag_str);
                APPROVED_MEMBERS_CACHE.write().insert(a_tag_str, members);
            } else {
                log::warn!("Approved member list has non-community a_tag: {}", a_tag_str);
            }
        } else {
            // Try 'd' tag as fallback - some implementations use d tag with community identifier
            if let Some(d_tag) = event.tags.iter()
                .find(|t| t.kind() == TagKind::d())
                .and_then(|t| t.content())
            {
                log::info!("  No 'a' tag, but found 'd' tag: {}", d_tag);

                // The d_tag might be the community a_tag directly, or just an identifier
                // Try to construct the a_tag if it looks like a community reference
                let potential_a_tag = if d_tag.starts_with(&format!("{}:", KIND_COMMUNITY_DEFINITION)) {
                    d_tag.to_string()
                } else {
                    // d_tag might just be the community name/identifier
                    // We'd need to know the owner pubkey to construct the full a_tag
                    // For now, skip these
                    log::warn!("  d_tag '{}' doesn't look like a community a_tag", d_tag);
                    continue;
                };

                member_community_a_tags.insert(potential_a_tag.clone());

                let members: HashSet<String> = event.tags.iter()
                    .filter(|t| t.kind() == TagKind::p())
                    .filter_map(|t| t.content().map(|s| s.to_lowercase()))
                    .collect();

                log::info!("Caching {} approved members from d_tag for {}", members.len(), &potential_a_tag);
                APPROVED_MEMBERS_CACHE.write().insert(potential_a_tag, members);
            } else {
                log::warn!("Approved member list event {} has no 'a' or 'd' tag", event.id.to_hex());
            }
        }
    }

    // Fetch community definitions for communities where user is an approved member
    let member_community_count = member_community_a_tags.len();
    if !member_community_a_tags.is_empty() {
        log::info!("Fetching {} communities where user is approved member: {:?}",
            member_community_count,
            member_community_a_tags.iter().take(5).collect::<Vec<_>>());

        for a_tag in member_community_a_tags {
            // Skip if we already have this community
            if seen.contains(&a_tag) {
                log::debug!("Skipping already-seen community: {}", a_tag);
                continue;
            }

            // Try to fetch from cache first (use read, not write to avoid borrow conflicts)
            let cached = COMMUNITIES_CACHE.read().peek(&a_tag).cloned();
            if let Some(community) = cached {
                log::info!("Found member community in cache: {}", community.a_tag);
                if seen.insert(a_tag.clone()) {
                    communities.push(community);
                }
            } else {
                // Fetch from network (no locks held across await)
                log::info!("Fetching member community from network: {}", a_tag);
                match fetch_community_by_a_tag(&a_tag).await {
                    Ok(Some(community)) => {
                        log::info!("Successfully fetched member community: {}", community.a_tag);
                        if seen.insert(community.a_tag.clone()) {
                            communities.push(community);
                        }
                    }
                    Ok(None) => {
                        log::warn!("Community not found for a_tag: {}", a_tag);
                    }
                    Err(e) => {
                        log::error!("Failed to fetch community {}: {}", a_tag, e);
                    }
                }
            }
        }
    }

    // Process recent communities - check client-side if user is owner/moderator/member
    // This handles relays that don't support p-tag filtering
    for event in recent_events.into_iter() {
        if let Some(community) = parse_community_event(&event) {
            // Check if user is owner or moderator
            let is_owner = community.pubkey.to_lowercase() == user_pk_str;
            let is_moderator = community.moderators.iter().any(|m| m.to_lowercase() == user_pk_str);

            // Also check if user is in approved members cache
            let is_member = APPROVED_MEMBERS_CACHE.read()
                .get(&community.a_tag)
                .map(|members| members.contains(&user_pk_str))
                .unwrap_or(false);

            if (is_owner || is_moderator || is_member) && seen.insert(community.a_tag.clone()) {
                log::info!("Found user community from recent: {} (owner={}, mod={}, member={})",
                    community.name.as_ref().unwrap_or(&community.d_tag),
                    is_owner, is_moderator, is_member);
                communities.push(community);
            }
        }
    }

    // Sort by created_at (newest first)
    communities.sort_by(|a, b| b.created_at.cmp(&a.created_at));

    // Cache communities
    cache_communities(&communities);

    log::info!("Fetched {} user communities for {} ({} member communities attempted)",
        communities.len(), truncate_pubkey(user_pubkey), member_community_count);
    Ok(communities)
}

/// Search communities by name/description
/// Tries NIP-50 search first, falls back to fetching all and filtering client-side
pub async fn search_communities(query: &str, limit: usize) -> std::result::Result<Vec<Community>, String> {
    let client = crate::stores::nostr_client::get_client()
        .ok_or("Client not initialized")?;

    let query_lower = query.to_lowercase();

    // Try NIP-50 search first (if relay supports it)
    let search_filter = Filter::new()
        .kind(Kind::Custom(KIND_COMMUNITY_DEFINITION))
        .search(query)
        .limit(limit);

    let search_result = client.fetch_events(search_filter, Duration::from_secs(5)).await;

    // If NIP-50 returns results, use them
    if let Ok(events) = search_result {
        if !events.is_empty() {
            let communities: Vec<Community> = events
                .into_iter()
                .filter_map(|e| parse_community_event(&e))
                .collect();

            cache_communities(&communities);
            log::info!("NIP-50 search returned {} communities for '{}'", communities.len(), query);
            return Ok(communities);
        }
    }

    // Fallback: fetch more communities and filter client-side
    let fallback_filter = Filter::new()
        .kind(Kind::Custom(KIND_COMMUNITY_DEFINITION))
        .limit(500);  // Fetch more for better client-side search

    let events = client.fetch_events(fallback_filter, Duration::from_secs(10)).await
        .map_err(|e| format!("Search fallback failed: {}", e))?;

    let mut communities: Vec<Community> = events
        .into_iter()
        .filter_map(|e| parse_community_event(&e))
        .filter(|c| {
            // Match name or description
            c.name.as_ref().map(|n| n.to_lowercase().contains(&query_lower)).unwrap_or(false) ||
            c.description.as_ref().map(|d| d.to_lowercase().contains(&query_lower)).unwrap_or(false) ||
            c.d_tag.to_lowercase().contains(&query_lower)
        })
        .take(limit)
        .collect();

    // Sort by relevance (name match first, then description)
    communities.sort_by(|a, b| {
        let a_name_match = a.name.as_ref().map(|n| n.to_lowercase().contains(&query_lower)).unwrap_or(false);
        let b_name_match = b.name.as_ref().map(|n| n.to_lowercase().contains(&query_lower)).unwrap_or(false);
        b_name_match.cmp(&a_name_match)
    });

    cache_communities(&communities);
    log::info!("Client-side search returned {} communities for '{}'", communities.len(), query);
    Ok(communities)
}

/// Fetch communities with pagination (for infinite scroll)
/// Uses `until` timestamp to fetch older communities
pub async fn fetch_communities_page(
    limit: usize,
    until: Option<u64>,
) -> std::result::Result<Vec<Community>, String> {
    let client = crate::stores::nostr_client::get_client()
        .ok_or("Client not initialized")?;

    // Wait for at least one relay to be connected before fetching
    crate::stores::nostr_client::ensure_relays_ready(&client).await;

    let mut filter = Filter::new()
        .kind(Kind::Custom(KIND_COMMUNITY_DEFINITION))
        .limit(limit);

    // Add `until` for pagination
    if let Some(ts) = until {
        filter = filter.until(Timestamp::from(ts));
    }

    let events = client.fetch_events(filter, Duration::from_secs(10)).await
        .map_err(|e| format!("Failed to fetch communities page: {}", e))?;

    let events_count = events.len();
    let mut communities: Vec<Community> = events
        .into_iter()
        .filter_map(|e| parse_community_event(&e))
        .collect();

    // Sort by created_at (newest first)
    communities.sort_by(|a, b| b.created_at.cmp(&a.created_at));

    // Cache communities
    cache_communities(&communities);

    log::info!("fetch_communities_page: events={}, parsed={}, until={:?}",
        events_count, communities.len(), until);
    Ok(communities)
}

/// Fetch approved members list (kind 34551) for a community
pub async fn fetch_approved_members(community: &Community) -> std::result::Result<HashSet<String>, String> {
    let client = crate::stores::nostr_client::get_client()
        .ok_or("Client not initialized")?;

    let owner_pubkey = PublicKey::from_hex(&community.pubkey)
        .map_err(|e| format!("Invalid community pubkey: {}", e))?;

    // Filter for kind 34551 by community owner with matching d-tag
    let filter = Filter::new()
        .kind(Kind::Custom(KIND_APPROVED_MEMBERS))
        .author(owner_pubkey)
        .identifier(&community.d_tag)
        .limit(1);

    let events = client.fetch_events(filter, Duration::from_secs(5)).await
        .map_err(|e| format!("Failed to fetch approved members: {}", e))?;

    let mut members = HashSet::new();

    // Parse p tags from the most recent event
    if let Some(event) = events.into_iter().next() {
        for tag in event.tags.iter() {
            if tag.kind() == TagKind::p() {
                if let Some(pubkey) = tag.content() {
                    members.insert(pubkey.to_string());
                }
            }
        }
    }

    // Update cache
    APPROVED_MEMBERS_CACHE.write().insert(community.a_tag.clone(), members.clone());

    log::info!("Fetched {} approved members for {}", members.len(), community.a_tag);
    Ok(members)
}

/// Update approved members list (publishes new kind 34551)
pub async fn update_approved_members(
    community: &Community,
    members: Vec<String>,
) -> std::result::Result<String, String> {
    let client = crate::stores::nostr_client::get_client()
        .ok_or("Client not initialized")?;

    if !*crate::stores::nostr_client::HAS_SIGNER.read() {
        return Err("No signer attached".to_string());
    }

    // Verify caller is owner
    let current_pubkey = crate::stores::auth_store::get_pubkey()
        .ok_or("Not logged in")?;

    if community.pubkey != current_pubkey {
        return Err("Only the community owner can update approved members".to_string());
    }

    // Build tags: d-tag + a-tag + p-tags for members
    let mut tags: Vec<Tag> = vec![
        Tag::identifier(&community.d_tag),
        Tag::custom(TagKind::a(), vec![community.a_tag.clone()]),
    ];

    for member in &members {
        if let Ok(pk) = PublicKey::from_hex(member) {
            tags.push(Tag::public_key(pk));
        }
    }

    let builder = EventBuilder::new(Kind::Custom(KIND_APPROVED_MEMBERS), "")
        .tags(tags);

    let output = client.send_event_builder(builder).await
        .map_err(|e| format!("Failed to update approved members: {}", e))?;

    // Update cache
    APPROVED_MEMBERS_CACHE.write().insert(
        community.a_tag.clone(),
        members.into_iter().collect()
    );

    log::info!("Updated approved members: {}", output.id().to_hex());
    Ok(output.id().to_hex())
}

// ============================================================================
// Join Request Functions
// ============================================================================

/// Parse join request event (kind 4552)
pub fn parse_join_request(event: &NostrEvent) -> Option<JoinRequest> {
    if event.kind.as_u16() != KIND_JOIN_REQUEST {
        return None;
    }

    // Extract community a_tag from the a tag
    let community_a_tag = event.tags.iter()
        .find(|t| t.kind() == TagKind::a())
        .and_then(|t| t.content())
        .map(|s| s.to_string())?;

    Some(JoinRequest {
        id: event.id.to_hex(),
        community_a_tag,
        user_pubkey: event.pubkey.to_hex(),
        reason: if event.content.is_empty() { None } else { Some(event.content.clone()) },
        created_at: event.created_at.as_secs(),
        event: Some(event.clone()),
    })
}

/// Fetch user's pending join requests across all communities
pub async fn fetch_user_join_requests(user_pubkey: &str) -> std::result::Result<Vec<JoinRequest>, String> {
    let client = crate::stores::nostr_client::get_client()
        .ok_or("Client not initialized")?;

    let pubkey = PublicKey::from_hex(user_pubkey)
        .map_err(|e| format!("Invalid pubkey: {}", e))?;

    let filter = Filter::new()
        .kind(Kind::Custom(KIND_JOIN_REQUEST))
        .author(pubkey)
        .limit(100);

    let events = client.fetch_events(filter, Duration::from_secs(7)).await
        .map_err(|e| format!("Failed to fetch join requests: {}", e))?;

    let requests: Vec<JoinRequest> = events
        .into_iter()
        .filter_map(|e| parse_join_request(&e))
        .collect();

    // Update user's pending requests cache
    let mut cache = USER_PENDING_REQUESTS.write();
    for request in &requests {
        cache.insert(request.community_a_tag.clone(), request.clone());
    }

    log::info!("Fetched {} join requests for user", requests.len());
    Ok(requests)
}

/// Fetch pending join requests for a community (for moderators)
pub async fn fetch_community_join_requests(community: &Community) -> std::result::Result<Vec<JoinRequest>, String> {
    let client = crate::stores::nostr_client::get_client()
        .ok_or("Client not initialized")?;

    // Filter by a_tag pointing to this community
    let filter = Filter::new()
        .kind(Kind::Custom(KIND_JOIN_REQUEST))
        .custom_tag(SingleLetterTag::lowercase(Alphabet::A), &community.a_tag)
        .limit(200);

    let events = client.fetch_events(filter, Duration::from_secs(7)).await
        .map_err(|e| format!("Failed to fetch community join requests: {}", e))?;

    let requests: Vec<JoinRequest> = events
        .into_iter()
        .filter_map(|e| parse_join_request(&e))
        .collect();

    // Filter out users who are already approved, declined, or banned
    let approved = APPROVED_MEMBERS_CACHE.read();
    let declined = DECLINED_MEMBERS_CACHE.read();
    let banned = BANNED_MEMBERS_CACHE.read();

    let approved_set = approved.get(&community.a_tag);
    let declined_set = declined.get(&community.a_tag);
    let banned_set = banned.get(&community.a_tag);

    let pending_requests: Vec<JoinRequest> = requests
        .into_iter()
        .filter(|r| {
            let is_approved = approved_set.map(|s| s.contains(&r.user_pubkey)).unwrap_or(false);
            let is_declined = declined_set.map(|s| s.contains(&r.user_pubkey)).unwrap_or(false);
            let is_banned = banned_set.map(|s| s.contains(&r.user_pubkey)).unwrap_or(false);
            !is_approved && !is_declined && !is_banned
        })
        .collect();

    // Update cache
    PENDING_JOIN_REQUESTS_CACHE.write().insert(community.a_tag.clone(), pending_requests.clone());

    log::info!("Fetched {} pending join requests for {}", pending_requests.len(), community.a_tag);
    Ok(pending_requests)
}

/// Submit a join request to a community (kind 4552)
pub async fn submit_join_request(
    community: &Community,
    reason: Option<&str>,
) -> std::result::Result<String, String> {
    let client = crate::stores::nostr_client::get_client()
        .ok_or("Client not initialized")?;

    if !*crate::stores::nostr_client::HAS_SIGNER.read() {
        return Err("No signer attached".to_string());
    }

    let current_pubkey = crate::stores::auth_store::get_pubkey()
        .ok_or("Not logged in")?;

    // Check if already a member or has pending request
    let status = get_membership_status(&current_pubkey, community);
    match status {
        MembershipStatus::Owner | MembershipStatus::Moderator | MembershipStatus::Member => {
            return Err("You are already a member of this community".to_string());
        }
        MembershipStatus::Pending { .. } => {
            return Err("You already have a pending join request".to_string());
        }
        MembershipStatus::Banned { .. } => {
            return Err("You are banned from this community".to_string());
        }
        _ => {}
    }

    // Build coordinate for community
    let coord = Coordinate::new(
        Kind::Custom(KIND_COMMUNITY_DEFINITION),
        PublicKey::from_hex(&community.pubkey).map_err(|e| e.to_string())?
    ).identifier(&community.d_tag);

    let content = reason.unwrap_or("");

    let tags: Vec<Tag> = vec![
        Tag::coordinate(coord, None),
        Tag::public_key(PublicKey::from_hex(&community.pubkey).map_err(|e| e.to_string())?),
    ];

    let builder = EventBuilder::new(Kind::Custom(KIND_JOIN_REQUEST), content)
        .tags(tags);

    let output = client.send_event_builder(builder).await
        .map_err(|e| format!("Failed to submit join request: {}", e))?;

    let request_id = output.id().to_hex();

    // Update user's pending requests cache
    let request = JoinRequest {
        id: request_id.clone(),
        community_a_tag: community.a_tag.clone(),
        user_pubkey: current_pubkey,
        reason: reason.map(|s| s.to_string()),
        created_at: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
        event: None, // Event not available when self-submitted
    };
    USER_PENDING_REQUESTS.write().insert(community.a_tag.clone(), request);

    log::info!("Join request submitted: {}", request_id);
    Ok(request_id)
}

/// Approve a join request (adds user to approved members list)
pub async fn approve_join_request(
    community: &Community,
    request: &JoinRequest,
) -> std::result::Result<String, String> {
    let current_pubkey = crate::stores::auth_store::get_pubkey()
        .ok_or("Not logged in")?;

    if !can_moderate(&current_pubkey, community) {
        return Err("You are not a moderator of this community".to_string());
    }

    // Get current approved members
    let mut members: Vec<String> = APPROVED_MEMBERS_CACHE.read()
        .get(&community.a_tag)
        .map(|s| s.iter().cloned().collect())
        .unwrap_or_default();

    // Add the new member
    if !members.contains(&request.user_pubkey) {
        members.push(request.user_pubkey.clone());
    }

    // Update approved members list
    let result = update_approved_members(community, members).await?;

    // Remove from pending requests cache
    if let Some(requests) = PENDING_JOIN_REQUESTS_CACHE.write().get_mut(&community.a_tag) {
        requests.retain(|r| r.id != request.id);
    }

    log::info!("Approved join request {} for user {}", request.id, request.user_pubkey);
    Ok(result)
}

/// Decline a join request (adds user to declined members list)
pub async fn decline_join_request(
    community: &Community,
    user_pubkey: &str,
    reason: Option<&str>,
) -> std::result::Result<String, String> {
    let client = crate::stores::nostr_client::get_client()
        .ok_or("Client not initialized")?;

    if !*crate::stores::nostr_client::HAS_SIGNER.read() {
        return Err("No signer attached".to_string());
    }

    let current_pubkey = crate::stores::auth_store::get_pubkey()
        .ok_or("Not logged in")?;

    if !can_moderate(&current_pubkey, community) {
        return Err("You are not a moderator of this community".to_string());
    }

    // Get current declined members and add new one
    let mut declined: Vec<String> = DECLINED_MEMBERS_CACHE.read()
        .get(&community.a_tag)
        .map(|s| s.iter().cloned().collect())
        .unwrap_or_default();

    if !declined.contains(&user_pubkey.to_string()) {
        declined.push(user_pubkey.to_string());
    }

    // Build tags
    let mut tags: Vec<Tag> = vec![
        Tag::identifier(&community.d_tag),
        Tag::custom(TagKind::a(), vec![community.a_tag.clone()]),
    ];

    for pubkey in &declined {
        if let Ok(pk) = PublicKey::from_hex(pubkey) {
            tags.push(Tag::public_key(pk));
        }
    }

    let content = reason.unwrap_or("");

    let builder = EventBuilder::new(Kind::Custom(KIND_DECLINED_MEMBERS), content)
        .tags(tags);

    let output = client.send_event_builder(builder).await
        .map_err(|e| format!("Failed to decline join request: {}", e))?;

    // Update cache
    DECLINED_MEMBERS_CACHE.write().insert(
        community.a_tag.clone(),
        declined.into_iter().collect()
    );

    // Remove from pending requests cache
    if let Some(requests) = PENDING_JOIN_REQUESTS_CACHE.write().get_mut(&community.a_tag) {
        requests.retain(|r| r.user_pubkey != user_pubkey);
    }

    log::info!("Declined join request for user {}", user_pubkey);
    Ok(output.id().to_hex())
}

/// Fetch declined members list for a community
pub async fn fetch_declined_members(community: &Community) -> std::result::Result<HashSet<String>, String> {
    let client = crate::stores::nostr_client::get_client()
        .ok_or("Client not initialized")?;

    let owner_pubkey = PublicKey::from_hex(&community.pubkey)
        .map_err(|e| format!("Invalid community pubkey: {}", e))?;

    let filter = Filter::new()
        .kind(Kind::Custom(KIND_DECLINED_MEMBERS))
        .author(owner_pubkey)
        .identifier(&community.d_tag)
        .limit(1);

    let events = client.fetch_events(filter, Duration::from_secs(5)).await
        .map_err(|e| format!("Failed to fetch declined members: {}", e))?;

    let mut members = HashSet::new();

    if let Some(event) = events.into_iter().next() {
        for tag in event.tags.iter() {
            if tag.kind() == TagKind::p() {
                if let Some(pubkey) = tag.content() {
                    members.insert(pubkey.to_string());
                }
            }
        }
    }

    DECLINED_MEMBERS_CACHE.write().insert(community.a_tag.clone(), members.clone());

    log::info!("Fetched {} declined members for {}", members.len(), community.a_tag);
    Ok(members)
}

/// Fetch banned members list for a community
pub async fn fetch_banned_members(community: &Community) -> std::result::Result<HashSet<String>, String> {
    let client = crate::stores::nostr_client::get_client()
        .ok_or("Client not initialized")?;

    let owner_pubkey = PublicKey::from_hex(&community.pubkey)
        .map_err(|e| format!("Invalid community pubkey: {}", e))?;

    let filter = Filter::new()
        .kind(Kind::Custom(KIND_BANNED_MEMBERS))
        .author(owner_pubkey)
        .identifier(&community.d_tag)
        .limit(1);

    let events = client.fetch_events(filter, Duration::from_secs(5)).await
        .map_err(|e| format!("Failed to fetch banned members: {}", e))?;

    let mut members = HashSet::new();

    if let Some(event) = events.into_iter().next() {
        for tag in event.tags.iter() {
            if tag.kind() == TagKind::p() {
                if let Some(pubkey) = tag.content() {
                    members.insert(pubkey.to_string());
                }
            }
        }
    }

    BANNED_MEMBERS_CACHE.write().insert(community.a_tag.clone(), members.clone());

    log::info!("Fetched {} banned members for {}", members.len(), community.a_tag);
    Ok(members)
}

// ============================================================================
// Thread Tree Building
// ============================================================================

/// Represents a post with its nested replies (for threaded display)
#[derive(Clone, Debug)]
pub struct CommunityThread {
    pub post: CommunityPost,
    pub replies: Vec<CommunityThread>,
    pub depth: usize,
}

/// Build a thread tree from flat posts list
/// Returns only top-level posts with nested replies
pub fn build_community_thread_tree(posts: Vec<CommunityPost>) -> Vec<CommunityThread> {
    // Group posts by parent_id
    let mut children_map: HashMap<Option<String>, Vec<CommunityPost>> = HashMap::new();

    for post in posts {
        children_map
            .entry(post.parent_id.clone())
            .or_default()
            .push(post);
    }

    // Sort each group by created_at (oldest first for chronological replies)
    for posts_vec in children_map.values_mut() {
        posts_vec.sort_by(|a, b| a.created_at.cmp(&b.created_at));
    }

    // Recursively build tree
    fn build_tree(
        parent_id: Option<String>,
        map: &HashMap<Option<String>, Vec<CommunityPost>>,
        depth: usize,
        max_depth: usize,
    ) -> Vec<CommunityThread> {
        if depth > max_depth {
            return Vec::new();
        }

        map.get(&parent_id)
            .map(|posts| {
                posts.iter().map(|post| {
                    CommunityThread {
                        post: post.clone(),
                        replies: build_tree(Some(post.id.clone()), map, depth + 1, max_depth),
                        depth,
                    }
                }).collect()
            })
            .unwrap_or_default()
    }

    // Start with top-level posts (parent_id = None)
    // Max depth of 10 to prevent infinite recursion
    let mut tree = build_tree(None, &children_map, 0, 10);

    // Sort top-level by created_at (newest first)
    tree.sort_by(|a, b| b.post.created_at.cmp(&a.post.created_at));

    tree
}

/// Flatten a thread tree back to a list (for sequential rendering with depth)
pub fn flatten_thread_tree(tree: Vec<CommunityThread>) -> Vec<(CommunityPost, usize)> {
    let mut result = Vec::new();

    fn flatten_recursive(threads: Vec<CommunityThread>, result: &mut Vec<(CommunityPost, usize)>) {
        for thread in threads {
            result.push((thread.post, thread.depth));
            flatten_recursive(thread.replies, result);
        }
    }

    flatten_recursive(tree, &mut result);
    result
}
