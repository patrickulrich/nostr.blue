use lru::LruCache;
use nostr_sdk::{Event, EventId, TagKind};
use std::collections::HashMap;
use std::num::NonZeroUsize;
use std::sync::{Mutex, OnceLock};
use instant::{Duration, Instant};

/// Represents a node in a threaded conversation tree
#[derive(Debug, Clone, PartialEq)]
pub struct ThreadNode {
    pub event: Event,
    pub children: Vec<ThreadNode>,
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
    // First, try lowercase 'e' tags (standard NIP-10 and NIP-22 parent reference)
    let e_tags: Vec<_> = event.tags.iter()
        .filter(|tag| tag.kind() == TagKind::SingleLetter(nostr_sdk::SingleLetterTag::lowercase(nostr_sdk::Alphabet::E)))
        .collect();

    if !e_tags.is_empty() {
        // First, look for a tag with "reply" marker (NIP-10 preferred reply)
        for tag in &e_tags {
            let content = tag.content();
            if let Some(parts) = content {
                let parts_vec: Vec<&str> = parts.split('\t').collect();
                // parts_vec[0] = event id, parts_vec[2] = marker (optional)
                if parts_vec.len() >= 3 && parts_vec[2] == "reply" {
                    if let Ok(event_id) = EventId::from_hex(parts_vec[0]) {
                        return Some(event_id);
                    }
                }
            }
        }

        // If we only have one 'e' tag, it's the parent
        if e_tags.len() == 1 {
            if let Some(content) = e_tags[0].content() {
                let parts: Vec<&str> = content.split('\t').collect();
                if let Ok(event_id) = EventId::from_hex(parts[0]) {
                    return Some(event_id);
                }
            }
        }

        // Positional fallback: last 'e' tag is the parent (NIP-10 deprecated positional)
        if let Some(last_tag) = e_tags.last() {
            if let Some(content) = last_tag.content() {
                let parts: Vec<&str> = content.split('\t').collect();
                if let Ok(event_id) = EventId::from_hex(parts[0]) {
                    return Some(event_id);
                }
            }
        }
    }

    // NIP-22 fallback: For kind 1111 comments, if no lowercase 'e' tag found,
    // check for uppercase 'E' tag (root reference)
    // This handles non-compliant comments that might only have uppercase tags
    if event.kind == nostr_sdk::Kind::Comment {
        let upper_e_tags: Vec<_> = event.tags.iter()
            .filter(|tag| tag.kind() == TagKind::SingleLetter(nostr_sdk::SingleLetterTag::uppercase(nostr_sdk::Alphabet::E)))
            .collect();

        if let Some(first_tag) = upper_e_tags.first() {
            if let Some(content) = first_tag.content() {
                let parts: Vec<&str> = content.split('\t').collect();
                if let Ok(event_id) = EventId::from_hex(parts[0]) {
                    return Some(event_id);
                }
            }
        }
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
            // Entry expired, will be overwritten on next insert
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
    #[allow(dead_code)]
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
    THREAD_TREE_CACHE.get_or_init(|| {
        Mutex::new(ThreadTreeCache::new(
            200,
            Duration::from_secs(600), // 10 minutes
        ))
    })
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

    // Phase 3.5: Check L2 cache first
    {
        let mut cache = get_thread_tree_cache().lock().unwrap();
        if let Some(cached_tree) = cache.get(&root_id_hex) {
            log::debug!("Thread tree cache HIT for {}", root_id_hex);
            return cached_tree;
        }
        log::debug!("Thread tree cache MISS for {}, building tree...", root_id_hex);
        // Cache lock released here
    }

    // Cache miss - build the tree
    // Create a map of event ID to node for quick lookup
    let mut node_map: HashMap<EventId, ThreadNode> = HashMap::new();

    // Initialize nodes for all replies
    for reply in &replies {
        node_map.insert(
            reply.id,
            ThreadNode {
                event: reply.clone(),
                children: Vec::new(),
            },
        );
    }

    // Array to store top-level replies (direct replies to the root event)
    let mut root_replies: Vec<ThreadNode> = Vec::new();

    // Build the tree by connecting parent-child relationships
    for reply in &replies {
        // Get parent event ID using NIP-10 logic
        let parent_event_id = get_parent_id(reply);

        match parent_event_id {
            None => {
                // No parent reference, treat as root-level reply
                if let Some(node) = node_map.remove(&reply.id) {
                    root_replies.push(node);
                }
            }
            Some(parent_id) => {
                // Guard against self-referential parents
                if parent_id == reply.id {
                    // Self-reference detected, treat as root-level
                    if let Some(node) = node_map.remove(&reply.id) {
                        root_replies.push(node);
                    }
                    continue;
                }

                if parent_id == *root_event_id {
                    // This is a direct reply to the root event
                    if let Some(node) = node_map.remove(&reply.id) {
                        root_replies.push(node);
                    }
                } else {
                    // This is a reply to another reply
                    // We need to add this node to its parent's children
                    // But we can't easily modify the HashMap while iterating
                    // So we'll do a second pass
                }
            }
        }
    }

    // Second pass: connect nested replies
    // We need to rebuild this because we removed nodes from the map
    let mut node_map: HashMap<EventId, ThreadNode> = HashMap::new();
    for reply in &replies {
        node_map.insert(
            reply.id,
            ThreadNode {
                event: reply.clone(),
                children: Vec::new(),
            },
        );
    }

    // Build parent-child relationships
    let mut processed: HashMap<EventId, ThreadNode> = HashMap::new();

    for reply in &replies {
        let parent_event_id = get_parent_id(reply);

        if let Some(parent_id) = parent_event_id {
            if parent_id != reply.id && parent_id != *root_event_id {
                // This is a nested reply - we'll handle it after collecting root replies
                continue;
            }
        }

        // This is a root-level reply
        if let Some(node) = node_map.remove(&reply.id) {
            processed.insert(reply.id, node);
        }
    }

    // Now recursively attach children
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
                        // Recursively attach this node's children
                        node.children = attach_children(&reply.id, all_replies, node_map);
                        children.push(node);
                    }
                }
            }
        }

        // Sort children by timestamp
        children.sort_by(|a, b| a.event.created_at.cmp(&b.event.created_at));
        children
    }

    // Attach children to root replies
    root_replies = processed.into_values().collect();
    for node in &mut root_replies {
        node.children = attach_children(&node.event.id, &replies, &mut node_map);
    }

    // Sort root replies by timestamp
    root_replies.sort_by(|a, b| a.event.created_at.cmp(&b.event.created_at));

    // Phase 3.5: Cache the result for future calls
    {
        let mut cache = get_thread_tree_cache().lock().unwrap();
        cache.insert(root_id_hex, root_replies.clone());
    }

    root_replies
}

/// Count the total number of replies in a thread tree (including nested replies)
#[allow(dead_code)]
pub fn count_total_replies(nodes: &[ThreadNode]) -> usize {
    let mut count = 0;
    for node in nodes {
        count += 1; // Count this node
        count += count_total_replies(&node.children); // Count descendants
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
#[allow(dead_code)]
pub fn invalidate_thread_tree_cache(root_event_id: &EventId) {
    let root_id_hex = root_event_id.to_hex();
    {
        let mut cache = get_thread_tree_cache().lock().unwrap();
        cache.invalidate(&root_id_hex);
    }
    log::debug!("Invalidated thread tree cache for {}", root_id_hex);
}

/// Invalidate cached thread trees for multiple root events at once
#[allow(dead_code)]
pub fn invalidate_thread_tree_cache_batch(root_event_ids: &[EventId]) {
    {
        let mut cache = get_thread_tree_cache().lock().unwrap();
        for event_id in root_event_ids {
            let root_id_hex = event_id.to_hex();
            cache.invalidate(&root_id_hex);
        }
    }
    log::debug!("Invalidated thread tree cache for {} threads", root_event_ids.len());
}
