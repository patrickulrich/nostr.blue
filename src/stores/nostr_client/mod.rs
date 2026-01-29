//! Nostr client module
//!
//! This module provides centralized Nostr client state management.
//! It is organized into submodules for different concerns:
//!
//! - `error` - Error types for client operations
//! - `types` - Shared types (PublishResult, MuteListTags)
//! - `signals` - Reactive state signals (GlobalSignal definitions)
//! - `fetching` - Event fetching strategies
//! - `streaming` - Progressive event streaming
//! - `notes` - Text notes (kind 1)
//! - `reactions` - Reactions (kind 7)
//! - `contacts` - Follow/unfollow (kind 3)
//! - `muting` - Mute/block/report (kind 10000, 1984)
//! - `reposts` - Reposts (kind 6)
//! - `profile` - Metadata (kind 0)
//! - `articles` - Long-form (kind 30023)
//! - `media` - Picture/video/voice
//! - `polls` - Polls (kind 1068)
//! - `custom_nips` - Custom NIPs (kind 30817)
//! - `relay_publishing` - Relay-specific publishing
//!
//! # Design Principles
//!
//! **IMPORTANT**: Functions that need client access take `client: &Client` as
//! a parameter rather than calling `get_client()` internally. This avoids
//! circular dependencies and makes the code more testable.
//!
//! ```rust,ignore
//! // WRONG - creates circular dependency
//! pub async fn add_relay(url: &str) {
//!     let client = nostr_client::get_client().unwrap();  // DON'T DO THIS
//! }
//!
//! // CORRECT - parameterized, no circular dependency
//! pub async fn add_relay(client: &Client, url: &str) {
//!     // Works correctly
//! }
//! ```

#![allow(unused_imports)]  // Re-exports are public API for external consumers

use dioxus::prelude::*;
use dioxus_core::spawn_forever;
use futures::future::join_all;
use nostr_sdk::Client;
use nostr_sdk::prelude::*;
use nostr::Url;
use std::num::NonZeroUsize;
use std::sync::Arc;
use std::time::Duration;

#[cfg(target_arch = "wasm32")]
use nostr_indexeddb::WebDatabase;

use crate::stores::signer::SignerType;
use crate::stores::pinned_notes;
use crate::stores::relay;

#[cfg(target_arch = "wasm32")]
use crate::services::admission_policy::NostrBlueAdmissionPolicy;

// =============================================================================
// Tier 1: Private Module Declarations
// =============================================================================

mod error;
mod types;
mod signals;
mod fetching;
mod streaming;
mod notes;
mod reactions;
mod contacts;
mod muting;
mod reposts;
mod profile;
mod articles;
mod media;
mod polls;
mod custom_nips;
mod relay_publishing;

// =============================================================================
// Tier 2: Public Re-exports (Backward Compatibility)
// =============================================================================

// Error type
pub use error::Error;

// Types
pub use types::PublishResult;
// MuteListTags is pub(crate), not re-exported

// Signals (most important for consumers)
pub use signals::{
    NOSTR_CLIENT, CLIENT_INITIALIZED, HAS_SIGNER, CURRENT_SIGNER,
    invalidate_contacts_cache, MUTE_BLOCK_INVALIDATE, invalidate_mute_block_cache,
};

// Fetching
pub use fetching::{
    fetch_events_aggregated, fetch_events_from_relays,
    fetch_events_aggregated_outbox, fetch_profile_events_db,
    fetch_profile_events_from_relays, fetch_video_events,
    fetch_events_from_connected_relays, fetch_video_events_from_connected_relays,
};

// Streaming
pub use streaming::{
    stream_events_with_callback, stream_events_batched,
    stream_events_from_connected_relays_batched, stream_events_collected,
};

// Publishing (notes, reactions, contacts, etc.)
pub use notes::{publish_note_tracked, publish_note};
pub use reactions::{publish_reaction_tracked, publish_reaction};
pub use contacts::{
    fetch_contacts, publish_contacts_tracked, publish_contacts,
    follow_user, unfollow_user, is_following,
};
pub use muting::{
    get_muted_posts, get_blocked_users, get_mute_list_data, MuteListData,
    is_post_muted, is_post_muted_cached,
    is_user_blocked, is_user_blocked_cached,
    mute_post, unmute_post, block_user, unblock_user, report_post,
};
pub use reposts::{publish_repost_tracked, publish_repost, delete_repost};
pub use profile::{
    publish_metadata_tracked, publish_metadata,
    update_profile_picture, update_profile_banner,
};
pub use articles::{
    fetch_articles, fetch_event_by_coordinate,
    fetch_event_by_coordinate_with_relays, publish_article_tracked, publish_article,
};
// Deprecated: fetch_article_by_coordinate - use fetch_event_by_coordinate(30023, pubkey, id) instead
pub use media::{
    publish_picture_tracked, publish_picture,
    publish_video_tracked, publish_video,
    publish_voice_message_tracked, publish_voice_message,
    publish_voice_message_reply_tracked, publish_voice_message_reply,
};
pub use polls::{
    get_cached_pubkey,
    publish_poll_vote_tracked, publish_poll_vote,
    publish_poll_tracked, publish_poll,
};
pub use custom_nips::{
    KIND_CUSTOM_NIP, fetch_custom_nips, fetch_custom_nip_by_naddr,
    publish_custom_nip_tracked, publish_custom_nip,
    generate_custom_nip_naddr, search_custom_nips,
};
pub use relay_publishing::{
    publish_note_to_relays, publish_reaction_to_relays,
    send_presigned_event_to_relays,
};

// =============================================================================
// Tier 3: External Re-exports (Backward Compatibility)
// =============================================================================

// Re-export relay types for backward compatibility
// New code should use crate::stores::relay directly
pub use crate::stores::relay::{
    RelayInfo, RelayPoolStoreStoreExt, RelayStatus,
    RELAY_CONNECTED, RELAY_POOL, USER_RELAYS_APPLIED,
};
pub use crate::stores::relay::pool::DEFAULT_RELAYS;
pub use crate::stores::relay::display::RelayDisplayInfo;

// =============================================================================
// Platform Helpers
// =============================================================================

/// Cross-platform async sleep helper (Dioxus pattern: compile-time cfg)
pub(crate) async fn platform_sleep_ms(ms: u64) {
    #[cfg(target_arch = "wasm32")]
    gloo_timers::future::TimeoutFuture::new(ms as u32).await;
    #[cfg(not(target_arch = "wasm32"))]
    tokio::time::sleep(std::time::Duration::from_millis(ms)).await;
}

// =============================================================================
// Initialization
// =============================================================================

/// Discovery relays for gossip model fallback
/// These are used by the SDK to find users' relay lists (NIP-65/NIP-17)
/// when gossip data is outdated or missing
const DISCOVERY_RELAYS: &[&str] = &[
    "wss://relay.damus.io",
    "wss://purplepag.es",
    "wss://nos.lol",
];

/// Initialize the Nostr client and connect to relays
pub async fn initialize_client() -> std::result::Result<Arc<Client>, String> {
    log::info!("Initializing Nostr client with IndexedDB...");

    // Configure relay options for better performance
    let relay_opts = RelayOptions::new()
        // Skip relays with average latency > 2 seconds
        .max_avg_latency(Some(Duration::from_secs(2)))
        // Verify that events match subscription filters
        .verify_subscriptions(true)
        // Ban relays that send mismatched events
        .ban_relay_on_mismatch(true)
        // Adjust retry interval based on success rate
        .adjust_retry_interval(true)
        // Initial retry interval: 10 seconds
        .retry_interval(Duration::from_secs(10))
        // Enable automatic reconnection
        .reconnect(true);

    // Create client with database
    #[cfg(target_arch = "wasm32")]
    let client = {
        // Open IndexedDB database
        let database = WebDatabase::open("nostr-blue-db")
            .await
            .map_err(|e| {
                log::error!("Failed to open IndexedDB: {}", e);
                format!("Failed to open IndexedDB: {}", e)
            })?;

        log::info!("IndexedDB opened successfully");

        // Enable gossip with bounded in-memory storage to prevent unbounded memory growth
        // NostrGossipMemory is WASM-compatible and provides automatic relay routing
        // Limit to 10,000 entries (conservative; nostr-sdk defaults to 35,000 for events)
        let gossip = nostr_gossip_memory::store::NostrGossipMemory::bounded(
            NonZeroUsize::new(10_000).expect("10_000 is non-zero")
        );

        // Configure client options for gossip-discovered relays
        // This is CRITICAL: Without this, gossip relays won't verify events match filters
        let client_opts = ClientOptions::new()
            .verify_subscriptions(true)
            .ban_relay_on_mismatch(true)
            .max_avg_latency(Duration::from_secs(2));

        Client::builder()
            .database(database)
            .gossip(gossip)
            .admit_policy(NostrBlueAdmissionPolicy)
            .opts(client_opts)
            .build()
    };

    #[cfg(not(target_arch = "wasm32"))]
    let client = Client::builder().build();

    let client = Arc::new(client);

    // Add default relays with options in PARALLEL (will be replaced if user has kind 10002)
    // This significantly speeds up initialization by not waiting for each relay sequentially
    let relay_futures: Vec<_> = DEFAULT_RELAYS
        .iter()
        .filter_map(|relay_url| {
            Url::parse(relay_url).ok().map(|url| {
                let opts = relay_opts.clone();
                let pool = client.pool();
                let url_str = relay_url.to_string();
                async move {
                    match pool.add_relay(url, opts).await {
                        Ok(_) => {
                            log::debug!("Added relay with opts: {}", url_str);
                            // Mark as Connecting, not Connected (actual connection is async)
                            RelayInfo::new(url_str, RelayStatus::Connecting)
                        }
                        Err(e) => {
                            log::error!("Failed to add relay {}: {}", url_str, e);
                            RelayInfo::new(url_str, RelayStatus::Disconnected)
                        }
                    }
                }
            })
        })
        .collect();

    let relay_infos: Vec<RelayInfo> = join_all(relay_futures).await;

    RELAY_POOL.read().data().write().clone_from(&relay_infos);

    // Store client first (so it's available for queries)
    *NOSTR_CLIENT.write() = Some(client.clone());

    // Add discovery relays for gossip bootstrapping
    // These are used by the SDK to find users' relay lists (NIP-65/NIP-17)
    // when gossip data is outdated or missing
    log::info!("Adding discovery relays for gossip...");
    for discovery_url in DISCOVERY_RELAYS {
        if let Err(e) = client.add_discovery_relay(*discovery_url).await {
            log::warn!("Failed to add discovery relay {}: {}", discovery_url, e);
        }
    }

    // Spawn relay connections in background (required for WASM - can't block main thread)
    log::info!("Spawning relay connections...");
    #[cfg(target_arch = "wasm32")]
    {
        let client_for_connect = client.clone();
        wasm_bindgen_futures::spawn_local(async move {
            client_for_connect.connect().await;
            log::info!("Background relay connections initiated");
        });
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let client_for_connect = client.clone();
        tokio::spawn(async move {
            client_for_connect.connect().await;
            log::info!("Background relay connections initiated");
        });
    }

    // Wait for at least one relay to connect before marking initialized
    // This ensures CLIENT_INITIALIZED means "ready to fetch events"
    use nostr_relay_pool::RelayStatus as PoolRelayStatus;
    const TIMEOUT_MS: u64 = 3000;
    const POLL_INTERVAL_MS: u64 = 100;

    // Single implementation using platform_sleep_ms instead of duplicated cfg blocks
    {
        let start = instant::Instant::now();

        loop {
            // Yield to allow background connection task to progress
            platform_sleep_ms(POLL_INTERVAL_MS).await;

            let relays_now = client.relays().await;
            let connected = relays_now.values().any(|r| r.status() == PoolRelayStatus::Connected);

            if connected {
                log::info!("First relay connected after {}ms", start.elapsed().as_millis());
                if !*RELAY_CONNECTED.peek() {
                    *RELAY_CONNECTED.write() = true;
                }
                break;
            }

            if start.elapsed().as_millis() > TIMEOUT_MS as u128 {
                log::warn!("Relay connection timeout after {}ms, proceeding anyway", TIMEOUT_MS);
                // Signal false so downstream watchers know init completed without relay
                // They can retry via ensure_relays_ready when a relay connects later
                *RELAY_CONNECTED.write() = false;
                break;
            }
        }
    }

    // Now mark as initialized - relays are ready (or timed out)
    *CLIENT_INITIALIZED.write() = true;

    log::info!("Nostr client initialized with relays ready");
    Ok(client)
}

// =============================================================================
// Client Access
// =============================================================================

/// Get the current client instance
pub fn get_client() -> Option<Arc<Client>> {
    NOSTR_CLIENT.read().clone()
}

/// Check if the client has a signer attached
#[allow(dead_code)]
pub fn has_signer() -> bool {
    *HAS_SIGNER.read()
}

/// Get the current signer
pub fn get_signer() -> Option<SignerType> {
    CURRENT_SIGNER.read().clone()
}

// =============================================================================
// Signer Management
// =============================================================================

/// Initialize client with a signer (enables publishing)
pub async fn set_signer(signer: SignerType) -> std::result::Result<(), String> {
    log::info!("Setting signer: {}", signer.backend_name());

    // Get existing client - don't recreate!
    let client = get_client().ok_or("Client not initialized")?;

    // Just update the signer, keep all relay connections
    let nostr_signer = signer.as_nostr_signer();
    client.set_signer(nostr_signer).await;

    *HAS_SIGNER.write() = true;
    *CURRENT_SIGNER.write() = Some(signer.clone());

    // Load user's relay configuration in background
    // SDK gossip handles dynamic relay routing automatically, so we only need to:
    // 1. Load user's relay metadata for Settings UI display
    // 2. Apply local relays (browser-only storage)
    // 3. Load NIP-51 lists (search/blocked - not handled by gossip)
    //
    // Using spawn_forever to ensure tasks run at root scope and survive component unmounts.
    // These background init tasks should not be cancelled if the calling component unmounts.
    let client_clone = client.clone();
    spawn_forever(async move {
        // Apply local relays FIRST (browser-only storage) - this is instant
        relay::apply_local_relays_to_client(client_clone.clone()).await;

        // Signal immediately after local relays are applied
        // SDK gossip discovers relays dynamically per-pubkey, so we don't need
        // to wait for Settings metadata before feed fetching can begin
        *relay::USER_RELAYS_APPLIED.write() = true;
        log::info!("User relays applied, feed fetching unblocked");

        // Load user's relay metadata for Settings UI (slow network fetch)
        // This is non-blocking for feeds - only needed for Settings display
        if let Err(e) = relay::init_user_relay_lists(client_clone.clone()).await {
            log::warn!("Failed to load user relay lists: {}", e);
        }

        // Load NIP-51 relay lists (search/blocked) - non-blocking for feed
        if let Err(e) = relay::init_nip51_relay_lists(client_clone.clone()).await {
            log::warn!("Failed to load NIP-51 relay lists: {}", e);
        }

        // After NIP-51 lists load, remove any blocked relays that were added before
        relay::pool::remove_blocked_relays_from_pool(&client_clone).await;
    });

    // Load user's pinned notes (kind 10001) in background
    spawn_forever(async move {
        if let Err(e) = pinned_notes::init_pinned_notes().await {
            log::warn!("Failed to load user pinned notes: {}", e);
        }
    });

    log::info!("Signer updated successfully");
    Ok(())
}

/// Switch to read-only mode (removes signer)
pub async fn set_read_only() -> std::result::Result<(), String> {
    log::info!("Switching to read-only mode");

    // Get existing client
    let client = get_client().ok_or("Client not initialized")?;

    // Remove signer
    client.unset_signer().await;

    *HAS_SIGNER.write() = false;
    *CURRENT_SIGNER.write() = None;

    log::info!("Switched to read-only mode");
    Ok(())
}

// =============================================================================
// Relay Wrappers (Backward Compatibility)
// =============================================================================

/// Wait for at least one relay to be ready before fetching
/// Delegates to relay::connection::ensure_relays_ready for the actual implementation.
///
/// This is needed because connect() is non-blocking and spawns background tasks.
/// Call this before any direct client.fetch_events() calls.
pub async fn ensure_relays_ready(client: &Client) {
    relay::connection::ensure_relays_ready(client).await;
}

/// Create an naddr (NIP-19) with relay hints for an addressable event
/// This includes relay hints from the user's write relays for better discoverability.
/// Delegates to relay::hints::make_naddr_with_hints
pub async fn make_naddr_with_hints(
    kind: u16,
    pubkey: &nostr::PublicKey,
    identifier: &str,
) -> std::result::Result<String, String> {
    let client = get_client().ok_or("Client not initialized")?;
    relay::make_naddr_with_hints(&client, kind, pubkey, identifier).await
}

/// Add a custom relay
/// Delegates to relay::pool::add_relay for the actual implementation.
#[allow(dead_code)]
pub async fn add_relay(relay_url: &str) -> std::result::Result<(), String> {
    let client = get_client().ok_or("Client not initialized")?;
    relay::pool::add_relay(&client, relay_url).await
}

/// Remove a relay
/// Delegates to relay::pool::remove_relay for the actual implementation.
#[allow(dead_code)]
pub async fn remove_relay(relay_url: &str) -> std::result::Result<(), String> {
    let client = get_client().ok_or("Client not initialized")?;
    relay::pool::remove_relay(&client, relay_url).await
}

/// Disconnect from all relays
/// Delegates to relay::connection::disconnect for the actual implementation.
#[allow(dead_code)]
pub async fn disconnect() {
    if let Some(client) = get_client() {
        relay::connection::disconnect(&client).await;
    }
}

/// Reconnect to all relays
/// Delegates to relay::connection::reconnect for the actual implementation.
#[allow(dead_code)]
pub async fn reconnect() {
    if let Some(client) = get_client() {
        relay::connection::reconnect(&client).await;
    }
}

/// Get display info for all connected relays (for Connections tab in settings)
///
/// This is a convenience wrapper that calls get_client() internally.
/// See [`relay::get_relay_display_info`] for the implementation.
pub async fn get_relay_display_info() -> Vec<RelayDisplayInfo> {
    let Some(client) = get_client() else {
        return vec![];
    };
    relay::get_relay_display_info(&client).await
}
