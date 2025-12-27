//! Podcast Subscription Store
//!
//! NIP-51 based podcast subscription management using Kind 30003 (Bookmark Sets)
//! with d-tag "podcast-subscriptions".
//!
//! Supports both RSS/Podcast 2.0 feeds (via `r` tags) and native Nostr podcasts
//! (via `a` tags referencing Kind 30078 podcast metadata).

use dioxus::prelude::*;
use nostr_sdk::{EventBuilder, Filter, Kind, Tag, FromBech32};
use serde::{Deserialize, Serialize};
use std::time::Duration;

use crate::stores::{auth_store, nostr_client};

// ============================================================================
// Constants
// ============================================================================

/// NIP-51 Kind 30003 - Bookmark Sets
const LIST_KIND: u16 = 30003;

/// D tag identifier for podcast subscriptions
const D_TAG: &str = "podcast-subscriptions";

// ============================================================================
// Data Structures
// ============================================================================

/// A podcast subscription entry (RSS feed or Nostr podcast)
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PodcastSubscription {
    /// Podcast GUID for RSS podcasts (NIP-73 compliant identifier, stored in `i` tag)
    pub podcast_guid: Option<String>,
    /// Podcast Index numeric ID (cached for API efficiency, not stored in event)
    pub podcast_id: Option<u64>,
    /// RSS feed URL (cached for convenience, not stored in event)
    pub feed_url: Option<String>,
    /// Nostr podcast coordinate (from `a` tag, e.g., "30078:pubkey:d-tag")
    pub nostr_coordinate: Option<String>,
    /// Relay hint for Nostr podcasts
    pub relay_hint: Option<String>,
    /// Cached podcast title (for display before fetching full metadata)
    pub title: Option<String>,
    /// Cached podcast image URL
    pub image: Option<String>,
}

impl PodcastSubscription {
    /// Create a subscription from a Podcast Index GUID and optional numeric ID
    /// GUID is the primary identifier (NIP-73 compliant), ID is cached for API efficiency
    pub fn from_rss(podcast_guid: String, podcast_id: Option<u64>, feed_url: Option<String>) -> Self {
        Self {
            podcast_guid: Some(podcast_guid),
            podcast_id,
            feed_url,
            nostr_coordinate: None,
            relay_hint: None,
            title: None,
            image: None,
        }
    }

    /// Create a subscription from a Nostr coordinate
    pub fn from_nostr(coordinate: String, relay_hint: Option<String>) -> Self {
        Self {
            podcast_guid: None,
            podcast_id: None,
            feed_url: None,
            nostr_coordinate: Some(coordinate),
            relay_hint,
            title: None,
            image: None,
        }
    }

    /// Get unique identifier for this subscription
    /// Returns GUID for RSS podcasts (NIP-73 compliant), coordinate for Nostr podcasts
    /// Returns None if the subscription is invalid
    pub fn id(&self) -> Option<String> {
        if let Some(ref guid) = self.podcast_guid {
            Some(guid.clone())
        } else if let Some(ref coordinate) = self.nostr_coordinate {
            Some(coordinate.clone())
        } else {
            log::warn!("Invalid subscription: no podcast_guid or nostr_coordinate");
            None
        }
    }

    /// Get the Podcast Index numeric ID if available (for API calls)
    pub fn numeric_id(&self) -> Option<u64> {
        self.podcast_id
    }

    /// Check if this is an RSS subscription
    pub fn is_rss(&self) -> bool {
        self.podcast_guid.is_some()
    }

    /// Check if this is a Nostr subscription
    pub fn is_nostr(&self) -> bool {
        self.nostr_coordinate.is_some()
    }
}

// ============================================================================
// Global State
// ============================================================================

/// Global subscriptions list
pub static SUBSCRIPTIONS: GlobalSignal<Vec<PodcastSubscription>> = Signal::global(Vec::new);

/// Loading state
pub static SUBSCRIPTIONS_LOADING: GlobalSignal<bool> = Signal::global(|| false);

/// Error state
pub static SUBSCRIPTIONS_ERROR: GlobalSignal<Option<String>> = Signal::global(|| None);

/// Whether subscriptions have been fetched at least once
pub static SUBSCRIPTIONS_LOADED: GlobalSignal<bool> = Signal::global(|| false);

/// Available podcast categories for discovery
pub fn get_categories() -> Vec<PodcastCategory> {
    vec![
        PodcastCategory::new("Technology", "Coding, gadgets, and digital trends"),
        PodcastCategory::new("Business", "Finance, entrepreneurship, and markets"),
        PodcastCategory::new("News", "Current events and journalism"),
        PodcastCategory::new("Science", "Research, discoveries, and nature"),
        PodcastCategory::new("Education", "Learning and self-improvement"),
        PodcastCategory::new("True Crime", "Investigations and mysteries"),
        PodcastCategory::new("Comedy", "Humor and entertainment"),
        PodcastCategory::new("Sports", "Games, athletes, and competition"),
        PodcastCategory::new("Health", "Wellness and fitness"),
        PodcastCategory::new("Society", "Culture and social issues"),
    ]
}

/// Podcast category for discovery tiles
#[derive(Clone, Debug, PartialEq)]
pub struct PodcastCategory {
    pub name: &'static str,
    pub description: &'static str,
}

impl PodcastCategory {
    pub const fn new(name: &'static str, description: &'static str) -> Self {
        Self { name, description }
    }
}

// ============================================================================
// Fetch Functions
// ============================================================================

/// Fetch podcast subscriptions from Nostr relays
pub async fn fetch_subscriptions() -> Result<Vec<PodcastSubscription>, String> {
    log::info!("Fetching podcast subscriptions from Nostr (NIP-51 Kind 30003)...");
    SUBSCRIPTIONS_LOADING.write().clone_from(&true);
    SUBSCRIPTIONS_ERROR.write().clone_from(&None);

    // Check if authenticated
    if !auth_store::is_authenticated() {
        log::info!("Not authenticated, no subscriptions to fetch");
        SUBSCRIPTIONS_LOADING.write().clone_from(&false);
        SUBSCRIPTIONS_LOADED.write().clone_from(&true);
        return Ok(Vec::new());
    }

    // Get client and pubkey - clear loading state on early failures
    let client = match nostr_client::NOSTR_CLIENT.read().as_ref() {
        Some(c) => c.clone(),
        None => {
            let err = "Client not initialized".to_string();
            log::warn!("{}", err);
            SUBSCRIPTIONS_LOADING.write().clone_from(&false);
            SUBSCRIPTIONS_LOADED.write().clone_from(&true);
            SUBSCRIPTIONS_ERROR.write().clone_from(&Some(err.clone()));
            return Err(err);
        }
    };

    let auth = auth_store::AUTH_STATE.read();
    let pubkey_str = match auth.pubkey.as_ref() {
        Some(p) => p,
        None => {
            let err = "No pubkey".to_string();
            log::warn!("{}", err);
            SUBSCRIPTIONS_LOADING.write().clone_from(&false);
            SUBSCRIPTIONS_LOADED.write().clone_from(&true);
            SUBSCRIPTIONS_ERROR.write().clone_from(&Some(err.clone()));
            return Err(err);
        }
    };
    let pubkey = match nostr_sdk::PublicKey::from_bech32(pubkey_str)
        .or_else(|_| nostr_sdk::PublicKey::from_hex(pubkey_str))
    {
        Ok(pk) => pk,
        Err(e) => {
            let err = format!("Invalid pubkey: {}", e);
            log::warn!("{}", err);
            SUBSCRIPTIONS_LOADING.write().clone_from(&false);
            SUBSCRIPTIONS_LOADED.write().clone_from(&true);
            SUBSCRIPTIONS_ERROR.write().clone_from(&Some(err.clone()));
            return Err(err);
        }
    };

    // Build filter for subscription list event
    let filter = Filter::new()
        .author(pubkey)
        .kind(Kind::from(LIST_KIND))
        .identifier(D_TAG)
        .limit(1);

    // Ensure relays are ready
    nostr_client::ensure_relays_ready(&client).await;

    // Fetch subscription list event
    let subscriptions = match client.fetch_events(filter, Duration::from_secs(10)).await {
        Ok(events) => {
            if let Some(event) = events.into_iter().next() {
                log::info!("Found podcast subscriptions event: {}", event.id);
                parse_subscription_event(&event)
            } else {
                log::info!("No podcast subscriptions found on Nostr");
                Vec::new()
            }
        }
        Err(e) => {
            let error_msg = format!("Fetch error: {}", e);
            log::warn!("Failed to fetch subscriptions: {}", e);
            SUBSCRIPTIONS_ERROR.write().clone_from(&Some(error_msg.clone()));
            SUBSCRIPTIONS_LOADING.write().clone_from(&false);
            return Err(error_msg);
        }
    };

    // Update global state
    SUBSCRIPTIONS.write().clone_from(&subscriptions);
    SUBSCRIPTIONS_LOADING.write().clone_from(&false);
    SUBSCRIPTIONS_LOADED.write().clone_from(&true);

    Ok(subscriptions)
}

/// Parse subscriptions from a Kind 30003 event
fn parse_subscription_event(event: &nostr_sdk::Event) -> Vec<PodcastSubscription> {
    let mut subscriptions = Vec::new();

    for tag in event.tags.iter() {
        let tag_vec: Vec<&str> = tag.as_slice().iter().map(|s| s.as_str()).collect();

        match tag_vec.as_slice() {
            // NIP-73 format: ["i", "podcast:guid:<guid>"] or with optional URL hint
            ["i", identifier] | ["i", identifier, _] => {
                if let Some(guid) = identifier.strip_prefix("podcast:guid:") {
                    subscriptions.push(PodcastSubscription::from_rss(
                        guid.to_string(),
                        None, // No cached numeric ID from Nostr
                        None,
                    ));
                }
            }
            // Legacy format: r tag with numeric ID (backward compatibility)
            // TODO: Remove this once migration is complete
            ["r", podcast_id_str] => {
                log::warn!("Found legacy r tag with numeric ID: {}. Consider re-subscribing for NIP-73 compliance.", podcast_id_str);
                // We can't use this directly since we need the GUID
                // Skip legacy subscriptions - they'll need to be re-added
            }
            // Nostr coordinate (a tag for Kind 30078)
            ["a", coordinate] => {
                subscriptions.push(PodcastSubscription::from_nostr(coordinate.to_string(), None));
            }
            ["a", coordinate, relay_hint] => {
                subscriptions.push(PodcastSubscription::from_nostr(
                    coordinate.to_string(),
                    Some(relay_hint.to_string()),
                ));
            }
            _ => {}
        }
    }

    subscriptions
}

// ============================================================================
// Modify Functions
// ============================================================================

/// Add an RSS feed subscription by Podcast GUID (NIP-73 compliant)
/// - `podcast_guid`: The podcast's GUID (required for NIP-73 compliance)
/// - `podcast_id`: Optional Podcast Index numeric ID (cached for API efficiency)
/// - `feed_url`: Optional feed URL (cached for convenience)
pub async fn add_rss_subscription(
    podcast_guid: &str,
    podcast_id: Option<u64>,
    feed_url: Option<&str>,
) -> Result<(), String> {
    log::info!("Adding RSS subscription: guid={}, id={:?}", podcast_guid, podcast_id);

    // Check if already subscribed (by GUID)
    if is_subscribed(podcast_guid) {
        return Err("Already subscribed to this podcast".to_string());
    }

    // Add to local state
    let mut subs = SUBSCRIPTIONS.read().clone();
    subs.push(PodcastSubscription::from_rss(
        podcast_guid.to_string(),
        podcast_id,
        feed_url.map(String::from),
    ));

    // Publish updated list
    publish_subscriptions(&subs).await?;

    // Update global state
    SUBSCRIPTIONS.write().clone_from(&subs);

    Ok(())
}

/// Add a Nostr podcast subscription
pub async fn add_nostr_subscription(coordinate: &str, relay_hint: Option<&str>) -> Result<(), String> {
    log::info!("Adding Nostr subscription: {}", coordinate);

    // Check if already subscribed
    if is_subscribed(coordinate) {
        return Err("Already subscribed to this podcast".to_string());
    }

    // Add to local state
    let mut subs = SUBSCRIPTIONS.read().clone();
    subs.push(PodcastSubscription::from_nostr(
        coordinate.to_string(),
        relay_hint.map(String::from),
    ));

    // Publish updated list
    publish_subscriptions(&subs).await?;

    // Update global state
    SUBSCRIPTIONS.write().clone_from(&subs);

    Ok(())
}

/// Remove a subscription by ID (feed URL or coordinate)
pub async fn remove_subscription(id: &str) -> Result<(), String> {
    log::info!("Removing subscription: {}", id);

    // Remove from local state (skip invalid subscriptions with None id)
    let mut subs = SUBSCRIPTIONS.read().clone();
    subs.retain(|s| s.id().as_deref() != Some(id));

    // Publish updated list
    publish_subscriptions(&subs).await?;

    // Update global state
    SUBSCRIPTIONS.write().clone_from(&subs);

    Ok(())
}

/// Publish the subscription list to Nostr
async fn publish_subscriptions(subscriptions: &[PodcastSubscription]) -> Result<(), String> {
    // Check if authenticated and have a signer
    if !auth_store::is_authenticated() {
        return Err("Not authenticated".to_string());
    }
    if !nostr_client::has_signer() {
        return Err("No signer available".to_string());
    }

    // Get client
    let client = nostr_client::NOSTR_CLIENT
        .read()
        .as_ref()
        .ok_or("Client not initialized")?
        .clone();

    // Ensure relays are ready before publishing
    nostr_client::ensure_relays_ready(&client).await;

    // Build tags
    let mut tags = vec![
        Tag::identifier(D_TAG),
        Tag::custom(
            nostr_sdk::TagKind::Custom(std::borrow::Cow::Borrowed("title")),
            vec!["My Podcast Subscriptions".to_string()],
        ),
    ];

    for sub in subscriptions {
        if let Some(ref guid) = sub.podcast_guid {
            // RSS podcast - use NIP-73 `i` tag with podcast:guid:<guid> format
            tags.push(Tag::custom(
                nostr_sdk::TagKind::Custom(std::borrow::Cow::Borrowed("i")),
                vec![format!("podcast:guid:{}", guid)],
            ));
            // Also add the k tag for the external content type
            tags.push(Tag::custom(
                nostr_sdk::TagKind::Custom(std::borrow::Cow::Borrowed("k")),
                vec!["podcast:guid".to_string()],
            ));
        } else if let Some(ref coordinate) = sub.nostr_coordinate {
            // Nostr podcast - use a tag
            let mut a_values = vec![coordinate.clone()];
            if let Some(ref relay) = sub.relay_hint {
                a_values.push(relay.clone());
            }
            tags.push(Tag::custom(
                nostr_sdk::TagKind::Custom(std::borrow::Cow::Borrowed("a")),
                a_values,
            ));
        }
    }

    // Build NIP-51 event (Kind 30003 with d tag)
    let builder = EventBuilder::new(Kind::from(LIST_KIND), "")
        .tags(tags);

    // Publish to relays
    client
        .send_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to publish subscriptions: {}", e))?;

    log::info!("Podcast subscriptions saved to Nostr successfully");

    Ok(())
}

// ============================================================================
// Query Functions
// ============================================================================

/// Get all subscriptions
pub fn get_subscriptions() -> Vec<PodcastSubscription> {
    SUBSCRIPTIONS.read().clone()
}

/// Get RSS subscriptions (for iterating and handling mixed ID types)
pub fn get_rss_subscriptions() -> Vec<PodcastSubscription> {
    SUBSCRIPTIONS
        .read()
        .iter()
        .filter(|s| s.is_rss())
        .cloned()
        .collect()
}

/// Get only Nostr podcast coordinates
pub fn get_nostr_podcasts() -> Vec<String> {
    SUBSCRIPTIONS
        .read()
        .iter()
        .filter_map(|s| s.nostr_coordinate.clone())
        .collect()
}

/// Check if subscribed to a feed/podcast
pub fn is_subscribed(id: &str) -> bool {
    SUBSCRIPTIONS.read().iter().any(|s| s.id().as_deref() == Some(id))
}

/// Check if subscriptions have been loaded
pub fn is_loaded() -> bool {
    *SUBSCRIPTIONS_LOADED.read()
}

/// Check if currently loading
pub fn is_loading() -> bool {
    *SUBSCRIPTIONS_LOADING.read()
}
