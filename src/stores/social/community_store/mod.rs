//! Community Store
//! Handles NIP-72 Moderated Communities - caching, filtering, and state management
//!
//! NIP-72 Event Kinds:
//! - 34550: Community definition (addressable)
//! - 4550: Moderator approval events
//! - 4551: Post removal events
//! - 1111: Posts/comments (NIP-22)
//! - 34551: Approved members list
//!
//! Submodules:
//! - `fetch` - Fetching communities, posts, members, join requests
//! - `publish` - Creating communities, posting, approvals, membership management
#![allow(dead_code)]
#![allow(unused_imports)]

mod fetch;
mod publish;

pub use fetch::*;
pub use publish::*;

use crate::utils::format::truncate_pubkey;
use dioxus::prelude::*;
use lru::LruCache;
use nostr::Event as NostrEvent;
use nostr_sdk::prelude::*;
use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::num::NonZeroUsize;
use std::time::Duration;

pub const KIND_COMMUNITY_DEFINITION: u16 = 34550;
pub const KIND_COMMUNITY_POST: u16 = 1111;
pub const KIND_APPROVAL: u16 = 4550;
pub const KIND_REMOVAL: u16 = 4551;
pub const KIND_APPROVED_MEMBERS: u16 = 34551;
pub const KIND_JOIN_REQUEST: u16 = 4552;
pub const KIND_DECLINED_MEMBERS: u16 = 4553;
pub const KIND_BANNED_MEMBERS: u16 = 4554;

const COMMUNITY_CACHE_SIZE: usize = 100;
const POST_CACHE_SIZE: usize = 500;

#[derive(Clone, Debug, PartialEq)]
pub struct Community {
    pub id: String,
    pub pubkey: String,
    pub d_tag: String,
    pub name: Option<String>,
    pub description: Option<String>,
    pub image: Option<String>,
    pub banner: Option<String>,
    pub rules: Option<String>,
    pub moderators: Vec<String>,
    pub a_tag: String,
    pub naddr: String,
    pub created_at: u64,
    pub event: NostrEvent,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CommunityPost {
    pub id: String,
    pub pubkey: String,
    pub content: String,
    pub community_a_tag: String,
    pub parent_id: Option<String>,
    pub parent_pubkey: Option<String>,
    pub kind: u16,
    pub created_at: u64,
    pub event: NostrEvent,
    pub approval_status: ApprovalStatus,
    pub is_top_level: bool,
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
    Pending,
    Declined,
    Visitor,
}

/// Membership status for a user in a community (more detailed than UserRole)
#[derive(Clone, Debug, PartialEq)]
pub enum MembershipStatus {
    Owner,
    Moderator,
    Member,
    Pending {
        request_id: String,
        requested_at: u64,
    },
    Declined {
        reason: Option<String>,
    },
    Banned {
        reason: Option<String>,
    },
    None,
}

/// Join request event (kind 4552)
#[derive(Clone, Debug, PartialEq)]
pub struct JoinRequest {
    pub id: String,
    pub community_a_tag: String,
    pub user_pubkey: String,
    pub reason: Option<String>,
    pub created_at: u64,
    pub event: Option<NostrEvent>,
}

/// Community with membership context (for display with role badges)
#[derive(Clone, Debug, PartialEq)]
pub struct CommunityWithMembership {
    pub community: Community,
    pub membership_status: MembershipStatus,
    pub is_pinned: bool,
    pub pending_request_count: Option<u32>,
}

/// Represents a post with its nested replies (for threaded display)
#[derive(Clone, Debug)]
pub struct CommunityThread {
    pub post: CommunityPost,
    pub replies: Vec<CommunityThread>,
    pub depth: usize,
}

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
pub static REMOVALS_CACHE: GlobalSignal<HashMap<String, Removal>> = GlobalSignal::new(HashMap::new);

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
    COMMUNITIES_CACHE
        .write()
        .put(community.a_tag.clone(), community);
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

/// Parse community definition (kind 34550)
pub fn parse_community_event(event: &NostrEvent) -> Option<Community> {
    use nostr_sdk::{Alphabet, SingleLetterTag, TagKind};
    if event.kind.as_u16() != KIND_COMMUNITY_DEFINITION {
        return None;
    }
    let d_tag = event
        .tags
        .iter()
        .find(|t| t.kind() == TagKind::SingleLetter(SingleLetterTag::lowercase(Alphabet::D)))
        .and_then(|t| t.content())
        .map(|s| s.to_string())?;
    let name = extract_tag_value(&event.tags, "name");
    let description = extract_tag_value(&event.tags, "description");
    let image = extract_tag_value(&event.tags, "image");
    let banner = extract_tag_value(&event.tags, "banner");
    let rules = extract_tag_value(&event.tags, "rules");
    let moderators: Vec<String> = event
        .tags
        .iter()
        .filter(|t| t.kind() == TagKind::p())
        .filter(|t| {
            let slice = t.as_slice();
            slice.get(3).map(|s| s.as_str()) == Some("moderator")
        })
        .filter_map(|t| t.content().map(|s| s.to_string()))
        .collect();
    let a_tag = format!(
        "{}:{}:{}",
        KIND_COMMUNITY_DEFINITION,
        event.pubkey.to_hex(),
        d_tag,
    );
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
    let big_a_tag = extract_uppercase_tag(&event.tags, "A");
    let small_a_tag = event
        .tags
        .iter()
        .find(|t| t.kind() == TagKind::a())
        .and_then(|t| t.content())
        .map(|s| s.to_string());
    let post_community = big_a_tag.as_ref().or(small_a_tag.as_ref());
    if post_community != Some(&community_a_tag.to_string()) {
        return None;
    }
    let is_top_level =
        big_a_tag == small_a_tag || event.tags.iter().all(|t| t.kind() != TagKind::e());
    let parent_id = event
        .tags
        .iter()
        .find(|t| t.kind() == TagKind::e())
        .and_then(|t| t.content())
        .map(|s| s.to_string());
    let parent_pubkey = if !is_top_level {
        event
            .tags
            .iter()
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
        approval_status: ApprovalStatus::Pending,
        is_top_level,
    })
}

/// Parse approval event (kind 4550)
pub fn parse_approval_event(event: &NostrEvent) -> Option<(String, Approval)> {
    if event.kind.as_u16() != KIND_APPROVAL {
        return None;
    }
    let post_id = event
        .tags
        .iter()
        .find(|t| t.kind() == TagKind::e())
        .and_then(|t| t.content())
        .map(|s| s.to_string())?;
    Some((
        post_id,
        Approval {
            event_id: event.id.to_hex(),
            moderator_pubkey: event.pubkey.to_hex(),
            approved_at: event.created_at.as_secs(),
        },
    ))
}

/// Parse removal event (kind 4551)
pub fn parse_removal_event(event: &NostrEvent) -> Option<(String, Removal)> {
    if event.kind.as_u16() != KIND_REMOVAL {
        return None;
    }
    let post_id = event
        .tags
        .iter()
        .find(|t| t.kind() == TagKind::e())
        .and_then(|t| t.content())
        .map(|s| s.to_string())?;
    Some((
        post_id,
        Removal {
            event_id: event.id.to_hex(),
            moderator_pubkey: event.pubkey.to_hex(),
            removed_at: event.created_at.as_secs(),
            reason: if event.content.is_empty() {
                None
            } else {
                Some(event.content.clone())
            },
        },
    ))
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

/// Parse join request event (kind 4552)
pub fn parse_join_request(event: &NostrEvent) -> Option<JoinRequest> {
    if event.kind.as_u16() != KIND_JOIN_REQUEST {
        return None;
    }
    let community_a_tag = event
        .tags
        .iter()
        .find(|t| t.kind() == TagKind::a())
        .and_then(|t| t.content())
        .map(|s| s.to_string())?;
    Some(JoinRequest {
        id: event.id.to_hex(),
        community_a_tag,
        user_pubkey: event.pubkey.to_hex(),
        reason: if event.content.is_empty() {
            None
        } else {
            Some(event.content.clone())
        },
        created_at: event.created_at.as_secs(),
        event: Some(event.clone()),
    })
}

/// Determine user's role in a community
pub fn get_user_role(user_pubkey: &str, community: &Community) -> UserRole {
    if community.pubkey == user_pubkey {
        return UserRole::Owner;
    }
    if community.moderators.contains(&user_pubkey.to_string()) {
        return UserRole::Moderator;
    }
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
        UserRole::ApprovedMember => {
            ApprovalStatus::AutoApproved(AutoApprovalReason::ApprovedMember)
        }
        UserRole::Pending | UserRole::Declined | UserRole::Visitor => {
            let removals = REMOVALS_CACHE.read();
            if let Some(removal) = removals.get(&post.id) {
                return ApprovalStatus::Removed(removal.clone());
            }
            let approvals = APPROVALS_CACHE.read();
            if let Some(post_approvals) = approvals.get(&post.id) {
                let valid_approvals: Vec<_> = post_approvals
                    .iter()
                    .filter(|a| {
                        community.pubkey == a.moderator_pubkey
                            || community.moderators.contains(&a.moderator_pubkey)
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
    if community.pubkey == user_pubkey {
        return MembershipStatus::Owner;
    }
    if community.moderators.contains(&user_pubkey.to_string()) {
        return MembershipStatus::Moderator;
    }
    let approved_members = APPROVED_MEMBERS_CACHE.read();
    if let Some(members) = approved_members.get(&community.a_tag) {
        if members.contains(user_pubkey) {
            return MembershipStatus::Member;
        }
    }
    drop(approved_members);
    let banned_members = BANNED_MEMBERS_CACHE.read();
    if let Some(banned) = banned_members.get(&community.a_tag) {
        if banned.contains(user_pubkey) {
            return MembershipStatus::Banned { reason: None };
        }
    }
    drop(banned_members);
    let declined_members = DECLINED_MEMBERS_CACHE.read();
    if let Some(declined) = declined_members.get(&community.a_tag) {
        if declined.contains(user_pubkey) {
            return MembershipStatus::Declined { reason: None };
        }
    }
    drop(declined_members);
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
            let pending_request_count = match &membership_status {
                MembershipStatus::Owner | MembershipStatus::Moderator => {
                    PENDING_JOIN_REQUESTS_CACHE
                        .read()
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
    result.sort_by(|a, b| {
        match (a.is_pinned, b.is_pinned) {
            (true, false) => return std::cmp::Ordering::Less,
            (false, true) => return std::cmp::Ordering::Greater,
            _ => {}
        }
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
                let a_name = a.community.name.as_ref().unwrap_or(&a.community.d_tag);
                let b_name = b.community.name.as_ref().unwrap_or(&b.community.d_tag);
                a_name.to_lowercase().cmp(&b_name.to_lowercase())
            }
            other => other,
        }
    });
    result
}

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
pub fn posts_filter_by_community(
    community_a_tag: &str,
    limit: usize,
    until: Option<u64>,
) -> Filter {
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
    let parts: Vec<&str> = community_a_tag.splitn(3, ':').collect();
    if parts.len() != 3 {
        return Filter::new()
            .kind(Kind::Custom(KIND_APPROVED_MEMBERS))
            .limit(0);
    }
    let pubkey = match PublicKey::from_hex(parts[1]) {
        Ok(pk) => pk,
        Err(_) => {
            return Filter::new()
                .kind(Kind::Custom(KIND_APPROVED_MEMBERS))
                .limit(0)
        }
    };
    Filter::new()
        .kind(Kind::Custom(KIND_APPROVED_MEMBERS))
        .author(pubkey)
        .custom_tag(SingleLetterTag::lowercase(Alphabet::A), community_a_tag)
        .limit(1)
}

/// Build a thread tree from flat posts list
/// Returns only top-level posts with nested replies
pub fn build_community_thread_tree(posts: Vec<CommunityPost>) -> Vec<CommunityThread> {
    let mut children_map: HashMap<Option<String>, Vec<CommunityPost>> = HashMap::new();
    for post in posts {
        children_map
            .entry(post.parent_id.clone())
            .or_default()
            .push(post);
    }
    for posts_vec in children_map.values_mut() {
        posts_vec.sort_by_key(|a| a.created_at);
    }
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
                posts
                    .iter()
                    .map(|post| CommunityThread {
                        post: post.clone(),
                        replies: build_tree(Some(post.id.clone()), map, depth + 1, max_depth),
                        depth,
                    })
                    .collect()
            })
            .unwrap_or_default()
    }
    let mut tree = build_tree(None, &children_map, 0, 10);
    tree.sort_by_key(|b| std::cmp::Reverse(b.post.created_at));
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
