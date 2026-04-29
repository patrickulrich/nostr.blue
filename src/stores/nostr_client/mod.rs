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
#![allow(unused_imports)]
use crate::services::admission_policy::NostrBlueAdmissionPolicy;
use crate::stores::pinned_notes;
use crate::stores::relay;
use crate::stores::signer::SignerType;
use dioxus::prelude::*;
use dioxus_core::spawn_forever;
use futures::future::join_all;
use nostr::Url;
#[cfg(target_arch = "wasm32")]
use nostr_indexeddb::WebDatabase;
#[cfg(feature = "native")]
use nostr_ndb::NdbDatabase;
use nostr_sdk::prelude::*;
use nostr_sdk::Client;
use std::num::NonZeroUsize;
use std::sync::Arc;
use std::time::Duration;
mod articles;
mod contacts;
mod custom_nips;
pub mod edits;
mod error;
pub mod fetching;
mod media;
mod muting;
mod notes;
mod polls;
mod profile;
mod reactions;
mod relay_publishing;
mod reposts;
mod signals;
mod streaming;
mod types;
pub use crate::stores::relay::display::RelayDisplayInfo;
pub use crate::stores::relay::pool::DEFAULT_RELAYS;
pub use crate::stores::relay::{
    wait_for_user_relays, RelayInfo, RelayPoolStoreStoreExt, RELAY_CONNECTED,
    RELAY_POOL, USER_RELAYS_APPLIED,
};
pub use nostr_relay_pool::RelayStatus;
pub use articles::{
    fetch_articles, fetch_event_by_coordinate, fetch_event_by_coordinate_with_relays,
    publish_article, publish_article_tracked,
};
pub use contacts::{
    fetch_contacts, follow_user, follow_users_batch, is_following, publish_contacts,
    publish_contacts_tracked, unfollow_user,
};
pub use custom_nips::{
    fetch_custom_nip_by_naddr, fetch_custom_nips, generate_custom_nip_naddr, publish_custom_nip,
    publish_custom_nip_tracked, search_custom_nips, KIND_CUSTOM_NIP,
};
pub use edits::{publish_edit, EditPublishResult};
pub use error::Error;
pub use fetching::{
    fetch_event_targeted, fetch_events_aggregated, fetch_events_aggregated_outbox,
    fetch_events_from_connected_relays, fetch_events_from_relays, fetch_metadata_targeted,
    fetch_profile_events_db, fetch_profile_events_from_relays, fetch_profile_events_targeted,
    parse_event_id, ParsedEventId,
};
pub(crate) use fetching::fetch_events_from_connected_relays_with_client;
#[cfg(feature = "native")]
pub use fetching::fetch_events_ndb_first;
pub use media::{
    publish_picture, publish_picture_tracked, publish_video, publish_video_tracked,
    publish_voice_message, publish_voice_message_reply, publish_voice_message_reply_tracked,
    publish_voice_message_tracked,
};
pub use muting::{
    block_user, get_blocked_users, get_mute_list_data, get_muted_posts, is_post_muted,
    is_post_muted_cached, is_user_blocked, is_user_blocked_cached, mute_post, report_post,
    unblock_user, unmute_post, MuteListData,
};
pub use notes::{publish_note, publish_note_tracked};
pub use polls::{
    get_cached_pubkey, publish_poll, publish_poll_tracked, publish_poll_vote,
    publish_poll_vote_tracked,
};
pub use profile::{
    publish_metadata, publish_metadata_tracked, update_profile_banner, update_profile_picture,
};
pub use reactions::{publish_reaction, publish_reaction_tracked};
pub use relay_publishing::{
    broadcast_presigned_event, publish_note_to_relays, publish_reaction_to_relays,
    publish_vanish_request_to_relays, send_presigned_event_to_relays,
};
pub use reposts::{delete_repost, publish_repost, publish_repost_tracked};
pub use signals::{
    get_contacts_cache, invalidate_contacts_cache, invalidate_mute_block_cache, CLIENT_INITIALIZED,
    CURRENT_SIGNER, HAS_SIGNER, MUTE_BLOCK_INVALIDATE, NOSTR_CLIENT,
};
pub use streaming::{
    stream_events_batched, stream_events_collected, stream_events_from_connected_relays_batched,
    stream_events_immediate, stream_events_with_callback,
    stream_video_events_from_connected_relays_batched,
};
pub use types::PublishResult;
/// Cross-platform async sleep helper (Dioxus pattern: compile-time cfg)
pub async fn platform_sleep_ms(ms: u64) {
    #[cfg(target_arch = "wasm32")]
    gloo_timers::future::TimeoutFuture::new(ms as u32).await;
    #[cfg(not(target_arch = "wasm32"))]
    tokio::time::sleep(std::time::Duration::from_millis(ms)).await;
}
const DEFAULT_DISCOVERY_RELAYS: &[&str] = &[
    "wss://purplepag.es",
    "wss://relay.nos.social",
    "wss://relay.damus.io",
];
/// Initialize the Nostr client and connect to relays
pub async fn initialize_client() -> std::result::Result<Arc<Client>, String> {
    log::info!("Initializing Nostr client with IndexedDB...");
    let relay_opts = RelayOptions::new()
        .max_avg_latency(Some(Duration::from_secs(2)))
        .verify_subscriptions(true)
        .ban_relay_on_mismatch(true)
        .adjust_retry_interval(true)
        .reconnect(true);
    #[cfg(target_arch = "wasm32")]
    let client = {
        let database = WebDatabase::open("nostr-blue-db").await.map_err(|e| {
            log::error!("Failed to open IndexedDB: {}", e);
            format!("Failed to open IndexedDB: {}", e)
        })?;
        log::info!("IndexedDB opened successfully");
        let gossip = nostr_gossip_memory::store::NostrGossipMemory::bounded(
            NonZeroUsize::new(10_000).expect("10_000 is non-zero"),
        );
        let gossip_limits = GossipRelayLimits {
            read_relays_per_user: 5,
            write_relays_per_user: 3,
            hint_relays_per_user: 2,
            ..Default::default()
        };
        let client_opts = ClientOptions::new()
            .verify_subscriptions(true)
            .ban_relay_on_mismatch(true)
            .max_avg_latency(Duration::from_secs(2))
            .sleep_when_idle(SleepWhenIdle::Enabled {
                timeout: Duration::from_secs(30),
            })
            .gossip(GossipOptions::default().limits(gossip_limits))
            .pool(RelayPoolOptions::new().max_relays(Some(50)));
        Client::builder()
            .database(database)
            .gossip(gossip)
            .admit_policy(NostrBlueAdmissionPolicy)
            .opts(client_opts)
            .build()
    };
    #[cfg(feature = "native")]
    let client = {
        let db_path = crate::platform::storage::data_dir().join("nostr-blue-ndb");
        std::fs::create_dir_all(&db_path)
            .map_err(|e| format!("Failed to create NDB dir: {}", e))?;
        let db_path_str = db_path.to_string_lossy().to_string();

        let (wake_tx, wake_rx) = std::sync::mpsc::channel::<()>();
        let wake_tx = std::sync::Mutex::new(wake_tx);

        let config = nostrdb::Config::new()
            .set_ingester_threads(2)
            .set_mapsize(1024usize * 1024 * 1024 * 1024)
            .set_sub_callback(move |_sub_id: u64| {
                let _ = wake_tx.lock().unwrap().send(());
            });

        let ndb = nostrdb::Ndb::new(&db_path_str, &config)
            .map_err(|e| format!("Failed to open NDB: {}", e))?;
        let database = NdbDatabase::from(ndb);

        let raw_db = database.clone();
        crate::stores::ndb::set_ndb(raw_db)
            .map_err(|_| "NDB already initialized".to_string())?;
        crate::stores::ndb::worker::set_wake_receiver(wake_rx);

        let gossip = nostr_gossip_memory::store::NostrGossipMemory::bounded(
            NonZeroUsize::new(10_000).expect("10_000 is non-zero"),
        );
        let gossip_limits = GossipRelayLimits {
            read_relays_per_user: 5,
            write_relays_per_user: 3,
            hint_relays_per_user: 2,
            ..Default::default()
        };
        let client_opts = ClientOptions::new()
            .verify_subscriptions(true)
            .ban_relay_on_mismatch(true)
            .max_avg_latency(Duration::from_secs(2))
            .sleep_when_idle(SleepWhenIdle::Enabled {
                timeout: Duration::from_secs(30),
            })
            .gossip(GossipOptions::default().limits(gossip_limits))
            .pool(RelayPoolOptions::new().max_relays(Some(50)));
        Client::builder()
            .database(database)
            .gossip(gossip)
            .admit_policy(NostrBlueAdmissionPolicy)
            .opts(client_opts)
            .build()
    };
    #[cfg(all(not(target_arch = "wasm32"), not(feature = "native")))]
    let client = Client::builder().build();
    let client = Arc::new(client);
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
    *NOSTR_CLIENT.write() = Some(client.clone());
    log::info!("Adding discovery relays for gossip...");
    let discovery_urls = crate::stores::relay::nip65::get_indexer_relay_urls();
    let discovery_urls = if discovery_urls.is_empty() {
        DEFAULT_DISCOVERY_RELAYS.iter().map(|s| s.to_string()).collect::<Vec<_>>()
    } else {
        discovery_urls
    };
    for discovery_url in &discovery_urls {
        if let Err(e) = client.add_discovery_relay(discovery_url).await {
            log::warn!("Failed to add discovery relay {}: {}", discovery_url, e);
        }
    }
    // Use fast connection with 2-second timeout for quick initial connection
    log::info!("Attempting fast relay connection...");
    let _connected = relay::try_connect_relays(&client, Duration::from_secs(2)).await;

    // Always spawn background retry to ensure all relays get connection attempts
    // Even if some relays connected, others may have failed and need retries
    log::info!("Spawning background connection task for relay reliability...");
    #[cfg(target_arch = "wasm32")]
    {
        let client_for_connect = client.clone();
        wasm_bindgen_futures::spawn_local(async move {
            relay::connection::ensure_relays_ready(&client_for_connect).await;
            log::info!("Background relay connections completed");
        });
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let client_for_connect = client.clone();
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<bool>();
        tokio::spawn(async move {
            let output = client_for_connect.try_connect(Duration::from_secs(3)).await;
            let success = !output.success.is_empty();
            let _ = tx.send(success);
            log::info!("Background relay connections completed (success={})", success);
        });
        spawn_forever(async move {
            if let Some(success) = rx.recv().await {
                if success && !*RELAY_CONNECTED.peek() {
                    *RELAY_CONNECTED.write() = true;
                }
            }
        });
    }
    *CLIENT_INITIALIZED.write() = true;
    #[cfg(feature = "native")]
    {
        if crate::stores::ndb::get_ndb().is_some() {
            if let Err(e) = crate::stores::ndb::start_ndb_worker() {
                log::error!("Failed to start NdbWorker: {}", e);
            }
            crate::stores::ndb::start_ndb_event_processor();
            spawn_forever(async move {
                loop {
                    tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                    let mut ids = {
                        let mut guard = crate::stores::ndb::unknown_ids::UNKNOWN_IDS.lock().unwrap();
                        std::mem::take(&mut *guard)
                    };
                    if !ids.is_empty() {
                        let _ = ids.process_queued_events().await;
                        if ids.ready_to_send() {
                            if let Some(c) = crate::stores::nostr_client::get_client() {
                                let _ = ids.send_and_clear(&c).await;
                            }
                        }
                    }
                    {
                        let mut guard = crate::stores::ndb::unknown_ids::UNKNOWN_IDS.lock().unwrap();
                        std::mem::swap(&mut *guard, &mut ids);
                    }
                }
            });
        }
    }
    relay::start_health_poll(client.clone());
    crate::stores::notification_dispatcher::NotificationDispatcher::init(client.clone());
    if let Some(dispatcher) = crate::stores::notification_dispatcher::NotificationDispatcher::instance() {
        dispatcher.start_listener();
    }
    log::info!("Nostr client initialized with relays ready");
    Ok(client)
}
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
/// Initialize client with a signer (enables publishing)
pub async fn set_signer(signer: SignerType) -> std::result::Result<(), String> {
    log::info!("Setting signer: {}", signer.backend_name());
    let client = get_client().ok_or("Client not initialized")?;
    let nostr_signer = signer.as_nostr_signer();
    client.set_signer(nostr_signer).await;
    *HAS_SIGNER.write() = true;
    *CURRENT_SIGNER.write() = Some(signer.clone());
    let client_clone = client.clone();
    spawn_forever(async move {
        relay::apply_local_relays_to_client(client_clone.clone()).await;
        if let Err(e) = relay::init_user_relay_lists(client_clone.clone()).await {
            log::warn!("Failed to load user relay lists: {}", e);
        }
        *relay::USER_RELAYS_APPLIED.write() = true;
        log::info!("User relays applied, feed fetching unblocked");
        if let Err(e) = relay::init_nip51_relay_lists(client_clone.clone()).await {
            log::warn!("Failed to load NIP-51 relay lists: {}", e);
        }
        if let Err(e) = relay::init_private_relay_lists(client_clone.clone()).await {
            log::warn!("Failed to load private relay lists: {}", e);
        }
        relay::nip65::add_indexer_relays_to_client(client_clone.clone()).await;
        relay::pool::remove_blocked_relays_from_pool(&client_clone).await;
        relay::nip65::fetch_own_lists_from_indexers(client_clone.clone()).await;
    });
    spawn_forever(async move {
        if let Err(e) = pinned_notes::init_pinned_notes().await {
            log::warn!("Failed to load user pinned notes: {}", e);
        }
    });
    spawn_forever(async move {
        crate::stores::publish_queue::load_from_storage().await;
        crate::stores::publish_queue::start_processor();
        log::info!("Publish queue loaded and processor started");
    });
    log::info!("Signer updated successfully");
    Ok(())
}
/// Switch to read-only mode (removes signer)
pub async fn set_read_only() -> std::result::Result<(), String> {
    log::info!("Switching to read-only mode");
    let client = get_client().ok_or("Client not initialized")?;
    client.unset_signer().await;
    *HAS_SIGNER.write() = false;
    *CURRENT_SIGNER.write() = None;
    if let Err(e) = crate::stores::publish_queue::processor::process_once_guarded().await {
        log::warn!("Flush before read-only failed: {}", e);
    }
    crate::stores::publish_queue::stop_processor();
    log::info!("Switched to read-only mode");
    Ok(())
}
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
