/// NIP-78: Preferred Reactions Storage
/// Stores user's preferred reaction emojis on Nostr relays using kind 30078 events
use dioxus::prelude::*;
use nostr_sdk::{EventBuilder, Filter, Kind, Tag, FromBech32};
use serde::{Deserialize, Serialize};
use std::time::Duration;

use crate::stores::{auth_store, nostr_client};
use crate::hooks::ReactionEmoji;

/// A user's preferred reaction - either a standard unicode emoji or a custom NIP-30 emoji
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum PreferredReaction {
    #[serde(rename = "standard")]
    Standard { emoji: String },
    #[serde(rename = "custom")]
    Custom { shortcode: String, url: String },
}

impl PreferredReaction {
    /// Convert to ReactionEmoji for use with the reaction hook
    pub fn to_reaction_emoji(&self) -> ReactionEmoji {
        match self {
            Self::Standard { emoji } => ReactionEmoji::Standard(emoji.clone()),
            Self::Custom { shortcode, url } => ReactionEmoji::Custom {
                shortcode: shortcode.clone(),
                url: url.clone(),
            },
        }
    }

    /// Render the emoji content (for display)
    pub fn display(&self) -> String {
        match self {
            Self::Standard { emoji } => emoji.clone(),
            Self::Custom { shortcode, .. } => format!(":{}:", shortcode),
        }
    }
}

/// NIP-78 data structure for storing reactions
#[derive(Clone, Debug, Serialize, Deserialize)]
struct ReactionsData {
    reactions: Vec<PreferredReaction>,
    #[serde(default)]
    version: u32,
}

impl Default for ReactionsData {
    fn default() -> Self {
        Self {
            reactions: default_reactions(),
            version: 1,
        }
    }
}

/// NIP-78 kind for arbitrary custom app data
const APP_DATA_KIND: u16 = 30078;

/// D tag identifier for reaction preferences
const REACTIONS_D_TAG: &str = "nostr.blue/reactions";

/// Maximum number of preferred reactions
pub const MAX_REACTIONS: usize = 10;

/// Default emoji reactions (used when user has no custom preferences)
const DEFAULT_EMOJIS: &[&str] = &["❤️", "👍", "😂", "🔥", "😮", "😢", "🎉", "🤔", "👀", "🙏"];

/// Create the default reactions list
pub fn default_reactions() -> Vec<PreferredReaction> {
    DEFAULT_EMOJIS
        .iter()
        .map(|e| PreferredReaction::Standard { emoji: e.to_string() })
        .collect()
}

/// Global state for preferred reactions
pub static PREFERRED_REACTIONS: GlobalSignal<Vec<PreferredReaction>> = Signal::global(default_reactions);
pub static REACTIONS_LOADED: GlobalSignal<bool> = Signal::global(|| false);
pub static REACTIONS_LOADING: GlobalSignal<bool> = Signal::global(|| false);

/// Get the user's default reaction (first in the list)
pub fn get_default_reaction() -> Option<PreferredReaction> {
    PREFERRED_REACTIONS.read().first().cloned()
}

/// Load preferred reactions from Nostr relays (NIP-78)
pub async fn load_preferred_reactions() {
    // Atomically check and set loading flag to avoid duplicate loads
    {
        let mut loading = REACTIONS_LOADING.write();
        if *loading {
            return;
        }
        *loading = true;
    }

    log::info!("Loading preferred reactions from Nostr (NIP-78)...");

    // Check if authenticated
    if !auth_store::is_authenticated() {
        log::info!("Not authenticated, using default reactions");
        *REACTIONS_LOADING.write() = false;
        *REACTIONS_LOADED.write() = true;
        return;
    }

    // Get client and pubkey
    let client = match nostr_client::NOSTR_CLIENT.read().as_ref() {
        Some(c) => c.clone(),
        None => {
            log::warn!("Client not initialized");
            *REACTIONS_LOADING.write() = false;
            *REACTIONS_LOADED.write() = true; // Mark as loaded (with defaults) to prevent retry loops
            return;
        }
    };

    let auth = auth_store::AUTH_STATE.read();
    let pubkey = match auth.pubkey.as_ref() {
        Some(pk_str) => {
            match nostr_sdk::PublicKey::from_bech32(pk_str)
                .or_else(|_| nostr_sdk::PublicKey::from_hex(pk_str))
            {
                Ok(pk) => pk,
                Err(e) => {
                    log::error!("Invalid pubkey: {}", e);
                    *REACTIONS_LOADING.write() = false;
                    *REACTIONS_LOADED.write() = true; // Mark as loaded (with defaults) to prevent retry loops
                    return;
                }
            }
        }
        None => {
            log::warn!("No pubkey available");
            *REACTIONS_LOADING.write() = false;
            *REACTIONS_LOADED.write() = true; // Mark as loaded (with defaults) to prevent retry loops
            return;
        }
    };

    // Build filter for reactions event
    let filter = Filter::new()
        .author(pubkey)
        .kind(Kind::from(APP_DATA_KIND))
        .identifier(REACTIONS_D_TAG)
        .limit(1);

    // Ensure relays are ready before fetching
    nostr_client::ensure_relays_ready(&client).await;

    // Fetch reactions event
    match client.fetch_events(filter, Duration::from_secs(5)).await {
        Ok(events) => {
            if let Some(event) = events.into_iter().next() {
                log::info!("Found reactions preference event: {}", event.id);

                // Parse reactions from content
                match serde_json::from_str::<ReactionsData>(&event.content) {
                    Ok(data) => {
                        if !data.reactions.is_empty() {
                            // Truncate to MAX_REACTIONS to handle malformed/malicious events
                            let reactions: Vec<PreferredReaction> = data.reactions
                                .into_iter()
                                .take(MAX_REACTIONS)
                                .collect();
                            log::info!("Loaded {} preferred reactions from Nostr", reactions.len());
                            *PREFERRED_REACTIONS.write() = reactions;
                        }
                    }
                    Err(e) => {
                        log::warn!("Failed to parse reactions data: {}", e);
                    }
                }
            } else {
                log::info!("No reactions preferences found on Nostr, using defaults");
            }
        }
        Err(e) => {
            log::warn!("Failed to fetch reactions preferences: {}", e);
        }
    }

    *REACTIONS_LOADING.write() = false;
    *REACTIONS_LOADED.write() = true;
}

/// Save preferred reactions to Nostr relays (NIP-78)
pub async fn save_preferred_reactions(reactions: Vec<PreferredReaction>) -> Result<(), String> {
    log::info!("Saving {} preferred reactions to Nostr (NIP-78)...", reactions.len());

    // Check if authenticated
    if !auth_store::is_authenticated() {
        return Err("Not authenticated".to_string());
    }

    // Validate reaction count
    if reactions.len() > MAX_REACTIONS {
        return Err(format!("Too many reactions (max {})", MAX_REACTIONS));
    }

    // Get client
    let client = nostr_client::NOSTR_CLIENT.read()
        .as_ref()
        .ok_or("Client not initialized")?
        .clone();

    // Ensure relays are ready before publishing
    nostr_client::ensure_relays_ready(&client).await;

    // Create data structure
    let data = ReactionsData {
        reactions: reactions.clone(),
        version: 1,
    };

    // Serialize to JSON
    let content = serde_json::to_string(&data)
        .map_err(|e| format!("Failed to serialize reactions: {}", e))?;

    // Build NIP-78 event (kind 30078 with 'd' tag)
    let builder = EventBuilder::new(Kind::from(APP_DATA_KIND), content)
        .tag(Tag::identifier(REACTIONS_D_TAG));

    // Publish to relays
    client.send_event_builder(builder).await
        .map_err(|e| format!("Failed to publish reactions: {}", e))?;

    log::info!("Reactions preferences saved to Nostr successfully");

    // Update global state
    *PREFERRED_REACTIONS.write() = reactions;

    Ok(())
}
