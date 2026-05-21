//! use_reaction hook - Centralized reaction handling with optimistic updates
//!
//! This hook encapsulates all reaction (like/unlike) logic including:
//! - Fetching current reaction state
//! - Optimistic UI updates
//! - Rollback on failure
//! - NIP-25 compliant toggle (+ to like, - to unlike)
//! - NIP-30 custom emoji reactions
use crate::services::aggregation::{invalidate_interaction_counts, InteractionCounts};
use crate::stores::nostr_client::{get_client, publish_reaction_tracked, HAS_SIGNER};
use crate::stores::signer::SIGNER_INFO;
use dioxus::prelude::*;
use nostr_sdk::{Filter, Kind};
use std::time::Duration;
/// Maximum reactions to fetch per event
const MAX_REACTIONS_FETCH: usize = 500;
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
    let precomputed_count = precomputed_counts.map(|c| c.likes);
    let precomputed_is_liked = precomputed_counts.and_then(|c| c.user_liked);
    let precomputed_user_reaction = precomputed_counts.and_then(|c| {
        c.user_reaction.as_ref().map(|r| {
            if r == "+" {
                ReactionEmoji::Like
            } else if r == "-" {
                ReactionEmoji::Unlike
            } else if r.starts_with(':') && r.ends_with(':') && r.len() > 2 {
                let shortcode = r[1..r.len() - 1].to_string();
                if let Some(url) = c.user_reaction_url.as_ref() {
                    ReactionEmoji::Custom {
                        shortcode,
                        url: url.clone(),
                    }
                } else {
                    ReactionEmoji::Standard(r.clone())
                }
            } else {
                ReactionEmoji::Standard(r.clone())
            }
        })
    });
    let mut is_liked = use_signal(|| precomputed_is_liked.unwrap_or(false));
    let mut like_count = use_signal(|| precomputed_count.unwrap_or(0));
    let mut state = use_signal(|| ReactionState::Idle);
    let mut user_reaction: Signal<Option<ReactionEmoji>> =
        use_signal(|| precomputed_user_reaction.clone());
    use_effect(use_reactive(
        &(
            precomputed_count,
            precomputed_is_liked,
            precomputed_user_reaction.clone(),
        ),
        move |(count_opt, liked_opt, reaction_opt)| {
            if let Some(count) = count_opt {
                let current = *like_count.peek();
                if count > current || (count > 0 && current == 0) {
                    like_count.set(count);
                }
            }
            if let Some(liked) = liked_opt {
                is_liked.set(liked);
            }
            if let Some(reaction) = reaction_opt {
                user_reaction.set(Some(reaction.clone()));
            }
        },
    ));
    let event_id_fetch = event_id.clone();
    let has_batch_data = precomputed_counts.is_some();
    let mut has_precomputed_data = use_signal(|| has_batch_data);
    use_effect(use_reactive(&has_batch_data, move |has_data| {
        if has_data {
            has_precomputed_data.set(true);
        }
    }));
    use_effect(use_reactive(&event_id_fetch, move |event_id_for_fetch| {
        if *has_precomputed_data.peek() {
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
            let filter = Filter::new()
                .kind(Kind::Reaction)
                .event(event_id_parsed)
                .limit(MAX_REACTIONS_FETCH);
            if let Ok(reactions) = client.fetch_events(filter, Duration::from_secs(5)).await {
                let current_user_pk: Option<nostr_sdk::PublicKey> = SIGNER_INFO
                    .peek()
                    .as_ref()
                    .and_then(|info| nostr_sdk::PublicKey::from_hex(&info.public_key).ok());
                let mut positive_count = 0usize;
                let mut user_liked = false;
                let mut user_unliked = false;
                let mut user_emoji: Option<ReactionEmoji> = None;
                let mut reactions_vec: Vec<_> = reactions.iter().collect();
                reactions_vec.sort_by_key(|r| r.created_at);
                for reaction in reactions_vec.iter() {
                    let content = reaction.content.trim();
                    let is_from_user = current_user_pk
                        .map(|pk| reaction.pubkey == pk)
                        .unwrap_or(false);
                    if content == "-" {
                        if is_from_user {
                            user_unliked = true;
                            user_emoji = None;
                        }
                    } else {
                        positive_count += 1;
                        if is_from_user {
                            user_liked = true;
                            user_unliked = false;
                            if content == "+" {
                                user_emoji = Some(ReactionEmoji::Like);
                            } else if content.starts_with(':')
                                && content.ends_with(':')
                                && content.len() > 2
                            {
                                let shortcode = &content[1..content.len() - 1];
                                let emoji_url = reaction.tags.iter().find_map(|tag| {
                                    let tag_slice = tag.as_slice();
                                    if tag_slice.len() >= 3
                                        && tag_slice.first().map(|s| s.as_str()) == Some("emoji")
                                        && tag_slice.get(1).map(|s| s.as_str()) == Some(shortcode)
                                    {
                                        tag_slice.get(2).map(|s| s.to_string())
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
                                    user_emoji = Some(ReactionEmoji::Standard(content.to_string()));
                                }
                            } else {
                                user_emoji = Some(ReactionEmoji::Standard(content.to_string()));
                            }
                        }
                    }
                }
                let final_liked = user_liked && !user_unliked;
                let current_count = *like_count.peek();
                if positive_count > current_count {
                    like_count.set(positive_count);
                }
                if !*has_precomputed_data.peek() {
                    is_liked.set(final_liked);
                    user_reaction.set(if final_liked { user_emoji } else { None });
                }
            }
        });
    }));
    let event_id_handler = event_id.clone();
    let event_author_handler = event_author.clone();
    let toggle_like = use_callback(move |_: ()| {
        if !*HAS_SIGNER.read() {
            state.set(ReactionState::Error("No signer available".to_string()));
            return;
        }
        if matches!(*state.peek(), ReactionState::Pending) {
            return;
        }
        let was_liked = *is_liked.peek();
        let prev_count = *like_count.peek();
        let content = if was_liked { "-" } else { "+" };
        let prev_reaction = user_reaction.peek().clone();
        state.set(ReactionState::Pending);
        is_liked.set(!was_liked);
        if was_liked {
            like_count.set(prev_count.saturating_sub(1));
            user_reaction.set(None);
        } else {
            like_count.set(prev_count.saturating_add(1));
            user_reaction.set(Some(ReactionEmoji::Like));
        }
        let event_id_clone = event_id_handler.clone();
        let event_author_clone = event_author_handler.clone();
        let content_str = content.to_string();
        spawn(async move {
            match publish_reaction_tracked(
                event_id_clone.clone(),
                event_author_clone,
                content_str,
                None,
            )
            .await
            {
                Ok(result) => {
                    log::info!(
                        "{} event {}, reaction ID: {}",
                        if was_liked { "Unliked" } else { "Liked" },
                        event_id_clone,
                        result.event_id,
                    );
                    state.set(ReactionState::Success);
                    invalidate_interaction_counts(&event_id_clone);
                    crate::platform::timer::sleep_ms(500).await;
                    state.set(ReactionState::Idle);
                }
                Err(e) => {
                    log::error!(
                        "Failed to {} event: {}",
                        if was_liked { "unlike" } else { "like" },
                        e
                    );
                    is_liked.set(was_liked);
                    like_count.set(prev_count);
                    user_reaction.set(prev_reaction);
                    state.set(ReactionState::Error(format!(
                        "Failed to {}: {}",
                        if was_liked { "unlike" } else { "like" },
                        e,
                    )));
                }
            }
        });
    });
    let event_id_react = event_id.clone();
    let event_author_react = event_author.clone();
    let react_with = use_callback(move |emoji: ReactionEmoji| {
        if !*HAS_SIGNER.read() {
            state.set(ReactionState::Error("No signer available".to_string()));
            return;
        }
        if matches!(*state.peek(), ReactionState::Pending) {
            return;
        }
        let prev_liked = *is_liked.peek();
        let prev_count = *like_count.peek();
        let prev_reaction = user_reaction.peek().clone();
        let content = emoji.content();
        let emoji_tag = emoji.emoji_tag();
        let is_positive = !matches!(emoji, ReactionEmoji::Unlike);
        if !is_positive && !prev_liked {
            return;
        }
        state.set(ReactionState::Pending);
        if is_positive && !prev_liked {
            is_liked.set(true);
            like_count.set(prev_count.saturating_add(1));
            user_reaction.set(Some(emoji.clone()));
        } else if is_positive && prev_liked {
            user_reaction.set(Some(emoji.clone()));
        } else if !is_positive && prev_liked {
            is_liked.set(false);
            like_count.set(prev_count.saturating_sub(1));
            user_reaction.set(None);
        }
        let event_id_clone = event_id_react.clone();
        let event_author_clone = event_author_react.clone();
        spawn(async move {
            match publish_reaction_tracked(
                event_id_clone.clone(),
                event_author_clone,
                content.clone(),
                emoji_tag,
            )
            .await
            {
                    Ok(result) => {
                        log::info!(
                            "Reacted to event {} with '{}', reaction ID: {}",
                            event_id_clone,
                            content,
                            result.event_id,
                        );
                        state.set(ReactionState::Success);
                        invalidate_interaction_counts(&event_id_clone);
                        crate::platform::timer::sleep_ms(500).await;
                        state.set(ReactionState::Idle);
                    }
                Err(e) => {
                    log::error!("Failed to react with '{}': {}", content, e);
                    is_liked.set(prev_liked);
                    like_count.set(prev_count);
                    user_reaction.set(prev_reaction);
                    state.set(ReactionState::Error(format!("Failed to react: {}", e)));
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
    if count > MAX_REACTIONS_FETCH {
        format!("{}+", MAX_REACTIONS_FETCH)
    } else if count > 0 {
        count.to_string()
    } else {
        String::new()
    }
}
