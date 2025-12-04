//! use_reaction hook - Centralized reaction handling with optimistic updates
//!
//! This hook encapsulates all reaction (like/unlike) logic including:
//! - Fetching current reaction state
//! - Optimistic UI updates
//! - Rollback on failure
//! - NIP-25 compliant toggle (+ to like, - to unlike)
//! - NIP-30 custom emoji reactions

use dioxus::prelude::*;
use nostr_sdk::{Filter, Kind};
use std::time::Duration;

use crate::stores::nostr_client::{get_client, publish_reaction, HAS_SIGNER};
use crate::stores::signer::SIGNER_INFO;
use crate::services::aggregation::{invalidate_interaction_counts, InteractionCounts};

/// State of the reaction action
#[derive(Clone, Debug, PartialEq)]
pub enum ReactionState {
    /// No action in progress
    Idle,
    /// Publishing reaction
    Pending,
    /// Action completed successfully
    Success,
    /// Action failed with error message
    Error(String),
}

/// Represents an emoji for reactions (standard or custom per NIP-30)
#[derive(Clone, Debug, PartialEq)]
pub enum ReactionEmoji {
    /// Standard unicode emoji (e.g., "❤️", "👍")
    Standard(String),
    /// Custom emoji with shortcode and URL (NIP-30)
    Custom { shortcode: String, url: String },
    /// Simple like (+)
    Like,
    /// Unlike (-)
    Unlike,
}

impl ReactionEmoji {
    /// Get the content string for the reaction event
    pub fn content(&self) -> String {
        match self {
            Self::Standard(emoji) => emoji.clone(),
            Self::Custom { shortcode, .. } => format!(":{}:", shortcode),
            Self::Like => "+".to_string(),
            Self::Unlike => "-".to_string(),
        }
    }

    /// Get emoji tag data if this is a custom emoji (shortcode, url)
    pub fn emoji_tag(&self) -> Option<(String, String)> {
        match self {
            Self::Custom { shortcode, url } => Some((shortcode.clone(), url.clone())),
            _ => None,
        }
    }
}

/// Return type for the use_reaction hook
#[derive(Clone)]
pub struct UseReaction {
    /// Whether the current user has liked this event
    pub is_liked: Signal<bool>,
    /// Total positive reaction count
    pub like_count: Signal<usize>,
    /// Current state of the reaction action
    pub state: Signal<ReactionState>,
    /// The user's current reaction emoji (if any)
    pub user_reaction: Signal<Option<ReactionEmoji>>,
    /// Function to toggle like state (like if not liked, unlike if liked)
    pub toggle_like: EventHandler<()>,
    /// Function to react with a specific emoji (standard, custom, or like/unlike)
    pub react_with: EventHandler<ReactionEmoji>,
}

impl PartialEq for UseReaction {
    fn eq(&self, other: &Self) -> bool {
        // Compare signals by their current values for memoization
        // EventHandlers are not compared (they're always considered equal for this purpose)
        *self.is_liked.read() == *other.is_liked.read()
            && *self.like_count.read() == *other.like_count.read()
            && *self.state.read() == *other.state.read()
            && *self.user_reaction.read() == *other.user_reaction.read()
    }
}

/// Hook for managing reaction state on an event
///
/// # Arguments
/// * `event_id` - The hex ID of the event to react to
/// * `event_author` - The hex pubkey of the event author
/// * `precomputed_counts` - Optional precomputed InteractionCounts (from batch fetches)
///
/// # Returns
/// A `UseReaction` struct with signals and handlers for reaction state
///
/// # Example
/// ```rust
/// let reaction = use_reaction(
///     event.id.to_hex(),
///     event.pubkey.to_string(),
///     precomputed_counts.as_ref(),
/// );
///
/// button {
///     disabled: matches!(*reaction.state.read(), ReactionState::Pending),
///     onclick: move |_| reaction.toggle_like.call(()),
///     HeartIcon { filled: *reaction.is_liked.read() }
/// }
/// ```
pub fn use_reaction(
    event_id: String,
    event_author: String,
    precomputed_counts: Option<&InteractionCounts>,
) -> UseReaction {
    // Extract precomputed values from InteractionCounts
    let precomputed_count = precomputed_counts.map(|c| c.likes);
    let precomputed_is_liked = precomputed_counts.and_then(|c| c.user_liked);
    let precomputed_user_reaction = precomputed_counts.and_then(|c| {
        c.user_reaction.as_ref().map(|r| {
            // Convert string to ReactionEmoji
            if r == "+" {
                ReactionEmoji::Like
            } else if r == "-" {
                ReactionEmoji::Unlike
            } else {
                ReactionEmoji::Standard(r.clone())
            }
        })
    });

    // Signals for reaction state
    let mut is_liked = use_signal(|| precomputed_is_liked.unwrap_or(false));
    let mut like_count = use_signal(|| precomputed_count.unwrap_or(0));
    let mut state = use_signal(|| ReactionState::Idle);
    let mut user_reaction: Signal<Option<ReactionEmoji>> = use_signal(|| precomputed_user_reaction);

    // Clone for effect
    let event_id_fetch = event_id.clone();

    // Fetch initial state if not precomputed
    // Only fetch if we don't have precomputed data with actual is_liked state
    let should_fetch = precomputed_is_liked.is_none();

    // Use use_reactive to properly track event_id dependency and re-run when it changes
    use_effect(use_reactive(&event_id_fetch, move |event_id_for_fetch| {
        if !should_fetch {
            return;
        }

        spawn(async move {
            let client = match get_client() {
                Some(c) => c,
                None => return,
            };

            let event_id_parsed = match nostr_sdk::EventId::from_hex(&event_id_for_fetch) {
                Ok(id) => id,
                Err(_) => return,
            };

            // Fetch reactions for this event
            let filter = Filter::new()
                .kind(Kind::Reaction)
                .event(event_id_parsed)
                .limit(500);

            if let Ok(reactions) = client.fetch_events(filter, Duration::from_secs(5)).await {
                // Parse current user's pubkey once for efficient comparison
                let current_user_pk: Option<nostr_sdk::PublicKey> = SIGNER_INFO
                    .read()
                    .as_ref()
                    .and_then(|info| nostr_sdk::PublicKey::from_hex(&info.public_key).ok());

                let mut positive_count = 0usize;
                let mut user_liked = false;
                let mut user_unliked = false;
                let mut user_emoji: Option<ReactionEmoji> = None;

                // Sort reactions by created_at (ascending) to process chronologically
                // This ensures the final state reflects the user's most recent action
                let mut reactions_vec: Vec<_> = reactions.iter().collect();
                reactions_vec.sort_by_key(|r| r.created_at);

                for reaction in reactions_vec.iter() {
                    let content = reaction.content.trim();
                    let is_from_user = current_user_pk
                        .map(|pk| reaction.pubkey == pk)
                        .unwrap_or(false);

                    if content == "-" {
                        // Negative reaction (unlike/downvote)
                        if is_from_user {
                            user_unliked = true;
                            user_emoji = None; // Clear any previous reaction
                        }
                        // Per NIP-25, "-" reactions reduce count
                        // We'll handle this by not counting them as positive
                    } else {
                        // Positive reaction (+, emoji, etc.)
                        positive_count += 1;
                        if is_from_user {
                            user_liked = true;
                            // Parse the user's reaction emoji
                            if content == "+" {
                                user_emoji = Some(ReactionEmoji::Like);
                            } else if content.starts_with(':') && content.ends_with(':') {
                                // Custom emoji - look for emoji tag
                                let shortcode = &content[1..content.len()-1];
                                // Find emoji tag with matching shortcode
                                let emoji_url = reaction.tags.iter().find_map(|tag| {
                                    let tag_vec = tag.clone().to_vec();
                                    if tag_vec.len() >= 3
                                        && tag_vec.first().map(|s| s.as_str()) == Some("emoji")
                                        && tag_vec.get(1).map(|s| s.as_str()) == Some(shortcode)
                                    {
                                        tag_vec.get(2).map(|s| s.to_string())
                                    } else {
                                        None
                                    }
                                });
                                if let Some(url) = emoji_url {
                                    user_emoji = Some(ReactionEmoji::Custom {
                                        shortcode: shortcode.to_string(),
                                        url,
                                    });
                                } else {
                                    // No emoji tag found, treat as standard emoji
                                    user_emoji = Some(ReactionEmoji::Standard(content.to_string()));
                                }
                            } else {
                                // Standard unicode emoji
                                user_emoji = Some(ReactionEmoji::Standard(content.to_string()));
                            }
                        }
                    }
                }

                // User's final state: since we process chronologically, user_liked/user_unliked
                // reflect the most recent action. Final state is liked only if no subsequent unlike.
                let final_liked = user_liked && !user_unliked;

                like_count.set(positive_count);
                is_liked.set(final_liked);
                user_reaction.set(if final_liked { user_emoji } else { None });
            }
        });
    }));

    // Clone for handler
    let event_id_handler = event_id.clone();
    let event_author_handler = event_author.clone();

    // Toggle like handler with optimistic updates
    let toggle_like = use_callback(move |_: ()| {
        // Check preconditions
        if !*HAS_SIGNER.read() {
            state.set(ReactionState::Error("No signer available".to_string()));
            return;
        }

        if matches!(*state.peek(), ReactionState::Pending) {
            return; // Already processing
        }

        // Capture current state for potential rollback
        let was_liked = *is_liked.peek();
        let prev_count = *like_count.peek();

        // Determine action: like (+) or unlike (-)
        let content = if was_liked { "-" } else { "+" };

        // Save previous user reaction for rollback
        let prev_reaction = user_reaction.peek().clone();

        // Optimistic update
        state.set(ReactionState::Pending);
        is_liked.set(!was_liked);
        if was_liked {
            // Unliking - decrement count and clear reaction
            like_count.set(prev_count.saturating_sub(1));
            user_reaction.set(None);
        } else {
            // Liking - increment count and set to Like
            like_count.set(prev_count.saturating_add(1));
            user_reaction.set(Some(ReactionEmoji::Like));
        }

        let event_id_clone = event_id_handler.clone();
        let event_author_clone = event_author_handler.clone();
        let content_str = content.to_string();

        spawn(async move {
            match publish_reaction(event_id_clone.clone(), event_author_clone, content_str, None).await {
                Ok(reaction_id) => {
                    log::info!(
                        "{} event {}, reaction ID: {}",
                        if was_liked { "Unliked" } else { "Liked" },
                        event_id_clone,
                        reaction_id
                    );
                    state.set(ReactionState::Success);

                    // Invalidate cache so next fetch gets fresh data
                    invalidate_interaction_counts(&event_id_clone);

                    // Reset to Idle after a short delay
                    #[cfg(not(target_arch = "wasm32"))]
                    {
                        tokio::time::sleep(Duration::from_millis(500)).await;
                    }
                    #[cfg(target_arch = "wasm32")]
                    {
                        gloo_timers::future::TimeoutFuture::new(500).await;
                    }
                    state.set(ReactionState::Idle);
                }
                Err(e) => {
                    log::error!("Failed to {} event: {}", if was_liked { "unlike" } else { "like" }, e);

                    // Rollback optimistic update
                    is_liked.set(was_liked);
                    like_count.set(prev_count);
                    user_reaction.set(prev_reaction);

                    state.set(ReactionState::Error(format!(
                        "Failed to {}: {}",
                        if was_liked { "unlike" } else { "like" },
                        e
                    )));
                }
            }
        });
    });

    // Clone for react_with handler
    let event_id_react = event_id.clone();
    let event_author_react = event_author.clone();

    // React with specific emoji handler (supports standard, custom, like/unlike)
    let react_with = use_callback(move |emoji: ReactionEmoji| {
        // Check preconditions
        if !*HAS_SIGNER.read() {
            state.set(ReactionState::Error("No signer available".to_string()));
            return;
        }

        if matches!(*state.peek(), ReactionState::Pending) {
            return; // Already processing
        }

        // Capture current state for potential rollback
        let prev_liked = *is_liked.peek();
        let prev_count = *like_count.peek();
        let prev_reaction = user_reaction.peek().clone();

        // Get content and emoji tag from the ReactionEmoji
        let content = emoji.content();
        let emoji_tag = emoji.emoji_tag();

        // Determine if this is a positive reaction (not unlike)
        let is_positive = !matches!(emoji, ReactionEmoji::Unlike);

        // Skip publishing unlike when user hasn't liked - no action needed
        if !is_positive && !prev_liked {
            return;
        }

        // Optimistic update
        state.set(ReactionState::Pending);
        if is_positive && !prev_liked {
            // Adding a positive reaction when not already liked
            is_liked.set(true);
            like_count.set(prev_count.saturating_add(1));
            user_reaction.set(Some(emoji.clone()));
        } else if is_positive && prev_liked {
            // Changing reaction - just update the emoji, count stays same
            user_reaction.set(Some(emoji.clone()));
        } else if !is_positive && prev_liked {
            // Removing reaction (unlike)
            is_liked.set(false);
            like_count.set(prev_count.saturating_sub(1));
            user_reaction.set(None);
        }

        let event_id_clone = event_id_react.clone();
        let event_author_clone = event_author_react.clone();

        spawn(async move {
            match publish_reaction(event_id_clone.clone(), event_author_clone, content.clone(), emoji_tag).await {
                Ok(reaction_id) => {
                    log::info!(
                        "Reacted to event {} with '{}', reaction ID: {}",
                        event_id_clone,
                        content,
                        reaction_id
                    );
                    state.set(ReactionState::Success);

                    // Invalidate cache so next fetch gets fresh data
                    invalidate_interaction_counts(&event_id_clone);

                    // Reset to Idle after a short delay
                    #[cfg(not(target_arch = "wasm32"))]
                    {
                        tokio::time::sleep(Duration::from_millis(500)).await;
                    }
                    #[cfg(target_arch = "wasm32")]
                    {
                        gloo_timers::future::TimeoutFuture::new(500).await;
                    }
                    state.set(ReactionState::Idle);
                }
                Err(e) => {
                    log::error!("Failed to react with '{}': {}", content, e);

                    // Rollback optimistic update
                    is_liked.set(prev_liked);
                    like_count.set(prev_count);
                    user_reaction.set(prev_reaction);

                    state.set(ReactionState::Error(format!(
                        "Failed to react: {}",
                        e
                    )));
                }
            }
        });
    });

    UseReaction {
        is_liked,
        like_count,
        state,
        user_reaction,
        toggle_like,
        react_with,
    }
}

/// Format a count for display (e.g., "500+" for large numbers)
pub fn format_count(count: usize) -> String {
    if count > 500 {
        "500+".to_string()
    } else if count > 0 {
        count.to_string()
    } else {
        String::new()
    }
}
