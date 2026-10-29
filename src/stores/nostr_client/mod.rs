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
use nostr::types::url::Url;
#[cfg(feature = "native")]
use nostr_ndb::NdbDatabase;
use nostr_sdk::prelude::*;
use nostr_sdk::client::Client;
use std::num::NonZeroUsize;
use std::sync::Arc;
use std::time::Duration;
mod articles;
mod contacts;
mod custom_nips;
pub mod compat;
pub mod edits;
mod error;
pub mod eose;
pub mod ephemeral;
pub mod facade;
pub mod fetching;
pub mod observe;
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
mod workouts;
pub use types::detect_mime_type;
pub use crate::stores::relay::display::RelayDisplayInfo;
pub use crate::stores::relay::pool::DEFAULT_RELAYS;
pub use crate::stores::relay::{
    wait_for_user_relays, RelayInfo, RelayPoolStoreStoreExt, RELAY_CONNECTED,
    RELAY_POOL, USER_RELAYS_APPLIED,
};
pub use nostr_sdk::relay::RelayStatus;
pub use articles::{
    fetch_articles, fetch_event_by_coordinate, fetch_event_by_coordinate_with_relays,
    publish_article_tracked, ARTICLES_PAGINATION_WINDOW_SECS,
};
pub use contacts::{
    fetch_contacts, follow_user, follow_users_batch, is_following, publish_contacts,
    publish_contacts_tracked, unfollow_user, warm_contacts_cache,
};
pub use custom_nips::{
    fetch_custom_nip_by_naddr, fetch_custom_nips, generate_custom_nip_naddr, publish_custom_nip,
    publish_custom_nip_tracked, search_custom_nips, KIND_CUSTOM_NIP,
};
pub use edits::{publish_edit, EditPublishResult};
pub use error::Error;
pub use facade::{
    fetch, fetch_event_by_id, fetch_with_client, parse_event_ref, EventRef, FetchProfile,
    FetchTarget, LocatedEvent,
};
pub use fetching::{
    connected_read_relay_urls, fetch_custom_nip_events, fetch_event_targeted,
    fetch_events_aggregated, fetch_events_aggregated_outbox, fetch_events_db_merge,
    fetch_events_db_merge_from_connected, fetch_events_from_connected_relays,
    fetch_events_from_connected_relays_background_with_client, fetch_events_from_relays,
    fetch_events_quorum, fetch_metadata_from_indexers, fetch_metadata_targeted,
    fetch_page_per_relay, fetch_profile_events_db, fetch_profile_events_from_relays,
    fetch_profile_events_from_relays_direct, fetch_profile_events_targeted,
    fetch_radio_events, fetch_nest_events, fetch_topic_events, parse_event_id,
    parse_metadata_content, query_nest_db_cache, ParsedEventId, RelayPageResult,
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
    block_user, get_blocked_users, get_mute_list_data, get_muted_posts, get_muted_words,
    is_post_muted, is_post_muted_cached, is_user_blocked, is_user_blocked_cached, is_word_muted,
    mute_post, mute_word, report_post, unblock_user, unmute_post, unmute_word, MuteListData,
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
    get_client, get_contacts_cache, invalidate_contacts_cache, invalidate_mute_block_cache,
    set_nostr_client, CLIENT_INITIALIZED, CONTACTS_VERSION, CURRENT_SIGNER, HAS_SIGNER,
    MUTE_BLOCK_INVALIDATE, NOSTR_CLIENT,
};
pub use streaming::{
    stream_events_batched, stream_events_collected, stream_events_from_connected_relays_batched,
    stream_events_immediate, stream_events_immediate_with_origin, stream_events_with_callback,
    stream_profile_events_from_relays,
    stream_video_events_from_connected_relays_batched,
};
pub use types::PublishResult;
pub use workouts::{publish_workout, publish_workout_tracked};
pub use compat::{db_contact_profiles, db_contacts_public_keys, db_metadata, fetch_metadata};
pub use eose::{fetch_quorum_threshold, realtime_liveness_threshold};
/// Cross-platform async sleep helper (Dioxus pattern: compile-time cfg)
pub async fn platform_sleep_ms(ms: u64) {
    #[cfg(target_arch = "wasm32")]
    gloo_timers::future::TimeoutFuture::new(ms as u32).await;
    #[cfg(not(target_arch = "wasm32"))]
    tokio::time::sleep(std::time::Duration::from_millis(ms)).await;
}
/// Wait for `HAS_SIGNER` to flip true, polling at 50ms intervals.
/// Mirrors `relay::wait_for_user_relays` (connection.rs) — callers poll in
/// bounded windows and decide whether to retry or give up.
/// Returns true if a signer attached within the timeout.
pub async fn wait_for_signer(timeout: Duration, context: &str) -> bool {
    use dioxus::signals::ReadableExt;
    wait_for_gate(
        || *HAS_SIGNER.peek(),
        timeout,
        50,
        context,
    )
    .await
}

/// Bounded poll for a boolean readiness gate (shared implementation of the
/// signer/relay-readiness waits — the poll loops were copy-pasted).
///
/// Checks the gate first (fast path), then polls every `poll_ms` until it
/// flips or `timeout` elapses. The context string only identifies the
/// waiter in logs.
pub async fn wait_for_gate(
    mut gate: impl FnMut() -> bool,
    timeout: Duration,
    poll_ms: u64,
    context: &str,
) -> bool {
    if gate() {
        return true;
    }
    log::debug!("{context}: waiting (polling every {poll_ms}ms, bounded by {timeout:?})...");
    let start = instant::Instant::now();
    while !gate() && start.elapsed() < timeout {
        platform_sleep_ms(poll_ms).await;
    }
    let ready = gate();
    if ready {
        log::debug!(
            "{context}: gate ready after {}ms",
            start.elapsed().as_millis()
        );
    } else {
        log::debug!("{context}: gate not ready after {:?}", start.elapsed());
    }
    ready
}


use nostr_database::NostrDatabase;
use nostr_sdk::monitor::Monitor;
use std::sync::OnceLock;

/// Shared client resources that survive login/logout client rebuilds.
struct SharedResources {
    database: Arc<dyn NostrDatabase>,
    gossip: Arc<dyn NostrGossip>,
    monitor: Monitor,
}

static SHARED: OnceLock<SharedResources> = OnceLock::new();

/// WASM: register OPFS SAHPool VFS, then relaxed-IDB VFS. Returns the VFS
/// name to use, or None when neither is available (caller falls back to
/// in-memory). Mirrors the nostr-sqlite README pattern.
///
/// NOTE: SAHPool SyncAccessHandles are exclusive per directory — a second
/// tab of the app cannot install the same pool and will fall through to the
/// IDB VFS, meaning each tab keeps an independent (relay-rehydratable)
/// cache. That is accepted behavior for a cache.
#[cfg(target_arch = "wasm32")]
async fn register_sqlite_vfs() -> Option<&'static str> {
    const SQLITE_OPFS_VFS: &str = "opfs-sahpool";
    const SQLITE_IDB_VFS: &str = "relaxed-idb";
    use sqlite_wasm_rs::WasmOsCallback;
    use sqlite_wasm_vfs::relaxed_idb::{install as install_relaxed_idb_vfs, RelaxedIdbCfgBuilder};
    use sqlite_wasm_vfs::sahpool::{install as install_opfs_sahpool_vfs, OpfsSAHPoolCfgBuilder};

    let opfs = OpfsSAHPoolCfgBuilder::new()
        .vfs_name(SQLITE_OPFS_VFS)
        .directory(".nostr-blue-opfs")
        .build();
    if install_opfs_sahpool_vfs::<WasmOsCallback>(&opfs, false).await.is_ok() {
        log::info!("SQLite VFS: OPFS SAHPool registered");
        return Some(SQLITE_OPFS_VFS);
    }
    let idb = RelaxedIdbCfgBuilder::new().vfs_name(SQLITE_IDB_VFS).build();
    if install_relaxed_idb_vfs::<WasmOsCallback>(&idb, false).await.is_ok() {
        log::info!("SQLite VFS: relaxed IndexedDB registered (OPFS unavailable — possibly another tab holds the SAH pool)");
        return Some(SQLITE_IDB_VFS);
    }
    log::warn!("SQLite VFS: neither OPFS nor IDB available — database will be in-memory only (data lost on refresh)");
    None
}

/// Open the platform event database. The result is shared across client
/// rebuilds so event history survives login/logout.
async fn open_shared_database() -> Result<Arc<dyn NostrDatabase>, String> {
    #[cfg(target_arch = "wasm32")]
    {
        use nostr_sqlite::store::NostrSqlite;
        let builder = match register_sqlite_vfs().await {
            Some(vfs) => NostrSqlite::builder().persistent_with_vfs("nostr-blue-db.sqlite3", vfs),
            None => NostrSqlite::builder().in_memory(),
        };
        let db = builder
            .build()
            .await
            .map_err(|e| format!("Failed to open nostr-sqlite: {e}"))?;
        Ok(Arc::new(db))
    }
    #[cfg(all(not(target_arch = "wasm32"), feature = "native"))]
    {
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
                let _ = wake_tx.lock().unwrap_or_else(|e| e.into_inner()).send(());
            });

        let ndb = nostrdb::Ndb::new(&db_path_str, &config)
            .map_err(|e| format!("Failed to open NDB: {}", e))?;
        let database = NdbDatabase::from(ndb);

        let raw_db = database.clone();
        crate::stores::ndb::set_ndb(raw_db)
            .map_err(|_| "NDB already initialized".to_string())?;
        crate::stores::ndb::worker::set_wake_receiver(wake_rx);

        Ok(Arc::new(database))
    }
    #[cfg(all(not(target_arch = "wasm32"), not(feature = "native")))]
    {
        Ok(Arc::new(nostr_memory::MemoryDatabase::unbounded()))
    }
}

/// Open the shared gossip store. sqlite-backed where possible so NIP-65
/// relay lists survive sessions (faster DM/outbox cold starts); falls back
/// to the bounded in-memory store.
async fn open_shared_gossip() -> Arc<dyn NostrGossip> {
    #[cfg(feature = "native")]
    {
        let path = crate::platform::storage::data_dir().join("nostr-blue-gossip.sqlite3");
        match nostr_gossip_sqlite::store::NostrGossipSqlite::open(&path).await {
            Ok(store) => {
                log::info!("Gossip store: sqlite at {:?}", path);
                return Arc::new(store);
            }
            Err(e) => log::warn!("Gossip sqlite open failed ({e}); falling back to memory"),
        }
    }
    #[cfg(target_arch = "wasm32")]
    {
        if let Some(vfs) = register_sqlite_vfs().await {
            match nostr_gossip_sqlite::store::NostrGossipSqlite::open_with_vfs(
                "nostr-blue-gossip.sqlite3",
                vfs,
            )
            .await
            {
                Ok(store) => {
                    log::info!("Gossip store: sqlite ({vfs})");
                    return Arc::new(store);
                }
                Err(e) => log::warn!("Gossip sqlite open failed ({e}); falling back to memory"),
            }
        }
    }
    Arc::new(nostr_gossip_memory::store::NostrGossipMemory::bounded(
        NonZeroUsize::new(10_000).expect("10_000 is non-zero"),
    ))
}

/// Default per-relay options (used for the bootstrap DEFAULT_RELAYS).
fn relay_options_default() -> nostr_sdk::relay::RelayOptions {
    nostr_sdk::relay::RelayOptions::new()
        .max_avg_latency(Some(Duration::from_secs(2)))
        .verify_subscriptions(true)
        // DO NOT enable `ban_relay_on_mismatch`: the SDK's verification
        // includes the filter's `since` (`MatchEventOptions::new()` has
        // `since: true`, nostr-sdk `relay/inner.rs` MATCH_EVENT_OPTS), so a
        // relay delivering legitimately-stored events older than a live
        // subscription's `since` — or more pre-EOSE events than a filter's
        // `limit` — gets PERMANENTLY banned (terminal status, no unban in
        // 0.45). This silently ejected users' own NIP-65 relays mid-session
        // and degraded every later fetch/publish. Keeping
        // `verify_subscriptions(true)` still validates and drops mismatched
        // events; we just never escalate to a ban.
        .ban_relay_on_mismatch(false)
        .adjust_retry_interval(true)
        .reconnect(true)
}

/// Base ClientBuilder carrying the shared configuration. Used at boot AND
/// on login/logout rebuilds.
fn base_client_builder(
    shared: &SharedResources,
    authenticator: Option<crate::stores::signer::BlueAuthenticator>,
) -> nostr_sdk::client::ClientBuilder {
    let gossip_limits = GossipRelayLimits {
        read_relays_per_user: 5,
        write_relays_per_user: 3,
        hint_relays_per_user: 2,
        ..Default::default()
    };
    let mut builder = nostr_sdk::client::ClientBuilder::new()
        .database(shared.database.clone())
        .gossip(shared.gossip.clone())
        .gossip_config(
            GossipConfig::default()
                .limits(gossip_limits)
                // v2 boot-stall plan: `onion` defaults to `true` upstream and
                // the gossip breakdown add+connects every resolved relay —
                // with no SOCKS proxy configured anywhere in this app those
                // dials can only fail (observed in the 98630372 smoke).
                // Non-TLS ws:// is already excluded by the default
                // `without_tls: false`.
                .allowed(GossipAllowedRelays {
                    onion: false,
                    ..Default::default()
                }),
        )
        .verify_subscriptions(true)
        // See relay_options_default(): banning on filter mismatch is a trap
        // (the SDK's match includes `since`, so stored-event delivery on
        // `since`-bearing live subs bans the relay terminally).
        .ban_relay_on_mismatch(false)
        .max_avg_latency(Duration::from_secs(2))
        .sleep_when_idle(SleepWhenIdle::Enabled {
            timeout: Duration::from_secs(30),
        })
        .monitor(shared.monitor.clone())
        .admit_policy(NostrBlueAdmissionPolicy);
    // Cap the pool at 100 relays on WASM. The browser WebSocket limit
    // is ~200/tab; 100 leaves headroom for other consumers (HLS audio,
    // nests, service worker). SleepWhenIdle(30s) closes idle relay
    // WebSockets, and cleanup_gossip_relays (5 min) evicts stale
    // GOSSIP-only relays, so the active WebSocket count stays well
    // under 100 in practice. Desktop stays uncapped.
    #[cfg(target_arch = "wasm32")]
    {
        builder = builder.max_relays(NonZeroUsize::new(100));
    }
    if let Some(auth) = authenticator {
        builder = builder.authenticator(auth);
    }
    builder
}

/// Add the bootstrap relays (defaults + DISCOVERY-only indexers) to a fresh
/// client.
async fn bootstrap_relay_pool(client: &Client) {
    let relay_opts = relay_options_default();
    let relay_futures: Vec<_> = DEFAULT_RELAYS
        .iter()
        .filter_map(|relay_url| {
            Url::parse(relay_url).ok().map(|_| {
                let url_str = relay_url.to_string();
                let opts = relay_opts.clone();
                let url = *relay_url;
                async move {
                    match client.add_relay(url).opts(opts).await {
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

    // P2P/Mostro relays are NOT pre-added here — they're added lazily by
    // `ensure_p2p_relays_connected` when P2P routes mount (post-login,
    // when the user's actual Mostro node config is available). Pre-adding
    // hardcoded defaults wastes pool slots on relays the user may not need.

    log::info!("Adding discovery relays for gossip...");
    let discovery_urls = crate::stores::relay::nip65::get_indexer_relay_urls();
    let discovery_urls = if discovery_urls.is_empty() {
        // Consolidated indexer defaults (single source: plan item #12 — the
        // legacy 3-relay DEFAULT_DISCOVERY_RELAYS set was merged into this
        // 6-relay taxonomy fallback).
        crate::stores::relay::nip65::DEFAULT_INDEXER_RELAYS
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>()
    } else {
        discovery_urls
    };
    for discovery_url in &discovery_urls {
        if let Err(e) = client
            .add_relay(discovery_url)
            .capabilities(nostr_sdk::relay::RelayCapabilities::DISCOVERY)
            .await
        {
            log::warn!("Failed to add discovery relay {}: {}", discovery_url, e);
        }
    }
}

/// Rebuild the client with (login) or without (logout) an authenticator.
///
/// 0.45 removed `Client::set_signer`; the only signer attachment point is
/// the build-time `ClientBuilder::authenticator`. Order matters: the new
/// client is written to `NOSTR_CLIENT` BEFORE the old one shuts down, so
/// the outer re-read loops (dispatcher, EOSE tracker, health poll,
/// provenance) pick up the new client immediately; the old client's
/// notification streams then terminate on `Shutdown`, letting stale
/// holders exit cleanly instead of keeping the old pool's sockets alive
/// (the pool shuts down on drop of the last Client handle).
/// Serializes client rebuilds: two interleaved rebuilds (login racing a
/// logout/npub-login on the multithreaded desktop runtime) would both
/// capture the same `old` client, and one old pool would never be shut
/// down — its sockets and per-relay retry tasks leak for the process
/// lifetime. `tokio::sync::Mutex` (not `std`) because the guard is held
/// across awaits: a std mutex re-locked from the same single-threaded wasm
/// task would deadlock.
static REBUILD_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

async fn rebuild_client(
    authenticator: Option<crate::stores::signer::BlueAuthenticator>,
) -> Result<Arc<Client>, String> {
    // Held across the whole swap (including the settle sleep) so the next
    // rebuild observes this rebuild's client as `old`.
    let _rebuild_serial = REBUILD_LOCK.lock().await;
    let shared = SHARED.get().ok_or("Shared resources not initialized")?;
    let new_client = Arc::new(base_client_builder(shared, authenticator).build());
    bootstrap_relay_pool(&new_client).await;
    new_client.connect().await;
    let old = NOSTR_CLIENT.read().clone();
    set_nostr_client(Some(new_client.clone()));
    if let Some(old_client) = old {
        // Give re-readers a moment to observe the new signal, then stop the
        // old pool. Streams end, stale tasks wind down.
        platform_sleep_ms(150).await;
        old_client.shutdown().await;
        log::info!("Old client shut down after rebuild");
    }
    Ok(new_client)
}

/// Initialize the Nostr client and connect to relays
pub async fn initialize_client() -> std::result::Result<Arc<Client>, String> {
    log::info!("Initializing Nostr client...");
    let database = open_shared_database().await?;
    let gossip = open_shared_gossip().await;
    let monitor = Monitor::new(1024);
    let shared = SharedResources {
        database,
        gossip,
        monitor,
    };
    if SHARED.set(shared).is_err() {
        return Err("Client resources already initialized".to_string());
    }
    let shared = SHARED.get().expect("just set");
    let client = Arc::new(base_client_builder(shared, None).build());
    bootstrap_relay_pool(&client).await;

    // Use fast connection with 2-second timeout for quick initial connection
    log::info!("Attempting fast relay connection...");
    let _connected = relay::try_connect_relays(&client, Duration::from_secs(2)).await;

    // Spawn PERSISTENT connection tasks for every pool member. `try_connect`
    // above is one-shot (no retry on failure); on a cold WASM start the TLS
    // handshakes often exceed its 2s window, leaving relays (including the
    // DISCOVERY-only indexers) unconnected until something else connects them.
    // `connect()` is non-blocking — it just spawns a retry task per relay
    // (with the relay's default `reconnect: true`) — so relays keep trying in
    // the background and become usable within a few seconds. This is what makes
    // the indexer metadata fetches (and logged-out global feed) work on a cold
    // first boot, instead of staying "No relays connected" until login.
    client.connect().await;

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
        use futures::FutureExt;
        let client_for_connect = client.clone();
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<bool>();
        tokio::spawn(async move {
            let output = client_for_connect
                .try_connect()
                .timeout(Duration::from_secs(3))
                .await;
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
    set_nostr_client(Some(client.clone()));
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
                        let mut guard = crate::stores::ndb::unknown_ids::UNKNOWN_IDS.lock().unwrap_or_else(|e| e.into_inner());
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
                        let mut guard = crate::stores::ndb::unknown_ids::UNKNOWN_IDS.lock().unwrap_or_else(|e| e.into_inner());
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
    crate::stores::eose_tracker::EoseTracker::init(client.clone());
    log::info!("Nostr client initialized with relays ready");
    Ok(client)
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
///
/// 0.45 removed runtime signer attachment; this rebuilds the client with a
/// NIP-42 authenticator built from the signer (shared database + gossip
/// store survive the rebuild).
pub async fn set_signer(signer: SignerType) -> std::result::Result<(), String> {
    log::info!("Setting signer: {}", signer.backend_name());
    get_client().ok_or("Client not initialized")?;
    let nostr_signer = signer.as_nostr_signer();
    rebuild_client(Some(crate::stores::signer::BlueAuthenticator(nostr_signer))).await?;
    *HAS_SIGNER.write() = true;
    *CURRENT_SIGNER.write() = Some(signer.clone());
    // Relay setup (connect, NIP-65 fetch, add user relays) is handled
    // sequentially in `run_post_login_init` — NOT here. Previously this
    // was a fire-and-forget spawn_forever that raced with run_post_login_init's
    // wait_for_user_relays poll, causing NIP-78 loaders to proceed before
    // user relays were connected.
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
    get_client().ok_or("Client not initialized")?;
    rebuild_client(None).await?;
    *HAS_SIGNER.write() = false;
    *CURRENT_SIGNER.write() = None;
    *relay::USER_RELAYS_APPLIED.write() = false;
    if let Some(client) = get_client() {
        relay::pool::reset_pool_to_defaults(&client).await;
    }
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
    pubkey: &nostr::key::PublicKey,
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
