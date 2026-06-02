//! Topic Store
//! Handles NIP-73 web URL topical communities (NIP-22 comments on external content)
//!
//! Reads from both nostr.blue and clawstr.com URL namespaces.
//! Publishes with nostr.blue URLs.
//!
//! Event Kinds:
//! - 1111 (Kind::Comment): Topic posts & replies (NIP-22 with NIP-73 web URL)
//! - 10073: User subscriptions (replaceable, `I` tags for subscribed topics)
//! - 7 (Kind::Reaction): Voting - "+" upvote, "-" downvote
//!
//! Submodules:
//! - `fetch` - Fetching topic posts, subscriptions, votes, discovery
//! - `publish` - Creating posts, replies, votes, subscriptions
#![allow(dead_code)]
#![allow(unused_imports)]

mod fetch;
mod publish;

pub use fetch::*;
pub use publish::*;

use dioxus::prelude::*;
use lru::LruCache;
use nostr::Event as NostrEvent;
use nostr_sdk::prelude::*;
use std::collections::{HashMap, HashSet};
use std::num::NonZeroUsize;
use std::rc::Rc;

pub const KIND_TOPIC_SUBSCRIPTION: u16 = 10073;

const TOPIC_URL_BASE: &str = "https://nostr.blue/topics/t/";
const TOPIC_URL_CLAWSTR: &str = "https://clawstr.com/c/";

pub fn topic_to_publish_url(topic: &str) -> String {
    format!("{}{}", TOPIC_URL_BASE, topic)
}

pub fn topic_to_filter_urls(topic: &str) -> Vec<String> {
    vec![
        format!("{}{}", TOPIC_URL_BASE, topic),
        format!("{}{}", TOPIC_URL_CLAWSTR, topic),
    ]
}

fn url_to_topic(url: &str) -> Option<String> {
    url.strip_prefix(TOPIC_URL_BASE)
        .or_else(|| url.strip_prefix(TOPIC_URL_CLAWSTR))
        .map(String::from)
}

fn is_topic_url(url: &str) -> bool {
    url.starts_with(TOPIC_URL_BASE) || url.starts_with(TOPIC_URL_CLAWSTR)
}

const TOPIC_POSTS_CACHE_SIZE: usize = 500;
const TOPIC_VOTES_CACHE_SIZE: usize = 1000;
const SUBSCRIBED_TOPICS_CACHE_SIZE: usize = 100;
const DISCOVERED_TOPICS_CACHE_SIZE: usize = 200;

// --- Models ---

#[derive(Clone, Debug, PartialEq)]
pub struct TopicPost {
    pub id: String,
    pub pubkey: String,
    pub content: String,
    pub topic: String,
    pub parent_id: Option<String>,
    pub parent_pubkey: Option<String>,
    pub is_root: bool,
    pub created_at: u64,
    pub event: NostrEvent,
}

pub fn merge_pending_posts(
    mut posts: Signal<Vec<TopicPost>>,
    pending: Vec<TopicPost>,
) {
    if pending.is_empty() {
        return;
    }
    let mut current = posts.read().clone();
    let existing: HashSet<String> = current.iter().map(|p| p.id.clone()).collect();
    for p in pending {
        if !existing.contains(&p.id) {
            current.push(p);
        }
    }
    current.sort_by_key(|b| std::cmp::Reverse(b.created_at));
    posts.set(current);
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct VoteCounts {
    pub upvotes: u64,
    pub downvotes: u64,
    pub user_vote: Option<VoteDirection>,
}

impl VoteCounts {
    pub fn score(&self) -> i64 {
        self.upvotes as i64 - self.downvotes as i64
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VoteDirection {
    Up,
    Down,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TopicInfo {
    pub name: String,
    pub post_count: usize,
    pub latest_post_at: Option<u64>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TopicThread {
    pub post: TopicPost,
    pub replies: Vec<Rc<TopicThread>>,
    pub depth: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ScoredPost {
    pub post: TopicPost,
    pub score: f64,
}

// --- Global Caches ---

/// Topic posts cache (keyed by event_id)
pub static TOPIC_POSTS_CACHE: GlobalSignal<LruCache<String, TopicPost>> =
    GlobalSignal::new(|| LruCache::new(NonZeroUsize::new(TOPIC_POSTS_CACHE_SIZE).unwrap()));

/// Vote counts cache (keyed by event_id)
pub static TOPIC_VOTES_CACHE: GlobalSignal<LruCache<String, VoteCounts>> =
    GlobalSignal::new(|| LruCache::new(NonZeroUsize::new(TOPIC_VOTES_CACHE_SIZE).unwrap()));

/// User's subscribed topics (keyed by topic name)
pub static SUBSCRIBED_TOPICS: GlobalSignal<LruCache<String, bool>> =
    GlobalSignal::new(|| LruCache::new(NonZeroUsize::new(SUBSCRIBED_TOPICS_CACHE_SIZE).unwrap()));

/// Discovered topics with metadata
pub static DISCOVERED_TOPICS: GlobalSignal<LruCache<String, TopicInfo>> =
    GlobalSignal::new(|| LruCache::new(NonZeroUsize::new(DISCOVERED_TOPICS_CACHE_SIZE).unwrap()));

/// Loading states
pub static LOADING_TOPIC_POSTS: GlobalSignal<bool> = GlobalSignal::new(|| false);
pub static LOADING_SUBSCRIPTIONS: GlobalSignal<bool> = GlobalSignal::new(|| false);

// --- Cache Accessors ---

pub fn get_cached_topic_post(event_id: &str) -> Option<TopicPost> {
    TOPIC_POSTS_CACHE.read().peek(event_id).cloned()
}

pub fn cache_topic_post(post: TopicPost) {
    TOPIC_POSTS_CACHE.write().put(post.id.clone(), post);
}

pub fn cache_topic_posts(posts: &[TopicPost]) {
    let mut cache = TOPIC_POSTS_CACHE.write();
    for post in posts {
        cache.put(post.id.clone(), post.clone());
    }
}

pub fn get_cached_votes(event_id: &str) -> Option<VoteCounts> {
    TOPIC_VOTES_CACHE.read().peek(event_id).cloned()
}

pub fn cache_votes(event_id: &str, counts: VoteCounts) {
    TOPIC_VOTES_CACHE.write().put(event_id.to_string(), counts);
}

pub fn is_topic_subscribed(topic: &str) -> bool {
    SUBSCRIBED_TOPICS
        .read()
        .peek(topic)
        .copied()
        .unwrap_or(false)
}

pub fn get_subscribed_topic_names() -> Vec<String> {
    SUBSCRIBED_TOPICS
        .read()
        .iter()
        .filter(|(_, subscribed)| **subscribed)
        .map(|(name, _)| name.clone())
        .collect()
}

// --- Parsing ---

/// Parse a kind 1111 event with NIP-73 hashtag tags into a TopicPost
pub fn parse_topic_post(event: &NostrEvent) -> Option<TopicPost> {
    if event.kind != Kind::Comment {
        return None;
    }

    let topic = extract_topic_name(event)?;

    // Determine if this is a root post or reply
    // Root posts have uppercase I/K tags; replies also have lowercase e/k tags
    let parent_id = event
        .tags
        .iter()
        .find(|t| t.kind() == TagKind::SingleLetter(SingleLetterTag::lowercase(Alphabet::E)))
        .and_then(|t| t.content())
        .map(|s| s.to_string());

    let parent_pubkey = if parent_id.is_some() {
        // Get the p tag for the parent author
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

    let is_root = parent_id.is_none();

    Some(TopicPost {
        id: event.id.to_hex(),
        pubkey: event.pubkey.to_hex(),
        content: event.content.clone(),
        topic,
        parent_id,
        parent_pubkey,
        is_root,
        created_at: event.created_at.as_secs(),
        event: event.clone(),
    })
}

/// Parse kind 10073 subscription event into a list of topic names
pub fn parse_subscriptions(event: &NostrEvent) -> Vec<String> {
    if event.kind != Kind::Custom(KIND_TOPIC_SUBSCRIPTION) {
        return Vec::new();
    }

    event
        .tags
        .iter()
        .filter_map(|tag| {
            if let Some(TagStandard::ExternalContent {
                content,
                uppercase: true,
                ..
            }) = tag.as_standardized()
            {
                match content {
                    ExternalContentId::Url(url) => url_to_topic(url.as_str()),
                    ExternalContentId::Hashtag(name) => Some(name.clone()),
                    _ => None,
                }
            } else {
                None
            }
        })
        .collect()
}

/// Parse a kind 7 reaction event for vote direction
pub fn parse_vote(event: &NostrEvent) -> Option<(String, VoteDirection)> {
    if event.kind != Kind::Reaction {
        return None;
    }

    let target_id = event
        .tags
        .iter()
        .find(|t| t.kind() == TagKind::e())
        .and_then(|t| t.content())
        .map(|s| s.to_string())?;

    let direction = match event.content.as_str() {
        "+" => VoteDirection::Up,
        "-" => VoteDirection::Down,
        _ => return None,
    };

    Some((target_id, direction))
}

// --- Helpers ---

/// Check if a kind 1111 event is a topic post (has NIP-73 web URL external content)
pub fn is_topic_post(event: &NostrEvent) -> bool {
    event.kind == Kind::Comment
        && event.tags.iter().any(|tag| {
            match tag.as_standardized() {
                Some(TagStandard::ExternalContent {
                    content: ExternalContentId::Url(url),
                    ..
                }) => is_topic_url(url.as_str()),
                _ => false,
            }
        })
}

/// Extract topic name from a topic post event (prefer uppercase I tag = root)
pub fn extract_topic_name(event: &NostrEvent) -> Option<String> {
    for tag in event.tags.iter() {
        if let Some(TagStandard::ExternalContent {
            content: ExternalContentId::Url(url),
            uppercase: true,
            ..
        }) = tag.as_standardized()
        {
            if let Some(name) = url_to_topic(url.as_str()) {
                return Some(name);
            }
        }
    }
    for tag in event.tags.iter() {
        if let Some(TagStandard::ExternalContent {
            content: ExternalContentId::Url(url),
            ..
        }) = tag.as_standardized()
        {
            if let Some(name) = url_to_topic(url.as_str()) {
                return Some(name);
            }
        }
    }
    None
}

// --- Filter Builders ---

/// Filter for topic posts in a specific topic (queries both nostr.blue and clawstr URLs)
pub fn topic_posts_filter(topic: &str, limit: usize, until: Option<u64>) -> Filter {
    let urls = topic_to_filter_urls(topic);
    let mut filter = Filter::new()
        .kind(Kind::Comment)
        .custom_tags(SingleLetterTag::uppercase(Alphabet::I), urls)
        .custom_tag(SingleLetterTag::uppercase(Alphabet::K), "web".to_string())
        .limit(limit);
    if let Some(ts) = until {
        filter = filter.until(Timestamp::from(ts));
    }
    filter
}

/// Filter for recent topic posts across all topics (global feed via #K web)
pub fn recent_topic_posts_filter(limit: usize, until: Option<u64>) -> Filter {
    let mut filter = Filter::new()
        .kind(Kind::Comment)
        .custom_tag(SingleLetterTag::uppercase(Alphabet::K), "web".to_string())
        .limit(limit);
    if let Some(ts) = until {
        filter = filter.until(Timestamp::from(ts));
    }
    filter
}

/// Filter for user's topic subscriptions (kind 10073)
pub fn subscriptions_filter(pubkey: PublicKey) -> Filter {
    Filter::new()
        .kind(Kind::Custom(KIND_TOPIC_SUBSCRIPTION))
        .author(pubkey)
        .limit(1)
}

/// Filter for votes on specific events
pub fn votes_filter(event_ids: Vec<EventId>) -> Filter {
    Filter::new().kind(Kind::Reaction).events(event_ids)
}

/// Filter for replies to a specific post
pub fn post_replies_filter(post_id: &str, limit: usize) -> Filter {
    Filter::new()
        .kind(Kind::Comment)
        .custom_tag(SingleLetterTag::lowercase(Alphabet::E), post_id)
        .limit(limit)
}

// --- Thread Tree ---

/// Extract the root external content ID (uppercase I tag) from an event.
/// Used when replying to preserve the original root for thread consistency.
pub fn extract_root_external_content(event: &NostrEvent) -> Option<ExternalContentId> {
    event.tags.iter().find_map(|tag| {
        if let Some(TagStandard::ExternalContent {
            content,
            uppercase: true,
            ..
        }) = tag.as_standardized()
        {
            if let ExternalContentId::Url(url) = content {
                if is_topic_url(url.as_str()) {
                    return Some(content.clone());
                }
            }
        }
        None
    })
}

/// Build a thread tree from flat posts list
pub fn build_topic_thread_tree(posts: Vec<TopicPost>) -> Vec<Rc<TopicThread>> {
    let mut children_map: HashMap<Option<String>, Vec<TopicPost>> = HashMap::new();
    for post in posts {
        children_map
            .entry(post.parent_id.clone())
            .or_default()
            .push(post);
    }
    // Sort children by created_at ascending (oldest first)
    for posts_vec in children_map.values_mut() {
        posts_vec.sort_by_key(|a| a.created_at);
    }

    fn build_tree(
        parent_id: Option<String>,
        map: &HashMap<Option<String>, Vec<TopicPost>>,
        depth: usize,
        max_depth: usize,
    ) -> Vec<Rc<TopicThread>> {
        if depth > max_depth {
            return Vec::new();
        }
        map.get(&parent_id)
            .map(|posts| {
                posts
                    .iter()
                    .map(|post| {
                        Rc::new(TopicThread {
                            post: post.clone(),
                            replies: build_tree(Some(post.id.clone()), map, depth + 1, max_depth),
                            depth,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    let mut tree = build_tree(None, &children_map, 0, 10);
    tree.sort_by_key(|b| std::cmp::Reverse(b.post.created_at));
    tree
}

/// Flatten a thread tree to a list with depth info
pub fn flatten_topic_thread_tree(tree: Vec<Rc<TopicThread>>) -> Vec<(TopicPost, usize)> {
    let mut result = Vec::new();
    fn flatten_recursive(threads: &[Rc<TopicThread>], result: &mut Vec<(TopicPost, usize)>) {
        for thread in threads {
            result.push((thread.post.clone(), thread.depth));
            flatten_recursive(&thread.replies, result);
        }
    }
    flatten_recursive(&tree, &mut result);
    result
}

// --- Hot Score ---

/// Compute a "hot" ranking score for a post (higher = hotter)
/// Balances engagement with time decay
pub fn compute_hot_score(
    votes: &VoteCounts,
    reply_count: usize,
    zap_sats: u64,
    created_at: u64,
    now: u64,
) -> f64 {
    let engagement = (zap_sats as f64 * 0.1)
        + (votes.upvotes as f64 - votes.downvotes as f64)
        + (reply_count as f64 * 2.0);
    let hours_age = (now.saturating_sub(created_at)) as f64 / 3600.0;
    engagement / (hours_age + 2.0).powf(1.5)
}
