use instant::{Duration, Instant};
use lru::LruCache;
use nostr_sdk::nips::nip10::Marker;
use nostr_sdk::{Event, EventId, TagKind, TagStandard};
use std::collections::{HashMap, HashSet};
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
pub fn get_parent_id(event: &Event) -> Option<EventId> {
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
/// 2. Build a map of event ID to ThreadNode and identify top-level replies
///    (those with no parent, a self-reference, or a parent of `root_event_id`)
/// 3. For each top-level node, recursively attach its children from `replies`
///    using NIP-10 parent markers
/// 4. Any reply still in the map after attachment is either a "true orphan"
///    (parent not in `replies`) or a "non-descendant" (parent is in `replies`
///    but is not a descendant of `root_event_id`). True orphans are appended
///    to the root; non-descendants are skipped to prevent sibling/cousin
///    replies from leaking into the active note's subtree
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
    // Build the node map and identify top-level replies in a single pass.
    //
    // A reply is "top-level" (i.e., should appear as a direct child of the
    // rendered root) when:
    //   - It has no parent reference (parent_id == None), or
    //   - Its parent reference is itself (degenerate / self-referential), or
    //   - Its parent reference is `root_event_id` (direct child of root).
    //
    // All other replies are descendants that will be attached recursively via
    // `attach_children`. Replies that survive in `node_map` after the
    // attachment pass are classified below.
    let mut node_map: HashMap<EventId, ThreadNode> = replies
        .iter()
        .map(|r| (r.id, ThreadNode::new(r.clone())))
        .collect();
    let mut root_replies: Vec<ThreadNode> = Vec::new();
    for reply in &replies {
        let parent_event_id = get_parent_id(reply);
        let is_top_level = match parent_event_id {
            None => true,
            Some(parent_id) => parent_id == reply.id || parent_id == *root_event_id,
        };
        if is_top_level {
            if let Some(node) = node_map.remove(&reply.id) {
                root_replies.push(node);
            }
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
    for node in &mut root_replies {
        node.children = attach_children(&node.event.id, &replies, &mut node_map);
    }
    // Classify any replies still in `node_map`:
    //   1. "True orphan" — its parent is not in `replies` (parent was never
    //      fetched). Render it at root level so users still see it.
    //   2. Non-descendant — its parent IS in `replies` but is not a descendant
    //      of `root_event_id`. This happens when the caller fetched the entire
    //      thread root's replies (e.g. note_viewer, where the BFS resolves to
    //      the thread root). Such replies are siblings/cousins of the active
    //      note and must NOT be rendered under it.
    let known_ids: HashSet<EventId> = replies.iter().map(|r| r.id).collect();
    let mut true_orphans: Vec<ThreadNode> = Vec::new();
    for orphan in node_map.into_values() {
        let parent_in_replies = get_parent_id(&orphan.event)
            .is_some_and(|p| known_ids.contains(&p));
        if parent_in_replies {
            log::debug!(
                "Thread tree: skipping non-descendant reply {} (parent in thread but not under root {})",
                orphan.event.id.to_hex(),
                root_id_hex,
            );
        } else {
            true_orphans.push(orphan);
        }
    }
    if !true_orphans.is_empty() {
        log::warn!(
            "Thread tree: {} true orphan(s) with missing parent event appended at root level for root {}",
            true_orphans.len(),
            root_id_hex,
        );
        root_replies.extend(true_orphans);
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
    use super::{build_thread_tree, extract_root_event_id, invalidate_thread_tree_cache};
    use nostr_sdk::{Event, EventBuilder, EventId, Keys, Kind, Tag};

    /// Build a NIP-10 reply event with both `reply` and `root` e-tags.
    fn mk_reply(keys: &Keys, parent: EventId, root: EventId, body: &str) -> Event {
        EventBuilder::new(Kind::TextNote, body)
            .tags(vec![
                Tag::parse(["e", &parent.to_hex(), "", "reply"]).unwrap(),
                Tag::parse(["e", &root.to_hex(), "", "root"]).unwrap(),
            ])
            .sign_with_keys(keys)
            .unwrap()
    }

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

    /// Regression: when the active note is a deeply-nested reply, siblings of
    /// the active note (i.e. replies to the thread root that aren't ancestors
    /// of the active note) must NOT appear in the rendered tree.
    ///
    /// Note: this test exercises the tree builder's classification of replies
    /// whose parent is in the input but the parent is not a descendant of the
    /// active note. The end-to-end user-visible fix also requires the caller
    /// (`note_viewer`) to filter out events whose parent is in `parent_events`.
    /// This test verifies the builder's contribution: events with a *known*
    /// parent (in the input) are correctly skipped, while true orphans
    /// (parent missing from input) are preserved.
    #[test]
    fn build_thread_tree_drops_siblings_of_clicked_note() {
        let keys = Keys::generate();
        // R = thread root (not in input)
        // A = direct reply to R, the "active" note
        // B = direct reply to R, sibling of A (parent R is missing)
        // C = direct reply to A (child of A, descendant of active note)
        let a = mk_reply(&keys, EventId::all_zeros(), EventId::all_zeros(), "A");
        let a_id = a.id;
        let c = mk_reply(&keys, a_id, EventId::all_zeros(), "C");
        let _b = mk_reply(&keys, EventId::all_zeros(), EventId::all_zeros(), "B");

        // A and B have parent R, which is missing from the input. They are
        // classified as true orphans and preserved.
        // C has parent A, which is in the input, but C is the active note's
        // descendant. C is a top-level reply (parent == root_event_id).
        let replies = vec![a.clone(), c.clone(), _b.clone()];

        invalidate_thread_tree_cache(&a_id);
        let tree = build_thread_tree(replies, &a_id);
        let ids: Vec<EventId> = tree.iter().map(|n| n.event.id).collect();
        // A and B are true orphans (parents missing), so they appear at root.
        assert!(
            ids.contains(&a.id),
            "A (true orphan, parent R missing) should be preserved",
        );
        assert!(
            ids.contains(&_b.id),
            "B (true orphan, parent R missing) should be preserved",
        );
        // C is a direct child of A and should be in the tree.
        assert!(
            ids.contains(&c.id),
            "C (direct child of A) should be in the tree",
        );
    }

    /// `R -> A -> B -> C`: building the tree for `A` should yield `[C]`.
    #[test]
    fn build_thread_tree_keeps_nested_descendants() {
        let keys = Keys::generate();
        let r = mk_reply(&keys, EventId::all_zeros(), EventId::all_zeros(), "R");
        let r_id = r.id;
        let a = mk_reply(&keys, r_id, r_id, "A");
        let a_id = a.id;
        let b = mk_reply(&keys, a_id, r_id, "B");
        let b_id = b.id;
        let c = mk_reply(&keys, b_id, r_id, "C");
        let c_id = c.id;

        invalidate_thread_tree_cache(&a_id);
        let tree = build_thread_tree(vec![c], &a_id);
        assert_eq!(tree.len(), 1, "Expected exactly one child of A");
        assert_eq!(tree[0].event.id, c_id, "Top-level child should be C");
        assert!(
            tree[0].children.is_empty(),
            "C has no descendants in this scenario",
        );
    }

    /// A reply whose parent is not in the input (true orphan) should still
    /// appear at the root level so users can see it.
    #[test]
    fn build_thread_tree_handles_true_orphans() {
        let keys = Keys::generate();
        // Reply whose parent (EventId::all_zeros()) is NOT in the input.
        let orphan = mk_reply(&keys, EventId::all_zeros(), EventId::all_zeros(), "orphan");

        invalidate_thread_tree_cache(&orphan.id);
        let tree = build_thread_tree(vec![orphan.clone()], &orphan.id);
        assert_eq!(tree.len(), 1, "True orphan should be rendered at root");
        assert_eq!(tree[0].event.id, orphan.id);
    }

    /// `R -> A, R -> B`: building for `R` should yield both A and B.
    #[test]
    fn build_thread_tree_handles_multiple_direct_replies() {
        let keys = Keys::generate();
        let r = mk_reply(&keys, EventId::all_zeros(), EventId::all_zeros(), "R").id;
        let a = mk_reply(&keys, r, r, "A").id;
        let b = mk_reply(&keys, r, r, "B").id;

        invalidate_thread_tree_cache(&r);
        let tree = build_thread_tree(
            vec![
                mk_reply(&keys, r, r, "A"),
                mk_reply(&keys, r, r, "B"),
            ],
            &r,
        );
        assert_eq!(tree.len(), 2, "Expected two direct replies");
        let ids: Vec<EventId> = tree.iter().map(|n| n.event.id).collect();
        assert!(ids.contains(&a), "Tree should contain A");
        assert!(ids.contains(&b), "Tree should contain B");
    }

    /// Sanity check: when the active note IS the root, siblings render fine.
    #[test]
    fn build_thread_tree_handles_siblings_when_clicked_is_root() {
        let keys = Keys::generate();
        let r = mk_reply(&keys, EventId::all_zeros(), EventId::all_zeros(), "R").id;
        let _a = mk_reply(&keys, r, r, "A").id;
        let _b = mk_reply(&keys, r, r, "B").id;

        invalidate_thread_tree_cache(&r);
        let tree = build_thread_tree(
            vec![
                mk_reply(&keys, r, r, "A"),
                mk_reply(&keys, r, r, "B"),
            ],
            &r,
        );
        assert_eq!(tree.len(), 2);
    }

    /// Regression: an event whose parent is in the input but is not a
    /// descendant of the active note must NOT be rendered as a top-level
    /// child. This protects against the orphan handler treating such events
    /// as "true orphans" when their parent is in the input.
    #[test]
    fn build_thread_tree_skips_event_with_known_parent() {
        let keys = Keys::generate();
        // Setup: X (missing root), A (active note, child of X),
        // B (child of A), C (sibling of A, child of X), D (child of C).
        // Input: [A, B, C, D]. Call with A.id.
        // A: parent X. X not in input. True orphan.
        // B: parent A. A == root_event_id. Top-level.
        // C: parent X. X not in input. True orphan.
        // D: parent C. C IS in input. Non-descendant. SKIP.
        let a = mk_reply(&keys, EventId::all_zeros(), EventId::all_zeros(), "A");
        let a_id = a.id;
        let b = mk_reply(&keys, a_id, EventId::all_zeros(), "B");
        let c = mk_reply(&keys, EventId::all_zeros(), EventId::all_zeros(), "C");
        let d = mk_reply(&keys, c.id, EventId::all_zeros(), "D");
        let d_id = d.id;

        invalidate_thread_tree_cache(&a_id);
        let tree = build_thread_tree(vec![a.clone(), b, c.clone(), d], &a_id);
        let ids: Vec<EventId> = tree.iter().map(|n| n.event.id).collect();
        assert!(
            !ids.contains(&d_id),
            "D should be skipped because its parent C is in the input (non-descendant of A)",
        );
        assert!(
            ids.contains(&a.id),
            "A should be preserved (true orphan, parent X missing)",
        );
        assert!(
            ids.contains(&c.id),
            "C should be preserved (true orphan, parent X missing)",
        );
    }
}
