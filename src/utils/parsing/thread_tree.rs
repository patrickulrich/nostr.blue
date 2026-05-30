use instant::{Duration, Instant};
use lru::LruCache;
use nostr_sdk::nips::nip10::Marker;
use nostr_sdk::{Event, EventId, TagKind, TagStandard};
use std::collections::HashMap;
use std::num::NonZeroUsize;
use std::sync::{Mutex, OnceLock};

/// Represents a node in a threaded conversation tree
#[derive(Debug, Clone, PartialEq)]
pub struct ThreadNode {
    pub event: Event,
    pub children: Vec<ThreadNode>,
}

impl ThreadNode {
    /// Create a new thread node
    pub fn new(event: Event) -> Self {
        Self {
            event,
            children: Vec::new(),
        }
    }
}

/// Get the parent event ID from a reply event
///
/// This implements NIP-10 logic for regular replies and NIP-22 logic for comments:
/// - For NIP-10 (kind 1 replies):
///   - Looks for lowercase 'e' tags with "reply" marker
///   - Falls back to 'e' tags with "root" marker if no reply marker
///   - Falls back to last 'e' tag if no markers present (positional)
/// - For NIP-22 (kind 1111 comments):
///   - Looks for lowercase 'e' tag (parent reference)
///   - Falls back to uppercase 'E' tag (root reference) if no lowercase 'e' tag
fn get_parent_id(event: &Event) -> Option<EventId> {
    let mut reply_marker_id = None;
    let mut root_marker_id = None;
    let mut last_unmarked_id = None;

    for tag in event.tags.iter() {
        if let Some(TagStandard::Event {
            event_id,
            marker,
            uppercase: false,
            ..
        }) = tag.as_standardized()
        {
            match marker {
                Some(Marker::Reply) => {
                    if reply_marker_id.is_none() {
                        reply_marker_id = Some(*event_id);
                    }
                }
                Some(Marker::Root) => {
                    if root_marker_id.is_none() {
                        root_marker_id = Some(*event_id);
                    }
                }
                None => {
                    last_unmarked_id = Some(*event_id);
                }
            }
        }
    }

    if reply_marker_id.is_some() {
        return reply_marker_id;
    }
    if root_marker_id.is_some() {
        return root_marker_id;
    }
    if last_unmarked_id.is_some() {
        return last_unmarked_id;
    }

    if event.kind == nostr_sdk::Kind::Comment {
        let upper_e_tags: Vec<_> = event
            .tags
            .iter()
            .filter(|tag| {
                tag.kind()
                    == TagKind::SingleLetter(nostr_sdk::SingleLetterTag::uppercase(
                        nostr_sdk::Alphabet::E,
                    ))
            })
            .collect();
        if let Some(first_tag) = upper_e_tags.first() {
            if let Some(TagStandard::Event { event_id, .. }) = first_tag.as_standardized() {
                return Some(*event_id);
            }
        }
    }
    None
}

/// Extract the root event id from an event's NIP-10 root tag.
pub fn extract_root_event_id(event: &Event) -> Option<EventId> {
    event.tags.iter().find_map(|tag| {
        let slice = tag.as_slice();
        if slice.first().map(|s| s.as_str()) == Some("e")
            && slice.get(3).map(|s| s.as_str()) == Some("root")
        {
            slice.get(1).and_then(|id| EventId::from_hex(id).ok())
        } else {
            None
        }
    })
}

/// Resolve the thread root event ID from a note's tags, without network access.
///
/// Uses NIP-10 tag semantics to determine the thread root:
/// 1. Explicit `marker=root` on a lowercase `e` tag (preferred)
/// 2. For NIP-22 comments: uppercase `E` tag (root reference)
/// 3. Legacy positional: first lowercase `e` tag when multiple unmarked tags exist
/// 4. Returns `None` if the event has no parent references (it IS the root)
pub fn resolve_thread_root_id(event: &Event) -> Option<EventId> {
    let mut root_marker_id = None;
    let mut first_e_tag_id = None;
    let mut e_tag_count = 0usize;

    for tag in event.tags.iter() {
        if let Some(TagStandard::Event {
            event_id,
            marker,
            uppercase: false,
            ..
        }) = tag.as_standardized()
        {
            e_tag_count += 1;
            if first_e_tag_id.is_none() {
                first_e_tag_id = Some(*event_id);
            }
            if *marker == Some(Marker::Root) && root_marker_id.is_none() {
                root_marker_id = Some(*event_id);
            }
        }
    }

    if root_marker_id.is_some() {
        return root_marker_id;
    }

    if event.kind == nostr_sdk::Kind::Comment {
        let upper_e_tag = nostr_sdk::SingleLetterTag::uppercase(nostr_sdk::Alphabet::E);
        for tag in event.tags.iter() {
            if tag.kind() == TagKind::SingleLetter(upper_e_tag) {
                if let Some(TagStandard::Event { event_id, .. }) = tag.as_standardized() {
                    return Some(*event_id);
                }
            }
        }
    }

    if e_tag_count >= 2 {
        return first_e_tag_id;
    }

    None
}

/// Cached thread tree with TTL tracking
#[derive(Clone, Debug)]
struct CachedThreadTree {
    tree: Vec<ThreadNode>,
    cached_at: Instant,
}

impl CachedThreadTree {
    fn new(tree: Vec<ThreadNode>) -> Self {
        Self {
            tree,
            cached_at: Instant::now(),
        }
    }

    /// Check if cache entry is still valid (within TTL)
    fn is_valid(&self, ttl: Duration) -> bool {
        self.cached_at.elapsed() < ttl
    }
}

/// L2 cache for NIP-10 thread trees (Phase 3.5)
///
/// In-memory LRU cache that sits between database and UI:
/// - Reduces expensive thread tree computations for recently-viewed threads
/// - Automatic TTL-based freshness control
/// - LRU eviction prevents unbounded growth
struct ThreadTreeCache {
    cache: LruCache<String, CachedThreadTree>,
    ttl: Duration,
}

impl ThreadTreeCache {
    fn new(capacity: usize, ttl: Duration) -> Self {
        Self {
            cache: LruCache::new(NonZeroUsize::new(capacity).unwrap()),
            ttl,
        }
    }

    /// Get cached thread tree if it exists and is still valid
    fn get(&mut self, root_event_id: &str) -> Option<Vec<ThreadNode>> {
        if let Some(cached) = self.cache.get(root_event_id) {
            if cached.is_valid(self.ttl) {
                return Some(cached.tree.clone());
            }
        }
        None
    }

    /// Cache thread tree for a root event
    fn insert(&mut self, root_event_id: String, tree: Vec<ThreadNode>) {
        self.cache.put(root_event_id, CachedThreadTree::new(tree));
    }

    /// Invalidate (remove) cached thread tree for a root event
    ///
    /// Useful when a new reply is posted to the thread
    fn invalidate(&mut self, root_event_id: &str) {
        self.cache.pop(root_event_id);
    }
}

/// Global L2 cache for thread trees
///
/// Cache configuration:
/// - Capacity: 200 threads (enough for typical browsing session)
/// - TTL: 10 minutes (threads don't change as frequently as counts)
static THREAD_TREE_CACHE: OnceLock<Mutex<ThreadTreeCache>> = OnceLock::new();

/// Get or initialize the thread tree cache
fn get_thread_tree_cache() -> &'static Mutex<ThreadTreeCache> {
    THREAD_TREE_CACHE
        .get_or_init(|| Mutex::new(ThreadTreeCache::new(200, Duration::from_secs(600))))
}

/// Build a threaded conversation tree from a flat list of reply events
///
/// Returns a vec of top-level ThreadNode objects (direct replies to root event)
/// Each ThreadNode can have nested children representing the conversation thread
///
/// **Phase 3.5 L2 Caching**: Results are cached with 10-minute TTL to avoid
/// expensive re-computation of thread trees on repeated views.
///
/// # Arguments
/// * `replies` - Flat list of reply events
/// * `root_event_id` - The ID of the root event being replied to
///
/// # Algorithm
/// 1. Check L2 cache for existing tree (if valid)
/// 2. Create a map of event ID to ThreadNode for fast lookup
/// 3. For each reply, determine its parent using NIP-10 logic
/// 4. Build parent-child relationships
/// 5. Sort by timestamp (chronological order)
/// 6. Cache result for future calls
pub fn build_thread_tree(replies: Vec<Event>, root_event_id: &EventId) -> Vec<ThreadNode> {
    let root_id_hex = root_event_id.to_hex();
    {
        let mut cache = get_thread_tree_cache().lock().unwrap_or_else(|poisoned| {
            log::warn!("Thread tree cache mutex was poisoned, recovering");
            poisoned.into_inner()
        });
        if let Some(cached_tree) = cache.get(&root_id_hex) {
            log::debug!("Thread tree cache HIT for {}", root_id_hex);
            return cached_tree;
        }
        log::debug!(
            "Thread tree cache MISS for {}, building tree...",
            root_id_hex
        );
    }
    let mut node_map: HashMap<EventId, ThreadNode> = HashMap::new();
    for reply in &replies {
        node_map.insert(reply.id, ThreadNode::new(reply.clone()));
    }
    let mut root_replies: Vec<ThreadNode> = Vec::new();
    for reply in &replies {
        let parent_event_id = get_parent_id(reply);
        match parent_event_id {
            None => {
                if let Some(node) = node_map.remove(&reply.id) {
                    root_replies.push(node);
                }
            }
            Some(parent_id) => {
                if parent_id == reply.id {
                    if let Some(node) = node_map.remove(&reply.id) {
                        root_replies.push(node);
                    }
                    continue;
                }
                if parent_id == *root_event_id {
                    if let Some(node) = node_map.remove(&reply.id) {
                        root_replies.push(node);
                    }
                }
            }
        }
    }
    let mut node_map: HashMap<EventId, ThreadNode> = HashMap::new();
    for reply in &replies {
        node_map.insert(reply.id, ThreadNode::new(reply.clone()));
    }
    let mut processed: HashMap<EventId, ThreadNode> = HashMap::new();
    for reply in &replies {
        let parent_event_id = get_parent_id(reply);
        if let Some(parent_id) = parent_event_id {
            if parent_id != reply.id && parent_id != *root_event_id {
                continue;
            }
        }
        if let Some(node) = node_map.remove(&reply.id) {
            processed.insert(reply.id, node);
        }
    }
    fn attach_children(
        parent_id: &EventId,
        all_replies: &[Event],
        node_map: &mut HashMap<EventId, ThreadNode>,
    ) -> Vec<ThreadNode> {
        let mut children = Vec::new();
        for reply in all_replies {
            if let Some(reply_parent_id) = get_parent_id(reply) {
                if reply_parent_id == *parent_id && reply.id != *parent_id {
                    if let Some(mut node) = node_map.remove(&reply.id) {
                        node.children = attach_children(&reply.id, all_replies, node_map);
                        children.push(node);
                    }
                }
            }
        }
        children.sort_by_key(|a| a.event.created_at);
        children
    }
    root_replies = processed.into_values().collect();
    for node in &mut root_replies {
        node.children = attach_children(&node.event.id, &replies, &mut node_map);
    }
    root_replies.sort_by_key(|a| a.event.created_at);
    {
        let mut cache = get_thread_tree_cache().lock().unwrap_or_else(|poisoned| {
            log::warn!("Thread tree cache mutex was poisoned, recovering");
            poisoned.into_inner()
        });
        cache.insert(root_id_hex, root_replies.clone());
    }
    root_replies
}

/// Count the total number of replies in a thread tree (including nested replies)
#[cfg(test)]
#[allow(dead_code)]
pub fn count_total_replies(nodes: &[ThreadNode]) -> usize {
    let mut count = 0;
    for node in nodes {
        count += 1;
        count += count_total_replies(&node.children);
    }
    count
}

/// Invalidate cached thread tree for a root event
///
/// Call this when a new reply is published to a thread to ensure
/// the next call to build_thread_tree() rebuilds the tree with fresh data.
///
/// # Example
/// ```
/// // After user publishes a reply
/// publish_reply(root_event_id, content).await?;
/// invalidate_thread_tree_cache(&root_event_id);
/// ```
pub fn invalidate_thread_tree_cache(root_event_id: &EventId) {
    let root_id_hex = root_event_id.to_hex();
    {
        let mut cache = get_thread_tree_cache().lock().unwrap_or_else(|poisoned| {
            log::warn!("Thread tree cache mutex was poisoned, recovering");
            poisoned.into_inner()
        });
        cache.invalidate(&root_id_hex);
    }
    log::debug!("Invalidated thread tree cache for {}", root_id_hex);
}

#[cfg(test)]
mod tests {
    use super::extract_root_event_id;
    use nostr_sdk::{EventBuilder, EventId, Keys, Kind, Tag};

    #[test]
    fn extract_root_event_id_returns_root_marker_event() {
        let keys = Keys::generate();
        let root_id = EventId::all_zeros();
        let event = EventBuilder::new(Kind::TextNote, "reply")
            .tags(vec![
                Tag::parse(["e", &root_id.to_hex(), "", "root"]).unwrap()
            ])
            .sign_with_keys(&keys)
            .unwrap();

        assert_eq!(extract_root_event_id(&event), Some(root_id));
    }

    #[test]
    fn extract_root_event_id_returns_none_without_root_marker() {
        let keys = Keys::generate();
        let event = EventBuilder::new(Kind::TextNote, "reply")
            .tags(vec![
                Tag::parse(["e", &EventId::all_zeros().to_hex()]).unwrap()
            ])
            .sign_with_keys(&keys)
            .unwrap();

        assert_eq!(extract_root_event_id(&event), None);
    }
}
