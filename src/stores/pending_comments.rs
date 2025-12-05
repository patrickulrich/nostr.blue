//! Pending Comments Store - Optimistic updates for comments
//!
//! This store manages comments that are being published or have failed.
//! It enables optimistic UI updates so users see their comments immediately.

use dioxus::prelude::*;
use nostr_sdk::{Event as NostrEvent, EventId, Kind, PublicKey, Timestamp};
use nostr_sdk::prelude::{EventBuilder, CommentTarget, Tags};
use nostr_sdk::secp256k1::schnorr::Signature;
use std::collections::HashMap;
use std::str::FromStr;
use wasm_bindgen_futures::spawn_local;

use crate::stores::nostr_client::{get_client, publish_note};
use crate::utils::thread_tree::invalidate_thread_tree_cache;

/// Status of a pending comment
#[derive(Clone, Debug, PartialEq)]
pub enum CommentStatus {
    /// Comment is being published to relays
    Pending,
    /// Comment published successfully
    Confirmed(EventId),
    /// Comment failed to publish with error message
    Failed(String),
}

/// Represents a comment that hasn't been confirmed on relays yet
#[derive(Clone, Debug)]
pub struct PendingComment {
    /// Temporary local ID (UUID)
    pub local_id: String,
    /// The content of the comment
    pub content: String,
    /// The event being commented on (root for NIP-22)
    pub target_event_id: EventId,
    /// Parent comment ID (if replying to another comment)
    pub parent_comment_id: Option<EventId>,
    /// The kind of comment (1 for NIP-10 reply, 1111 for NIP-22 comment)
    pub kind: Kind,
    /// Current publication status
    pub status: CommentStatus,
    /// Timestamp when comment was created locally
    pub created_at: Timestamp,
    /// Author's public key
    pub author_pubkey: PublicKey,
    /// Original target event (needed for retry - prefixed to silence warning until retry is implemented)
    #[allow(dead_code)]
    pub target_event: NostrEvent,
    /// Original parent comment (needed for retry - prefixed to silence warning until retry is implemented)
    #[allow(dead_code)]
    pub parent_comment: Option<NostrEvent>,
}

impl PendingComment {
    /// Create a pseudo-Event for display purposes
    /// This allows ThreadedComment to render pending comments using the same UI
    ///
    /// Uses Event::new() directly to construct an event with the correct author pubkey
    /// without requiring cryptographic signing. The signature is a dummy value since
    /// this event is only used for local display and is never verified or published.
    pub fn to_display_event(&self) -> NostrEvent {
        // Use empty tags for display event
        let tags = Tags::new();

        // Compute the event ID correctly using the real author pubkey
        let id = EventId::new(
            &self.author_pubkey,
            &self.created_at,
            &self.kind,
            &tags,
            &self.content,
        );

        // Create a dummy signature (won't verify, but that's OK for display-only)
        // All zeros is a valid 64-byte hex string for Signature parsing
        // Use unwrap_or_else instead of expect to avoid unhelpful WASM panics
        let dummy_sig = Signature::from_str(
            "0000000000000000000000000000000000000000000000000000000000000000\
             0000000000000000000000000000000000000000000000000000000000000000"
        ).unwrap_or_else(|e| {
            log::error!("Failed to create dummy signature (should never happen): {}", e);
            // Fallback: create from raw bytes - 64 zero bytes is always valid
            Signature::from_slice(&[0u8; 64]).expect("64 zero bytes is valid signature format")
        });

        // Construct Event directly with correct author pubkey - no signing needed
        NostrEvent::new(
            id,
            self.author_pubkey,
            self.created_at,
            self.kind,
            tags,
            &self.content,
            dummy_sig,
        )
    }
}

/// Global store for pending comments
/// Key: target_event_id (hex string) -> Vec<PendingComment>
pub static PENDING_COMMENTS: GlobalSignal<HashMap<String, Vec<PendingComment>>> =
    Signal::global(|| HashMap::new());

/// Add a pending comment to the store
///
/// Includes deduplication: if a pending comment with the same content already exists
/// for this target and is still in Pending status, the new comment is skipped.
/// This prevents duplicate comments from rapid double-clicks.
pub fn add_pending_comment(comment: PendingComment) {
    let target_id = comment.target_event_id.to_hex();
    log::debug!("Adding pending comment {} for target {}", comment.local_id, target_id);

    let mut store = PENDING_COMMENTS.write();
    let comments = store.entry(target_id).or_insert_with(Vec::new);

    // Prevent duplicates: check if we already have a pending comment with same content
    let already_exists = comments.iter().any(|c|
        c.content == comment.content &&
        matches!(c.status, CommentStatus::Pending)
    );

    if already_exists {
        log::warn!("Duplicate pending comment detected, skipping: {}", comment.local_id);
        return;
    }

    comments.push(comment);
}

/// Update status of a pending comment
pub fn update_pending_status(local_id: &str, status: CommentStatus) {
    log::debug!("Updating pending comment {} status to {:?}", local_id, status);
    let mut store = PENDING_COMMENTS.write();
    for comments in store.values_mut() {
        if let Some(comment) = comments.iter_mut().find(|c| c.local_id == local_id) {
            comment.status = status;
            break;
        }
    }
}

/// Remove a pending comment (after confirmation or user dismissal)
pub fn remove_pending_comment(local_id: &str) {
    log::debug!("Removing pending comment {}", local_id);
    let mut store = PENDING_COMMENTS.write();
    for comments in store.values_mut() {
        comments.retain(|c| c.local_id != local_id);
    }
}

/// Get pending comments for a specific target event
pub fn get_pending_comments(target_event_id: &EventId) -> Vec<PendingComment> {
    PENDING_COMMENTS
        .read()
        .get(&target_event_id.to_hex())
        .cloned()
        .unwrap_or_default()
}

/// Retry publishing a failed pending comment
///
/// This function finds the pending comment by local_id, resets its status to Pending,
/// and attempts to republish it using the original content and target event data.
pub fn retry_pending_comment(local_id: &str) {
    // Find and clone the pending comment data we need for retry
    let comment_data = {
        let store = PENDING_COMMENTS.read();
        let mut found = None;
        for comments in store.values() {
            if let Some(comment) = comments.iter().find(|c| c.local_id == local_id) {
                found = Some((
                    comment.local_id.clone(),
                    comment.content.clone(),
                    comment.kind,
                    comment.target_event.clone(),
                    comment.parent_comment.clone(),
                    comment.target_event_id,
                ));
                break;
            }
        }
        found
    };

    let Some((local_id, content, kind, target_event, parent_comment, target_event_id)) = comment_data else {
        log::warn!("Retry failed: pending comment {} not found", local_id);
        return;
    };

    // Reset status to Pending
    update_pending_status(&local_id, CommentStatus::Pending);

    log::info!("Retrying pending comment {}", local_id);

    // Spawn async task to republish
    spawn_local(async move {
        if kind == Kind::Comment {
            // NIP-22 Comment
            let client = match get_client() {
                Some(c) => c,
                None => {
                    log::error!("Client not initialized for retry");
                    update_pending_status(&local_id, CommentStatus::Failed("Client not initialized".to_string()));
                    return;
                }
            };

            // Determine comment_to and root based on whether this is a reply to another comment
            let (comment_to, root) = if let Some(ref parent) = parent_comment {
                // Replying to a comment: comment_to = parent comment, root = original event
                (parent, Some(&target_event))
            } else {
                // Top-level comment: comment_to = original event, root = None
                (&target_event, None)
            };

            // Build comment using CommentTarget API
            let comment_target = CommentTarget::event(
                comment_to.id,
                comment_to.kind,
                None,  // relay hint
                None   // marker
            );
            let root_target = root.map(|r| CommentTarget::event(
                r.id,
                r.kind,
                None,
                None
            ));
            let builder = EventBuilder::comment(&content, comment_target, root_target);

            match client.send_event_builder(builder).await {
                Ok(send_output) => {
                    log::info!("NIP-22 comment retry successful: {}", send_output.id().to_hex());

                    // Invalidate thread tree cache (matching NIP-10 behavior)
                    invalidate_thread_tree_cache(&target_event_id);

                    update_pending_status(&local_id, CommentStatus::Confirmed(*send_output.id()));
                }
                Err(e) => {
                    log::error!("Failed to retry comment: {}", e);
                    update_pending_status(&local_id, CommentStatus::Failed(format!("{}", e)));
                }
            }
        } else {
            // NIP-10 Reply (Kind::TextNote)
            // Build tags for reply following NIP-10
            let mut tags = Vec::new();

            // Get parent's author pubkey
            let author_pk = target_event.pubkey.to_hex();
            let event_id = target_event.id.to_hex();

            // Check if the event we're replying to has a root marker
            let parent_root = target_event.tags.iter().find_map(|tag| {
                let tag_vec = tag.clone().to_vec();
                if tag_vec.len() >= 4
                    && tag_vec[0] == "e"
                    && tag_vec[3] == "root" {
                    Some(tag_vec[1].clone())
                } else {
                    None
                }
            });

            // Determine thread root for cache invalidation
            let thread_root_id = parent_root.clone().unwrap_or_else(|| event_id.clone());

            if let Some(root_id) = parent_root {
                // This is a nested reply (replying to a reply)
                tags.push(vec!["e".to_string(), root_id, "".to_string(), "root".to_string()]);
                tags.push(vec!["e".to_string(), event_id.clone(), "".to_string(), "reply".to_string()]);
            } else {
                // This is a direct reply to root
                tags.push(vec!["e".to_string(), event_id.clone(), "".to_string(), "root".to_string()]);
            }

            // Add p tags: parent author + all p tags from parent event
            tags.push(vec!["p".to_string(), author_pk.clone()]);
            for tag in target_event.tags.iter() {
                let tag_vec = tag.clone().to_vec();
                if tag_vec.len() >= 2 && tag_vec[0] == "p" {
                    let pubkey = tag_vec[1].clone();
                    if pubkey != author_pk {
                        tags.push(vec!["p".to_string(), pubkey]);
                    }
                }
            }

            match publish_note(content, tags).await {
                Ok(published_event_id) => {
                    log::info!("Reply retry successful: {}", published_event_id);

                    // Invalidate thread tree cache
                    if let Ok(root_event_id) = EventId::from_hex(&thread_root_id) {
                        invalidate_thread_tree_cache(&root_event_id);
                    }

                    // Update pending comment status
                    match EventId::from_hex(&published_event_id) {
                        Ok(event_id_parsed) => {
                            update_pending_status(&local_id, CommentStatus::Confirmed(event_id_parsed));
                        }
                        Err(e) => {
                            log::error!("Failed to parse event ID '{}': {}", published_event_id, e);
                            update_pending_status(&local_id, CommentStatus::Failed("Event ID parse error".to_string()));
                        }
                    }
                }
                Err(e) => {
                    log::error!("Failed to retry reply: {}", e);
                    update_pending_status(&local_id, CommentStatus::Failed(format!("{}", e)));
                }
            }
        }
    });
}
