use nostr_sdk::{Event, EventId, TagKind};
use std::collections::HashMap;

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

/// Build a threaded conversation tree from a flat list of reply events
///
/// Returns a vec of top-level ThreadNode objects (direct replies to root event)
/// Each ThreadNode can have nested children representing the conversation thread
///
/// # Arguments
/// * `replies` - Flat list of reply events
/// * `root_event_id` - The ID of the root event being replied to
///
/// # Algorithm
/// 1. Create a map of event ID to ThreadNode for fast lookup
/// 2. For each reply, determine its parent using NIP-10 logic
/// 3. Build parent-child relationships
/// 4. Sort by timestamp (chronological order)
pub fn build_thread_tree(replies: Vec<Event>, root_event_id: &EventId) -> Vec<ThreadNode> {
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
