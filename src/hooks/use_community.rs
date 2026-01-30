//! use_community - Custom hooks for NIP-72 community operations
//!
//! This module provides reusable hooks for:
//! - Community moderation actions (approve, remove, post)
//! - Fetching community data with reactive dependencies
//! - Thread tree management

use dioxus::prelude::*;
use std::collections::HashSet;

use crate::services::aggregation::{fetch_interaction_counts_batch, InteractionCounts};
use crate::stores::auth_store;
use crate::stores::community_store::{
    approve_join_request, approve_post, build_community_thread_tree, can_moderate,
    decline_join_request, fetch_approved_members, fetch_community_by_a_tag,
    fetch_community_join_requests, fetch_community_posts, fetch_pending_posts,
    get_membership_status, get_user_role, post_to_community, remove_post,
    reply_to_post as store_reply_to_post, sort_communities_by_membership, submit_join_request,
    Community, CommunityPost, CommunityThread, CommunityWithMembership, JoinRequest,
    MembershipStatus, UserRole,
};
use crate::stores::pinned_communities::{
    get_pinned_communities, get_pinned_communities_set, init_pinned_communities, pin_community,
    unpin_community,
};
use crate::stores::profiles::{fetch_profiles_batch, get_cached_profile};

/// State of a community action
#[derive(Clone, Debug, PartialEq)]
pub enum CommunityActionState {
    Idle,
    Pending,
    Success(String),
    Error(String),
}

/// Actions available for community moderation
#[derive(Clone)]
pub struct UseCommunityActions {
    /// Approve a pending post
    pub approve: EventHandler<CommunityPost>,
    /// Remove a post (with optional reason)
    pub remove: EventHandler<(CommunityPost, Option<String>)>,
    /// Create a new post in the community
    pub post: EventHandler<String>,
    /// Reply to an existing post (parent_post, content)
    pub reply: EventHandler<(CommunityPost, String)>,
    /// Current state of the last action
    pub state: Signal<CommunityActionState>,
    /// Whether an action is in progress
    pub is_pending: Memo<bool>,
}

impl PartialEq for UseCommunityActions {
    fn eq(&self, other: &Self) -> bool {
        *self.state.read() == *other.state.read()
    }
}

/// Hook for community moderation and posting actions
///
/// # Arguments
/// * `community` - The community to perform actions on
///
/// # Returns
/// A `UseCommunityActions` struct with handlers and state
pub fn use_community_actions(community: Community) -> UseCommunityActions {
    let mut state = use_signal(|| CommunityActionState::Idle);
    let community_for_approve = community.clone();
    let community_for_remove = community.clone();
    let community_for_post = community.clone();
    let community_for_reply = community.clone();

    let is_pending = use_memo(move || matches!(*state.read(), CommunityActionState::Pending));

    // Approve handler
    let approve = use_callback(move |post: CommunityPost| {
        if matches!(*state.peek(), CommunityActionState::Pending) {
            return;
        }

        let community = community_for_approve.clone();
        state.set(CommunityActionState::Pending);

        spawn(async move {
            match approve_post(&community, &post).await {
                Ok(event_id) => {
                    log::info!("Post approved: {}", post.id);
                    state.set(CommunityActionState::Success(event_id));
                }
                Err(e) => {
                    log::error!("Failed to approve post: {}", e);
                    state.set(CommunityActionState::Error(e));
                }
            }
        });
    });

    // Remove handler
    let remove = use_callback(move |(post, reason): (CommunityPost, Option<String>)| {
        if matches!(*state.peek(), CommunityActionState::Pending) {
            return;
        }

        let community = community_for_remove.clone();
        state.set(CommunityActionState::Pending);

        spawn(async move {
            match remove_post(&community, &post, reason.as_deref()).await {
                Ok(event_id) => {
                    log::info!("Post removed: {}", post.id);
                    state.set(CommunityActionState::Success(event_id));
                }
                Err(e) => {
                    log::error!("Failed to remove post: {}", e);
                    state.set(CommunityActionState::Error(e));
                }
            }
        });
    });

    // Post handler
    let post = use_callback(move |content: String| {
        if matches!(*state.peek(), CommunityActionState::Pending) {
            return;
        }

        let community = community_for_post.clone();
        state.set(CommunityActionState::Pending);

        spawn(async move {
            match post_to_community(&community, &content).await {
                Ok(event_id) => {
                    log::info!("Post created: {}", event_id);
                    state.set(CommunityActionState::Success(event_id));
                }
                Err(e) => {
                    log::error!("Failed to create post: {}", e);
                    state.set(CommunityActionState::Error(e));
                }
            }
        });
    });

    // Reply handler
    let reply = use_callback(move |(parent_post, content): (CommunityPost, String)| {
        if matches!(*state.peek(), CommunityActionState::Pending) {
            return;
        }

        let community = community_for_reply.clone();
        state.set(CommunityActionState::Pending);

        spawn(async move {
            match store_reply_to_post(&community, &parent_post, &content).await {
                Ok(event_id) => {
                    log::info!("Reply created: {}", event_id);
                    state.set(CommunityActionState::Success(event_id));
                }
                Err(e) => {
                    log::error!("Failed to create reply: {}", e);
                    state.set(CommunityActionState::Error(e));
                }
            }
        });
    });

    UseCommunityActions {
        approve,
        remove,
        post,
        reply,
        state,
        is_pending,
    }
}

/// Data bundle for a community page
#[derive(Clone, Debug)]
pub struct CommunityData {
    pub community: Community,
    pub posts: Vec<CommunityPost>,
    pub pending_posts: Vec<CommunityPost>,
    pub approved_members: HashSet<String>,
    pub thread_tree: Vec<CommunityThread>,
}

/// Result type for community data fetching
pub type CommunityDataResult = Result<CommunityData, String>;

/// Hook for fetching all community data with reactive dependencies
///
/// This hook fetches:
/// - Community metadata
/// - Approved posts (as threaded tree)
/// - Pending posts (for moderators)
/// - Approved members list
///
/// # Arguments
/// * `a_tag` - The community's a-tag (34550:pubkey:d-tag)
///
/// # Returns
/// A Resource that resolves to CommunityData or an error
pub fn use_community_data(a_tag: String) -> Resource<CommunityDataResult> {
    use_resource(move || {
        let a_tag_clone = a_tag.clone();
        async move {
            // Fetch community metadata first
            let community = match fetch_community_by_a_tag(&a_tag_clone).await {
                Ok(Some(c)) => c,
                Ok(None) => return Err("Community not found".to_string()),
                Err(e) => return Err(e),
            };

            // Fetch posts and approved members in parallel
            let posts_future = fetch_community_posts(&community, 100, false, None);
            let approved_future = fetch_approved_members(&community);

            let (posts_result, approved_result) = futures::join!(posts_future, approved_future);

            let posts = posts_result.unwrap_or_default();
            let approved_members = approved_result.unwrap_or_default();

            // Build thread tree from posts
            let thread_tree = build_community_thread_tree(posts.clone());

            // Check if user is a moderator to fetch pending posts
            let current_pubkey = auth_store::get_pubkey();
            let is_mod = current_pubkey
                .as_ref()
                .map(|pk| can_moderate(pk, &community))
                .unwrap_or(false);

            let pending_posts = if is_mod {
                fetch_pending_posts(&community).await.unwrap_or_default()
            } else {
                Vec::new()
            };

            Ok(CommunityData {
                community,
                posts,
                pending_posts,
                approved_members,
                thread_tree,
            })
        }
    })
}

/// Hook for user's role in a community
///
/// # Arguments
/// * `community` - The community to check role for
///
/// # Returns
/// The user's role (Owner, Moderator, ApprovedMember, or Visitor)
pub fn use_user_role(community: Option<Community>) -> Memo<UserRole> {
    use_memo(move || {
        let current_pubkey = auth_store::get_pubkey();

        match (community.as_ref(), current_pubkey.as_ref()) {
            (Some(comm), Some(pk)) => get_user_role(pk, comm),
            _ => UserRole::Visitor,
        }
    })
}

/// Hook for checking if user can moderate
///
/// # Arguments
/// * `community` - The community to check moderation rights for
///
/// # Returns
/// Whether the current user can moderate
pub fn use_can_moderate(community: Option<Community>) -> Memo<bool> {
    use_memo(move || {
        let current_pubkey = auth_store::get_pubkey();

        match (community.as_ref(), current_pubkey.as_ref()) {
            (Some(comm), Some(pk)) => can_moderate(pk, comm),
            _ => false,
        }
    })
}

/// Hook for prefetching profiles for a list of posts
///
/// Automatically fetches and caches profiles for all post authors.
///
/// # Arguments
/// * `posts` - The posts to prefetch profiles for
pub fn use_prefetch_profiles(posts: Vec<CommunityPost>) {
    use_effect(move || {
        let posts = posts.clone();

        spawn(async move {
            // Collect unique pubkeys
            let pubkeys: Vec<String> = posts
                .iter()
                .map(|p| p.pubkey.clone())
                .collect::<HashSet<_>>()
                .into_iter()
                .collect();

            if !pubkeys.is_empty() {
                if let Err(e) = fetch_profiles_batch(pubkeys).await {
                    log::warn!("Failed to prefetch profiles: {}", e);
                }
            }
        });
    });
}

/// Interaction counts for community posts
#[derive(Clone, Default)]
pub struct PostInteractionCounts {
    /// Map of event_id -> InteractionCounts
    pub counts: std::collections::HashMap<String, InteractionCounts>,
}

impl PartialEq for PostInteractionCounts {
    fn eq(&self, other: &Self) -> bool {
        self.counts.len() == other.counts.len()
    }
}

/// Hook for fetching interaction counts for a list of posts
///
/// # Arguments
/// * `posts` - The posts to fetch counts for
///
/// # Returns
/// A signal containing the interaction counts map
pub fn use_post_interaction_counts(posts: Vec<CommunityPost>) -> Signal<PostInteractionCounts> {
    let mut counts = use_signal(PostInteractionCounts::default);

    use_effect(move || {
        let posts = posts.clone();

        spawn(async move {
            let event_ids: Vec<nostr_sdk::EventId> = posts
                .iter()
                .filter_map(|p| nostr_sdk::EventId::from_hex(&p.id).ok())
                .collect();

            if event_ids.is_empty() {
                return;
            }

            match fetch_interaction_counts_batch(event_ids, std::time::Duration::from_secs(5)).await
            {
                Ok(batch_counts) => {
                    counts.set(PostInteractionCounts {
                        counts: batch_counts,
                    });
                }
                Err(e) => {
                    log::warn!("Failed to fetch interaction counts: {}", e);
                }
            }
        });
    });

    counts
}

/// Helper to get a cached profile display name
pub fn get_display_name(pubkey: &str) -> String {
    get_cached_profile(pubkey)
        .and_then(|p| p.display_name.clone().or(p.name.clone()))
        .unwrap_or_else(|| format!("{}...", &pubkey[..8.min(pubkey.len())]))
}

/// Helper to get a cached profile picture
pub fn get_profile_picture(pubkey: &str) -> Option<String> {
    get_cached_profile(pubkey).and_then(|p| p.picture.clone())
}

// ============================================================================
// Membership Hooks
// ============================================================================

/// Hook for user's membership status in a community
///
/// Returns detailed membership status including pending/declined/banned states.
///
/// # Arguments
/// * `community` - The community to check membership for
///
/// # Returns
/// A memo that updates when auth state or community changes
pub fn use_membership_status(community: Option<Community>) -> Memo<MembershipStatus> {
    use_memo(move || {
        let current_pubkey = auth_store::get_pubkey();

        match (community.as_ref(), current_pubkey.as_ref()) {
            (Some(comm), Some(pk)) => get_membership_status(pk, comm),
            _ => MembershipStatus::None,
        }
    })
}

/// Actions available for join request operations
#[derive(Clone)]
pub struct UseJoinRequestActions {
    /// Submit a join request with optional reason
    pub submit: EventHandler<Option<String>>,
    /// Cancel/withdraw a pending join request (not implemented yet)
    pub cancel: EventHandler<()>,
    /// Current state of the last action
    pub state: Signal<CommunityActionState>,
    /// Whether an action is in progress
    pub is_pending: Memo<bool>,
}

impl PartialEq for UseJoinRequestActions {
    fn eq(&self, other: &Self) -> bool {
        *self.state.read() == *other.state.read()
    }
}

/// Hook for join request actions on a community
///
/// # Arguments
/// * `community` - The community to request to join
///
/// # Returns
/// A `UseJoinRequestActions` struct with handlers and state
pub fn use_join_request(community: Community) -> UseJoinRequestActions {
    let mut state = use_signal(|| CommunityActionState::Idle);
    let community_for_submit = community.clone();

    let is_pending = use_memo(move || matches!(*state.read(), CommunityActionState::Pending));

    // Submit join request handler
    let submit = use_callback(move |reason: Option<String>| {
        if matches!(*state.peek(), CommunityActionState::Pending) {
            return;
        }

        let community = community_for_submit.clone();
        state.set(CommunityActionState::Pending);

        spawn(async move {
            match submit_join_request(&community, reason.as_deref()).await {
                Ok(event_id) => {
                    log::info!("Join request submitted: {}", event_id);
                    state.set(CommunityActionState::Success(event_id));
                }
                Err(e) => {
                    log::error!("Failed to submit join request: {}", e);
                    state.set(CommunityActionState::Error(e));
                }
            }
        });
    });

    // Cancel request handler (placeholder - would need to publish a deletion event)
    let cancel = use_callback(move |_: ()| {
        log::warn!("Cancel join request not yet implemented");
    });

    UseJoinRequestActions {
        submit,
        cancel,
        state,
        is_pending,
    }
}

/// Actions for managing join requests (for moderators)
#[derive(Clone)]
pub struct UseJoinRequestManagement {
    /// Approve a join request
    pub approve: EventHandler<JoinRequest>,
    /// Decline a join request with optional reason
    pub decline: EventHandler<(JoinRequest, Option<String>)>,
    /// Pending join requests
    pub pending_requests: Signal<Vec<JoinRequest>>,
    /// Whether loading requests
    pub loading: Signal<bool>,
    /// Current action state
    pub state: Signal<CommunityActionState>,
}

impl PartialEq for UseJoinRequestManagement {
    fn eq(&self, other: &Self) -> bool {
        *self.state.read() == *other.state.read()
            && self.pending_requests.read().len() == other.pending_requests.read().len()
    }
}

/// Hook for managing join requests (for community moderators)
///
/// # Arguments
/// * `community` - The community to manage requests for
///
/// # Returns
/// A `UseJoinRequestManagement` struct with handlers and state
pub fn use_join_request_management(community: Community) -> UseJoinRequestManagement {
    let mut state = use_signal(|| CommunityActionState::Idle);
    let mut pending_requests = use_signal(Vec::<JoinRequest>::new);
    let mut loading = use_signal(|| false);

    let community_for_approve = community.clone();
    let community_for_decline = community.clone();
    let community_for_fetch = community.clone();

    // Fetch pending requests on mount
    use_effect(move || {
        let community = community_for_fetch.clone();
        loading.set(true);

        spawn(async move {
            match fetch_community_join_requests(&community).await {
                Ok(requests) => {
                    pending_requests.set(requests);
                }
                Err(e) => {
                    log::error!("Failed to fetch join requests: {}", e);
                }
            }
            loading.set(false);
        });
    });

    // Approve handler
    let approve = use_callback(move |request: JoinRequest| {
        if matches!(*state.peek(), CommunityActionState::Pending) {
            return;
        }

        let community = community_for_approve.clone();
        let request_id = request.id.clone();
        state.set(CommunityActionState::Pending);

        spawn(async move {
            match approve_join_request(&community, &request).await {
                Ok(event_id) => {
                    log::info!("Join request approved: {}", event_id);
                    // Remove from local list
                    pending_requests.write().retain(|r| r.id != request_id);
                    state.set(CommunityActionState::Success(event_id));
                }
                Err(e) => {
                    log::error!("Failed to approve join request: {}", e);
                    state.set(CommunityActionState::Error(e));
                }
            }
        });
    });

    // Decline handler
    let decline = use_callback(move |(request, reason): (JoinRequest, Option<String>)| {
        if matches!(*state.peek(), CommunityActionState::Pending) {
            return;
        }

        let community = community_for_decline.clone();
        let user_pubkey = request.user_pubkey.clone();
        let request_id = request.id.clone();
        state.set(CommunityActionState::Pending);

        spawn(async move {
            match decline_join_request(&community, &user_pubkey, reason.as_deref()).await {
                Ok(event_id) => {
                    log::info!("Join request declined: {}", event_id);
                    // Remove from local list
                    pending_requests.write().retain(|r| r.id != request_id);
                    state.set(CommunityActionState::Success(event_id));
                }
                Err(e) => {
                    log::error!("Failed to decline join request: {}", e);
                    state.set(CommunityActionState::Error(e));
                }
            }
        });
    });

    UseJoinRequestManagement {
        approve,
        decline,
        pending_requests,
        loading,
        state,
    }
}

// ============================================================================
// Pinned Communities Hooks
// ============================================================================

/// Pinned communities state and actions
#[derive(Clone, PartialEq)]
pub struct UsePinnedCommunities {
    /// Current pinned community a_tags
    pub pinned: Signal<Vec<String>>,
    /// Pin a community
    pub pin: EventHandler<String>,
    /// Unpin a community
    pub unpin: EventHandler<String>,
    /// Pinned set for quick lookups
    pub pinned_set: Signal<HashSet<String>>,
}

/// Hook for managing pinned communities
///
/// # Returns
/// A `UsePinnedCommunities` struct with state and handlers
pub fn use_pinned_communities() -> UsePinnedCommunities {
    // Use local signals that get updated on actions
    let mut pinned = use_signal(Vec::<String>::new);
    let mut pinned_set = use_signal(HashSet::<String>::new);

    // Initialize pinned communities on first use
    use_effect(move || {
        spawn(async move {
            if auth_store::get_pubkey().is_some() {
                if let Err(e) = init_pinned_communities().await {
                    log::warn!("Failed to init pinned communities: {}", e);
                }
                // Update local signals from global state
                pinned.set(get_pinned_communities());
                pinned_set.set(get_pinned_communities_set());
            }
        });
    });

    // Pin handler
    let pin = use_callback(move |a_tag: String| {
        spawn(async move {
            if let Err(e) = pin_community(a_tag.clone()).await {
                log::error!("Failed to pin community: {}", e);
            } else {
                // Update local signals
                pinned.set(get_pinned_communities());
                pinned_set.set(get_pinned_communities_set());
            }
        });
    });

    // Unpin handler
    let unpin = use_callback(move |a_tag: String| {
        spawn(async move {
            if let Err(e) = unpin_community(a_tag.clone()).await {
                log::error!("Failed to unpin community: {}", e);
            } else {
                // Update local signals
                pinned.set(get_pinned_communities());
                pinned_set.set(get_pinned_communities_set());
            }
        });
    });

    UsePinnedCommunities {
        pinned,
        pin,
        unpin,
        pinned_set,
    }
}

/// Hook to get communities sorted by membership status
///
/// # Arguments
/// * `communities` - List of communities to sort
///
/// # Returns
/// Communities wrapped with membership status and sorted by priority
pub fn use_sorted_communities(communities: Vec<Community>) -> Memo<Vec<CommunityWithMembership>> {
    use_memo(move || {
        let current_pubkey = auth_store::get_pubkey();
        let pinned_set = get_pinned_communities_set();

        sort_communities_by_membership(communities.clone(), current_pubkey.as_deref(), &pinned_set)
    })
}
