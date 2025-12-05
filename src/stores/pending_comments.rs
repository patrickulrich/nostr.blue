//! Pending Comments Store - Optimistic updates for comments
//!
//! This store manages comments that are being published or have failed.
//! It enables optimistic UI updates so users see their comments immediately.

use dioxus::prelude::*;
use nostr_sdk::{Event as NostrEvent, EventId, Kind, PublicKey, Timestamp};
use std::collections::HashMap;

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
    pub fn to_display_event(&self) -> NostrEvent {
        // Create a minimal event structure for display
        // We use a temporary keypair to sign, since we need a valid Event struct
        // The signature doesn't matter since this is just for local display
        let temp_keys = nostr_sdk::Keys::generate();
        let unsigned = nostr_sdk::EventBuilder::new(self.kind, &self.content)
            .custom_created_at(self.created_at)
            .build(self.author_pubkey);

        // Sign with temp keys to get a valid Event structure
        // Replace the pubkey with the actual author's pubkey for display
        unsigned.sign_with_keys(&temp_keys).unwrap_or_else(|_| {
            // Fallback: create a minimal event if signing fails
            nostr_sdk::EventBuilder::text_note(&self.content)
                .custom_created_at(self.created_at)
                .sign_with_keys(&temp_keys)
                .expect("Failed to create fallback event")
        })
    }
}

/// Global store for pending comments
/// Key: target_event_id (hex string) -> Vec<PendingComment>
pub static PENDING_COMMENTS: GlobalSignal<HashMap<String, Vec<PendingComment>>> =
    Signal::global(|| HashMap::new());

/// Add a pending comment to the store
pub fn add_pending_comment(comment: PendingComment) {
    let target_id = comment.target_event_id.to_hex();
    log::debug!("Adding pending comment {} for target {}", comment.local_id, target_id);
    PENDING_COMMENTS
        .write()
        .entry(target_id)
        .or_insert_with(Vec::new)
        .push(comment);
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
