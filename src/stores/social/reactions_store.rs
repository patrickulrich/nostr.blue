use crate::hooks::ReactionEmoji;
use crate::platform::storage;
use crate::stores::sidebar_store::Nip78LoadState;
use crate::stores::{auth_store, nostr_client};
/// NIP-78: Preferred Reactions Storage
/// Stores user's preferred reaction emojis on Nostr relays using kind 30078 events
use dioxus::prelude::*;
use nostr_sdk::{EventBuilder, Filter, FromBech32, Kind, Tag};
use serde::{Deserialize, Serialize};
use std::time::Duration;
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
/// localStorage key for caching reaction preferences
const REACTIONS_LOCAL_STORAGE_KEY: &str = "nostr_blue_reaction_prefs";
/// Maximum number of preferred reactions
pub const MAX_REACTIONS: usize = 10;
/// Default emoji reactions (used when user has no custom preferences)
const DEFAULT_EMOJIS: &[&str] = &["❤️", "👍", "😂", "🔥", "😮", "😢", "🎉", "🤔", "👀", "🙏"];
/// Create the default reactions list
pub fn default_reactions() -> Vec<PreferredReaction> {
    DEFAULT_EMOJIS
        .iter()
        .map(|e| PreferredReaction::Standard {
            emoji: e.to_string(),
        })
        .collect()
}
/// Global state for preferred reactions
pub static PREFERRED_REACTIONS: GlobalSignal<Vec<PreferredReaction>> =
    Signal::global(default_reactions);
/// NIP-78 load state for reaction preferences
pub static REACTIONS_STATE: GlobalSignal<Nip78LoadState> = Signal::global(Nip78LoadState::default);
/// Get the user's default reaction (first in the list)
pub fn get_default_reaction() -> Option<PreferredReaction> {
    PREFERRED_REACTIONS.read().first().cloned()
}
/// Load cached reaction preferences from localStorage
fn load_cached_reactions() -> Option<ReactionsData> {
    storage::get::<String>(REACTIONS_LOCAL_STORAGE_KEY)
        .ok()
        .and_then(|json| serde_json::from_str(&json).ok())
}
/// Save reaction preferences to localStorage
fn cache_reactions(data: &ReactionsData) {
    if let Ok(json) = serde_json::to_string(data) {
        let _ = storage::set(REACTIONS_LOCAL_STORAGE_KEY, &json);
    }
}
/// Initialize reactions from localStorage cache (synchronous, for instant UI)
/// Call this during app init BEFORE async client initialization
pub fn init_reactions_from_cache() {
    if let Some(cached) = load_cached_reactions() {
        if !cached.reactions.is_empty() {
            log::info!(
                "Initialized {} reactions from localStorage",
                cached.reactions.len()
            );
            let reactions: Vec<PreferredReaction> =
                cached.reactions.into_iter().take(MAX_REACTIONS).collect();
            *PREFERRED_REACTIONS.write() = reactions;
        }
    }
}
/// Load preferred reactions from Nostr relays (NIP-78)
/// Uses a 3-step loading strategy for reliability:
/// 1. Load from localStorage first for instant UI
/// 2. Query local database (nostr-sdk caches events)
/// 3. Fetch from relays to sync any updates
pub async fn load_preferred_reactions() {
    {
        let state = REACTIONS_STATE.read().clone();
        if state.is_loading() {
            return;
        }
        *REACTIONS_STATE.write() = Nip78LoadState::Loading;
    }
    log::info!("Loading preferred reactions...");
    let mut loaded_from_cache = false;
    if let Some(cached) = load_cached_reactions() {
        if !cached.reactions.is_empty() {
            log::info!(
                "Loaded {} reactions from localStorage",
                cached.reactions.len()
            );
            let reactions: Vec<PreferredReaction> =
                cached.reactions.into_iter().take(MAX_REACTIONS).collect();
            *PREFERRED_REACTIONS.write() = reactions;
            loaded_from_cache = true;
        }
    }
    if !auth_store::is_authenticated() {
        log::info!(
            "Not authenticated, using {} reactions",
            if loaded_from_cache {
                "cached"
            } else {
                "default"
            }
        );
        *REACTIONS_STATE.write() = Nip78LoadState::LoadedDefaults;
        return;
    }
    let client = match nostr_client::NOSTR_CLIENT.read().as_ref() {
        Some(c) => c.clone(),
        None => {
            log::warn!("Client not initialized");
            *REACTIONS_STATE.write() = if loaded_from_cache {
                Nip78LoadState::Loaded
            } else {
                Nip78LoadState::Failed("Client not initialized".into())
            };
            return;
        }
    };
    let pubkey_str = auth_store::AUTH_STATE.read().pubkey.clone();
    let pubkey = match pubkey_str.as_ref() {
        Some(pk_str) => {
            match nostr_sdk::PublicKey::from_bech32(pk_str)
                .or_else(|_| nostr_sdk::PublicKey::from_hex(pk_str))
            {
                Ok(pk) => pk,
                Err(e) => {
                    log::error!("Invalid pubkey: {}", e);
                    *REACTIONS_STATE.write() = if loaded_from_cache {
                        Nip78LoadState::Loaded
                    } else {
                        Nip78LoadState::LoadedDefaults
                    };
                    return;
                }
            }
        }
        None => {
            log::warn!("No pubkey available");
            *REACTIONS_STATE.write() = if loaded_from_cache {
                Nip78LoadState::Loaded
            } else {
                Nip78LoadState::LoadedDefaults
            };
            return;
        }
    };
    let filter = Filter::new()
        .author(pubkey)
        .kind(Kind::from(APP_DATA_KIND))
        .identifier(REACTIONS_D_TAG)
        .limit(1);
    if let Ok(db_events) = client.database().query(filter.clone()).await {
        if let Some(event) = db_events.into_iter().next() {
            log::info!("Found reactions preference in local database: {}", event.id);
            if let Ok(data) = serde_json::from_str::<ReactionsData>(&event.content) {
                if !data.reactions.is_empty() {
                    let reactions: Vec<PreferredReaction> = data
                        .reactions
                        .clone()
                        .into_iter()
                        .take(MAX_REACTIONS)
                        .collect();
                    *PREFERRED_REACTIONS.write() = reactions;
                    cache_reactions(&data);
                    loaded_from_cache = true;
                }
            }
        }
    }
    nostr_client::ensure_relays_ready(&client).await;
    match client.fetch_events(filter, Duration::from_secs(10)).await {
        Ok(events) => {
            if let Some(event) = events.into_iter().next() {
                log::info!("Found reactions preference event from relays: {}", event.id);
                match serde_json::from_str::<ReactionsData>(&event.content) {
                    Ok(data) => {
                        if !data.reactions.is_empty() {
                            let reactions: Vec<PreferredReaction> = data
                                .reactions
                                .clone()
                                .into_iter()
                                .take(MAX_REACTIONS)
                                .collect();
                            log::info!(
                                "Loaded {} preferred reactions from Nostr relays",
                                reactions.len()
                            );
                            *PREFERRED_REACTIONS.write() = reactions;
                            cache_reactions(&data);
                        }
                        *REACTIONS_STATE.write() = Nip78LoadState::Loaded;
                    }
                    Err(e) => {
                        log::warn!("Failed to parse reactions data: {}", e);
                        *REACTIONS_STATE.write() = if loaded_from_cache {
                            Nip78LoadState::Loaded
                        } else {
                            Nip78LoadState::LoadedDefaults
                        };
                    }
                }
            } else {
                log::info!("No reactions preferences found on relays");
                *REACTIONS_STATE.write() = if loaded_from_cache {
                    Nip78LoadState::Loaded
                } else {
                    Nip78LoadState::LoadedDefaults
                };
            }
        }
        Err(e) => {
            log::warn!("Failed to fetch reactions preferences: {}", e);
            *REACTIONS_STATE.write() = if loaded_from_cache {
                Nip78LoadState::Loaded
            } else {
                Nip78LoadState::Failed(e.to_string())
            };
        }
    }
}
/// Save preferred reactions to Nostr relays (NIP-78)
pub async fn save_preferred_reactions(reactions: Vec<PreferredReaction>) -> Result<(), String> {
    log::info!(
        "Saving {} preferred reactions to Nostr (NIP-78)...",
        reactions.len()
    );
    if !auth_store::is_authenticated() {
        return Err("Not authenticated".to_string());
    }
    if reactions.len() > MAX_REACTIONS {
        return Err(format!("Too many reactions (max {})", MAX_REACTIONS));
    }
    let client = nostr_client::NOSTR_CLIENT
        .read()
        .as_ref()
        .ok_or("Client not initialized")?
        .clone();
    nostr_client::ensure_relays_ready(&client).await;
    let data = ReactionsData {
        reactions: reactions.clone(),
        version: 1,
    };
    let content = serde_json::to_string(&data)
        .map_err(|e| format!("Failed to serialize reactions: {}", e))?;
    let builder =
        EventBuilder::new(Kind::from(APP_DATA_KIND), content).tag(Tag::identifier(REACTIONS_D_TAG));
    let output = client
        .send_event_builder(crate::utils::nips::nip89::tag_event_builder(builder))
        .await
        .map_err(|e| format!("Failed to publish reactions: {}", e))?;
    let success_count = output.success.len();
    let failed_count = output.failed.len();
    let total = success_count + failed_count;
    log::info!(
        "Reactions preferences saved: {} ({}/{} relays succeeded)",
        output.id().to_hex(),
        success_count,
        total
    );
    if !output.failed.is_empty() {
        for (relay, error) in &output.failed {
            log::warn!("Relay {} failed: {}", relay, error);
        }
    }
    if output.success.is_empty() {
        let error = format!(
            "Failed to publish reactions: no relays accepted event {}",
            output.id().to_hex()
        );
        log::warn!("{}", error);
        *REACTIONS_STATE.write() = Nip78LoadState::Failed(error.clone());
        return Err(error);
    }
    cache_reactions(&data);
    *PREFERRED_REACTIONS.write() = reactions;
    *REACTIONS_STATE.write() = Nip78LoadState::Loaded;
    Ok(())
}
